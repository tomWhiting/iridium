# Theme System Map — bundling a look, without hard-coding it

**Ground verified 8 Aug 2026.** Read-only lane; **no build, no test, no clippy
was run** — another workflow is editing `crates/iridium-editor/src/input/keyboard/`
in this checkout. Every claim below is from reading a file, and every file is
named. Where a fact comes from the research pass rather than from a file I
re-opened in this session, it says so.

The ask, in Tom's words (8 Aug):

> *"Could you look up how Zed does its themes because it would be good to be
> able to sort of bundle up you know themes and icons and all that kind of stuff
> together for it without sort of having to hard code stuff in."*

Two separable things are in that sentence: **a theme that is a file rather than
a Rust literal**, and **a bundle that carries a theme, icons and whatever else
as one droppable unit**. They have different answers and different prices, and
this map keeps them apart.

Standing context that binds this map: **L-0 was ruled to tier 2 on 8 Aug**
(`docs/SESSION-STATE.md:4718-4726`) — languages become drop-in extensions with
**no rebuild**, accepting a measured **6.6 MiB and 90 crates on every native
build** (`docs/IN-FLIGHT-languages.md:216-226`, task **#88**). So an
extension-loading mechanism is being built regardless. Whether themes ride it is
**T-3** below.

### Citation legend

| tag | file |
| --- | --- |
| `Th:` | `crates/iridium-editor/src/theme/colors.rs` |
| `Tm:` | `crates/iridium-editor/src/theme/mod.rs` |
| `Tf:` | `crates/iridium-editor/src/theme/fonts.rs` |
| `Cap:` | `crates/iridium-syntax/src/highlight/capture.rs` |
| `CapT:` | `crates/iridium-syntax/src/highlight/tests/capture.rs` |
| `Sp:` | `crates/iridium-syntax/src/highlight/span.rs` |
| `Sy:` | `crates/iridium-editor/src/syntax.rs` |
| `It:` | `crates/iridium-editor/src/span_index/interval_tree.rs` |
| `Cs:` | `crates/iridium-editor/src/render/compositor/settings.rs` |
| `Tx:` | `crates/iridium-editor/src/render/text.rs` |
| `O:` | `apps/iridium-desktop/src/overlay.rs` |
| `Fr:` | `apps/iridium-desktop/src/file_tree/rows.rs` |
| `St:` | `apps/iridium-desktop/src/app/startup.rs` |
| `Tt:` | `apps/iridium/src/theme.rs` (the TUI's `--theme` loader) |
| `P:` | `crates/iridium-tui/src/frame/palette.rs` |
| `W:` | `crates/iridium-bindings/src/wasm.rs` |
| `Ws:` | `crates/iridium-bindings/src/web_span_index.rs` |
| `Ex:` | `crates/iridium-explorer/src/node.rs` |
| `Gl:` | vendored `~/.cargo/registry/src/…/glyphon-0.10.0/src/` |
| `Zt:` | Zed `crates/settings_content/src/theme.rs` (cached copy, §2.0) |
| `Zs:` | Zed `crates/syntax_theme/src/syntax_theme.rs` (cached copy) |
| `Zi:` | Zed `crates/theme/src/icon_theme_schema.rs` (`/tmp/zedsrc`, commit `38ca9106`) |
| `Zf:` | Zed `crates/file_icons/src/file_icons.rs` (same clone) |
| `Zm:` | Zed `crates/extension/src/extension_manifest.rs` (cached copy) |
| `Z1:` | Zed `assets/themes/one/one.json` (cached copy) |

Prior art: `docs/design/LIGHT-THEME-MAP.md` (D-1..D-8 — §8 says exactly which
this map supersedes), `docs/design/DESKTOP-CHROME-MAP.md` (the chrome
derivation rules), `docs/IN-FLIGHT-languages.md` §4-5 (the extension shape and
L-0..L-8), `docs/IN-FLIGHT-registry.md` (tier 1's mechanics).

---

## The four load-bearing facts, up front

1. **Iridium already loads themes from files. The desktop face is the one that
   cannot.** `Theme::from_json` (Tm:182), `Theme::from_vscode_json` (Tm:245) and
   a whole `--theme` loader with two parsers and a stated precedence rule
   (Tt:1-31, `ThemeChoice` Tt:41-49) all exist and are wired — **on the terminal
   face only** (`apps/iridium/src/app/mod.rs:157-159`). The desktop face
   constructs `Workspace::new(user.editor.clone(), Theme::default())` (St:185),
   which is `Theme::dark()` (Tm:89-93). It has no `--theme` flag, and
   `crates/iridium-config/src/` contains the string "theme" **zero times**
   (grep, this session). There is **no `themes/` directory and no theme JSON
   file anywhere in the repository** (find, excluding `node_modules`/`target`).
   So: the format is loadable, the loader exists, and nothing ships a file.

2. **The theme's syntax vocabulary is fourteen colours behind a closed
   twenty-nine-variant enum, and twenty-nine of the vendored captures reach the
   screen with no colour at all.** `SyntaxColors` is 14 fields (Th:207-238);
   `HighlightType` is 29 variants (Cap:17-76); `highlight_to_color` (Sy:18-77)
   collapses the 29 onto the 14. Iridium's own vendored queries emit **107
   distinct capture names** (extracted this session from
   `crates/iridium-lang/src/languages/queries/*/highlights.scm`), of which
   `HighlightType::from_capture_name` resolves 78 and **29 resolve to `None`** —
   six of them deliberately, six named as a known gap with a ratchet test
   (`KNOWN_UNSTYLED_GAP`, CapT:78-84), the rest silently.

3. **Zed's theme system is not really a file format — it is a resolution model,
   and Iridium's web face already implements half of it.** Zed's `syntax` is an
   **open string map** resolved by longest dotted prefix (Zs:78-93). Iridium's
   *web* face resolves capture names the same way — `color_for_highlight_type`
   walks `rsplit_once('.')` up a `HashMap<String, Color>` (W:3138-3153) — while
   the *native* faces go through the closed enum. The two faces are structurally
   different palettes today, and the web one is fed only from JavaScript
   (`setSyntaxTheme`, W:1966-1992), which **nothing in this repository calls**
   (grep of all `.ts/.tsx/.js/.html` outside `node_modules`/`dist` returns the
   two definitions in `controller/index.ts:314,1676` and no call site). The
   web's syntax map is therefore empty in every shipping configuration.

4. **Iridium draws no icons at all — and the GPU face could, today, with no new
   pipeline.** The explorer draws `▾ `/`▸ ` and a trailing `/` (Fr:51-61); type
   is conveyed by colour, not iconography (Fr:66-73). But glyphon 0.10 — already
   the text renderer — carries a **custom-glyph path**: `TextArea.custom_glyphs`
   (`Gl:lib.rs:124`), `CustomGlyph { id, left, top, width, height, color, … }`
   (`Gl:custom_glyph.rs:8-30`), and `TextRenderer::prepare_with_custom(…,
   rasterize_custom_glyph: impl FnMut(RasterizeCustomGlyphRequest) ->
   Option<RasterizedCustomGlyph>)` (`Gl:text_render.rs:99-109`) which takes
   `Vec<u8>` of `ContentType::Mask` or `Color`. Iridium passes
   `custom_glyphs: &[]` (Tx:968) and calls plain `prepare`
   (`compositor/frame.rs:361`). **The hook is there and empty.** What is missing
   is not a renderer — it is something that turns an SVG into pixels. **No
   crate in the graph parses SVG**: there is no `usvg`, no `resvg`, no `lyon`.
   See §7.2, which corrects an earlier draft of this bullet.

---

## 1. Verified ground — what Iridium's theme system is today

### 1.1 The type, and the whole vocabulary a theme file can address

`Theme` (Tm:76-87) — five fields: `name`, `is_dark`, `editor`, `syntax`,
`typography`. `#[derive(Serialize, Deserialize)]`, so the native file format is
whatever serde makes of the struct.

- **`EditorColors` — 24 fields** (Th:76-125): `background`, `foreground`,
  `selection`, `selection_inactive`, `cursor`, `line_number`,
  `line_number_active`, `current_line`, `gutter`, `minimap_background`,
  `search_match`, `search_match_current`, `diff_added_bg`, `diff_deleted_bg`,
  `diff_added_gutter`, `diff_deleted_gutter`, `change_added`, `change_modified`,
  `change_deleted`, `blame_foreground`, `diagnostic_error`,
  `diagnostic_warning`, `diagnostic_info`, `diagnostic_hint`.
- **`SyntaxColors` — 14 fields** (Th:207-238): `keyword`, `string`, `number`,
  `comment`, `function`, `variable`, `type_name`, `operator`, `punctuation`,
  `property`, `constant`, `tag`, `attribute`, `error`.
- **`Typography` — 4 fields** (Tf:7-16): `font_family`, `font_size`,
  `line_height`, `letter_spacing`.

**42 keys plus `name`/`is_dark`. That is the entire theme vocabulary.** There is
no `panel_border` (D-5 has not landed — grep of Th, this session).

Both colour structs carry container-level `#[serde(default)]` (Th:75, Th:208),
and both `Default` impls are the **dark** preset (Th:194-200, Th:289-295). So a
*light* theme file that omits a field silently inherits a **dark** value. That
is the current fallback model, and it is wrong in a way §5 has to fix.

`Color` is `{r,g,b,a: f32}` in 0..1 (Th:7-16). `Color::from_hex` (Th:33-54)
accepts **6 or 8 hex digits**, `#` optional, and returns `None` otherwise — no
`#RGB`, no `#RGBA` short form.

### 1.2 Hard-coded, plainly

Every theme in use is a Rust literal. The constructors: `EditorColors::dark`
(Th:130), `::light` (Th:164), `SyntaxColors::dark` (Th:243), `::light`
(Th:264), `Theme::dark` (Tm:98), `::light` (Tm:110), `::default` → dark
(Tm:89), plus the three unreachable classic variants in
`crates/iridium-editor/src/theme/classic.rs`.

| face | how it gets a theme | can it load a file? |
| --- | --- | --- |
| **Terminal** (`apps/iridium`) | `--theme dark\|light\|<path>`; `ThemeChoice::parse` (Tt:58-64), native parser first then VS Code, both complaints reported on failure (Tt:13-31) | **Yes** |
| **Desktop** (`apps/iridium-desktop`) | `Theme::default()` at St:185. No flag, no config key | **No** |
| **Web** (wasm) | `setDarkTheme(bool)` → `compositor.set_dark_theme` (W:1878-1881) — picks between the two built-ins; `setSyntaxTheme` takes a `Record<string,string>` (W:1966-1992) but nothing calls it | **Partly, and unused** |

**There is no theme command.** Grep of `crates/iridium-editor/src/commands/`
for "theme" returns **zero** (this session). `Editor::set_theme` emits
`EditorEvent::ThemeChanged` which no face subscribes to. This is D-3 in
`LIGHT-THEME-MAP.md`, still open.

### 1.3 Where the theme lives, and what a change has to touch

Four holders, per `LIGHT-THEME-MAP.md` §1.3 and re-checked here:
`EditorState::theme`, `Workspace::theme`, `FrameCompositor::theme`, and the
compositor's separate `syntax_theme: HashMap<String, Color>` for the web.
`FrameCompositor::set_theme` (Cs:105-125) derives the fallback keyword bridge
from the theme (`SimpleSyntaxColors::from_theme`, Cs:118-119) and bumps
`theme_generation` (Cs:124), which invalidates retained shaped buffers. The
TUI rebuilds its entire `Palette` from `editor.get_theme()` every frame
(P:63-95), so it needs no plumbing. The desktop overlay holds no theme and
derives all chrome per frame from the `&Theme` argument (O:1233-1306).

⭐ **Confirmed landed since `LIGHT-THEME-MAP.md`: D-8.** The fallback bridge is
no longer selected on `is_dark`; `set_theme` derives it from `theme.syntax`
(Cs:112-121, with the whole defect written up in the comment). The two-palette
divergence that map's Fact 2 describes is fixed.

### 1.4 The two capture→colour paths, side by side

**Native:** tree-sitter capture name → `HighlightType::from_capture_name`
(Cap:84-263: a 60-arm exact match, then 26 `starts_with` prefix arms) →
`HighlightSpan { start, end, highlight: HighlightType }` (Sp:9-16) → stored in
`Lapper<usize, HighlightType>` (It:33) → `highlight_to_color(ht, &SyntaxColors)`
(Sy:18-77) → `Color`.

**Web:** capture name → `WebSpan { start, end, highlight_type: String }`
(Ws:10-17, its doc says "rather than the native `HighlightType` enum") →
`Lapper<usize, String>` (Ws:25) → `color_for_highlight_type` walking the dotted
name up a `HashMap<String, Color>` (W:3138-3153) → falls back to `foreground`.

**These are two different resolution models in one repository.** The web one is
Zed's. The native one is a closed enum.

### 1.5 What renders unstyled today, by name

`CapT:88-124` is a ratchet test asserting every vendored capture maps to
something, with two exception lists:

- `DELIBERATELY_UNSTYLED` (CapT:58-65): `_isinstance`, `_issubclass`, `none`,
  `text`, `text.jsx`, `nested` — predicate operands and prose, correctly
  colourless.
- `KNOWN_UNSTYLED_GAP` (CapT:78-84): **`link_text.markup`, `link_uri.markup`,
  `title.markup`, `selector.class`, `selector.id`, `selector.pseudo`** — the
  test's own comment: *"markdown headings and link text, every CSS selector …
  reach the screen in the plain foreground"*, listed *"because choosing a colour
  for each is a presentation decision."*

⭐ **All six are keys a shipped Zed theme already sets** (§3). The presentation
decision the ratchet is waiting on has been made by someone else, in a file.

### 1.6 Icons — none exist

Repo-wide grep for `icon` across `apps/` and `crates/` `.rs` files returns two
hits, both a doc comment about the macOS bundle icon
(`apps/iridium-desktop/src/lib.rs:64-65`). The explorer's row composer (Fr) has
no icon column; `EntryKind` is `Directory | File | Symlink | Other` (Ex:41-50)
and carries nothing icon-adjacent. The TUI has no explorer. The web face has no
file tree.

---

## 2. What Zed does, at schema level

### 2.0 What I read, and what is second-hand

Two grounds, both real: a **sparse clone of Zed `main` at commit
`38ca9106c5306ef93e52c35643df015a27f15b72`, dated 7 Aug 2026**, at `/tmp/zedsrc`
(contains `crates/{theme, file_icons, extension, extension_host, extension_api,
extension_cli, extensions_ui, paths, project_panel, icons, zed}`); and **cached
verbatim copies** of the files that clone lacks, fetched from
`raw.githubusercontent.com` by the research pass, in
`…/scratchpad/zedtheme/`. I re-read and re-parsed the structs, the shipped
`one.json`, the schema files and the prefix-resolution code in this session; the
key **counts** (146 `ThemeColorsContent` fields, 42 `StatusColorsContent`) come
from an extraction that pass made (`zedtheme/current_keys.txt`) whose key
*names* I read but whose extraction I did not re-run. ⚠️ Treat the counts as
approximately right and the names as verified.

⚠️ **The published schema is stale.** `https://zed.dev/schema/themes/v0.2.0.json`
is the newest that exists — the cached bodies for `v0.3.0` and `v0.4.0` are
227 KB Next.js **404 error pages**, not schemas, while `v0.1.0` and `v0.2.0` are
32 KB and 35 KB of real JSON Schema. Current Zed accepts ~50 keys v0.2.0 does
not describe. **Validating against v0.2.0 would reject valid modern Zed themes.**

### 2.1 The file

Root is `ThemeFamilyContent` — `{ name, author, themes: [ThemeContent] }`, all
three required. `ThemeContent` is `{ name, appearance, style }`, all required,
`appearance` an enum of exactly `"light" | "dark"`. **One file is a theme
*family* containing N themes**; the family is metadata and grouping, not a
fallback parent. Verified against `Z1`: `one.json` is
`{"$schema", "name": "One", "author": "Zed Industries", "themes": [One Dark
(dark, 46 syntax keys), One Light (light, 46 syntax keys)]}`.

`$schema` appears in every shipped theme but **is not a declared field** and is
ignored by the parser — advisory for editors only.

### 2.2 `style`

```rust
pub struct ThemeStyleContent {                                      // Zt:537-556
    #[serde(rename = "background.appearance")]
    pub window_background_appearance: Option<WindowBackgroundContent>,
    #[serde(default)] pub accents: Vec<AccentContent>,
    #[serde(flatten, default)] pub colors: ThemeColorsContent,      // ~146 keys
    #[serde(flatten, default)] pub status: StatusColorsContent,     // 14 × 3
    #[serde(default)] pub players: Vec<PlayerColorContent>,
    #[serde(default)] pub syntax: IndexMap<String, HighlightStyleContent>,
}
```

Two `#[serde(flatten)]` structs: **UI colours and status colours are one flat
dotted namespace in the JSON**, e.g. `"editor.background"`, `"error.border"`.
Status is 14 semantic names (`conflict, created, deleted, error, hidden, hint,
ignored, info, modified, predictive, renamed, success, unreachable, warning`)
× `{base, .background, .border}`.

`players` is an array of `{cursor, background, selection}`; **index 0 is always
the local user**, and collaborator `i` is `self.0[(i % (len-1)) + 1]`. There is
**no multi-cursor colour** — all of a user's carets take player 0.
⚠️ *Reported by the research pass from `crates/theme/src/styles/players.rs`; I
did not re-read that file.*

### 2.3 `syntax` — the part that matters

`IndexMap<String, HighlightStyleContent>` — **an open map. The schema imposes no
vocabulary at all.** A style entry has exactly four fields:

```rust
pub struct HighlightStyleContent {          // Zt:1177-1197
    pub color: Option<ThemeColor>,
    pub background_color: Option<ThemeColor>,   // treat_error_as_none
    pub font_style: Option<FontStyleContent>,   // "normal"|"italic"|"oblique"
    pub font_weight: Option<FontWeightContent>, // number 100–900
}
```

**No underline, no strikethrough, no fade-out is expressible in a theme file.**
Zed's runtime `HighlightStyle` has them; there is no serialized field to set
them from.

Colours are **hex strings only**: `ThemeColor` is a transparent newtype over
`String` with the hand-written schema pattern
`^#([0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$`
(Zt:127) — so `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`, leading `#` required,
**alpha supported**. Shipped themes write the long form with explicit alpha
(`"#5d636fff"`, verified in `Z1`).

### 2.4 Resolution — a base for chrome, none for syntax

**Chrome refines off a built-in.** Per theme, keyed on the declared
`appearance`, Zed starts from `ThemeColors::light()/dark()` (One Light / One
Dark), builds an all-`Option` refinement from the file, and applies it: `Some`
wins, `None` keeps the default. There is a further layer of intra-theme
derivations (`scrollbar.thumb.active_background` → `scrollbar.thumb.background`,
`pane_group.border` → `border`, the six `editor.diff_hunk.*` from
`version_control.added/.deleted` at fixed opacities, and ~a dozen more). An
**unparseable colour string silently vanishes** and the base shows through.
⚠️ *Research-pass reading of `theme_settings.rs:294` and `schema.rs:237`; I did
not re-read those two files this session — the crate is absent from the sparse
clone.*

**Syntax has no base.** `SyntaxTheme` is built from the theme's own map only.
A capture that matches nothing renders in the editor's plain text colour
(`editor.foreground`). An under-specified theme yields unhighlighted text, not
One Dark showing through.

⭐ **Before that, the longest-dot-prefix rule runs** — this is the load-bearing
mechanism and I read it verbatim (Zs:78-93):

```rust
pub fn highlight_id(&self, capture_name: &str) -> Option<u32> {
    self.capture_name_map
        .range::<str, _>((…first segment…, Included(capture_name)))
        .rfind(|(prefix, _)| capture_name.strip_prefix(*prefix)
            .is_some_and(|r| r.is_empty() || r.starts_with('.')))
        .map(|(_, index)| *index as u32)
}
```

`function.method.builtin` tries `function.method.builtin`, then
`function.method`, then `function`. **This is why 46 theme keys cover a hundred-
plus grammar captures.** Note the map is a plain `BTreeMap` — no dependency is
implied by copying this.

⚠️ **Right-to-left multi-capture fallback** (`(type_identifier) @type @variable`
— try the rightmost, walk left until the theme has a style) is documented in
Zed's `docs/src/extensions/languages.md` but the research pass **did not locate
the implementing code**, and neither did I. Docs-only.

### 2.5 Distribution

`extension.toml` at the extension root. Required: `id`, `name`, `version`,
`schema_version`. Asset sections, all `#[serde(default)]` and all independent
(Zm:85-106): `themes: Vec<RelPathBuf>`, `icon_themes: Vec<RelPathBuf>`,
`languages: Vec<RelPathBuf>`, `grammars: BTreeMap<…>`, plus `language_servers`,
`context_servers`, `slash_commands`, `snippets`, `capabilities`,
`debug_adapters`, `debug_locators`, `language_model_providers`, `lib`.

- **A theme extension does not list its themes.** The host scans
  `<ext>/themes/*.json`, deserialises each, reads `themes[].name`, and
  back-fills the manifest. The index is keyed by **theme name**, not by family
  or filename; duplicate names across families silently overwrite.
- **Loose themes need no extension at all**: `~/.config/zed/themes` is scanned
  at startup, created if absent, and watched with a 100 ms debounce.
- **A theme-only extension needs no wasm.** Verified three ways by the research
  pass: the host `continue`s past any extension with no `lib.kind`;
  `lib.kind = Rust` is only set when a `Cargo.toml` exists; and **there is not
  one theme or icon-theme function in the entire WIT world** — themes are
  declarative JSON parsed natively, the sandbox never sees them.
- **No version field inside a theme JSON.** Versioning is at the extension
  layer (`schema_version`, currently `1`, range `0..=1`). Nothing rejects
  unknown or missing keys — no `deny_unknown_fields`, `#[serde(default)]`
  everywhere. Renamed keys are handled by **keeping both forever** plus a log
  line (`scrollbar_thumb.background` → `scrollbar.thumb.background`). That is
  the entire migration mechanism.
- ⚠️ **Registry policy contradicts the data model.** Zed's docs say themes and
  icon themes *"should not be published as part of extensions that provide other
  features."* Nothing in the manifest, the loader or the builder enforces it —
  it is a human review rule for the public registry. **For a drop-in scheme with
  no curated registry, the technical model binds, and it permits the bundle.**

### 2.6 Icon themes

Same family shape (`Zi:10-49`, read verbatim from the clone):

```rust
pub struct IconThemeFamilyContent { name, author, themes: Vec<IconThemeContent> }
pub struct IconThemeContent {
    name, appearance,                                    // required
    directory_icons: DirectoryIconsContent,              // {collapsed, expanded}
    named_directory_icons: HashMap<String, DirectoryIconsContent>,
    chevron_icons: ChevronIconsContent,                  // {collapsed, expanded}
    file_stems: HashMap<String, String>,                 // token → icon key
    file_suffixes: HashMap<String, String>,              // token → icon key
    file_icons: HashMap<String, IconDefinitionContent>,  // icon key → { path }
}
```

Two-hop indirection: a filename token maps to an **icon key**, and the icon key
maps to a **path**. Lookup order, verbatim from `Zf:20-70`: whole file name →
repeated `split_once('.')` remainders → `multiple_extensions()` →
`extension_or_hidden_file_name()` → `extension()` → the literal key `"default"`.
At every step `file_stems` is tried before `file_suffixes` — **so despite the
name, they are one namespace with a priority order**, not stems vs suffixes.

⭐ **Language is not consulted.** There is a live TODO in the source
(`Zf:30-31`): *"Associate a type with the languages and have the file's language
override these associations."* Zed picks icons by filename and extension only.

Icons are **SVG, always** (the research pass found 2624 files in the Catppuccin
icon extension, 100% `.svg`), and paths resolve **relative to the extension
root**, not to the JSON file. An extension's icon theme is *additive* over Zed's
built-in associations — the default theme's `file_stems`, `file_suffixes` and
`named_directory_icons` are cloned and `.extend()`ed.

---

## 3. ⭐ The syntax vocabulary diff

This is the table that decides whether a Zed theme file can be consumed
directly.

### 3.1 Zed's syntax keys → Iridium

**Source of the left column:** the union of every `syntax` key set by the three
shipped Zed themes (One, Ayu, Gruvbox — 47 keys, parsed from the cached JSON
this session) and Zed's documented *"full list of captures supported by
themes"* (43 keys, `docs/src/extensions/languages.md`). Union = **50**.

**Columns 2 and 3** were computed by a faithful transcription of
`HighlightType::from_capture_name` (Cap:84-263) and `highlight_to_color`
(Sy:18-77) into Python and running it over the 50 names. ⚠️ *The transcription
was checked by hand against the Rust, not executed against it — no build was
run in this lane.*

| Zed key | Iridium `HighlightType` | Iridium colour field | what the gap means |
| --- | --- | --- | --- |
| `attribute` | `Attribute` | `attribute` | — |
| `boolean` | `Boolean` | `number` | collapsed onto numbers |
| `comment` | `Comment` | `comment` | — |
| `comment.doc` | `CommentDoc` | `comment` | **doc comments cannot differ from comments** |
| `constant` | `Constant` | `constant` | — |
| `constant.builtin` | `Constant` | `constant` | collapsed |
| `constructor` | `Type` | `type_name` | collapsed onto types |
| `diff.minus` | **none** | — | ⚠️ **unstyled** |
| `diff.plus` | **none** | — | ⚠️ **unstyled** |
| `embedded` | `Embedded` | `variable` | collapsed onto variables |
| `emphasis` | **none** | — | ⚠️ **unstyled — markdown italic** |
| `emphasis.strong` | **none** | — | ⚠️ **unstyled — markdown bold** |
| `enum` | `Constant` | `constant` | collapsed |
| `function` | `Function` | `function` | — |
| `function.builtin` | `Function` | `function` | collapsed |
| `hint` | **none** | — | ⚠️ unstyled (inlay hints — Iridium has none yet) |
| `keyword` | `Keyword` | `keyword` | — |
| `label` | `Property` | `property` | collapsed onto properties |
| `link_text` | **none** | — | ⚠️ **unstyled — in `KNOWN_UNSTYLED_GAP`** |
| `link_uri` | **none** | — | ⚠️ **unstyled — in `KNOWN_UNSTYLED_GAP`** |
| `namespace` | `Type` | `type_name` | collapsed, deliberately (Cap:134-143) |
| `number` | `Number` | `number` | — |
| `operator` | `Operator` | `operator` | — |
| `predictive` | **none** | — | ⚠️ unstyled (AI ghost text — Iridium has none yet) |
| `preproc` | **none** | — | ⚠️ **unstyled — `#include`, `#define`** |
| `primary` | **none** | — | ⚠️ unstyled |
| `property` | `Property` | `property` | — |
| `punctuation` | `PunctuationDelimiter` | `punctuation` | — |
| `punctuation.bracket` | `PunctuationBracket` | `punctuation` | **brackets cannot differ from commas** |
| `punctuation.delimiter` | `PunctuationDelimiter` | `punctuation` | — |
| `punctuation.list_marker` | `PunctuationDelimiter` | `punctuation` | collapsed — markdown bullets |
| `punctuation.markup` | `PunctuationDelimiter` | `punctuation` | collapsed |
| `punctuation.special` | `PunctuationSpecial` | `punctuation` | collapsed |
| `selector` | **none** | — | ⚠️ **unstyled — in `KNOWN_UNSTYLED_GAP`** |
| `selector.pseudo` | **none** | — | ⚠️ **unstyled — in `KNOWN_UNSTYLED_GAP`** |
| `string` | `String` | `string` | — |
| `string.escape` | `StringEscape` | `string` | **escapes cannot differ from strings** |
| `string.regex` | `String` | `string` | collapsed |
| `string.special` | `String` | `string` | collapsed |
| `string.special.symbol` | `String` | `string` | collapsed |
| `tag` | `Tag` | `tag` | — |
| `tag.doctype` | **none** | — | ⚠️ unstyled — no `tag` prefix arm exists, deliberately (Cap:162-170) |
| `text.literal` | **none** | — | ⚠️ **unstyled — markdown inline code** |
| `title` | **none** | — | ⚠️ **unstyled — in `KNOWN_UNSTYLED_GAP`** |
| `type` | `Type` | `type_name` | — |
| `type.builtin` | `TypeBuiltin` | `type_name` | **builtin types cannot differ from types** |
| `variable` | `Variable` | `variable` | — |
| `variable.parameter` | `VariableParameter` | `variable` | **parameters cannot differ from variables** |
| `variable.special` | `VariableSpecial` | `variable` | collapsed |
| `variant` | **none** | — | ⚠️ **unstyled — enum variants** |

**Counts: of Zed's 50 theme keys, 16 have no Iridium equivalent at all, and of
the 34 that do, only 12 land on a colour of their own.** The other 22 collapse
onto a colour some other key already owns. Feed a Zed theme through today's
pipeline and **a third of the author's decisions are discarded and another
sixth are silently merged.**

### 3.2 The reverse — Iridium's own captures, resolved both ways

**107 distinct capture names** appear in
`crates/iridium-lang/src/languages/queries/*/highlights.scm` (extracted this
session, 22 language directories). Resolving each two ways — through Iridium's
table, and through Zed's longest-prefix rule against **One's 46 keys**:

| resolver | resolves | leaves unstyled |
| --- | --- | --- |
| Iridium `from_capture_name` → 14 colours | 78 | **29** |
| Zed One (46 keys) + longest-dot-prefix | 90 | 17 |
| neither | — | 16 |

⭐ **Thirteen of Iridium's own vendored captures are coloured by a stock Zed
theme and by nothing in Iridium:**

| Iridium capture | Zed key that catches it |
| --- | --- |
| `diff.minus`, `diff.plus` | `diff.minus`, `diff.plus` |
| `emphasis.markup` | `emphasis` |
| `emphasis.strong.markup` | `emphasis.strong` |
| `label.regex` | `label` |
| `link_text.markup` | `link_text` |
| `link_uri.markup` | `link_uri` |
| `operator.regex` | `operator` |
| `selector.class`, `selector.id` | `selector` |
| `selector.pseudo` | `selector.pseudo` |
| `text.literal.markup` | `text.literal` |
| `title.markup` | `title` |

**Six of those are exactly `KNOWN_UNSTYLED_GAP`** (CapT:78-84). The other seven
are unstyled today and nothing reports them.

Going the other way, exactly **one** capture Iridium colours and stock One does
not: `lifetime` → `Lifetime` → `type_name` (Sy:63). Since Zed's `syntax` is an
open map, a theme may simply add a `lifetime` key; no schema change is needed.

The 16 neither resolves: `_isinstance`, `_issubclass`, `charset`, `diff.delta`,
`diff.delta.moved`, `import`, `keyframes`, `markup.heading`, `markup.link.url`,
`media`, `nested`, `none`, `strikethrough.markup`, `supports`, `text`,
`text.jsx`. Six of those are the deliberate list; the CSS at-rule captures
(`charset`, `keyframes`, `media`, `supports`, `import`) and
`markup.heading`/`markup.link.url`/`strikethrough.markup` are a genuine gap in
**both** vocabularies.

### 3.3 What the diff means, stated plainly

1. **A Zed theme file cannot be consumed directly by today's pipeline in any
   useful sense.** It parses fine — it is JSON with default-everything — but
   two-thirds of its syntax authorship is destroyed on the way in. Any option
   that says "read Zed themes" without changing the resolution model is
   selling something it does not deliver.
2. **The fix is not more enum variants.** It is the shape: an open dotted map
   with longest-prefix lookup. The prefix rule is what lets 46 keys cover 90
   captures. A closed enum needs a variant per distinction *and* a field per
   variant in every theme, in the TUI palette, and in the VS Code importer's
   scope table — the exact reason Cap:134-143 gives for folding `namespace`
   into `Type`.
3. ⭐ **The deferred Markdown renderer constrains this decision.** Tom has
   separately asked for a GitHub-like rendered Markdown view. The keys such a
   view needs — `emphasis`, `emphasis.strong`, `title`, `link_text`,
   `link_uri`, `text.literal`, `punctuation.list_marker`, `strikethrough` — are
   **eight names Iridium's enum has no slot for and Zed's vocabulary already
   carries seven of**. Under an open map they cost nothing. Under the enum they
   cost eight variants, eight `SyntaxColors` fields, and a migration of every
   theme file. **This is the one place the Markdown ask reaches into this map,
   and it points the same way as everything else.**

### 3.4 What it would cost to open the map

`HighlightSpan` carries `highlight: HighlightType` (Sp:9-16) and the index is
`Lapper<usize, HighlightType>` (It:33). `HighlightType` is `Copy` and one byte.
Storing a `String` per span — what the web face does (Ws:25) — would be a real
memory and allocation regression on the native path.

**The answer is interning.** `HighlightType` becomes `CaptureId(u16)`, an index
into a registry-owned `&'static`-lifetime name table built when the vendored
queries are compiled. It stays `Copy`, `Eq`, `Hash`, one word; the interval
tree, the span index and the retained-shaping cache keep their exact shape and
memory profile. Resolution becomes name-table lookup + longest-prefix walk over
a `BTreeMap<String, Style>` — **no new dependency; `BTreeMap` is what Zed itself
uses** (Zs:79).

**Files that mention `HighlightType` outside tests: 15** (grep, this session):

```
crates/iridium-syntax/src/{lib.rs, highlight/{mod,span,capture,highlighter}.rs}
crates/iridium-editor/src/{lib.rs, syntax.rs, syntax_stubs.rs,
    span_index/{mod,interval_tree}.rs, editor/ast/scope.rs,
    input/keyboard/behaviors.rs}
crates/iridium-tui/src/frame/palette.rs
crates/iridium-bindings/src/web_span_index.rs
apps/iridium-desktop/src/highlight/mod.rs
```

⚠️ **`input/keyboard/behaviors.rs` is in the directory another workflow is
editing right now.** It uses `HighlightType::is_within` (Cap:287-295) for
auto-pair scope suppression. Any slice touching the enum must land *after* that
work, and `is_within` becomes a prefix test on the interned name — which is
strictly more correct, since it would then catch `string.doc` and
`comment.documentation.workflow` by construction rather than by the folding in
`from_capture_name`.

---

## 4. The UI / chrome key diff

Zed's chrome namespace is ~146 `ThemeColorsContent` keys + 42 status keys.
Iridium's is 24. Most of the difference is Zed UI Iridium does not have
(`title_bar.*`, `tab_bar.*`, `debugger.*`, 18 `vim.*`, 24 `terminal.ansi.*`).
The rows that matter:

### 4.1 Iridium's 24 → Zed

| Iridium `EditorColors` field | Zed key | note |
| --- | --- | --- |
| `background` | `editor.background` | exact |
| `foreground` | `editor.foreground` | exact |
| `selection` | *(none)* → `players[0].selection` | ⚠️ **not a chrome key in Zed** — it is player 0's selection, or `element.selection_background` for UI lists |
| `selection_inactive` | **none** | ⚠️ Zed has **no inactive-selection colour** (grep of the key list for `inactive`: only `title_bar.inactive_background`, `tab.inactive_background`) |
| `cursor` | *(none)* → `players[0].cursor` | same as above |
| `line_number` | `editor.line_number` | exact |
| `line_number_active` | `editor.active_line_number` | exact |
| `current_line` | `editor.active_line.background` | exact |
| `gutter` | `editor.gutter.background` | exact |
| `minimap_background` | **none** | Zed themes only the minimap *thumb* (4 keys) |
| `search_match` | `search.match_background` | exact |
| `search_match_current` | `search.active_match_background` | exact; falls back to `search.match_background` in Zed |
| `diff_added_bg` | `editor.diff_hunk.added.background` | derived in Zed from `version_control.added` at fixed opacity |
| `diff_deleted_bg` | `editor.diff_hunk.deleted.background` | same |
| `diff_added_gutter` | `version_control.added` | — |
| `diff_deleted_gutter` | `version_control.deleted` | — |
| `change_added` | `version_control.added` | ⚠️ Zed has **one** key where Iridium has two (`diff_added_gutter` + `change_added`) |
| `change_modified` | `version_control.modified` | — |
| `change_deleted` | `version_control.deleted` | — |
| `blame_foreground` | **none** | nearest is `text.muted`, a UI key not an editor one |
| `diagnostic_error` | status `error` | — |
| `diagnostic_warning` | status `warning` | — |
| `diagnostic_info` | status `info` | — |
| `diagnostic_hint` | status `hint` | — |

**19 of 24 have a direct or one-hop equivalent. Five do not**: `selection` and
`cursor` live under `players` rather than in the chrome namespace;
`selection_inactive`, `minimap_background` and `blame_foreground` have no Zed
key at all. A Zed→Iridium importer must therefore read `players[0]` for two
fields, derive `selection_inactive` from `selection`, and leave two at the base
preset.

### 4.2 Zed keys Iridium has no field for, that it nonetheless *derives*

The desktop overlay computes chrome per frame from the 24 (O:1233-1306):

| Zed key | Iridium derivation today |
| --- | --- |
| `panel.background`, `elevated_surface.background` | `panel_background` = `current_line` over `background`, then `gutter` over that (O:1233-1247) |
| `status_bar.background` / `tab_bar.background` | `strip_background` = the same, *deliberately not* `theme.editor.gutter` (O:1249-1255) |
| `tab.active_background` | `tab_card_color` = `background` made opaque (O:1259-1272) |
| `text` / `text.muted` on the strip | `tab_colors` = `foreground` at three alphas over the band (O:1274-1296) |
| `border`, `border.variant`, `panel.focused_border` | `hairline_color` = `foreground` at `HAIRLINE_ALPHA = 0.18` (O:147) over the panel background — *"a derivation rather than a new theme field, so existing theme files keep deserializing untouched"* (O:1298-1306) |

⭐ **That last one is D-5's `panel_border` gap, stated by the code itself.** The
question is no longer "add one field" — it is "does chrome get a named key set,
and how big". §8, **T-4**.

---

## 5. Options, priced

Every price below is in files touched, dependencies added, and what stays in
sync across the three faces.

### Option A — adopt Zed's theme JSON as Iridium's format

Read `ThemeFamilyContent` directly; a `.json` from the Zed extension registry
works unchanged.

**Cost.** Two parts, and they are very different sizes.

- *A-lite (format only)*: a new `theme/zed.rs` alongside `theme/vscode.rs`
  (460 lines) — a ~200-key `match` over flattened dotted strings, plus
  `players[0]` handling and hex parsing extended to `#RGB`/`#RGBA` (Th:33-54
  currently rejects 3- and 4-digit). ~600–800 lines, **no new dependency**
  (`serde_json` is already in the graph). One new arm in the TUI's parser
  precedence (Tt:148-158) and the same in the desktop's when it gets one.
- *A-full (format + resolution)*: A-lite **plus** §3.4's interning work across
  15 files, plus a chrome refinement base.

**What A-lite buys: almost nothing.** §3.1 — two-thirds of the author's syntax
decisions are destroyed on the way in. A One Dark import would render `comment`
and `comment.doc` identically, brackets and commas identically, parameters and
variables identically, and leave 16 keys on the floor. It is a feature that
demos and does not deliver.

**What A-full buys:** thousands of published Zed themes work; Tom's light-theme
choice (D-1) becomes a runtime file rather than a compile-time preset; the six
`KNOWN_UNSTYLED_GAP` captures get colours for free.

**What A forecloses.** Iridium's `Theme` stops being the theme — the wire format
becomes someone else's, and it moves: current Zed has ~50 keys the published
schema lacks, and the published schema is stale (§2.0). Iridium would be
chasing a format whose owner does not publish its own schema faithfully. It also
imports ~120 chrome keys for UI Iridium does not have and, in the same stroke,
provides **no** key for the five fields §4.1 lists as missing — including
`selection_inactive`, which Iridium paints on every unfocused pane.

### Option B — an Iridium format informed by Zed's

Keep a **named, typed struct** for chrome (24 fields plus the ones §4.2 says the
overlay derives), adopt **Zed's shape wholesale for syntax** (open dotted map,
four style attributes, longest-prefix resolution, hex strings), and add a
declared `appearance` with a refinement base.

**Cost.** The §3.4 interning work across 15 files; `SyntaxColors`'s 14 fields
become `SyntaxStyles(BTreeMap<String, Style>)` with a `Style` of
`{color, background_color, font_style, font_weight}`; `highlight_to_color`
(Sy:18-77) is deleted and replaced by a prefix lookup; the TUI's `Palette`
(P:58) stops cloning `SyntaxColors` and clones the map instead; the web face's
`syntax_theme` (a `HashMap<String, Color>` already, W:3138-3153) **becomes the
same type as the native one and the two-palette split in Fact 3 disappears**.
`theme/vscode.rs`'s scope table keeps working — it already maps TextMate scopes
onto names. **No new dependency.**

**What it buys.** A key set sized to what Iridium actually paints, so a theme
author is not writing 120 keys for chrome that does not exist. Named chrome
keys stay typed, documented and greppable — which is the property CLAUDE.md's
bar wants. Font style and weight per token become expressible for the first
time (today `SyntaxColors` is colour-only, so no theme can italicise a comment).

**What it forecloses.** Direct reuse of published Zed themes — *unless* the
Zed importer of Option A-lite is also written. But under B that importer is
**much smaller**, because the hard half (the syntax map) is now a pass-through:
Zed's `syntax` object deserialises straight into Iridium's, name for name. The
chrome half is the ~20-row table in §4.1, not a 146-row one, because Iridium
ignores keys for UI it lacks — which is exactly what Zed does with unknown keys.

### Option C — keep themes in Rust, add a switch

Ship `Theme::light()` properly (D-1), add `view.toggleTheme` (D-3) and a
desktop `--theme` flag (D-7).

**Cost:** near zero — D-3 is one command id and a keymap row; D-7 reuses
`ThemeChoice` verbatim (Tt:41-49), which also hands the desktop the existing
JSON and VS Code loaders for free.

**What it buys:** the switch, which is genuinely missing today.

**What it forecloses:** nothing — but it does not answer the ask. "Without
having to hard code stuff in" is precisely the thing C does not do.

### The recommendation

**B, with A-lite bolted on afterwards as a third parser, and C's D-3/D-7 as its
prerequisites.** Concretely:

1. Adopt Zed's **resolution model** for syntax — this is the load-bearing part
   and is what makes any format survive the Markdown renderer and the next
   grammar refresh.
2. Keep chrome a **named struct** with a refinement base keyed on a declared
   `appearance`, and grow it by the handful of keys §4.2 shows the overlay is
   already deriving.
3. Add `Theme::from_zed_json` as a third arm beside `from_json` and
   `from_vscode_json`, so a Zed theme file is a supported *import*, not the
   native format. The precedence rule and both-complaints-reported behaviour
   are already written and reasoned about at Tt:13-31; this is a third entry in
   the same list.

That gets Tom Zed themes working, a native format sized to Iridium, and it
deletes the web/native palette split rather than institutionalising it.

---

## 6. The three-face problem

Under the recommendation:

| face | how it follows | new plumbing |
| --- | --- | --- |
| **Terminal** | `Palette::from_theme` runs every frame from `editor.get_theme()` (P:63-95, called at `frame/mod.rs:176`, `frame/status.rs:132,230`, `frame/highlight.rs:219`, `app/view.rs:50,66`). It clones `theme.syntax` (P:58) — after B it clones the map instead. `Palette::highlighted` (P:185-191) calls the kernel resolver rather than owning one, deliberately (*"a face with its own copy would drift … the first time a highlight type was added"*, P:182-184) | **None.** It follows for free |
| **Desktop** | Four steps, not three: `editor`/`workspace.set_theme` → `compositor.set_theme` (Cs:105-125, which bumps `theme_generation` and re-derives the bridge) → `window.request_redraw()` → ⭐ **`HighlightCache`'s generation must move too** — its own doc says so: *"if runtime theme switching ever lands, the switch must move this generation too, or retained frames keep the old palette"* (`apps/iridium-desktop/src/highlight/cache.rs:100-106`) | The four-step sequence, plus D-3's command and D-7's flag |
| **Web** | ⭐ **This is where the recommendation pays.** Today the web resolves string captures against a `HashMap<String, Color>` fed only from JS (W:1966-1992, W:3138-3153) — a second, structurally different palette that **nothing populates**, so every tree-sitter span on the web renders in plain `foreground`. Under B the native table *is* that shape, so `Theme` can populate it directly and `setSyntaxTheme` becomes an override rather than the only source | The wasm binding's `setDarkTheme` (W:1878-1881) must also move `self.editor`'s theme, which today it never touches (grep: `self.editor`'s theme is never read in `wasm.rs`) |

**No face is left behind, and the web face is fixed rather than worked around.**

Two honest limits:

- ⚠️ **The terminal cannot honour font style or weight from a theme in general.**
  Bold and italic are terminal attributes, not colours; `iridium-tui`'s `Cell`
  carries them, but at `ColorDepth::Ansi16` the whole palette degrades
  (`crates/iridium-tui/src/cell/color.rs:80-160`). That is a tier limit, not a
  design failure — it is the same note D-6 records.
- ⚠️ **`Typography::font_family` is read by nobody.** `TextRenderer::apply_typography`
  (`render/text.rs:906`) has zero callers and both GPU faces embed
  `JetBrainsMono-Regular.ttf`. A theme that names a font today changes nothing.
  Reported by the research pass; I did not re-grep the caller set.

---

## 7. Icon themes

### 7.1 What an Iridium icon theme would have to address

Given what the explorer draws (Fr:51-73) and what `EntryKind` knows (Ex:41-50),
the *minimum* addressable surface is smaller than Zed's:

| surface | Iridium today | an icon theme would supply |
| --- | --- | --- |
| disclosure | `"▾ "` / `"▸ "` literals (Fr:52-56) | Zed's `chevron_icons.{collapsed,expanded}` |
| directory | trailing `/` on the name (Fr:59-61) | `directory_icons.{collapsed,expanded}` |
| named directory | nothing | `named_directory_icons["src"]` etc. |
| file by name/extension | nothing — colour only (Fr:66-73) | `file_stems` + `file_suffixes` → `file_icons` |
| loading | `"…"` appended (Fr:62-64) | no Zed equivalent |
| error | text in `line_number` colour (Fr:93-101) | no Zed equivalent |

Two structural notes for whoever builds it:

- Zed's `file_stems`/`file_suffixes` split is **not** stems vs suffixes — both
  maps are queried with the same token at every step, `file_stems` first
  (`Zf:23-29`). Copy the priority, not the names.
- Zed picks icons **by filename only, never by language** (`Zf:30-31` is a live
  TODO). Iridium *has* a language registry (#88) and could do better — but that
  is a design choice to make deliberately, not a gap to fill by accident.

### 7.2 Can the pipeline draw one? — three separate answers

1. **Desktop (GPU): yes for rasters, today, with no new pipeline.** glyphon 0.10
   exposes `TextArea.custom_glyphs` (`Gl:lib.rs:124`) and
   `prepare_with_custom(…, rasterize_custom_glyph)` (`Gl:text_render.rs:99-109`),
   taking `RasterizedCustomGlyph { data: Vec<u8>, content_type }` where
   `ContentType` is `Mask` (tintable, `CustomGlyph.color`) or `Color`
   (`Gl:custom_glyph.rs:8-30, 58-64`). Glyphs are cached in the existing atlas
   by `(id, width, height, subpixel bin)`. Iridium passes `custom_glyphs: &[]`
   (Tx:968) and calls plain `prepare` (`compositor/frame.rs:361`). **The seam is
   two call-site changes wide.**
2. ⚠️ **Nothing in the workspace can PARSE an SVG.** No `usvg`, no `resvg`, no
   `lyon` — checked against the resolved graph, not the manifests.

   🔴 **CORRECTION, made at the overseeing seat after the draft was written.**
   The draft asserted that `resvg`, `usvg`, `tiny-skia`, `image` and `lyon` were
   absent "anywhere", and that the only raster crate was a dev-only `png`. Two
   of those five are wrong, and the method is why: **it grepped the root
   manifest and the two face manifests, which lists what is *declared*, not what
   is *linked*.** A transitive dependency is invisible to that check. Measured
   with `cargo tree -i`:

   | crate | actually present? | how it arrives |
   | --- | --- | --- |
   | `image` v0.25.10 | ⚠️ **yes, and not dev-only** | `image ← arboard 3.6.1 ← iridium-desktop` — the clipboard crate |
   | `tiny-skia` v0.11.4 | ⚠️ **yes, on Linux targets** | `tiny-skia ← sctk-adwaita 0.10.1 ← winit 0.30.13 ← iridium-desktop` — Wayland client-side decorations. Absent on macOS; `cargo tree -i tiny-skia` finds nothing on the host and needs `--target all` to see it. |
   | `usvg`, `resvg`, `lyon` | no | — |

   **The conclusion survives and the price changes.** Neither `image` nor
   `tiny-skia` can parse SVG — that is `usvg`'s job — so a Zed-compatible icon
   set still needs a new dependency. But the desktop face already links a raster
   decoder today, and on Linux it already links *the rendering half of resvg*,
   so the marginal cost is `usvg` + `resvg` rather than three crates from
   nothing. **It is still a dependency decision, and still a bigger one than any
   theme decision in this map** — it is simply a smaller number than the draft
   claimed.

   ⭐ **The lesson is Rule M's cousin and it is worth carrying:** a manifest
   grep answers *"what did we ask for"*, not *"what is in the binary"*. Every
   dependency claim in a design document must come from the resolved graph.
3. **Terminal: rasters are impossible; glyphs are free.** The TUI draws cells.
   An icon there is a code point in the font — which is exactly what `▾`/`▸`
   already are (Fr:52-56). A Nerd-Font-style code-point mapping works on all
   three faces and needs **no dependency at all**.
4. **Web: no explorer exists**, so the question does not arise yet.

⭐ **The honest finding: an Iridium icon theme has two tiers, and only one of
them is cheap.** A *glyph* icon theme — `extension → token → code point +
colour` — costs one JSON schema, a lookup in `entry_row`, and nothing else; it
works on desktop and terminal identically. A *raster* icon theme costs an SVG
rasterizer, an atlas budget, HiDPI re-rasterization on scale change, and it
cannot follow to the terminal. They are not the same feature and should not be
one decision.

⚠️ **Cross-link worth naming once:** the deferred **mermaid** requirement in the
Markdown-renderer ask needs vector drawing too. If the raster tier is ever
taken, it and mermaid should be priced together — they want the same
dependency. Not designed here.

---

## 8. Decisions for Tom — T-1 to T-5

### Relationship to `LIGHT-THEME-MAP.md`'s D-1..D-8

- **D-8 has landed** (verified at Cs:112-121). Closed.
- **D-5 (`panel_border` as a 25th `EditorColors` field) is SUPERSEDED by T-4.**
  The question is no longer whether to add one field; it is what the chrome key
  set is once themes come from files. Do not rule D-5 separately.
- **D-1, D-2, D-3, D-4, D-4b, D-6, D-7 all stand, untouched.** This map does not
  restate them and does not re-recommend them. Two notes, offered without
  re-ruling:
  - **D-1** gains a cheaper shape under this map: if themes load from files, all
    three classic variants ship as files and Tom picks at runtime rather than
    one of them being compiled in as `Theme::light()`.
  - **D-3 and D-7 are prerequisites of everything here.** A loading mechanism
    with no way to select a theme is not a feature. They are small; they should
    land first regardless of how T-1..T-5 go.

---

**T-1 — Does the syntax vocabulary become an open dotted map with longest-prefix
resolution?** *This is the ruling that decides every other one.*

**Recommendation: yes.** Three independent reasons, each measured above:
(a) 29 of Iridium's own 107 vendored captures render with no colour today, and a
stock Zed theme colours 13 of them including the whole `KNOWN_UNSTYLED_GAP`
ratchet; (b) the Markdown renderer needs eight names the enum has no slot for,
and each would otherwise cost a variant *plus* a field in every theme, the TUI
palette and the VS Code importer; (c) the web face already resolves this way
(W:3138-3153), so the enum is not "the" model — it is one of two, and the other
one is Zed's. Interning to `CaptureId(u16)` keeps `Copy`, keeps the interval
tree's memory, and adds **no dependency** (§3.4).
**Price: 15 non-test files, one of which — `input/keyboard/behaviors.rs` — is
being edited by another workflow right now, so this slice queues behind it.**

**T-2 — Which format is native: Zed's, or Iridium's own with a Zed importer?**

**Recommendation: Iridium's own, with a Zed importer.** Chrome stays a named
typed struct because Iridium paints 24 things and Zed names 146, five of the 24
have no Zed key at all (§4.1), and a typed struct is greppable and documented in
a way an untyped bag is not. Syntax adopts Zed's shape verbatim so the importer's
hard half becomes a pass-through. The importer is then a ~20-row chrome table
plus `players[0]`, not a 146-row one. ⚠️ Do **not** validate imports against the
published `v0.2.0` schema — it is stale by ~50 keys and would reject valid
modern Zed themes (§2.0).

**T-3 — Do themes ride the #88 extension directory, get their own `themes/`
directory, or both?**

**Recommendation: both, exactly as Zed does** — a plain `themes/` directory
scanned at startup for loose files, *and* a `themes/` subdirectory inside any
drop-in extension. Zed proves the shape works and its index is keyed by **theme
name**, not file or family (§2.5).
⭐ **The reason this is cheap and worth doing early: a theme needs no wasm.**
Zed's WIT world has no theme function at all; theme-only extensions have no
`lib.kind` and no `extension.wasm`. So **themes can land on the extension
mechanism at tier 1, before wasmtime and before the 6.6 MiB** — they are a
directory scan and a JSON parse. The theme half of "bundle up themes and icons
and all that kind of stuff" is available *now*; the language half is #88's
tier-2 work.
⚠️ Note against Zed's own registry policy, which forbids shipping a theme in the
same extension as a language: that is a **human review rule for a curated
marketplace**, enforced nowhere in Zed's code. Iridium has no curated registry,
so the technical model binds and the bundle is permitted. Say so out loud rather
than inheriting a rule whose reason does not apply.

**T-4 — Does chrome get a refinement base, and how many keys?** *(supersedes
D-5)*

**Recommendation: yes to the base, and grow the key set by the five the overlay
is already deriving.** Today a light theme file that omits a field inherits a
**dark** value, because both `Default` impls are the dark preset (Th:194-200,
Th:289-295) — a partial light theme is currently broken by construction. Zed's
answer is the right one: declare `appearance`, refine off the built-in of that
appearance, and let an unparseable colour fall through to the base rather than
failing the file.
The five keys to add are the ones O:1233-1306 derives by hand — a panel
background, a strip background, a border/hairline, a tab-card colour, and an
elevated-surface colour — with the current derivations kept as their **defaults**
so every existing theme file keeps deserialising unchanged, which is the exact
property O:1298-1306 says the derivation was chosen to preserve. That absorbs
D-5's `panel_border` and answers it in a way that does not need re-ruling per
variant.

**T-5 — What is an Iridium icon theme, and when?**

**Recommendation: rule the *tier* now, build it after T-1..T-4.** Take the
**glyph tier** — an icon theme maps a filename token to a **code point plus a
colour**, resolved by Zed's token order (whole name → dotted remainders →
extension → `default`). It needs no new dependency, it drops straight into
`entry_row` (Fr:51-73), and it works identically on desktop and terminal.
**Defer the raster tier as a separate, named decision**, because it costs an SVG
rasterizer (three crates), an atlas budget and HiDPI re-rasterization, and it
cannot follow to the terminal at all. Record the finding that makes it *possible*
whenever it is wanted: **glyphon already carries the custom-glyph path and
Iridium passes it empty** (Tx:968, `Gl:text_render.rs:99-109`) — no new render
pipeline is needed, only a rasterizer.
⚠️ If Tom wants Zed's *icon themes* to work as-is, the raster tier is not
optional — Zed's icon assets are 100% SVG.

---

## 9. Build order

Each slice lands something usable on its own and leaves the tree green.

**S-0 (prerequisite, not this map's) — the switch.** D-3's `view.toggleTheme`
and D-7's desktop `--theme`, reusing `ThemeChoice` verbatim (Tt:41-49). Without
this, nothing below is reachable by a user. Small; it should not wait.

**S-1 — Intern the capture vocabulary.** `HighlightType` → `CaptureId(u16)`
over a registry name table; `from_capture_name` becomes interning;
`is_within` becomes a prefix test on the interned name. 15 files. **Queues
behind the in-flight `input/keyboard/` work.** Lands: nothing visible, and the
existing ratchet tests (CapT:88-155) must still pass — including
`the_unstyled_lists_name_only_captures_that_are_still_unstyled_and_still_used`,
which will now *fail on the six gap captures* the moment a theme colours them,
and that is the signal that S-2 worked.

**S-2 — Open the syntax table.** `SyntaxColors`'s 14 fields →
`SyntaxStyles(BTreeMap<String, Style>)` with `{color, background_color,
font_style, font_weight}`; delete `highlight_to_color` (Sy:18-77) in favour of
longest-prefix lookup; the TUI's `Palette` and the web's `syntax_theme` become
the same type. Lands: **the six `KNOWN_UNSTYLED_GAP` captures and the seven
others in §3.2 get colours**, italic comments become expressible, and the
web/native palette split (Fact 3) is deleted.

**S-3 — Chrome base and the five derived keys.** Declared `appearance`,
refinement off the matching built-in, the five keys of T-4 defaulting to today's
overlay derivations. Lands: partial theme files stop inheriting dark values into
light themes; D-5 is answered.

**S-4 — Loose `themes/` directory.** Scan a themes directory at startup, load
every file, key by theme name; failure rules reused verbatim from
`iridium-config` (#59) — *a bad theme never stops the editor, a mistake costs
its own line, a refusal is reported, no themes is not a problem*
(`docs/IN-FLIGHT-languages.md:262-267`). Lands: **the ask, minimally** — a theme
is a file. All three classic variants (`theme/classic.rs`) ship as files, which
makes D-1 a runtime choice.

**S-5 — `Theme::from_zed_json`.** Third parser arm; §4.1's ~20-row chrome table
plus `players[0]`; hex parsing extended to `#RGB`/`#RGBA` (Th:33-54). Lands:
published Zed themes work.

**S-6 — Themes inside a drop-in extension.** A `themes/` subdirectory in an
extension directory, indexed the same way. **Requires no wasm and can precede
#88's tier-2 work entirely.** Lands: the *bundle* half of the ask.

**S-7 — Glyph icon themes** (T-5). Schema, resolution order, `entry_row`
integration, terminal parity. Raster tier deferred behind its own decision.

---

## 10. What was not checked

1. **Nothing was built, tested or linted.** Read-only lane, concurrent edits in
   `crates/iridium-editor/src/input/keyboard/`. I did not verify the tree
   compiles. The Python transcription of `from_capture_name` and
   `highlight_to_color` used to compute §3.1 and §3.2 was hand-checked against
   the Rust, **not executed against it**; a transcription error would move
   individual rows, though not the shape of the result.
2. **The 107-capture extraction is regex-based**, over the `.scm` text
   (`grep -ohE '@[a-zA-Z_][a-zA-Z0-9_.]*'`), not over compiled queries. Iridium's
   own test does it the careful way (`query.capture_names()`, CapT:98-108)
   *"so predicate arguments and comments cannot be mistaken for captures"* — my
   list may therefore include a name that only appears in a comment or a
   predicate. `_isinstance`, `_issubclass`, `none` and `nested` are in it, which
   is consistent with the compiled list, so I believe it is close; I did not
   prove it identical.
3. **Zed's chrome key counts (146 + 42)** come from an extraction the research
   pass made from `crates/settings_content/src/theme.rs`; I read the resulting
   key **names** and the `#[serde(flatten)]` struct definitions, but did not
   re-run the extraction. The names in §4 are verified; the totals are not.
4. **Zed's chrome refinement chain** (`refine_theme`, `theme_colors_refinement`,
   the diff-hunk opacity constants, the silent-drop of unparseable colours) is
   reported from the research pass. Those two files are **absent from the sparse
   clone** at `/tmp/zedsrc` and I did not re-fetch them. §2.4's *mechanism* is
   second-hand; §2.4's *prefix rule* I read verbatim (Zs:78-93).
5. **Zed's player-colour semantics** (index 0 = local, `(i % (len-1)) + 1` for
   collaborators, positional per-field merge) are second-hand for the same
   reason. This matters to §4.1, where two of Iridium's 24 fields map into
   `players[0]`.
6. **Right-to-left multi-capture fallback** — documented by Zed, code never
   located, by either reader. If it is real, it changes what a `@type @variable`
   pattern means to an importer. **Not confirmed.**
7. **Whether `setSyntaxTheme` is called by a consumer outside this repository.**
   Zero call sites in-repo, verified; a downstream embedder of `@iridium/core`
   could call it, and the checked-in `dist/` bundles were not decompiled.
8. **`LIGHT-THEME-MAP.md`'s own line citations were not re-verified**, beyond
   the three §1.3 facts I re-checked (D-8 landed, the four theme holders, the
   overlay derivations). That map's `C:`, `A:` and `Hl:` legend entries point at
   files that are now **directories** (`render/compositor.rs`,
   `apps/iridium-desktop/src/app.rs`, `apps/iridium-desktop/src/highlight.rs`),
   so its line numbers for those three are stale.
9. **glyphon's custom-glyph path was read, not exercised.** I read the public
   API and the atlas cache key; I did not write a probe that puts a raster in
   the atlas. The claim "no new pipeline is needed" follows from the API
   surface, not from a running frame.
10. **`Typography::font_family` having zero readers** is reported from the
    research pass; I did not re-grep `apply_typography`'s call sites.
11. **Zed 1.14.2 (the local install) vs `main` at `38ca9106`.** Everything about
    Zed here describes `main` as of 7 Aug 2026. Where the shipped 1.14.2 differs
    — and it demonstrably does in at least one place, since installed manifests
    carry `[agent_servers]` sections that no longer exist in the struct — this
    map follows `main`.
