# Iridium in the terminal — a daily driver's walkthrough

Everything in this document was verified against the source at the commit that
built it; nothing here is aspirational. Where the editor has a limit, the limit
is stated in its own section at the end rather than discovered by surprise.

## Starting it

```
Usage: iridium [OPTIONS] [FILE]

Arguments:
  [FILE]              The file to open. Created on save if it does not exist.
                      With no file, an empty unnamed buffer is opened.

Options:
  -l, --line <N>      Put the caret on line N, counting from one.
  +N                  The same, in the form every terminal editor accepts.
      --theme <T>     `dark`, `light`, or a path to a theme file. A theme file
                      may be in Iridium's own JSON format or VS Code's.
      --read-only     Open the file without allowing edits.
  -h, --help          Print this help and exit.
  -V, --version       Print the version and exit.
      --              Stop reading options; the next argument is the file.
```

One file at a time — a second path on the command line is refused with a
message, not silently ignored. Non-UTF-8 paths open fine. `iridium +42 notes.md`
lands the caret on line 42.

**Themes:** `--theme dark` (the default) and `--theme light` are built in.
Anything else is a path to a theme file — Iridium's own JSON format or a
VS Code theme, tried in that order. A fully transparent theme colour means
"use the terminal's own colour". The theme is fixed for the session; there is
no runtime switch yet.

## The survival card

| Key | What it does |
|---|---|
| `Ctrl+S` | Save. Refuses if the file changed on disk since you opened it. |
| `Ctrl+Alt+S` | Save anyway (overrides the changed-on-disk refusal). |
| `F5` | Re-read the file from disk (asks first if you have unsaved changes). |
| `Ctrl+Q` | Quit. Asks `Unsaved changes. Quit without saving? (y/n)` when dirty. |
| `Ctrl+G` | Go to line. |
| `Ctrl+Z` / `Ctrl+Shift+Z` (or `Ctrl+Y`) | Undo / redo. |
| `Ctrl+F` | Find (and replace). |
| `Ctrl+K` (or `Ctrl+P`) | The command palette: every command, by name. |
| `Ctrl+Alt+H` | The undo-tree panel: every branch of the history, visually. |
| `Escape` | Collapse to a single cursor; close panels. |

Saving is atomic: the file is written to a temp file in the same directory,
fsynced, then renamed over the target — a failure mid-save leaves your file
untouched. Staleness is detected by comparing bytes, not timestamps. Saving an
unnamed buffer opens a `Save as:` prompt. A read-only file refuses the save
with a message.

Dirtiness is content-based: undo back to exactly what is on disk and the `[+]`
indicator clears — quitting then asks nothing.

## The command palette

`Ctrl+K` (or `Ctrl+P` / `Ctrl+Shift+P`, for VS Code muscle memory) opens a
floating, rounded-cornered panel that runs any command by name — including the
41 commands that have no key on purpose: all fifteen `transform.*` verbs (case
conversions, sort lines, dedupe, trim trailing whitespace), the twenty `ast.*`
structural walks beyond expand/shrink, `edit.deleteToLineStart`/`End`, and
`history.redoBranch`.

Open it empty and it lists everything, most recently used first. Type to
filter — the fuzzy matcher, ranking and recency memory are the kernel's own,
so the same query finds the same command here as in every other face. Matched
characters are highlighted; a hit on an alias or a description shows the text
it actually hit; every command that has a key shows it, right-aligned.

| Key (palette open) | What it does |
|---|---|
| type | Narrow the list. |
| `Down` / `Up` (or `Ctrl+N` / `Ctrl+P`) | Move the selection. It clamps at the ends — no wrapping. |
| `PageDown` / `PageUp` | Hop a windowful. |
| `Enter` | Run the selected command. |
| `Escape` or `Ctrl+K` | Close. |

The palette is modal: while it is open every key belongs to it, so `Ctrl+S`
cannot save half-aimed. It floats over the document rather than taking rows
from it — closing it restores the exact screen underneath. A paste while it is
open lands in the query.

## Moving around

Arrows move; `Shift` extends the selection; `Ctrl` jumps by word (left/right)
or to the document ends (`Ctrl+Home` / `Ctrl+End`). `Home` is smart: first
press goes to the first non-whitespace character, second to column zero.
`PageUp` / `PageDown` hop a viewportful and keep your column, `Shift` extends.

Every motion has a `Select` twin on `Shift` — the full set is sixteen
horizontal verbs and eight vertical ones, and they behave identically from a
single cursor or twenty.

## Editing

Typing types. `Enter` applies auto-indent, bracket-block expansion and
code-fence expansion. `Tab` / `Shift+Tab` indent and outdent. `Backspace` /
`Delete` do what they say; with `Ctrl` they eat a word.

**Lines** — `Alt+Up` / `Alt+Down` move the current line (or every selected
line); `Shift+Alt+Up` / `Shift+Alt+Down` duplicate it; `Ctrl+Shift+K` deletes
it; `Ctrl+J` joins lines.

**Comments** — `Ctrl+/` toggles line comments; `Shift+Alt+A` toggles a block
comment (falls back to line comments for languages without a block pair).

**Clipboard** — `Ctrl+C` / `Ctrl+X` / `Ctrl+V`. Copy with nothing selected
copies each cursor's whole line. **The clipboard is process-local** — see the
limits section.

## Multi-cursor

| Key | What it does |
|---|---|
| `Ctrl+Alt+Up` / `Ctrl+Alt+Down` | Add a cursor above / below. |
| `Ctrl+D` | Add the next occurrence of the selection as another cursor. |
| `Ctrl+Alt+D` | Skip this occurrence, take the next one instead. |
| `Ctrl+Shift+L` | Select every occurrence at once. |
| `Ctrl+U` | Remove the most recently added cursor. |
| `Escape` | Back to one cursor. |

Every cursor gets a caret painted, every cursor edits, and the whole set
undoes as one step. On non-US layouts `AltGr` composing a character will not
spawn cursors — the keymap guards that trap explicitly.

## Structural selection (the syntax-aware part)

`Shift+Alt+Right` expands the selection to the enclosing syntax node —
identifier → expression → statement → block → function → file. Press it
repeatedly to climb. `Shift+Alt+Left` shrinks back down the exact path you
came, restoring precisely the cursors you started with. Works from a single
caret or from many.

This is driven by real tree-sitter parses of thirteen vendored grammars:
Rust, Python, TypeScript, JavaScript, TSX, Go, JSON, YAML, Markdown, CSS,
Bash, C, C++. Language is chosen by file extension; a file with no matching
extension is edited as plain text — no highlighting, no folds, no error.

## Folds

The gutter shows `v` on foldable lines and `>` on folded ones (ASCII on
purpose — the triangles are ambiguous-width and would break column alignment
on CJK-configured terminals).

| Key | What it does |
|---|---|
| `Alt+F` | Toggle the innermost fold around the caret. |
| `Ctrl+Alt+F` | Fold everything. |
| `Alt+U` | Unfold everything. |

Creating a fold over the caret lifts it to the fold's header. `Ctrl+G` to a
folded line unfolds whatever hides it.

## Search and replace

`Ctrl+F` opens a two-row panel below the text (it takes rows rather than
floating, so a match is never hidden under the panel that found it). Search is
incremental — every keystroke re-runs it and jumps to the match nearest the
caret.

| Key (panel open) | What it does |
|---|---|
| type | Edit the focused field. |
| `Tab` | Switch between Find and Replace fields. |
| `Enter` / `Down` | Next match. |
| `Shift+Enter` / `Up` | Previous match. |
| `Ctrl+R` | Replace the current match. |
| `Ctrl+Alt+R` | Replace all. |
| `Alt+C` / `Alt+W` / `Alt+R` | Toggle case-sensitive / whole-word / regex. |
| `Escape` | Close. |

Toggles render bracketed in text — `[Aa]` — not just in colour, so they read
on a 16-colour terminal. An invalid regex is a normal state: the count shows
`Invalid regex` and the engine's own complaint is displayed. The query
survives closing the panel and is offered back on reopen. `F3` / `Shift+F3`
step through matches with the panel closed. `Ctrl+S` still saves while the
panel is open — only the keys the panel understands are taken.

## The undo tree

Undo here is a tree, not a line: an edit after an undo starts a branch and the
abandoned branch is kept, so no sequence of undo/redo/type can lose work.

| Key | What it does |
|---|---|
| `Ctrl+Z` | Undo. |
| `Ctrl+Shift+Z` / `Ctrl+Y` | Redo along the preferred branch. |
| `Ctrl+Alt+Z` / `Ctrl+Alt+Y` | Cycle which branch redo will follow. |
| `Ctrl+Alt+H` | Open the undo-tree panel. |

The panel shows the whole tree: the state the file opened in at the top, time
running downward, each fork indenting its children. `*` marks where the
document is now; the bright rows are the path plain undo/redo travels;
dim rows are parked branches; each row shows its age. While it is open:

| Key (panel open) | What it does |
|---|---|
| `Up` / `Down` | Move the selection (clamps at the ends). |
| `PageUp` / `PageDown` | Hop by a windowful. |
| `Home` / `End` | The root / the newest state. |
| `Enter` | Jump the document to the selected state — the panel stays open, so you can watch the `*` move and hop between states. |
| `Escape` or `Ctrl+Alt+H` | Close. |

The panel is modal, like the palette: browsing history while stray keys
edited the document would grow the very tree being read.

History is in-memory: it dies with the process, and `F5` (reload) replaces it
along with the text — the confirm prompt exists because that discard is
unrecoverable.

## The status line

Left: the file name (`[No Name]` for an unnamed buffer) and `[+]` when dirty.
Right: `Ln n, Col n`, prefixed with a cursor count when there is more than
one. Messages and errors appear on this row until the next keypress; prompts
(`Go to line:`, `Save as:`, confirmations) paint over it rather than resizing
the viewport.

## Rendering notes worth knowing

- Tabs, CJK, emoji and grapheme clusters are measured properly; a double-width
  glyph clipped at the window edge keeps its cell rather than shifting the row.
- Long lines clip and the view scrolls horizontally with the caret — there is
  no soft wrap (deliberate; see limits).
- The painter diffs against what the terminal is already showing and emits
  minimal updates wrapped in synchronized output; truecolor / 256 / 16-colour
  support is negotiated, not assumed. An idle editor consumes nothing.
- Focus loss aborts a half-typed key sequence, so returning to the terminal
  never eats your first keystroke.

## The limits, honestly

- **One file per process.** No buffers, splits, tabs or windows. Run more than
  one `iridium`.
- **No mouse, at all** — no clicking, no wheel scroll. Deliberate for now: a
  terminal cannot supply the pixel metrics the kernel's hit-testing currently
  expects, and a caret in the wrong place is worse than no caret.
- **Clipboard is process-local.** Copy here pastes here, not into other apps.
  Use your terminal's own selection for cross-app copying until OSC 52 support
  lands. Stated rather than hidden, because a clipboard that silently isn't
  the system's is worse than one documented not to be.
- **No soft wrap** — long lines clip and scroll horizontally.
- **No Vim/modal mode.** The keymap is mode-free; a modal layer is a planned
  keymap, not a missing engine (the kernel supports keymap layers already).
- **No LSP, completion or diagnostics.**
- **No config file.** Keymap and settings are compile-time; only `--theme`
  takes a file today. Remaps and a user keymap file are kernel-supported but
  not yet wired to the face.
- **Language by extension only** — no shebang or content detection.
  (`.jsonc` currently gets no highlighting; `.json` does.)
- **Sticky column can be lost when a scroll pushes the caret off-screen** — a
  known kernel-surface gap, recorded, not yet fixed.
- **macOS durability caveat:** save uses fsync but not `F_FULLFSYNC`, so a
  power cut in the millisecond after a save can lose (never corrupt) that
  save. File ownership is not preserved across a save (permissions are).
- **No session/undo persistence** — history and window state die with the
  process.

## What "complete" looks like from here

In rough order of daily-driving value: OSC 52 clipboard, mouse support (needs
one kernel decision on cell-based hit-testing), runtime theme switching /
config file, soft wrap (needs the layout decision recorded in
`docs/SOFT-WRAP-DESIGN.md`), modal keymap layer. The command palette landed on
3 Aug 2026 and unlocked the 41 palette-only commands; the undo-tree panel
landed the same day.
