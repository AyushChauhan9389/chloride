# Changelog

## v2.1.0 — Find anything, inline

The headline: **`cl find`** — a search command with an interactive inline
picker — plus a round of polish across every inline UI.

### 🔍 New: `cl find` (alias `cl f`)

Search file **contents** (regex, smart-case) and file **names** in a single
walk, built on ripgrep's own engine (`ignore` + `grep-searcher`, linked in —
no `rg` binary needed):

```bash
cl find token            # content search
cl find '*.zip'          # glob → name search, detected automatically
cl f -e rs 'fn main' src # extension filter, explicit root
```

On a terminal it opens an **inline picker** that redraws in place — no
fullscreen takeover, no scrollback spam:

- results stream in live while the walk runs, grouped by file, newest first,
  with a spinner in the header until the walk finishes
- matched terms are highlighted inside each line; emoji badges (🦀 📜 📦…)
  make the list scannable — set `CL_NO_EMOJI=1` for plain ASCII tags
- a preview pane follows the cursor on wide terminals, centred on the match
- `↑`/`↓` hop between **files**, `Ctrl+↑`/`Ctrl+↓` (or `j`/`k`) move one
  **line**, `/` edits the query live (the stale walk is cancelled instantly),
  `Tab` unfolds past the 50-hit content cap, `e` opens `$EDITOR` at the line
- the picker draws on **stderr** and prints the selection on **stdout**, so
  `cl upload "$(cl find zip)"` composes exactly how you'd hope

Without a terminal it streams plain results instead — `-l` for file names
only, `-0` for `xargs -0`, `-m` to cap results, `-I` to ignore ignore-files,
`--no-input` to force streaming. Content output renders git-diff style with
line numbers, dimmed context, and `⋮` between hunks.

### 🧪 New: `cl logo`

The Chloride lockup, pre-rendered from the SVG into terminal art at build
time: truecolor half-block art on colour terminals, monochrome braille in
pipes and under `NO_COLOR` — zero runtime image dependencies.

### ✨ Inline UI improvements

- Redesigned picker layout: name hits are single self-contained rows (icon,
  path, size, age); content hits group under a dim-directory/bold-filename
  heading with hit-count and size chips; selection is an accent rail
- Hits inside one file are now ordered by line number, not arrival order
- Footer hints adapt to state (fold key only appears when the cap is in play)

### 🐛 Fixes

- Terminal widths are now counted in **columns**, not chars — emoji and CJK
  no longer wrap a full-width row and drift the block
- ANSI parsing handles OSC sequences (hyperlinks) and all CSI final bytes;
  truncated coloured lines keep their colour and always close attributes
- A race that could silently drop a late search hit is gone
- Inline blocks (upload progress, expiry picker, regenerate) close on stderr
  — redirecting stdout no longer leaves a dangling block or a stray newline
- Upload expiry picker, progress bars, and the regenerate selector all share
  one rendering kernel with a fixed row count, so blocks never drift

### 📦 Also in this release

- Brand assets (SVG + PNG icon and lockup) now live in `assets/logo/`
- README rewritten: logo header, `cl find` documentation, picker key table

**Full changelog**: https://github.com/AyushChauhan9389/chloride/compare/v2.0.0...v2.1.0

## v2.0.0

- Release pipeline on GitHub Actions: tag-driven versioning
  (`scripts/set-version.sh`), static musl Linux + Windows binaries with
  checksums, and install scripts
- Self-updater: daily silent check, `cl update` for right now,
  `CL_NO_UPDATE=1` to opt out
- Release pre-push hook (`.githooks/pre-push`) offering a version bump on
  every push of `main`
