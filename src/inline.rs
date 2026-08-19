//! Shared plumbing for the in-place inline UIs (expiry picker, file selector,
//! search picker).
//!
//! These draw a fixed-height block that redraws over itself, so the terminal
//! ends up with one block rather than a screenful of scrollback. The rules that
//! make that work:
//!
//! * Never emit `\x1b[J` (clear to end of screen) — it eats whatever the user
//!   had below the block.
//! * Clear each row individually with `\r\x1b[K` instead.
//! * Always draw the *same number of rows*, padding with blanks, so the
//!   cursor-up count stays correct and the block never drifts.
//!
//! Everything here draws to **stderr**, not stdout. That keeps stdout clean for
//! the actual result, so a picker can stay interactive on the terminal while
//! its selection flows down a pipe — `cl find zip | cl upload -` shows the
//! picker and pipes the chosen path.

use std::io::{self, Write};

/// Terminal width in columns, or 80 when it cannot be determined.
///
/// Some pseudo-terminals report a width of 0 rather than failing outright, so
/// an `unwrap_or` on the error alone is not enough — a 0 here truncates every
/// row to a lone ellipsis. Anything implausibly narrow falls back too.
pub fn term_width() -> usize {
    const FALLBACK: usize = 80;
    const MIN_USABLE: usize = 20;
    match crossterm::terminal::size() {
        Ok((w, _)) if (w as usize) >= MIN_USABLE => w as usize,
        _ => FALLBACK,
    }
}

/// Drop ANSI escape sequences so display width can be counted from the visible
/// characters alone.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Truncate to `max` visible columns, ignoring escape sequences when counting.
///
/// Escapes are copied through verbatim (they occupy no columns) so a truncated
/// line keeps its colour instead of rendering plain — rebuilding from the
/// stripped text loses every attribute, which shows up as unstyled rows
/// wherever a line happens to be long.
pub fn truncate(s: &str, max: usize) -> String {
    if strip_ansi(s).chars().count() <= max {
        return s.to_string();
    }

    let mut out = String::new();
    let mut visible = 0usize;
    let mut saw_escape = false;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Copy the whole sequence; it costs no display width.
            saw_escape = true;
            out.push(c);
            for next in chars.by_ref() {
                out.push(next);
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        if visible + 1 >= max {
            out.push('…');
            break;
        }
        out.push(c);
        visible += 1;
    }

    // Close any still-open attribute so it cannot bleed into the next row.
    if saw_escape {
        out.push_str("\x1b[0m");
    }
    out
}

/// Visible width of a string, ignoring escape sequences.
pub fn visible_width(s: &str) -> usize {
    strip_ansi(s).chars().count()
}

/// Truncate to `width` visible columns, then pad with spaces to exactly that
/// width. Needed to line up side-by-side columns: `format!("{s:<40}")` counts
/// escape bytes and would leave coloured rows short.
pub fn fit(s: &str, width: usize) -> String {
    let cut = truncate(s, width);
    let pad = width.saturating_sub(visible_width(&cut));
    format!("{cut}{}", " ".repeat(pad))
}

/// Redraw `lines` in place. `prev_lines` tracks how tall the block was last
/// time and is updated; pass `0` to draw fresh (after an editor has repainted
/// the screen, for instance).
///
/// Callers must pass a stable row count — use [`pad_to`].
pub fn redraw(lines: &[String], prev_lines: &mut usize) {
    let width = term_width();
    let mut out = String::new();

    if *prev_lines > 1 {
        // After drawing N rows the cursor sits on row N, so stepping up N would
        // land above the block. N-1 returns to the first row.
        out.push_str(&format!("\x1b[{}A", *prev_lines - 1));
    }

    for (i, line) in lines.iter().enumerate() {
        out.push_str("\r\x1b[K");
        out.push_str(&truncate(line, width));
        if i + 1 < lines.len() {
            out.push_str("\r\n");
        }
    }

    let mut err = io::stderr();
    let _ = err.write_all(out.as_bytes());
    let _ = err.flush();
    *prev_lines = lines.len();
}

/// Pad or trim to exactly `n` rows so the block height never changes.
pub fn pad_to(mut lines: Vec<String>, n: usize) -> Vec<String> {
    while lines.len() < n {
        lines.push(String::new());
    }
    lines.truncate(n);
    lines
}

/// Move below a drawn block so subsequent output continues after it instead of
/// overwriting it.
pub fn finish(prev_lines: usize) {
    if prev_lines > 0 {
        let mut err = io::stderr();
        let _ = err.write_all(b"\r\n");
        let _ = err.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_escapes_only() {
        assert_eq!(strip_ansi("\x1b[36mhi\x1b[0m"), "hi");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\x1b[2m▶\x1b[0m x"), "▶ x");
    }

    #[test]
    fn truncate_counts_visible_characters_not_escape_bytes() {
        // The bug in upload.rs's private copy: escape bytes counted as width,
        // so a coloured line was cut far too early.
        let coloured = "\x1b[36mabcdefghij\x1b[0m";
        assert_eq!(strip_ansi(&truncate(coloured, 100)).chars().count(), 10);
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
        assert_eq!(truncate("abc", 10), "abc");
        // Exactly at the limit is left alone.
        assert_eq!(truncate("abcde", 5), "abcde");
    }

    #[test]
    fn truncate_keeps_colour_on_a_cut_line() {
        // Rebuilding from stripped text used to drop every escape, so any line
        // long enough to truncate rendered unstyled.
        let cut = truncate("\x1b[36mabcdefghij\x1b[0m", 5);
        assert!(cut.contains("\x1b[36m"), "lost its colour: {cut:?}");
        assert!(cut.ends_with("\x1b[0m"), "left an attribute open: {cut:?}");
        assert_eq!(strip_ansi(&cut), "abcd…");
        // A plain line gains no stray reset.
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn fit_pads_by_visible_width_not_byte_length() {
        // A coloured string is far longer in bytes than on screen; padding on
        // byte length would leave columns ragged.
        let out = fit("\x1b[36mabc\x1b[0m", 8);
        assert_eq!(visible_width(&out), 8, "{out:?}");
        assert!(out.contains("\x1b[36m"));
        assert_eq!(visible_width(&fit("abcdefghij", 5)), 5);
        assert_eq!(fit("ab", 5), "ab   ");
    }

    #[test]
    fn term_width_never_returns_an_unusable_value() {
        // Whatever the environment reports, callers must get a width they can
        // actually lay out in — a pty reporting 0 columns once truncated every
        // row to a single ellipsis.
        let w = term_width();
        assert!(w >= 20, "got {w}");
    }

    #[test]
    fn pad_to_fixes_the_row_count_in_both_directions() {
        assert_eq!(pad_to(vec!["a".into()], 3), vec!["a", "", ""]);
        assert_eq!(pad_to(vec!["a".into(), "b".into(), "c".into()], 2), vec!["a", "b"]);
    }
}
