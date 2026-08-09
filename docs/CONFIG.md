# Configuring Iridium

Iridium reads one file:

```text
~/.config/iridium/config.toml
```

(or `$XDG_CONFIG_HOME/iridium/config.toml` if that variable is set to an
absolute path). **You do not need one.** Everything below has a default, and an
absent file is not a problem — the editor never mentions it.

**A first run writes one anyway.** Not because the editor needs it, but
because "edit `~/.config/iridium/config.toml`" is useless advice when no such
directory exists — which is what Tom found on 9 Aug 2026, having been told
exactly that. The file it writes has every setting in it at its real default
and **every line commented out**, so it means precisely what no file means:
you change something by deleting a `# `.

An existing file is never touched, and a home directory that cannot be written
to costs the courtesy and nothing else — the session opens as it always did.

Run **Edit Configuration** from the command palette to open it, whether or not
it exists yet. That tab is a real file: `⌘S` saves it, and `⌃⌥R` applies it.

The `[editor]` block in that file is generated from the settings struct
itself, so it cannot name a setting that does not exist or show a default that
has since changed. The table further down this page can be wrong; that file
cannot.

The file is only read by the native faces. A browser has no configuration file,
so nothing here applies to the web demo.

---

## Applying a change without restarting

Press **`⌃⌥R`** (or **`⌘⌥R`**), or run **Reload Configuration** from the command
palette. The file is read again and applied to the session you are in: the
`[editor]` settings reach every open tab, and your `[keys]` bindings *replace*
the ones already in force rather than stacking on top of them — so a binding you
**delete** from the file stops working, which is the half that is easy to get
wrong and the reason there is a command rather than a suggestion to restart.

A reload can never leave the editor unusable. A file with a mistake in it costs
that line and nothing else, exactly as at startup, and the bindings that were
working keep working. The `config.toml` tab is rewritten in place each time, so
there is always exactly one of it and it always describes the last read —
including saying so when nothing was refused.

Two things a reload does **not** touch. The theme is not in this file, so
`--theme` and the system appearance still decide it. And the file is read when
you ask, not when it changes: there is no watcher, deliberately, because a file
being edited passes through states its author never meant to apply.

---

## What happens when you get something wrong

Nothing stops the editor. That is deliberate, and the reason is circular: the
way you fix a configuration file is by opening it in the editor.

A mistake costs **its own line** and nothing else. A misspelled setting does not
take the settings written beside it, and a bad setting does not take your key
bindings. The one exception is a file that is not valid TOML at all — a missing
bracket, an unclosed quote — which leaves no sections to isolate, so everything
falls back to its default and the editor says so.

When anything could not be honoured, a tab called **`config.toml`** opens
behind whatever you were opening, listing every problem, and the message strip
tells you it is there. A misspelled setting is reported with the spelling that
would have worked.

---

## `[editor]` — settings

Write only what you want to change.

```toml
[editor]
tab_width = 2
font_size = 15.0
word_wrap = true
```

| setting | default | what it does |
|---|---|---|
| `tab_width` | `4` | Tab width, in spaces |
| `insert_spaces` | `true` | Insert spaces rather than a tab character |
| `auto_indent` | `true` | Carry the current indent onto a new line |
| `auto_pairs` | `true` | Close brackets and quotes as you type them |
| `line_comment_token` | *(none)* | Fallback comment token for a language that has none |
| `show_line_numbers` | `true` | The gutter |
| `show_minimap` | `true` | The minimap |
| `minimap_width` | `120.0` | Minimap width, in pixels |
| `minimap_position` | `"Right"` | `"Left"` or `"Right"` |
| `minimap_show_syntax_colors` | `true` | Colour in the minimap |
| `cursor_blink_ms` | `500` | Caret blink rate; `0` never blinks |
| `undo_group_timeout_ms` | `500` | Edits closer together than this undo as one |
| `scroll_past_end` | `false` | Allow scrolling past the last line |
| `word_wrap` | `false` | Wrap long lines |
| `highlight_current_line` | `true` | Tint the line the caret is on |
| `show_whitespace` | `false` | Draw spaces and tabs |
| `font_size` | `14.0` | Font size, in logical pixels |
| `line_height` | `1.5` | Line height, as a multiple of the font size |

If you misspell one, the editor tells you the name you probably meant. The list
of settings it checks against comes from the code itself, so it is never out of
date — this table can be, and that is the only reason to trust the editor's
answer over this one.

---

## `[keys]` — key bindings

The key is the chord, the value is the command it runs.

```toml
[keys]
"cmd+shift+p" = "palette.open"
"ctrl+alt+j"  = "cursor.lineDown"
"cmd+k cmd+c" = "comment.toggleLine"
```

Your bindings sit **on top of** the defaults, so binding a chord the editor
already uses replaces it. Everything you do not mention keeps working.

### Writing a chord

Modifiers, then the key, joined with `+` or `-`:

| modifier | also spelled |
|---|---|
| `ctrl` | `control` |
| `shift` | |
| `alt` | `option` |
| `meta` | `cmd`, `super`, `win` |

On a Mac, `cmd` is the Command key. The same file works on Linux, where it is
the Super key.

The key itself is a single character (`a`, `7`, `/`) or one of: `left`,
`right`, `up`, `down`, `home`, `end`, `pageup`, `pagedown`, `backspace`,
`delete`, `enter`, `tab`, `escape`, `space`, `minus`, `f1`…`f12`.

A **sequence** is chords separated by spaces: `"cmd+k cmd+c"` means press
`⌘K`, then `⌘C`.

### Finding a command's name

Open the command palette and run **List Every Command**. A tab opens with every
command's id, its title, and the key that runs it today — search it with `⌘F`.

(The palette itself matches on ids but shows titles, which is why there is a
command for this.)

### Unbinding

An empty command unbinds:

```toml
[keys]
"ctrl+f" = ""
```

This exists mainly for one situation. Binding a bare chord makes every longer
sequence starting with it unreachable — if `ctrl+f` opens search, then
`"ctrl+f x"` can never fire, because the editor runs `ctrl+f` the moment you
press it rather than waiting to see what comes next. The editor refuses such a
binding and says which sequence is in the way; unbinding it is how you make
room.

**The editor will never unbind something for you.** An unrelated line silently
switching off a key you rely on is exactly the kind of change you would find
out about at the worst moment. If a binding needs room made for it, you are
told, and you decide.
