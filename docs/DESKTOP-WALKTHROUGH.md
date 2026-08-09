# Iridium desktop — a walkthrough

Every key here was read out of the binding tables, not remembered. Where a
chord has both a `Ctrl` and a `⌘` spelling, **both work** — the kernel binds the
`Ctrl` one so a terminal can reach it, and the desktop face adds the `⌘` one on
top, pointing at the same command. Neither is a translation of the other; they
are two keys on one id.

If any of this drifts, the editor itself is the authority: run **List Every
Command** from the palette and you get every command by the id to write in
`config.toml`, with the key that runs it *today*, rendered from the live keymap.

> ⚠️ **Corrections, 9 Aug 2026.** The first revision of this page asserted two
> things that were not true, both found by Tom rather than by me: an
> `iridium-desktop` command on the PATH, and `⌘⌥E` for the file explorer. The
> `⌘` half of the second was inferred from the panel's *close* handler instead
> of read from the desktop face's binding table, which is the mistake this page
> claimed in its first line not to make. Both are marked below with the task
> that fixes them. #104 also adds a test that enumerates every chord rather
> than trusting a reading of the table, because a table read by eye is exactly
> what failed here.

---

## Starting it

⚠️ **There is no `iridium-desktop` command on your PATH.** An earlier revision
of this page said there was; there is not, and never was — the binary lives
inside `/Applications/iridium.app` and nothing puts it anywhere a shell looks.
Until #106 lands, the command line is reachable only through the bundle:

```bash
open -a iridium                                     # empty session
/Applications/iridium.app/Contents/MacOS/iridium-desktop ~/some/project
```

#106 makes `iridium <dir>` the one command, placed on PATH by the installer.

A **directory** opens as a project: nothing is read as text, the file explorer
comes up rooted where you pointed, and it stays rooted there for the session —
close the panel and reopen it and you are still in the project.

A **file** opens as a file, with no panel. A path that **does not exist yet** is
a file to be written: an empty buffer that knows its own name.

Without `--theme`, the window follows the system appearance and keeps following
it. The moment you toggle the theme by hand, it stops following — a flip at
sunset must not silently reverse a choice you made.

---

## The three panels

All three are toggles on the same `⌃⌥` shape, and all three are modal while up:
keys go to the panel, not the document.

| | Key | |
|---|---|---|
| File explorer | `⌃⌥E` **only** | any file in the project |
| Undo tree | `⌃⌥H` / `⌘⌥H` | every branch of the history |
| Command palette | `⌃K` / `⌘K`, or `⌃P` / `⌃⇧P` | every command by name |

`Esc` closes any of them. So does pressing the chord that opened it.

---

## The command palette — `⌘K`

Type to filter. It searches titles, ids, categories, descriptions and aliases,
and shows the key that runs each command beside it. Recently-run commands rank
higher, so the things you actually use surface first.

- `↑` / `↓`, or `⌃P` / `⌃N` — move
- `PageUp` / `PageDown` — jump
- `Enter` — run
- `Esc` or `⌘K` — close

**Two commands are palette-only and worth knowing:**

- **List Every Command** — the reference tab. This is how you find out what to
  write in `config.toml`.
- **Reload Configuration** — see below; it also has a key.

---

## The file explorer — `⌘⌥E`

### Browsing

- Type — filters. A leading `/` makes the rest a regular expression.
- `↑` / `↓`, or `⌃P` / `⌃N` — move
- `→` — expand a folder, `←` — collapse
- `Enter` — open a file, or expand/collapse a folder
- `Home` / `End` — top, bottom
- `⌘↓` / `⌃↓` — **make the selected folder the root**
- `⌘↑` / `⌃↑` — **go up one folder**
- `Backspace` — delete a character from the filter
- `Esc` — close

The re-rooting pair is how you move around a machine without restarting. If the
session was opened on a project, closing and reopening the panel returns to the
project root, not to wherever you had wandered.

### The oil buffer — press `Tab`

The right-hand end of the query row says `tab to edit these rows`. It is there
whenever `Tab` would actually work, and goes while you are typing a filter —
the field needs the width.

`Tab` turns the rows into **editable text**. You then rename, move, create and
delete files by editing lines, exactly as you would edit a document:

- **Rename** — edit the text of a row.
- **Move** — edit its path.
- **Create** — `⌃Enter` types a new row; write a name. A trailing `/` makes it a
  directory.
- **Delete** — `⌃D` strikes a row through. Press it again to un-strike; that is
  the undo, rather than retyping the name.
- `↑` / `↓` or `⌃P` / `⌃N`, `←` / `→`, `Home` / `End`, `Backspace`, `Delete` all
  behave as they do in a text field.

Then:

- **`⌘S`** — shows you exactly what it is about to do, whole paths, before
  anything happens.
- **`y`** — do it. **`n`** or `Esc` — back out.
- `Esc` from the edit buffer — stop editing. It **refuses** while there are
  unapplied edits and says so; press `Esc` again to throw them away.

Nothing touches the filesystem until you confirm, and the confirmation prints
whole paths rather than fragments, which is the right way to be wrong about a
destructive operation.

---

## The undo tree — `⌘⌥H`

Iridium never loses work. Undo, type something else, and the thing you undid is
still there on a branch.

In the document:

- `⌘Z` / `⌃Z` — undo, `⌘⇧Z` / `⌃⇧Z` / `⌃Y` — redo
- `⌃⌥Z` — previous branch, `⌃⌥Y` — next branch

In the panel:

- `↑` / `↓`, `PageUp` / `PageDown`, `Home` / `End` — move
- `Enter` — jump to that node; the document becomes exactly what it was there
- `Esc` or `⌘⌥H` — close

---

## Search — `⌘F` / `⌃F`

- Type — searches as you go
- `Enter` — next match, `↑` / `↓` — walk matches
- `Tab` — move to the replace field
- `⌃R` — replace this one, `⌃⌥R` — replace all
- `⌥`+letter — toggle the options (case, whole word, regex)
- `Esc` — close

Search is **not** modal: keys it does not bind stay the document's, so `⌘S`
works mid-search.

---

## Tabs

- `⌃⇧]` — next tab, `⌃⇧[` — previous tab
- `⌘S` / `⌃S` — save. Refuses if the file changed on disk underneath you.
- `⌘⌥S` / `⌃⌥S` — save anyway.

---

## Editing

Everything below is the kernel's, so it is identical in every face.

**Motion** — `⌥←` / `⌥→` by word, `⌘←` / `⌘→` line start/end, `⌘↑` / `⌘↓`
document start/end. Add `⇧` to any of them to select.

**Lines** — `⌥↑` / `⌥↓` move the line, `⇧⌥↑` / `⇧⌥↓` duplicate it, `⌃⇧K` delete
it, `⌃J` join.

**Multi-cursor** — `⌃⌥↑` / `⌃⌥↓` add a cursor above/below, `⌃D` add the next
occurrence of the selection, `⌃⇧L` select every occurrence, `⌃U` drop the last
cursor. `Esc` collapses back to one.

**Structure** — `⇧⌥→` expand the selection by syntax node, `⇧⌥←` shrink it.
`⌃⇧⌘→` / `⌃⇧⌘←` do the same (the mac spelling). Start on a token and press it
repeatedly to reach the enclosing object, then the whole document.

**Comments** — `⌃/` toggles a line comment, `⇧⌥A` a block comment.

**Delete** — `⌥⌫` / `⌥⌦` by word, `⌘⌫` / `⌘⌦` to line start/end.

**Clipboard** — `⌘C` / `⌘X` / `⌘V`, and the `⌃` spellings. Real system
clipboard.

**Right-click** — Cut, Copy, Paste, Select All, Command Palette…

---

## Configuration

One file, and you do not need one:

```text
~/.config/iridium/config.toml
```

```toml
[editor]
font_size = 15.0
tab_width = 2
show_whitespace = true

[keys]
"cmd+shift+p" = "palette.open"
"cmd+k cmd+c" = "edit.toggleLineComment"
"ctrl+k"      = ""                        # empty unbinds
```

`cmd`, `meta`, `super` and `win` are all the Command key. A space separates the
chords of a sequence. Command ids come from **List Every Command**.

### Applying it without restarting — `⌃⌥R` / `⌘⌥R`

Or **Reload Configuration** in the palette. `[editor]` settings reach every open
tab; `[keys]` bindings **replace** the ones in force rather than stacking on
them — so a binding you *delete* from the file stops working.

A mistake costs its own line. The bindings that were working keep working, a
`config.toml` tab tells you which line was refused and why, and the message
strip says the tab is there. That tab is rewritten in place each reload, so
there is always exactly one of it and it always describes the last read —
including saying so when nothing was refused.

The file is read **when you ask**, not when it changes: a file being edited
passes through states you never meant to apply.

---

## Themes

`⌃⌥T` / `⌘⌥T` swaps light and dark. `--theme <path>` takes an Iridium theme file
or a **VS Code** theme file. Two more light themes ship as files:

```bash
iridium-desktop --theme themes/paper.json
iridium-desktop --theme themes/monochrome.json
```

---

## What is not there yet

Said plainly so you are not looking for them:

- **No menu bar and no Open… dialog.** The right-click menu is the only menu.
  Opening a project is the command line. (#107)
- **No soft wrap** yet.
- **`ast.*` navigation beyond expand/shrink** is kernel-side but not all bound.
