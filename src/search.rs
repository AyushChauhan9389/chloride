//! File and content search, built on ripgrep's engine (the `ignore` walker and
//! the `grep-*` searcher crates — the libraries `rg` itself is made of, linked
//! in, so there is no `rg` binary to install).
//!
//! One mechanism serves both front ends: [`spawn`] starts a parallel walk on
//! background threads and streams [`Hit`]s back over a channel. Inline commands
//! block on that channel and print; the TUI polls it each frame and can cancel
//! mid-flight when the query changes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use anyhow::{Context, Result};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::{WalkBuilder, WalkState};

/// Bound on queued-but-unconsumed hits. Keeps a fast producer from ballooning
/// memory when the consumer (especially the TUI, which renders at ~60fps) is
/// slower than the walk.
const CHANNEL_BOUND: usize = 1024;

/// Directories never worth searching, even with hidden files enabled. In a
/// typical repo `.git/` is ~88% of all hidden entries and none of it is
/// content anyone means to find — `cl find config` would otherwise surface
/// `.git/config` ahead of `src/config.rs`.
///
/// Only skipped below the search root, so `cd .git && cl find HEAD` still
/// works as the escape hatch.
const SKIP_ALWAYS: [&str; 1] = [".git"];

/// Default cap on results for an interactive search (the inline picker and the
/// TUI overlay). A common word in a large tree matches tens of thousands of
/// lines; nobody scrolls past a few hundred, and gathering the rest just makes
/// the UI spend its time inserting and re-sorting instead of drawing. Narrow
/// the query to see different results rather than more of them.
pub const DEFAULT_LIMIT: usize = 500;

/// Build the query an interactive search should run for `pattern`.
///
/// Contents by default, names when the pattern cannot compile as a regex
/// (`*.zip` is a glob aimed at file names, and it is the commonest thing
/// anyone types), capped at [`DEFAULT_LIMIT`]. Both front ends go through
/// this so a given pattern searches the same thing wherever it is typed.
pub fn plan(pattern: impl Into<String>, root: impl Into<PathBuf>) -> Query {
    let pattern = pattern.into();
    let kind = if is_valid_regex(&pattern) {
        Kind::Content
    } else {
        Kind::Files
    };
    let mut query = Query::new(pattern, root, kind);
    query.limit = Some(DEFAULT_LIMIT);
    query
}

/// The order every result list is kept in: name hits before content hits,
/// newest file first, then path and line so hits inside one file read top to
/// bottom.
///
/// A parallel walk yields results in nondeterministic order, so lists insert
/// with this rather than appending — otherwise the list visibly reshuffles
/// while the user is trying to read it. Shared by both front ends so a result
/// sits in the same place wherever it is shown.
pub fn compare_hits(
    (a, a_mtime): (&Hit, std::time::SystemTime),
    (b, b_mtime): (&Hit, std::time::SystemTime),
) -> std::cmp::Ordering {
    fn line(hit: &Hit) -> u64 {
        match hit {
            Hit::File { .. } => 0,
            Hit::Line { line, .. } => *line,
        }
    }
    let is_file = |h: &Hit| matches!(h, Hit::File { .. });
    is_file(a)
        .cmp(&is_file(b))
        .reverse()
        .then_with(|| a_mtime.cmp(&b_mtime).reverse())
        .then_with(|| a.path().cmp(b.path()))
        .then_with(|| line(a).cmp(&line(b)))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Match against file names only.
    Files,
    /// Match against file contents only.
    Content,
    /// Both, in a single walk. Name and content matches for the same file are
    /// reported separately, so the caller can section them.
    Both,
}

impl Kind {
    fn wants_names(self) -> bool {
        matches!(self, Kind::Files | Kind::Both)
    }
    fn wants_content(self) -> bool {
        matches!(self, Kind::Content | Kind::Both)
    }
}

#[derive(Clone)]
pub struct Query {
    pub pattern: String,
    pub root: PathBuf,
    pub kind: Kind,
    /// Search files that ignore rules would normally skip.
    pub no_ignore: bool,
    /// Include dotfiles and dot-directories. On by default — unlike `rg`, which
    /// hides them — because the dotfiles people actually keep (`.env`,
    /// `.github/`, `.gitignore`) are usually the ones they are looking for.
    /// `.git/` is always excluded regardless; see [`SKIP_ALWAYS`].
    pub hidden: bool,
    /// Stop after this many hits. `None` means unlimited.
    pub limit: Option<usize>,
    /// Lines of context to emit either side of each match. Only meaningful for
    /// content searches, and only used by the rendered (non-picker) output.
    pub context: usize,
    /// Only consider files with this extension (no leading dot). The cheap
    /// 5-line stand-in for rg's `-t`, which needs a table of hundreds of
    /// language definitions.
    pub ext: Option<String>,
}

impl Query {
    pub fn new(pattern: impl Into<String>, root: impl Into<PathBuf>, kind: Kind) -> Self {
        Self {
            pattern: pattern.into(),
            root: root.into(),
            kind,
            no_ignore: false,
            hidden: true,
            limit: None,
            ext: None,
            context: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A file whose name matched.
    File { path: PathBuf },
    /// A line whose contents matched, or a neighbouring context line when
    /// [`Query::context`] is non-zero.
    Line {
        path: PathBuf,
        line: u64,
        text: String,
        /// True for surrounding context rather than an actual match. Context
        /// lines are shown but never counted as results or selected.
        context: bool,
    },
}

impl Hit {
    pub fn path(&self) -> &Path {
        match self {
            Hit::File { path } | Hit::Line { path, .. } => path,
        }
    }
}

/// A running search. Dropping this cancels the walk.
pub struct Search {
    pub hits: Receiver<Hit>,
    cancel: Arc<AtomicBool>,
}

impl Search {
    /// Ask the walk to stop. Worker threads notice at the next file boundary,
    /// so this returns immediately rather than blocking until they wind down.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// `rg`'s smart-case rule: an all-lowercase pattern matches case-insensitively,
/// any uppercase makes it case-sensitive.
fn is_smart_case_insensitive(pattern: &str) -> bool {
    !pattern.chars().any(char::is_uppercase)
}

/// Treat a pattern containing glob metacharacters as a glob, otherwise as a
/// plain substring. `cl find '*.zip'` and `cl find zip` both do what you expect.
// ponytail: no regex mode for file names yet — add a --regex flag if substring
// and glob turn out not to cover it.
fn file_name_matches(pattern: &str, ignore_case: bool, name: &str) -> bool {
    let (pattern, name) = if ignore_case {
        (pattern.to_lowercase(), name.to_lowercase())
    } else {
        (pattern.to_string(), name.to_string())
    };

    if pattern.contains(['*', '?']) {
        glob_matches(&pattern, &name)
    } else {
        name.contains(&pattern)
    }
}

/// Minimal glob: `*` spans any run of characters, `?` spans exactly one.
/// Iterative backtracking, so it cannot blow the stack on a hostile pattern.
fn glob_matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0usize, 0usize);
    // Where to resume if the current `*` guess turns out to be wrong.
    let (mut star, mut resume) = (None, 0usize);

    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            resume = ni;
            pi += 1;
        } else if let Some(s) = star {
            // Backtrack: let the `*` swallow one more character.
            pi = s + 1;
            resume += 1;
            ni = resume;
        } else {
            return false;
        }
    }
    // Trailing `*`s can match the empty remainder.
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Feeds hits down the channel, distinguishing real matches from the context
/// lines around them. Only matches count toward the limit.
struct Collector<'a> {
    path: &'a Path,
    tx: &'a SyncSender<Hit>,
    found: &'a AtomicUsize,
    limit: usize,
    cancel: &'a AtomicBool,
    quit: bool,
}

impl Collector<'_> {
    fn emit(&mut self, bytes: &[u8], line: Option<u64>, is_context: bool) -> bool {
        if self.cancel.load(Ordering::Relaxed) {
            self.quit = true;
            return false;
        }
        let text = String::from_utf8_lossy(bytes)
            // Deliberate deviation from rg, which keeps the \r on CRLF files: a
            // bare \r returns the cursor to column 0, overwriting the line in
            // terminal output and corrupting TUI rendering.
            .trim_end_matches(['\n', '\r'])
            .to_string();

        if !is_context && self.found.fetch_add(1, Ordering::Relaxed) >= self.limit {
            self.quit = true;
            return false;
        }
        let hit = Hit::Line {
            path: self.path.to_path_buf(),
            line: line.unwrap_or(0),
            text,
            context: is_context,
        };
        if self.tx.send(hit).is_err() {
            self.quit = true;
            return false;
        }
        true
    }
}

impl Sink for Collector<'_> {
    type Error = std::io::Error;

    fn matched(&mut self, _: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        Ok(self.emit(m.bytes(), m.line_number(), false))
    }

    fn context(&mut self, _: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
        Ok(self.emit(c.bytes(), c.line_number(), true))
    }
}

/// Whether a pattern can compile as a content regex. Callers use this to pick
/// a sensible default: `cl find TODO` means contents, but `cl find '*.zip'`
/// obviously means names, and `*.zip` fails to compile ("repetition operator
/// missing expression").
pub fn is_valid_regex(pattern: &str) -> bool {
    RegexMatcherBuilder::new().build(pattern).is_ok()
}

/// Start a search. Returns immediately; hits stream in on the channel, which
/// closes when the walk finishes, hits the limit, or is cancelled.
pub fn spawn(query: Query) -> Result<Search> {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = sync_channel(CHANNEL_BOUND);

    // Build the matcher up front so a bad regex is reported to the caller
    // rather than swallowed on a worker thread.
    //
    // A merged search must tolerate a pattern that is a valid glob but not a
    // valid regex — `*.zip` is the commonest thing anyone types, and it fails
    // to compile with "repetition operator missing expression". In `Both` mode
    // that silently means "names only" rather than an error; asking for
    // content explicitly still reports the bad pattern.
    let matcher = if query.kind.wants_content() {
        let built = RegexMatcherBuilder::new()
            .case_insensitive(is_smart_case_insensitive(&query.pattern))
            .line_terminator(Some(b'\n'))
            .build(&query.pattern);
        match built {
            Ok(m) => Some(m),
            Err(_) if query.kind == Kind::Both => None,
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("invalid search pattern: {}", query.pattern));
            }
        }
    } else {
        None
    };

    let worker_cancel = cancel.clone();
    std::thread::spawn(move || {
        walk(query, matcher, tx, worker_cancel);
    });

    Ok(Search { hits: rx, cancel })
}

fn walk(
    query: Query,
    matcher: Option<grep_regex::RegexMatcher>,
    tx: SyncSender<Hit>,
    cancel: Arc<AtomicBool>,
) {
    let ignore_case = is_smart_case_insensitive(&query.pattern);
    let found = Arc::new(AtomicUsize::new(0));
    let limit = query.limit.unwrap_or(usize::MAX);

    WalkBuilder::new(&query.root)
        .hidden(!query.hidden)
        .ignore(!query.no_ignore)
        .git_ignore(!query.no_ignore)
        .git_global(!query.no_ignore)
        .git_exclude(!query.no_ignore)
        // depth 0 is the search root itself, which must never be filtered out.
        .filter_entry(|e| {
            e.depth() == 0 || !SKIP_ALWAYS.iter().any(|skip| e.file_name() == *skip)
        })
        .build_parallel()
        .run(|| {
            let tx = tx.clone();
            let cancel = cancel.clone();
            let found = found.clone();
            let matcher = matcher.clone();
            let pattern = query.pattern.clone();
            let kind = query.kind;
            let ext = query.ext.clone();
            let context = query.context;
            // One searcher per worker thread, reused across files.
            let mut searcher = SearcherBuilder::new()
                // Skip binaries the moment a NUL byte shows up, like rg does.
                .binary_detection(BinaryDetection::quit(0))
                .line_number(true)
                .before_context(context)
                .after_context(context)
                .build();

            Box::new(move |entry| {
                if cancel.load(Ordering::Relaxed) || found.load(Ordering::Relaxed) >= limit {
                    return WalkState::Quit;
                }
                let Ok(entry) = entry else {
                    return WalkState::Continue;
                };
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    return WalkState::Continue;
                }
                let path = entry.path().to_path_buf();

                // Extension filter applies to both name and content matching.
                if let Some(want) = &ext {
                    let got = path.extension().map(|e| e.to_string_lossy().to_lowercase());
                    if got.as_deref() != Some(want.as_str()) {
                        return WalkState::Continue;
                    }
                }

                // Name match first: it costs no I/O, so it is free to do even
                // when the real work is the content scan below.
                if kind.wants_names() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if file_name_matches(&pattern, ignore_case, &name)
                        && (found.fetch_add(1, Ordering::Relaxed) >= limit
                            || tx
                                .send(Hit::File {
                                    path: path.clone(),
                                })
                                .is_err())
                    {
                        return WalkState::Quit;
                    }
                }

                match &matcher {
                    None => {}
                    // Content search: stream matching lines out of the file.
                    Some(matcher) => {
                        let mut sink = Collector {
                            path: &path,
                            tx: &tx,
                            found: &found,
                            limit,
                            cancel: &cancel,
                            quit: false,
                        };
                        // Unreadable files (permissions, races) are skipped,
                        // not fatal.
                        let _ = searcher.search_path(matcher, &path, &mut sink);
                        if sink.quit {
                            return WalkState::Quit;
                        }
                    }
                }
                WalkState::Continue
            })
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run to completion and collect everything, sorted for a stable assert
    /// (worker threads finish in nondeterministic order).
    fn collect(query: Query) -> Vec<String> {
        let search = spawn(query).unwrap();
        let mut out: Vec<String> = search
            .hits
            .iter()
            .map(|h| match h {
                Hit::File { path } => format!("{}", path.file_name().unwrap().to_string_lossy()),
                Hit::Line { path, line, text, .. } => format!(
                    "{}:{line}:{text}",
                    path.file_name().unwrap().to_string_lossy()
                ),
            })
            .collect();
        out.sort();
        out
    }

    /// A throwaway tree: two source files, a hidden file, and an ignored dir.
    fn fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cl-search-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("node_modules")).unwrap();
        // .gitignore is only consulted inside a git repo — that is ripgrep's
        // default too. The marker directory is enough for repo detection.
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".gitignore"), "node_modules/\n").unwrap();
        std::fs::write(dir.join("a.rs"), "fn main() {}\nlet needle = 1;\n").unwrap();
        std::fs::write(dir.join("b.txt"), "no match here\nNEEDLE upper\n").unwrap();
        std::fs::write(dir.join(".hidden.rs"), "needle hidden\n").unwrap();
        std::fs::write(dir.join("node_modules/junk.js"), "needle ignored\n").unwrap();
        // A binary file must be skipped rather than dumped as garbage.
        std::fs::write(dir.join("blob.bin"), b"needle\x00\x01\x02binary").unwrap();
        dir
    }

    #[test]
    fn hidden_files_are_searched_by_default_but_gitignore_still_applies() {
        let dir = fixture("content");
        let hits = collect(Query::new("needle", &dir, Kind::Content));
        // .hidden.rs is included (hidden defaults on), node_modules/junk.js is
        // not (gitignored), and blob.bin is not (binary). b.txt matches because
        // an all-lowercase pattern is smart-cased to insensitive.
        assert_eq!(
            hits,
            vec![
                ".hidden.rs:1:needle hidden",
                "a.rs:2:let needle = 1;",
                "b.txt:2:NEEDLE upper",
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dot_git_is_never_searched_even_though_hidden_is_on() {
        let dir = fixture("dotgit");
        std::fs::write(dir.join(".git/config"), "needle in git internals\n").unwrap();
        let hits = collect(Query::new("needle", &dir, Kind::Content));
        assert!(!hits.iter().any(|h| h.starts_with("config")), "{hits:?}");
        // ...and by name, the case that made `cl find config` misleading.
        let names = collect(Query::new("config", &dir, Kind::Files));
        assert!(names.is_empty(), "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_hidden_excludes_dotfiles() {
        let dir = fixture("nohidden");
        let mut q = Query::new("needle", &dir, Kind::Content);
        q.hidden = false;
        let hits = collect(q);
        assert_eq!(hits, vec!["a.rs:2:let needle = 1;", "b.txt:2:NEEDLE upper"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn smart_case_makes_uppercase_patterns_strict() {
        let dir = fixture("case");
        let hits = collect(Query::new("NEEDLE", &dir, Kind::Content));
        assert_eq!(hits, vec!["b.txt:2:NEEDLE upper"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_ignore_and_hidden_flags_widen_the_walk() {
        let dir = fixture("widen");
        let mut q = Query::new("needle", &dir, Kind::Content);
        q.no_ignore = true;
        let hits = collect(q);
        assert!(hits.iter().any(|h| h.starts_with("junk.js")), "{hits:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_search_matches_names_without_reading_contents() {
        let dir = fixture("names");
        // .hidden.rs is included: dotfiles are searched by default.
        assert_eq!(
            collect(Query::new("*.rs", &dir, Kind::Files)),
            vec![".hidden.rs", "a.rs"]
        );
        // "needle" appears in file *contents*, never in a file name.
        assert!(collect(Query::new("needle", &dir, Kind::Files)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ext_filter_restricts_both_names_and_content() {
        let dir = fixture("ext");
        let mut q = Query::new("needle", &dir, Kind::Content);
        q.ext = Some("rs".into());
        // b.txt also contains a match but is filtered out by extension.
        assert_eq!(
            collect(q),
            vec![".hidden.rs:1:needle hidden", "a.rs:2:let needle = 1;"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn limit_caps_the_number_of_hits() {
        let dir = fixture("limit");
        let mut q = Query::new("needle", &dir, Kind::Content);
        q.limit = Some(1);
        assert_eq!(collect(q).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_stops_the_walk_and_closes_the_channel() {
        let dir = fixture("cancel");
        let search = spawn(Query::new("needle", &dir, Kind::Content)).unwrap();
        search.cancel();
        // Draining must terminate rather than hang once the walk is cancelled.
        let _: Vec<Hit> = search.hits.iter().collect();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_returns_name_and_content_hits_from_one_walk() {
        let dir = fixture("both");
        // "a" matches the name a.rs, and appears inside several files.
        let hits = collect(Query::new("needle", &dir, Kind::Both));
        assert!(hits.iter().any(|h| h.contains(':')), "expected content hits: {hits:?}");

        // A name-only pattern still yields its file through Both.
        let names = collect(Query::new("a.rs", &dir, Kind::Both));
        assert!(names.contains(&"a.rs".to_string()), "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_tolerates_a_glob_that_is_not_a_valid_regex() {
        let dir = fixture("globregex");
        // '*.rs' cannot compile as a regex. Both must degrade to names only
        // rather than erroring — it is the commonest thing anyone types.
        let hits = collect(Query::new("*.rs", &dir, Kind::Both));
        assert_eq!(hits, vec![".hidden.rs", "a.rs"]);

        // Asking for content explicitly still surfaces the bad pattern.
        assert!(spawn(Query::new("*.rs", &dir, Kind::Content)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_pattern_is_reported_to_the_caller() {
        let dir = fixture("badpat");
        assert!(spawn(Query::new("a(b", &dir, Kind::Content)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn glob_semantics() {
        assert!(glob_matches("*.zip", "a.zip"));
        assert!(glob_matches("*.zip", ".zip"));
        assert!(!glob_matches("*.zip", "a.zip.bak"));
        assert!(glob_matches("a?c", "abc"));
        assert!(!glob_matches("a?c", "ac"));
        assert!(glob_matches("*", "anything"));
        assert!(glob_matches("*", ""));
        assert!(glob_matches("a*b*c", "axxbyyc"));
        assert!(!glob_matches("a*b*c", "axxbyy"));
        // Backtracking: the first `*` must give characters back.
        assert!(glob_matches("*ab", "aaab"));
        assert!(glob_matches("*a*a", "aa"));
        assert!(!glob_matches("", "x"));
        assert!(glob_matches("", ""));
    }

    #[test]
    fn regex_validity_drives_the_default_mode() {
        assert!(is_valid_regex("TODO"));
        assert!(is_valid_regex("^pub fn"));
        // Globs are the common name-search pattern and are not valid regexes.
        assert!(!is_valid_regex("*.zip"));
        assert!(!is_valid_regex("*.rs"));
        assert!(!is_valid_regex("a("));
    }

    #[test]
    fn plan_infers_the_kind_and_caps_results() {
        // Both front ends go through plan(), so a pattern searches the same
        // thing wherever it is typed.
        assert!(plan("TODO", ".").kind == Kind::Content);
        assert!(plan("^pub fn", ".").kind == Kind::Content);
        // A glob cannot compile as a regex; it means file names.
        assert!(plan("*.zip", ".").kind == Kind::Files);
        // Interactive searches are capped: past a few hundred hits nobody
        // scrolls, and gathering more just makes the UI lag.
        assert_eq!(plan("TODO", ".").limit, Some(DEFAULT_LIMIT));
        assert_eq!(DEFAULT_LIMIT, 500);
    }

    #[test]
    fn compare_hits_orders_names_then_recency_then_line() {
        use std::time::{Duration, SystemTime};
        let old = SystemTime::UNIX_EPOCH;
        let new = old + Duration::from_secs(60);
        let file = |p: &str| Hit::File { path: PathBuf::from(p) };
        let line = |p: &str, n: u64| Hit::Line {
            path: PathBuf::from(p),
            line: n,
            text: "x".into(),
            context: false,
        };

        let ord = std::cmp::Ordering::Less;
        // Name hits sort ahead of content hits.
        assert_eq!(compare_hits((&file("a"), old), (&line("a", 1), old)), ord);
        // Newer files first.
        assert_eq!(compare_hits((&file("a"), new), (&file("b"), old)), ord);
        // Within one file, by line.
        assert_eq!(
            compare_hits((&line("a", 1), old), (&line("a", 9), old)),
            ord
        );
        // Same mtime: grouped by path so a file's hits stay contiguous.
        assert_eq!(
            compare_hits((&line("a", 9), old), (&line("b", 1), old)),
            ord
        );
    }

    #[test]
    fn smart_case() {
        assert!(is_smart_case_insensitive("needle"));
        assert!(!is_smart_case_insensitive("Needle"));
        assert!(is_smart_case_insensitive("needle.rs"));
    }

    #[test]
    fn file_names_match_by_substring_and_glob() {
        assert!(file_name_matches("zip", true, "Archive.ZIP"));
        assert!(!file_name_matches("zip", false, "Archive.ZIP"));
        assert!(file_name_matches("*.zip", true, "Archive.ZIP"));
        assert!(file_name_matches("main", true, "main.rs"));
        assert!(!file_name_matches("mian", true, "main.rs"));
    }
}
