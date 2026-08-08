# Themes

Theme files in Iridium's native JSON format. Point any face at one with
`--theme`:

```bash
iridium --theme ./themes/paper.json
iridium-desktop --theme ./themes/monochrome.json
```

`--theme dark` and `--theme light` need no file — they are the two presets
built into the binary. Anything else is read as a path, offered first to the
native parser and then to VS Code's, so a `.json` exported from VS Code works
here too. See `crates/iridium-config/src/theme.rs` for why that order and not
the other.

## What is here

Both files are Variants B and C of `docs/design/LIGHT-THEME-MAP.md` §2.3, the
two classic-Mac light faces that D-1 did **not** rule as the shipped preset.
Variant A "Platinum" won and is compiled in as `Theme::light()`; the ruling
priced these two at one file each and nothing in the binary, which is exactly
what they are.

| File | Variant | Character |
| --- | --- | --- |
| `paper.json` | B — "Paper" | System 7 on good stock: warm off-white, minimal chrome, earthy ink, every grey warm-shifted. The one designed to be lived in eight hours a day. |
| `monochrome.json` | C — "Monochrome" | System 6: pure white and pure black, dither greys, one restrained accent, colour kept for the places where it carries meaning. |

The *why* behind every row of both — why Paper's greys are warm-shifted, why
Monochrome keeps a real red for `error` and a 0.85 panel border where the
others use 0.55 — is in §2.3 of the map, which is the one place that says it.

## Editing these, or adding your own

Every field is stated explicitly, and that is load-bearing rather than
verbose. `EditorColors` and `SyntaxColors` both carry a container-level
`#[serde(default)]`, so a field you delete does not fail to load — it silently
takes the **dark** preset's value, and a misspelt key is discarded without
comment. A theme with one typo is a theme with one wrong colour and no error
message.

`every_shipped_file_states_every_field` in
`crates/iridium-editor/src/theme/classic.rs` catches that for the files in this
directory by re-serialising what it parses and comparing bytes. It also means
these files are in canonical serialiser output: reformat one by hand and the
test will tell you.

Drop a new `.json` in here and it joins that check, plus every rule the map
states — surfaces opaque, gutter distinct from the page, panel surface at
least 17/255 off it, body text at 12:1, caret at 15:1, all fourteen syntax
colours clearing WCAG AA against their own background, and `attribute` never
equal to `error`.

Nothing in this directory is compiled into any binary. It is read at run time,
from wherever you point `--theme`.
