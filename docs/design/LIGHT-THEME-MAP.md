# Light Theme Map — the switch, and a classic-Mac light face

> ## ⭐ RULED 9 Aug 2026 — D-1..D-8 all decided, build may start
>
> Tom, 9 Aug: *"in terms of the two decisions, just make them, just make
> them right. I'm sick of being asked about them. I just want you to make
> them. And get on with it."* So they are made here, at this seat, and
> they are not to be re-raised with him. Every one takes §5's
> recommendation; the reasoning is in §5 and is not repeated.
>
> | | ruling |
> |---|---|
> | **D-1** | **Variant A "Platinum"** ships as `Theme::light()`. B "Paper" and C "Monochrome" ship as JSON files under `themes/`, which costs one file each and nothing in the binary. |
> | **D-2** | Follow the system appearance **and** allow a manual pin; a manual toggle stops following until the window closes. **Do not call `Window::set_theme`** — it permanently silences `ThemeChanged`. |
> | **D-3** | A kernel **host** command `view.toggleTheme`, `Ctrl+Alt+T` in the kernel keymap and `⌘⌥T` in the desktop ⌘ layer. One toggle, not a light/dark pair. |
> | **D-4** | Light inherits all ruled geometry and takes its own value for exactly three black-ink constants: backdrop dim `0.45 → 0.16`, shadow `16/48/0.55 → 3/2/0.34`, hairline `0.18 → 0.55` at 1 **logical** px. |
> | **D-4b** | **Radius 8 in both modes.** One chrome geometry across light and dark. Rounded corners with genuine arcs are Tom's standing rule and were never in question; this only settles the number. |
> | **D-5** | `panel_border` becomes a real 25th `EditorColors` field. Backdrop dim and shadow stay face constants keyed on `is_dark`. |
> | **D-6** | The TUI follows, with no special deference, and the two `Ansi16` caveats recorded rather than designed around. |
> | **D-7** | Add `--theme` to the desktop face, reusing the TUI's `ThemeChoice`. **No config file** in this change — the estate's first preferences store is not something a light theme should smuggle in. |
> | **D-8** | Fix the two-palette divergence **in this change**. Without it a designed light theme sits on an undesigned fallback, which is exactly the half-landed work this repo's standards forbid. |
>
> ⚠️ **§4.1's render-and-pick step is now a verification artefact, not a
> decision gate.** It was designed to let Tom choose from pixels; he has
> delegated the choice, so the shots are taken *after* A is built, to
> prove what was built, not to select it. Do not block the build on them.

Produced 5 Aug 2026 from a full read of the theme module, both syntax
palettes, the compositor's theme plumbing, the desktop overlay's derived
chrome, the terminal face's palette resolver, the command/keymap
machinery and winit 0.30.13's own sources. Read-only against a live
build in the same tree; **no build was run for it** (zero-build lane).

The ask, in Tom's words (5 Aug): *"Could it be great to be able to swap
between the two of them like dark and light mode and I kind of would
like a classic like light mode like you know old-school Mac OS. Like my
own type of styling."*

Two things, then: **(1)** a real light/dark switch — a key, a command, a
repaint that reaches every surface; **(2)** a light theme whose character
is System 6 → Platinum-era Mac OS, as *his* styling, not a generic
inverted dark theme.

Citation legend: `Th:` = `crates/iridium-editor/src/theme/colors.rs`,
`Tm:` = `crates/iridium-editor/src/theme/mod.rs`, `Tf:` =
`crates/iridium-editor/src/theme/fonts.rs`, `C:` =
`crates/iridium-editor/src/render/compositor.rs`, `Sh:` =
`crates/iridium-editor/src/render/simple_highlight.rs`, `E:` =
`crates/iridium-editor/src/editor/core.rs`, `O:` =
`apps/iridium-desktop/src/overlay.rs`, `A:` =
`apps/iridium-desktop/src/app.rs`, `R:` =
`apps/iridium-desktop/src/run.rs`, `Dc:` =
`apps/iridium-desktop/src/commands.rs`, `Hl:` =
`apps/iridium-desktop/src/highlight.rs`, `Sc:` =
`apps/iridium-desktop/tests/chrome_screenshots.rs`, `P:` =
`crates/iridium-tui/src/frame/palette.rs`, `Cd:` =
`crates/iridium-tui/src/cell/color.rs`, `H:` =
`crates/iridium-editor/src/commands/builtin/host.rs`, `Ka:` =
`crates/iridium-editor/src/commands/default_keymap.rs`, `K:` =
`crates/iridium-editor/src/commands/default_keymap_tests.rs`, `Ac:` =
`crates/iridium-editor/src/editor/command_api_tests.rs`, `Rt:` =
`crates/iridium-editor/tests/retained_shaping/` (`theme.rs` for the palette
rows), `Tt:` =
`apps/iridium/src/theme.rs`, `Tc:` = `apps/iridium/src/cli.rs`, `Wb:` =
`examples/web/src/App.tsx`, `W:` = vendored
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/winit-0.30.13/`.

Prior art and controlling rulings: `docs/design/DESKTOP-CHROME-MAP.md`
(the chrome ruled for dark, 4 Aug — its derivation rules bind this map),
`docs/design/CONTEXT-MENU-MAP.md` (a **concurrent** change in
`apps/iridium-desktop/src/{app,mouse,commands,surface}.rs`; §6 states how
this one stays behind it), `docs/DESKTOP-SHELL-PLAN.md` (v1 ships
`--theme` flag parity only — a claim §1.6 corrects).

---

## The three load-bearing facts, up front

1. **`Theme::light()` is not a theme, it is a transcription.**
   `EditorColors::light()` (Th:164-191) and `SyntaxColors::light()`
   (Th:264-281) are VS Code's *Light+* palette written out field by
   field — `keyword #0000ff`, `string #a31515`, `comment #008000`,
   `function #795e26`, `type #267f99`, `variable #001080`, `constant
   #0070c1`. It has never been designed, only filled in, and it carries
   at least two outright defects: **`attribute` and `error` are the same
   colour** (`#ff0000`, Th:278-279) so two token classes are
   indistinguishable, and `operator`/`punctuation` are `#000000`
   (Th:273-274) while plain `foreground` is `#333333` (Th:167) — commas
   are painted *darker* than the code they separate. There is nothing
   here to preserve. It is a blank sheet with a placeholder on it.

2. **Two different light code palettes ship in the same binary, and
   which one you see depends on whether a tree-sitter span arrived.**
   `theme::SyntaxColors` (Th:209-238) is what the desktop's resolver
   paints tree-sitter spans with (`HighlightCache::resolver(&theme.syntax)`,
   A:1144-1145, Hl:155). But the compositor's *fallback keyword bridge*
   has its **own type of the same name**,
   `render::simple_highlight::SyntaxColors` (Sh:41-58), whose `light()`
   (Sh:79-88) is an unrelated Nord/Solarized hybrid — `#5E81AC` keyword,
   `#689D6A` string, `#B16286` number, `#B58900` type, `#268BD2`
   function. `FrameCompositor::set_theme` selects between the bridge's
   two presets **on `theme.is_dark` alone** (C:2016-2028) and never
   consults `theme.syntax`. So a document with no grammar — or a
   grammar'd document before its spans land — is highlighted in a
   palette the theme does not contain and cannot influence. Any light
   theme that is not fixed here is honest on `.rs` and wrong on `.txt`.

3. **The switch is three lines of plumbing and one missing command.**
   Every consumer already accepts a theme at runtime: `Editor::set_theme`
   exists and emits `ThemeChanged` (E:1205-1214); `FrameCompositor::set_theme`
   landed 4 Aug with the retained-shaping generation bump (C:2016-2028,
   commit 6e22cbe); the terminal face rebuilds its whole `Palette` from
   `editor.get_theme()` **every frame** (P:63, called at
   `frame/mod.rs:175`, `frame/status.rs:132`, `frame/highlight.rs:219`),
   so the TUI needs no plumbing at all; the web face already has a
   working toggle button wired to `setDarkTheme` (Wb:174-180,
   `Iridium.tsx:206-210`, `wasm.rs:1637-1640`). What is missing is
   (a) a command id — **no theme verb exists anywhere in the registry**
   — and (b) the two-call sequence on the desktop face, because the
   compositor holds its *own* clone of the theme, set once at
   `Shell::open` (A:188) and never again.

---

## 1. Verified ground

### 1.1 The field inventory a theme must state

`Theme` (Tm:68-80) is five fields: `name`, `is_dark`, `editor`, `syntax`,
`typography`. `is_dark` is not decorative — it is what the compositor
keys the fallback bridge on (C:2019-2023) and what
`Editor::is_dark_theme` reports (E:1219-1220).

**`EditorColors` — 24 fields, all required** (Th:76-125). Deserialization
is forgiving (`#[serde(default)]` at Th:75, per-field fallback to the
*dark* preset via `Default` at Th:194-200), so a light theme that omits a
field silently inherits a **dark** value. A serious light preset states
all 24:

| # | Field | What paints it |
|---|---|---|
| 1 | `background` | the compositor's **clear colour, verbatim** (C:875-882) |
| 2 | `foreground` | document text; the overlay's panel text base (O:877); the hairline's source (O:962-970) |
| 3 | `selection` | selection quads; the overlay's selected-row band (O:851-859) |
| 4 | `selection_inactive` | unfocused selection |
| 5 | `cursor` | caret quad; the overlay's field caret (O:867); the TUI's caret cell background (P:81) |
| 6 | `line_number` | gutter digits; the TUI's `overlay_quiet` style (P:85-86) |
| 7 | `line_number_active` | the cursor line's digit; the TUI's `overlay_toggle_active` (P:87-89) |
| 8 | `current_line` | the current-line band **and, by derivation, every panel and the strip** (O:937-955) |
| 9 | `gutter` | the gutter band; the TUI statusline's band (P:`status_style`) |
| 10 | `minimap_background` | the minimap |
| 11 | `search_match` | non-current matches |
| 12 | `search_match_current` | the current match, at full alpha per-cluster in the palette (`command_palette.rs`) |
| 13-16 | `diff_added_bg`, `diff_deleted_bg`, `diff_added_gutter`, `diff_deleted_gutter` | diff view |
| 17-19 | `change_added`, `change_modified`, `change_deleted` | gutter change bars |
| 20 | `blame_foreground` | inline blame ghost text |
| 21-24 | `diagnostic_error`, `diagnostic_warning`, `diagnostic_info`, `diagnostic_hint` | gutter markers; `diagnostic_error` is also the TUI's overlay error style (P:90-91) |

**`SyntaxColors` — 14 fields** (Th:209-238): `keyword`, `string`,
`number`, `comment`, `function`, `variable`, `type_name`, `operator`,
`punctuation`, `property`, `constant`, `tag`, `attribute`, `error`.

**`Typography` — 4 fields** (Tf:7-16): `font_family`, `font_size`,
`line_height`, `letter_spacing`. Note the honest limit: both GPU faces
**embed one font** (`JetBrainsMono-Regular.ttf`, A:110, Sc:47) and never
read `typography.font_family`. Era typography is therefore *not*
available to this change — see §2.1.

### 1.2 What `Theme::light()` currently is

Fact 1 above, with the receipts: Th:164-191 and Th:264-281. `Theme::light()`
names it `"Iridium Light"` and sets `is_dark: false` (Tm:103-111). It is
reachable today from: the TUI's `--theme light` (Tc:162-169, Tt:56-58),
`ThemeBuilder::light()` (Tm:309-314), `FrameCompositor::set_dark_theme(false)`
(C:2009-2011), the web toggle (Wb:174-180), and **nothing on the desktop
face** — `iridium-desktop` parses *at most one argument, the file path*,
and has no flags at all (R:91-99). The desktop shell therefore always
opens on `Editor::with_defaults()`'s theme, which is `Theme::dark()`
(Tm:82-86, A:296). `DESKTOP-SHELL-PLAN.md`'s "`--theme` flag parity
only" line describes an intent that was never implemented.

The current light preset is exercised by real tests, which are the
regression fence any replacement must clear (§4).

### 1.3 The switch mechanics — three theme holders, not one

| Holder | Set by | Read for |
|---|---|---|
| `EditorState::theme` | `Editor::set_theme` (E:1205-1214), `use_light_theme`/`use_dark_theme` (E:1224-1231) | the desktop's syntax resolver (A:1144-1145), every overlay builder (A:1215-1224), the TUI's whole `Palette` (P:63) |
| `FrameCompositor::theme` | `FrameCompositor::set_theme` (C:2016-2028) / `set_dark_theme` (C:2009-2011) | the clear colour (C:875-882), gutter/selection/cursor quads, the retained shaped buffers' text colours |
| `OverlayPainter` | *nothing* — it takes `&Theme` per `paint` call (O:610-613) | all panel chrome, derived per frame |

The overlay is already correct by construction: it holds no theme. The
editor and the compositor hold independent copies, and **they can
disagree** — which is exactly the bug 6e22cbe fixed in the other
direction (the compositor sat frozen on `Theme::dark()` with no setter).
`Shell::open` documents the invariant in prose (A:165-167): *"the
compositor must never sit frozen on a preset the editor does not hold."*
A runtime toggle must therefore be **two calls in one place**:

```
editor.set_theme(next);                 // E:1205 — emits ThemeChanged
shell.compositor.set_theme(next);       // C:2016 — bumps theme_generation
shell.window.request_redraw();
```

`set_theme`'s generation bump (C:2024-2027) is what invalidates the
retained shaped buffers, and it is already pinned by a test
(`a_set_theme_misses_and_recomposes_identically`, Rt:788-820) that
asserts a warmed compositor after `set_theme` recomposes **byte-identical**
to a fresh one. The switch inherits that proof for free.

`Editor::set_theme` emits `EditorEvent::ThemeChanged { theme_name,
is_dark }` (E:1210-1213, variant at E:59-67). No face subscribes to it
today; it is available if a face would rather observe than be told.

### 1.4 The command surface — there is no theme verb

`grep` over `crates/iridium-editor/src/commands/builtin/ids.rs`: **zero**
matches for `theme`, and no `view.*` id of any kind. The registry is 99
built-in ids plus 2 host ids.

The right shape is a **host command**, and the module that defines them
argues the case itself (H:1-30): the kernel owns the *identity* of an
action whose behaviour a face must perform. Toggling a theme is exactly
that — the kernel cannot repaint a window or a terminal, but one id and
one chord must mean the same thing on all three faces. `palette.open`
and `history.togglePanel` are the two precedents (H:37, H:51).

**The exact fallout of adding one host command**, read from the tests:

- `IMPLEMENTED_COMMAND_COUNT == BUILTIN_COMMAND_COUNT` (dispatch_tests.rs:170-176)
  is **untouched**: host commands are deliberately absent from `BUILTIN`
  and from the action table (H:16-19), and adding one to the action
  table would be a compile error by design.
- `registry_tests.rs:189-190` (`registry.len() == BUILTIN_COMMAND_COUNT`)
  — untouched, built-ins only.
- `Ac:89` (`palette_order().len() == BUILTIN_COMMAND_COUNT + HOST_COMMAND_COUNT`)
  and `Ac:111` (`reported == HOST_COMMAND_COUNT`) — both **derived from
  `HOST.len()`** (H:77), so both stay green automatically. The new
  command must be *reported* rather than run, which is what a host
  command is (`CommandRunError::Unimplemented`).
- `K:321` — `assert_eq!(bound.len(), BUILTIN_COMMAND_COUNT +
  HOST_COMMAND_COUNT - 41)`. The `41` is the hand-maintained list of
  deliberately-unbound built-ins (K:230-318 — `edit.insertCharacter`,
  the fifteen `transform.*`, the palette-only `ast.*` set, and the
  no-op). This is the **one place a new command can break the suite**,
  and it breaks only if the command is left unbound. Bind it in the
  default keymap and `41` stays `41`.
- `Dc`'s own suite (`every_borrowed_kernel_verb_is_still_a_kernel_verb`,
  Dc:213-236) already admits host ids, so a ⌘ chord for it on the
  desktop face costs nothing extra.

**Chord availability, verified**: `Char('t')` / `Char('T')` appears in
**no binding table** in the estate — not the kernel default keymap, not
`apps/iridium/src/app/commands.rs`, not `Dc`. `Ctrl+Alt+T` and `⌘⌥T` are
both free. (`⌘⌥` already has the `AltGraph`-forbidden guard pattern at
Dc:120-121, so a new row reuses `META_ALT`.)

Cost of the whole command surface: one id + one `CommandMeta` in `H`
(≈12 lines), one row in `Ka`, one row in `Dc`'s `BINDINGS`, and the
dispatch arm in `A`. Face-local ids (the `file.save` pattern, Dc:62-64)
are the wrong choice here for the reason `H`'s module doc gives at
length: three faces would end up with three spellings.

### 1.5 System appearance — what winit 0.30.13 actually delivers

Verified in the vendored source, not from memory:

- **`winit::window::Theme`** is a two-variant enum, `Light | Dark`
  (W:`src/window.rs:1755-1761`).
- **`Window::theme() -> Option<Theme>`** (W:`src/window.rs:1383`). On
  macOS it is implemented (W:`src/platform_impl/macos/window_delegate.rs:1675-1687`):
  it reads the window's own `appearance()` if one was set, otherwise
  falls back to `NSApplication::sharedApplication().effectiveAppearance`,
  and returns `Some(Theme::Light)` if the selector is unavailable. It
  **never returns `None` on macOS**. (Unsupported on iOS/Android/X11/Orbital.)
- **`WindowEvent::ThemeChanged(Theme)`** (W:`src/event.rs:397`) is
  emitted on macOS from a KVO observer on `effectiveAppearance`
  (W:`macos/window_delegate.rs:477`), and it filters correctly — it
  fires only when the *mapped* theme changed, not on contrast-only
  appearance changes (W:`macos/window_delegate.rs:470-476`).
- **`WindowAttributes::with_theme(Option<Theme>)`** (W:`src/window.rs:398`)
  and **`Window::set_theme(Option<Theme>)`** (W:`src/window.rs:1365`)
  set/override the window's appearance; macOS honours both
  (W:`macos/window_delegate.rs:716`, `:1690`).

**The trap, stated plainly.** `ThemeChanged` is only emitted *"if the
window theme was not overridden by `Window::set_theme`"* (W:`event.rs:392`),
and the macOS implementation enforces it literally: the observer returns
early when `self.window().appearance().is_some()`
(W:`macos/window_delegate.rs:464-466`). So the two things one naturally
wants — (a) follow the system, and (b) make the *titlebar* match a
manually-pinned light theme — are mutually exclusive through this API.
Choosing a manual pin that also recolours the titlebar permanently
silences system-appearance events for that window. This is D-2's real
content, not a footnote.

Cost of following the system: one `WindowEvent::ThemeChanged` arm in
`A::window_event` and one `window.theme()` read in `Shell::open`.
Genuinely small — the honest cost is the *policy*, not the code.

### 1.6 The chrome consequence — what derives right, what derives wrong

The overlay's design values are constants in one documented block
(O:83-170), each citing the web style it was read from. Panel colours
come from the theme through three derivations (O:937-976).

**Derives correctly by construction in any theme** — these read theme
fields and need no light-specific value:

| Derived value | Where | Why it is fine in light |
|---|---|---|
| `panel_background` = `current_line` over `background`, gutter fallback | O:937-955 | states the theme's own band colour; asserted opaque in **both** presets today (O:1208-1217) |
| `strip_background` = the same | O:956-959 | same |
| selected-row band = `theme.editor.selection` | O:851-859 | translucent theme colour over the panel |
| field caret = `theme.editor.cursor` | O:867 | theme colour |
| panel text = `theme.editor.foreground` | O:875-877 | theme colour |
| match highlight = `theme.editor.search_match_current` | `command_palette.rs` | theme colour |
| all geometry — radius 8, pad 16/12, row radius 4, row inset 4, top anchor 12% | O:93-134 | darkness-independent |

**Derives *wrongly* in light — needs its own value.** Three constants
are darkness-dependent by construction because they are *black ink*, not
theme colour:

1. **`BACKDROP_ALPHA = 0.45` black** (O:119, applied at O:812-822). Over
   the dark preset's `#1a1a1a` this dims to ≈`#0E0E0E` — a subtle
   deepening. Over a **white** document it produces ≈`#8C8C8C`: opening
   the palette drops the entire editor to mid-grey. Unusable in light,
   and doubly wrong for the era — classic Mac modal dialogs did not dim
   the desktop at all.
2. **The shadow: `offset 16, blur 48, black @0.55`** (O:105-112, applied
   at O:824-833). On dark it reads as depth. On a light page a
   48-logical-px black smear at 0.55 is a bruise, and it is the single
   most anachronistic element available — the classic Mac shadow was a
   *hard* 1-2 px offset black rectangle with no blur whatsoever.
3. **`HAIRLINE_ALPHA = 0.18`** (O:139, applied via `hairline_color`,
   O:962-970). Computed for the current light preset: `foreground
   #333333` at 0.18 over `panel_background #F7F7F7` gives **`#D4D4D4`**
   — a 35/255 step against the panel and a 43/255 step against a white
   document. The era's defining feature is a *crisp* frame; 0.18 of
   near-black is a whisper. Light needs ≈0.45-0.75, or a stated colour.

One further light-specific finding: **`HAIRLINE = 1.0` is one *physical*
pixel** (O:143, deliberately — the web reference's `1px` on a
`devicePixelRatio` display). On a 2× display that is a half-logical-pixel
line. Correct for a modern hairline; wrong for Platinum, whose frames
were 1 *logical* px (= 2 physical on Retina). See D-4.

And the arithmetic that shows the current light preset is not shippable
even before the era question: `panel_background(Theme::light())` =
`current_line #F7F7F7` (opaque, a = 1.0 at Th:173, so the composite
returns it unchanged) over a `#FFFFFF` document — **8/255 apart**. The
panel is currently held apart from the page by a `#D4D4D4` hairline and a
black smear, and by nothing else.

**The context menu** (`CONTEXT-MENU-MAP.md`, ruled 4 Aug, landing
concurrently) uses the same three derivations plus a `line_number`-class
grey for disabled rows — so it inherits every finding above, and every
fix, without its own ruling.

### 1.7 The terminal face — what a light theme does to a terminal

`Palette::from_theme` (P:63-94) is rebuilt every frame from
`editor.get_theme()` (E:1177), so the TUI needs **no switch plumbing**.
What it needs is a policy, and the policy is already written into the
module's two rules (P:8-18):

- **`a = 0` means "the terminal's own"** — `solid()` returns
  `Color::Default` (P:194-200), which survives every degradation tier
  (Cd:47).
- **`0 < a < 1` composites over the editor background when there is one,
  and is used at full strength when there is not** (`over()`, P:203-216).

Since 6e22cbe the dark preset states an **opaque** `#1a1a1a`, so the TUI
paints its own background and no longer defers (pinned by
`the_dark_preset_paints_its_own_background`, P:290-297). A light preset
with an opaque background behaves identically — it will repaint a dark
terminal light, edge to edge. That is consistent, and it is the honest
consequence of the 4 Aug ruling; it is not a new decision so much as the
same one applied in the other direction (D-6 asks whether Tom wants it
anyway).

**Derivation rules the light preset must respect or the TUI degrades**
(each is an existing, passing test — breaking one is a real regression):

| Rule | Test | Consequence of violating it |
|---|---|---|
| `gutter != background` | `a_gutter_distinct_from_the_background_gets_a_coloured_statusline` (P:326-337) — **runs `Theme::light()` explicitly** | the statusline falls back to reverse video (P:`status_style`) |
| `current_line` distinguishable from `background` | `the_overlay_panel_stands_apart_from_the_text_in_both_presets` (P:424-435) | every TUI overlay becomes invisible — the terminal has no hairline to save it |
| `search_match != search_match_current` | `the_current_search_match_is_distinguishable_from_the_rest` (P:363-378) — both presets | the editor stops saying which match `Enter` leaves |
| `cursor` contrasts with `background` | `a_caret_inverts_the_cell_it_sits_on` (P:346-352) — **runs `Theme::light()`** | the caret's glyph vanishes into the caret |
| surfaces opaque | `the_dark_preset_surfaces_are_opaque` (Th:310-322) — dark only today | the compositor's clear colour (C:875-882) goes transparent → black on an opaque swapchain, the exact 6e22cbe bug |

**The degradation tiers** (Cd:30-38, `TrueColor` / `Ansi256` / `Ansi16`;
selected at `driver/capabilities.rs:160-172`) apply the same to any
theme, but light has a specific hazard: **near-white greys collapse.**
At `Ansi16`, `#EEEEEE`, `#F5F5F5` and `#FFFFFF` all degrade to the same
basic white (Cd:87+, `rgb_degrades_to_a_basic_colour_at_16_colours`,
Cd:409-425) — so on a 16-colour terminal a light theme's document,
gutter, panel and strip are **one flat colour**, held apart only by the
statusline's reverse video and the overlay's dim/bold attributes. This is
not fixable by colour choice; it is the tier's honest limit and it should
be *stated*, not designed around. At `Ansi256` the greys survive (the 24
step greyscale ramp resolves ≈10/255 apart) and all three variants below
read correctly.

---

## 2. The design — a classic Mac OS light face

### 2.1 What the era actually looked like

Concretely, from System 6 through Mac OS 9's Platinum, the things a
person actually *sees*:

- **Backgrounds are grey or off-white, never a glowing pure white.** The
  canonical Platinum dialog/window grey is `#DDDDDD`; System 7's utility
  windows and Finder list views sat on the same family. Pure white was
  reserved for the *document well* — the paper — and even then it was a
  paper, not a light source. The everyday surface of the OS is grey.
- **Framing is 1 px and black-ish, not soft.** Every window, dialog,
  list and menu is bounded by a hard one-pixel rule. Separation is done
  with *lines*, not with shadow or blur. Where depth is implied it is a
  two-tone bevel — white on the top-left, `#888`-ish on the
  bottom-right — over the grey, never a gradient.
- **The only shadow is hard.** A pulled-down menu drops a solid black
  rectangle offset a couple of pixels down and right. No blur, no falloff,
  no alpha ramp. A blurred 48-px shadow is from a different century.
- **Pinstripes.** Platinum's title bars and desktop pattern are 1 px
  alternating greys (`#DDDDDD`/`#CCCCCC` class) — texture from
  repetition at pixel scale, never from noise or gradient.
- **The accent is restrained and singular.** Classic Mac used *one*
  accent at a time (the Appearance control panel's blue by default, gold
  as the common alternative), applied to selection and scroll furniture,
  and nowhere else. Not a palette of accents; one.
- **Type is chunky, square and unapologetically bitmap.** Chicago
  (System 1-7) and Charcoal (Platinum) had high x-heights, flat
  terminals and no antialiasing; Geneva carried small text; Monaco was
  the monospace. The character is *dense and dark* — text that reads as
  ink, not as grey.
- **Colour in code is dark and saturated, on a light ground.** The
  era's own editors — MPW, THINK C, CodeWarrior — used near-black navy
  keywords, maroon strings and forest-green comments. Not pastels, not
  neons: dark inks that hold their own against a grey page.

**The honest limitation.** Both GPU faces embed exactly one font
(`JetBrainsMono-Regular.ttf`, A:110, Sc:47) and never read
`typography.font_family` (Tf:9). Chicago/Charcoal character is **not
available** to this change. What *is* available: the density and darkness
of the ink (a near-black `foreground`, not a polite grey), the greys, the
crisp framing, the hard shadow, and the one restrained accent. That is
five of the six era signals, delivered honestly, and it is enough — the
character reads from the surface and the frame long before it reads from
the letterforms. A future era font is a separate, larger change (a second
embedded face plus a `font_family` path through both GPU faces).

### 2.2 The rules every variant below respects

Every table in §2.3 satisfies, by construction:

- all of `background`, `gutter`, `minimap_background` **opaque**
  (Th:310-322's rule, C:875-882's requirement);
- `gutter != background` (§1.7, P:326-337);
- `current_line` opaque and ≥ 17/255 from `background` (§1.7, P:424-435;
  and it is the panel/strip surface, O:937-955);
- `search_match != search_match_current` (P:363-378);
- `cursor` ≥ 15:1 against `background` (P:346-352);
- `foreground` ≥ 12:1 against `background` — era ink, not grey text;
- every one of the 14 syntax colours ≥ 4.5:1 against its variant's
  `background`, and `attribute != error` (the Th:278-279 defect is not
  repeated). *(Corrected, 5 Aug: this clause originally read "no two of
  the 14 equal". That is the wrong shape of rule — deliberate ties are
  how a palette groups token classes, and all three variants use them.
  See §4's test 4 for the full withdrawal.)*
- `diagnostic_*`, `diff_*` and `change_*` keep functional hue in **all
  three variants**, including the monochrome one. This codebase's stated
  domain is financial, legal and healthcare documents; a diff whose
  additions and deletions differ only in grey is a defect, not a style.

### 2.3 Three variants

Genuinely different in character, not three shades of one idea: A is
grey-chrome Platinum with saturated dark ink; B is warm paper with earthy
ink and almost no chrome; C is white-and-black System 6 with colour used
only where meaning demands it.

> **Where these live now, per D-1.** A is `crates/iridium-editor/src/theme/classic.rs`
> and *is* `Theme::light()`. B and C are `themes/paper.json` and
> `themes/monochrome.json` — generated from the tables below, not retyped, and
> held to them by the tests in that same module. **This section remains the one
> place that says *why* each row is what it is**; JSON carries no comments, and
> the commentary was deliberately not paraphrased into a fourth copy.

---

#### Variant A — **"Platinum"** · faithful Mac OS 8.5

*For: someone who wants the editor to look like it shipped in 1998.* The
document well is a light grey, the panels are canonical Platinum grey a
step **below** it (as classic dialogs floated over documents), the frames
are near-black, and the code is inked in MPW/CodeWarrior colours.

**Editor colours**

| Field | Hex / RGBA | Why |
|---|---|---|
| `background` | `#EFEFEF` | the Platinum window well — grey, not white, the single strongest era signal |
| `foreground` | `#000000` | Chicago-era body text is pure black ink; 18.4:1 |
| `selection` | `#3355AA` @ 0.30 | the Appearance control panel's default blue, translucent so syntax survives under it |
| `selection_inactive` | `#3355AA` @ 0.14 | the same accent, halved — one accent, two strengths |
| `cursor` | `#000000` | the era's caret is a black bar |
| `line_number` | `#808080` | the 50 % dither grey, the one grey the 1-bit era actually had |
| `line_number_active` | `#000000` | full ink on the cursor's line |
| `current_line` | `#DDDDDD` @ 1.0 | **canonical Platinum grey** — also the panel/strip surface (O:937-955), which is exactly right: panels *are* Platinum chrome |
| `gutter` | `#DDDDDD` @ 1.0 | the same chrome family; distinct from `background` (statusline rule) |
| `minimap_background` | `#DDDDDD` @ 1.0 | ditto |
| `search_match` | `#FFD75F` @ 0.55 | the Appearance panel's "Gold" accent, the era's canonical alternative |
| `search_match_current` | `#FFA400` @ 0.85 | the same gold, driven hard |
| `diff_added_bg` | `#2E8B57` @ 0.16 | dark saturated ink, not pastel |
| `diff_deleted_bg` | `#A52A2A` @ 0.16 | ditto |
| `diff_added_gutter` | `#1F7A45` | legible against grey chrome |
| `diff_deleted_gutter` | `#A32020` | ditto |
| `change_added` | `#1F7A45` | one green, used consistently |
| `change_modified` | `#B8860B` | dark goldenrod — the gold accent as a bar |
| `change_deleted` | `#A32020` | one red |
| `blame_foreground` | `#808080` @ 0.75 | ghost text as the dither grey, quieted |
| `diagnostic_error` | `#A32020` | matches `change_deleted`; one red in the whole theme |
| `diagnostic_warning` | `#B8860B` | one amber |
| `diagnostic_info` | `#2A4E8C` | the accent blue, opaque |
| `diagnostic_hint` | `#6B6B6B` | a quieter grey than `line_number`, so hints recede |

**Syntax colours** — MPW/CodeWarrior inks

| Field | Hex | Why |
|---|---|---|
| `keyword` | `#00007F` | CodeWarrior's navy; ≈14:1 on `#EFEFEF` |
| `string` | `#7F0000` | maroon, the era's string colour |
| `number` | `#005F5F` | dark teal, distinct from both green and blue |
| `comment` | `#007000` | MPW's forest green; ≈5.4:1 — present but recessive |
| `function` | `#5C3D99` | aubergine, the one violet |
| `variable` | `#1A1A1A` | ink, barely off `foreground` — variables are the page |
| `type_name` | `#005F87` | steel blue, distinguishable from `keyword` at a glance |
| `operator` | `#000000` | full ink |
| `punctuation` | `#3A3A3A` | one step back from ink, so structure recedes (fixing Th:273-274's inversion) |
| `property` | `#2A4E8C` | the accent blue |
| `constant` | `#7A3E00` | burnt umber |
| `tag` | `#7F0000` | markup tags read as strings-of-structure |
| `attribute` | `#5C3D99` | **distinct from `error`** — the Th:278-279 defect not repeated |
| `error` | `#B00000` | the only pure-ish red in the syntax set |

---

#### Variant B — **"Paper"** · System 7 on good stock

*For: someone who will live in this eight hours a day.* Warm off-white,
minimal chrome, earthy ink. Closest to modern comfort while keeping the
era's warmth and crispness; the greys are all warm-shifted, which is what
separates it from a generic light theme.

**Editor colours**

| Field | Hex / RGBA | Why |
|---|---|---|
| `background` | `#FBF8F1` | warm paper — off-white, never a light source |
| `foreground` | `#24211C` | warm near-black; ≈15:1, ink not grey |
| `selection` | `#4A6FA5` @ 0.26 | a muted period blue, warm-compatible |
| `selection_inactive` | `#4A6FA5` @ 0.12 | halved |
| `cursor` | `#24211C` | the ink colour |
| `line_number` | `#A69E90` | warm grey, clearly subordinate |
| `line_number_active` | `#24211C` | full ink |
| `current_line` | `#F2ECE0` @ 1.0 | a warm card one step off the paper — the panel/strip surface |
| `gutter` | `#F5F0E6` | between paper and card; distinct from `background` (statusline rule) |
| `minimap_background` | `#F5F0E6` | ditto |
| `search_match` | `#E8C46A` @ 0.55 | aged-gold highlighter |
| `search_match_current` | `#D98E28` @ 0.85 | the same gold, driven |
| `diff_added_bg` | `#4E8B57` @ 0.14 | soft on paper |
| `diff_deleted_bg` | `#A85A5A` @ 0.14 | ditto |
| `diff_added_gutter` | `#3E7A4B` | legible on warm grey |
| `diff_deleted_gutter` | `#9E4040` | ditto |
| `change_added` | `#3E7A4B` | one green |
| `change_modified` | `#A88232` | one amber |
| `change_deleted` | `#9E4040` | one red |
| `blame_foreground` | `#A69E90` @ 0.85 | the gutter grey as ghost text |
| `diagnostic_error` | `#9E3030` | matches `change_deleted` family |
| `diagnostic_warning` | `#A88232` | matches `change_modified` |
| `diagnostic_info` | `#3E6099` | the accent blue |
| `diagnostic_hint` | `#8A8375` | warm grey, recessive |

**Syntax colours** — earthy inks

| Field | Hex | Why |
|---|---|---|
| `keyword` | `#26478D` | warm-shifted navy |
| `string` | `#8A3324` | burnt sienna |
| `number` | `#1D6A5A` | deep viridian |
| `comment` | `#4A7A3D` | olive-green; ≈4.8:1 — readable, recessive |
| `function` | `#7A4E1E` | tobacco brown |
| `variable` | `#33302A` | warm ink |
| `type_name` | `#2A6070` | slate teal |
| `operator` | `#4A453C` | warm grey-black |
| `punctuation` | `#6B6459` | one step further back — structure recedes |
| `property` | `#3A5A7A` | dusty blue |
| `constant` | `#8A4B10` | amber-brown |
| `tag` | `#8A3324` | as strings |
| `attribute` | `#7A4E1E` | **distinct from `error`** |
| `error` | `#9E3030` | the single red |

---

#### Variant C — **"Monochrome"** · System 6, in colour only where it must be

*For: someone who wants the 1-bit machine.* Pure white, pure black,
dither greys, and **one** restrained accent for selection. Code is
differentiated by very dark low-chroma inks — the page reads
near-monochrome, with hue as a whisper. Diagnostics and diff keep full
functional colour (§2.2's last rule) because a monochrome diff is a
defect.

**Editor colours**

| Field | Hex / RGBA | Why |
|---|---|---|
| `background` | `#FFFFFF` | the 1-bit page |
| `foreground` | `#000000` | the 1-bit ink; 21:1 |
| `selection` | `#7A93B8` @ 0.36 | the one accent — System 7's pale highlight blue; composites to ≈`#C6D0DE`, a hue clearly distinct from the neutral search greys |
| `selection_inactive` | `#7A93B8` @ 0.16 | halved |
| `cursor` | `#000000` | black bar |
| `line_number` | `#808080` | the 50 % dither |
| `line_number_active` | `#000000` | full ink |
| `current_line` | `#E6E6E6` @ 1.0 | the 90 % dither — panel/strip surface; 25/255 from the page so it **survives the terminal**, where there is no hairline to help (§1.7) |
| `gutter` | `#E0E0E0` | one dither step below the panel; distinct from `background` |
| `minimap_background` | `#E0E0E0` | ditto |
| `search_match` | `#808080` @ 0.30 | a 25 % dither wash — composites to ≈`#E0E0E0` |
| `search_match_current` | `#303030` @ 0.45 | ≈`#9E9E9E`; black text on it is ≈7:1, still crisp, and unmistakably darker than the other matches |
| `diff_added_bg` | `#2E7D32` @ 0.13 | functional colour, kept |
| `diff_deleted_bg` | `#B71C1C` @ 0.13 | kept |
| `diff_added_gutter` | `#1B5E20` | kept |
| `diff_deleted_gutter` | `#B71C1C` | kept |
| `change_added` | `#1B5E20` | kept |
| `change_modified` | `#8D6E00` | kept |
| `change_deleted` | `#B71C1C` | kept |
| `blame_foreground` | `#808080` @ 0.85 | dither grey |
| `diagnostic_error` | `#B71C1C` | kept — meaning outranks style |
| `diagnostic_warning` | `#8D6E00` | kept |
| `diagnostic_info` | `#1F3A6E` | kept |
| `diagnostic_hint` | `#606060` | dither |

**Syntax colours** — near-monochrome, hue as a whisper

| Field | Hex | Why |
|---|---|---|
| `keyword` | `#101070` | near-black navy — reads as *weight*, not colour |
| `string` | `#6A1B1B` | near-black maroon |
| `number` | `#1B4D4D` | near-black teal |
| `comment` | `#5A5A5A` | the era's comment: grey, not green; 7:1 |
| `function` | `#303030` | ink, one step lifted |
| `variable` | `#000000` | full ink |
| `type_name` | `#1B3A5A` | near-black steel |
| `operator` | `#000000` | full ink |
| `punctuation` | `#4A4A4A` | recedes |
| `property` | `#202020` | ink |
| `constant` | `#4A2A00` | near-black umber |
| `tag` | `#6A1B1B` | as strings |
| `attribute` | `#1B3A5A` | **distinct from `error`** |
| `error` | `#B71C1C` | the one place C permits a real red — an error must shout |

---

### 2.4 Does the just-landed chrome suit a classic-Mac light theme?

**Honest answer: it inherits the geometry and needs its own ruling on
exactly three values.** DESKTOP-CHROME-MAP ruled the chrome for dark
against the web demo as spec (its RULINGS block, "the web demo is the
spec"). That spec is silent on light, and three of its constants are
*black ink at fixed alpha* rather than theme colour (§1.6) — they cannot
follow a theme and must be told.

**Inherits unchanged** (all darkness-independent, all reading theme
fields): radius 8 logical (O:93), padding 16/12 (O:97-101), row radius 4
and inset 4 (O:130-134), top anchor 12 % (O:123), the 64/20-column caps
and 12-row limit (O:160-170), the panel/strip background derivation
(O:937-959), the selected-row band, the caret, the match highlight.

**Needs a light value:**

| Constant | Dark (ruled) | Light (proposed) | Reasoning |
|---|---|---|---|
| `BACKDROP_ALPHA` (O:119) | 0.45 black | **0.16 black** (or 0.0) | 0.45 over white = `#8C8C8C`, the whole editor to mid-grey. Classic Mac modals dimmed nothing; 0.16 is the smallest dim that still says "modal" |
| shadow (O:105-112) | offset 16, blur 48, `#000` @0.55 | **offset 3, blur 2, `#000` @0.34** | the era's shadow is *hard*: a solid offset rectangle. `RoundedQuad::shadow` already takes blur as a parameter (O:824-833, `render/rounded.rs`), so blur ≈0 is free — no new capability |
| `HAIRLINE_ALPHA` (O:139) | 0.18 | **0.55** (A, B) / **0.85** (C) | 0.18 of `#333` on a light panel is `#D4D4D4`; the era's frame is a crisp near-black rule |
| `HAIRLINE` width (O:143) | 1 **physical** px | **1 logical px × scale** in light | Platinum frames were 1 px at 1× = 2 physical on Retina. A half-logical hairline is a modern idiom, not an era one |

**Rounded corners stay** — Tom's standing rule (DESKTOP-CHROME-MAP:9-11:
*"Rounded corners are mandatory and must be genuine arcs"*) is not up for
revision, and this map does not propose sharp corners anywhere. The era
does argue for a **tighter** radius: Platinum's rounded elements were
4-5 px at 1×, not 8. Whether light drops to radius 5 while dark keeps 8
is a genuine taste call and appears as D-4b; my own recommendation is to
**keep 8 in both** so there is one chrome geometry across modes, and let
Tom move the number by eye at the rebuild — the same latitude R3 gave
him.

**Mechanism for the three values.** Two options, priced:

- **(i) Darkness-keyed constants** — `fn backdrop_alpha(theme: &Theme)`
  etc., branching on `theme.is_dark`. Zero schema change, ~20 lines in
  `O`, and it works for every theme immediately. But it hard-codes "light
  themes want a crisp frame" into the *face*, which is a statement a
  theme should be allowed to make.
- **(ii) A `panel_border` theme field** — DESKTOP-CHROME-MAP already
  flagged this as "a safe *later* addition" (its R4, citing the forgiving
  deserialization at Th:70-75). Additive: absent fields fall back to the
  dark preset, so every existing theme file keeps parsing. It is the
  honest answer for the border specifically, because the three variants
  above genuinely disagree about it (0.55 vs 0.85, and C wants a stated
  near-black rather than a derived alpha).

Recommendation: **both, in their proper places.** `panel_border` becomes
a real (25th) `EditorColors` field — a border is a colour a theme should
state — while the backdrop dim and the shadow stay face constants keyed
on `is_dark`, because they are *ink over the page*, not chrome colour,
and no theme should have to state them. See D-5.

---

## 3. The switch, priced

**File-touch list, whole change:**

| File | Change | Size |
|---|---|---|
| `crates/iridium-editor/src/theme/colors.rs` | `EditorColors::light()` and `SyntaxColors::light()` replaced with the chosen variant; `panel_border` field added (D-5) | ~90 lines rewritten, 1 field added |
| `crates/iridium-editor/src/theme/mod.rs` | `Theme::light()`'s `name` (e.g. `"Iridium Platinum"`) | 1 line |
| `crates/iridium-editor/src/render/simple_highlight.rs` | a `From<&theme::SyntaxColors>` for the bridge palette (fact 2) | ~25 lines |
| `crates/iridium-editor/src/render/compositor.rs` | `set_theme` maps the bridge palette from `theme.syntax` instead of branching on `is_dark` (C:2019-2023) | ~5 lines |
| `crates/iridium-editor/src/commands/builtin/host.rs` | `VIEW_TOGGLE_THEME` id + `CommandMeta` | ~14 lines |
| `crates/iridium-editor/src/commands/default_keymap.rs` | one binding, `Ctrl+Alt+T` | 1 row |
| `apps/iridium-desktop/src/app.rs` | the host-command dispatch arm (editor + compositor + redraw); optionally the `ThemeChanged` window-event arm | ~25 lines |
| `apps/iridium-desktop/src/commands.rs` | one `⌘⌥T` row in `BINDINGS` | 1 row |
| `apps/iridium-desktop/src/overlay.rs` | the three light-honest chrome values (§2.4) | ~30 lines |
| `apps/iridium-desktop/src/run.rs` + `app.rs` | `--theme` flag parity with the TUI, if ruled in (D-7) | ~40 lines |
| `apps/iridium/src/app/commands.rs` | the TUI's dispatch arm for the same host id | ~10 lines |
| `apps/iridium-desktop/tests/chrome_screenshots.rs` | a theme parameter through `capture`; the light shot set (§4) | ~40 lines |

**Sequencing.** `app.rs`, `mouse.rs`, `commands.rs` and `surface.rs` are
being edited **right now** by the context-menu implementation
(`CONTEXT-MENU-MAP.md`, ruled 4 Aug). Four of this change's touches land
in two of those files. **This change must sequence behind the context
menu**, not beside it. Everything kernel-side (`theme/`, `render/`,
`commands/builtin/host.rs`) is disjoint from that work and could start
immediately if Tom wants the colours in flight while the menu lands.

**What this change deliberately does not do:** persistence. There is no
config file, no preferences store and no state directory anywhere in the
estate — `--theme` is read per invocation (Tc:162-169) and forgotten. A
toggle therefore lasts until the window closes unless D-7 rules
otherwise. This is named rather than deferred silently.

---

## 4. Proof plan — how the light theme gets *proven*

### 4.1 The rendered options (how Tom chooses)

The harness exists: `apps/iridium-desktop/tests/chrome_screenshots.rs`,
`#[ignore]`d, headless by construction (no surface — `compatible_surface:
None`, Sc:79-91), composing onto an offscreen 3024×1964 Bgra8Unorm
texture at scale 2.0 and writing uncompressed PNGs with no image
dependency (Sc:338-380). Output directory from `IRIDIUM_CHROME_SHOT_DIR`
(Sc:400-402).

**What it needs:** one parameter. Today it never calls
`compositor.set_theme` at all (Sc:405-422) and takes the editor's default
— dark. Threading a `&Theme` through `run`/`capture`, calling **both**
`editor.set_theme` and `compositor.set_theme` (§1.3's two-call rule), is
the entire modification.

**The shot set — what Tom must see to choose.** Per variant:

1. **Plain document, `.rs`** (grammar live, real tree-sitter spans) — the
   surface, gutter, current-line band, caret, a live text selection, and
   the *theme's* syntax palette.
2. **Plain document, `.txt`** (no grammar → the compositor's keyword
   bridge). **Non-negotiable**, because of fact 2: until the bridge is
   fixed these two shots show *different palettes*, and if the bridge fix
   lands they must be shown identical. This shot is the proof either way.
3. **Command palette open**, query typed, selection moved off row 1 — the
   panel surface, the hairline, the shadow, the backdrop dim, the
   selected-row band, the match recolour. This is where §2.4's three
   values are judged.
4. **Search panel + strip** — the docked bar's own hairline against the
   document, `search_match` vs `search_match_current` in the live text.
5. **Undo tree** — branch badge and dim rows (the `line_number`-class
   grey).
6. **Context menu**, once the concurrent work lands — same three
   derivations, no separate judgement needed, but worth one frame.

3 variants × 6 states = **18 PNGs**, plus the current dark set as the
control. Sc's existing three shots (palette/search/history) already cover
states 3-5; states 1-2 are new and are ~15 lines each.

Not shootable headlessly and honestly excluded: the diff view and gutter
change bars need git state the harness has no source for. Those fields
(`diff_*`, `change_*`) are judged from the table, not from a frame.

### 4.2 What pins the theme once chosen (all CPU-side, no GPU)

**New tests in `theme/colors.rs`**, mirroring the dark preset's existing
guard (Th:310-322):

1. `the_light_preset_surfaces_are_opaque` — `background`, `gutter`,
   `minimap_background` all `a == 1.0`. The direct light twin of the
   6e22cbe regression.
2. `the_light_preset_gutter_is_distinct_from_its_background` — asserted
   kernel-side, not only from the TUI (P:326-337), because the rule is
   the *theme's*, not the terminal's.
3. `the_light_presets_panel_surface_stands_apart` — `current_line`
   composited over `background` differs by ≥ 16/255 per channel. Pins the
   §1.6 finding (8/255 today) so it cannot regress.
4. ~~`every_syntax_colour_is_distinct` — the 14 fields pairwise unequal,
   in **both** presets.~~ **Withdrawn, 5 Aug.** Nothing in this codebase
   satisfies it and nothing should. `classic.rs:44-55` reached this first
   and reported it as a map defect rather than improvising around it: all
   three §2.3 variants deliberately tie colours (`tag == string` in all
   three, `attribute == function` in A and B, `variable == operator` in
   C), and both shipped presets tie `operator == punctuation ==
   foreground` and group `variable`/`property`/`attribute` as one
   identifier family — dark at `#9cdcfe`, light now at `#001080`. Ties
   are how a palette says two things belong together. Pairwise
   distinctness was the wrong shape of rule everywhere it was asserted,
   including §2.2 above.

   What the presets can be held to, and now are — both landed with the
   dark preset passing unchanged, which is the evidence the invariants
   were already real and only light had drifted off them:

   - `a_diagnostic_never_wears_a_token_colour` — `attribute != error` in
     both presets. Pins the Th:278-279 defect, now fixed: light's
     `attribute` moved to `#001080`, rejoining the identifier family that
     dark's `attribute` has always sat in. Fixing `attribute` rather than
     softening `error` is deliberate — two reds a glance apart would pass
     an inequality assertion while leaving the classes indistinguishable,
     which is the thing the test exists to prevent.
   - `separators_are_never_louder_than_the_code_they_separate` —
     `operator == punctuation == foreground` in both presets. Pins the
     Th:273-274 defect, now fixed: light's separators moved from `#000000`
     to `#333333`. The invariant did not need inventing; dark held it and
     light lost it when `foreground` moved off black.
5. `every_syntax_colour_is_legible_on_its_surface` — WCAG contrast ratio
   ≥ 4.5:1 against `background`, both presets. Pure arithmetic; ~20 lines
   of relative-luminance helper.
6. `body_text_is_ink` — `foreground` vs `background` ≥ 12:1, both
   presets. This is the era rule made testable.
7. `the_two_syntax_palettes_agree` — `simple_highlight::SyntaxColors`
   derived from `theme.syntax` equals the theme's own values for the
   eight token classes the bridge models. Pins fact 2's fix.

**New tests in `overlay.rs`** (alongside O:1208-1239's existing three):

8. `the_hairline_separates_in_both_presets` — `hairline_color(theme)`
   differs from `panel_background(theme)` **and** from
   `theme.editor.background` by ≥ 40/255. Today light gives 35 and 43.
9. `the_backdrop_never_swamps_the_page` — the backdrop composited over
   `theme.editor.background` stays within a bounded lightness change in
   both presets.

**New tests in the TUI palette** — the existing five (§1.7's table)
already run `Theme::light()` and keep running; tighten
`the_overlay_panel_stands_apart_from_the_text_in_both_presets` (P:424-435)
from `assert_ne!` to a minimum channel distance, since `assert_ne!`
passes on an 8/255 difference that no terminal can show.

**New test in the command suite:** the existing count-derived assertions
(§1.4) carry the new host command automatically; add one explicit
`the_theme_toggle_is_bound_on_every_face` so the `K:321` arithmetic has a
named reason rather than a number that happened to stay 41.

**Pixel-level, headless:** `apps/iridium-desktop/tests/plain_frames.rs`
already has the machinery (offscreen compose + `pixels()` readback). One
addition: compose the same document under dark and under light and assert
the frames **differ**, then assert the light frame's corner pixel equals
the light preset's `background` — which is the clear colour by
construction (C:875-882), and therefore the tightest possible proof that
the switch reached the GPU.

**Already proven, inherited:** `Rt:788-820`
(`a_set_theme_misses_and_recomposes_identically`) shows a warmed
compositor after `set_theme` recomposes byte-identical to a fresh one.
The switch's correctness under retained shaping is already pinned.

**Judged by eye, honestly:** which variant, the exact backdrop alpha, the
exact shadow hardness, the radius question. No CPU test proves a shadow
looks right in light mode any more than it did in dark. The change ends
at a bundled rebuild in Tom's hands — the feel gate — with §2.4's numbers
explicitly his to move.

---

## 5. Decisions for the controlling seat

- **D-1 — Which variant.** **A "Platinum"** (grey Platinum chrome,
  saturated MPW/CodeWarrior ink) vs **B "Paper"** (warm off-white,
  minimal chrome, earthy ink) vs **C "Monochrome"** (white/black System 6,
  one accent, near-monochrome code).
  **Recommendation: render all three (§4.1) and let Tom pick from
  pixels, but ship the winner as `Theme::light()` and keep the other two
  as JSON theme files under `themes/`** — the theme system already loads
  arbitrary JSON (Tt, `Theme::from_json`, Tm:175-177), so a second and
  third variant cost one file each and nothing in the binary. My own
  reading of "old-school Mac OS, my own type of styling": **A**. B is the
  one that is easiest to *use*; C is the one that is most *faithful*; A
  is the one that is unmistakably classic Mac at a glance while still
  being a code editor, and "at a glance" is what the ask is about.

- **D-2 — Follow the system appearance, manual only, or both.** All
  three are available on macOS (§1.5), and the trap is real: setting the
  window's appearance to make the *titlebar* match a manual pin
  **permanently silences `ThemeChanged`** for that window
  (W:`event.rs:392`, `macos/window_delegate.rs:464-466`).
  **Recommendation: both, with the standard precedence** — read
  `Window::theme()` at `Shell::open` and follow `WindowEvent::ThemeChanged`
  thereafter; a manual toggle *pins* and stops following until the window
  closes. And **do not call `Window::set_theme`**: accept a
  system-dark titlebar over a manually-pinned light editor as the price
  of the system-following default. It is the lesser of the two
  compromises, and it is reversible (a later ruling can trade following
  for a matched titlebar with a one-line change).

- **D-3 — What key and what command.** **A kernel *host* command,
  `view.toggleTheme`, in `builtin/host.rs`**, bound `Ctrl+Alt+T` in the
  kernel default keymap and `⌘⌥T` in the desktop's ⌘ layer. Both chords
  verified free (§1.4). **Recommendation: yes, host command.** A
  face-local id (the `file.save` pattern) would be the exact drift
  `H:1-13` was written to prevent, and a host command costs *zero* count
  churn — every relevant assertion is derived from `HOST.len()`, and the
  one hand-maintained number (`K:321`'s `41`) is untouched provided the
  command is bound. A second id (`view.useLightTheme` /
  `view.useDarkTheme`) is worth *not* adding: `history.togglePanel`'s
  doc (H:47-49) already argues that a toggle beats a pair when there is
  no state to abandon.

- **D-4 — Does light get its own chrome ruling?** **Recommendation:
  partial ruling, not a fresh one.** Light **inherits** all the geometry
  DESKTOP-CHROME-MAP ruled (radius, padding, anchors, row treatment,
  caps) and takes its **own value** for exactly three things that are
  black ink rather than theme colour: backdrop dim `0.45 → 0.16`, shadow
  `offset 16 / blur 48 / 0.55 → offset 3 / blur 2 / 0.34` (a hard
  era-shadow, free — the SDF already takes blur as a parameter), and
  hairline alpha `0.18 → 0.55` at 1 **logical** px instead of 1 physical.
  Rounded corners are not in question: they stay, per Tom's standing rule.

- **D-4b — Radius in light.** Platinum's arcs were 4-5 px at 1×, not 8.
  Drop light to 5 for era fidelity, or keep 8 for one chrome geometry
  across modes? **Recommendation: keep 8 in both**, and treat the number
  as Tom's to move by eye at the rebuild — the same latitude R3 gave him.
  Two different radii for two modes is a maintenance seam bought with
  taste, and the shadow and hairline changes already carry the era.

- **D-5 — `panel_border` as a real theme field.** DESKTOP-CHROME-MAP's
  R4 flagged it as a safe later addition (Th:70-75's forgiving
  deserialization makes it purely additive). The three variants genuinely
  disagree about the border — 0.55, 0.55, 0.85, and C wants a *stated*
  near-black, not a derived alpha. **Recommendation: add it now**, as a
  25th `EditorColors` field, and keep the backdrop dim and the shadow as
  face constants keyed on `is_dark`. A border is a colour a theme should
  state; a modal dim is ink over the page and no theme should have to
  own it.

- **D-6 — Does the TUI follow?** The TUI needs no plumbing (it rebuilds
  its palette from `editor.get_theme()` every frame, §1.7) and will
  therefore follow `view.toggleTheme` for free. The question is whether
  it *should*: since 6e22cbe the dark preset is opaque, so a light theme
  will repaint a dark terminal edge-to-edge light.
  **Recommendation: yes, follow, with no special deference** — the 4 Aug
  ruling already chose "state the pixels honestly" over "defer to the
  terminal", and applying it in one direction only would be incoherent.
  Two honest notes to record with the ruling: (a) at `Ansi16` all three
  variants' near-white greys collapse to one basic white
  (Cd:409-425), so on a 16-colour terminal a light theme's document,
  gutter and panels are one flat colour held apart only by reverse video
  and dim/bold — a tier limit, not a design failure; (b) if Tom wants the
  terminal to keep its own background under a light theme, that is a
  *different* ruling — set `background.a = 0` in the light preset — and
  it would break the desktop face (transparent clear colour → black
  window, the 6e22cbe bug), so it would have to be a TUI-only theme file
  rather than the shared preset.

- **D-7 — Desktop `--theme` flag, and persistence.** The desktop face has
  **no flags at all** (R:91-99), contradicting
  `DESKTOP-SHELL-PLAN.md`'s "`--theme` flag parity only". And nothing in
  the estate persists a preference — a toggle dies with the window.
  **Recommendation: add `--theme` to the desktop face** (reusing the
  TUI's `ThemeChoice` verbatim — it is already face-neutral, Tt:38-61 —
  which also gives the desktop the VS Code theme loader for free), and
  **do not add a config file in this change**: with D-2's
  system-following default the session already opens correctly for the
  common case, and inventing the estate's first preferences file is a
  larger decision than a light theme should smuggle in. Named here so
  it is Tom's call, not a silent deferral.

- **D-8 — Fix the two-palette divergence in this change?** Fact 2: the
  compositor's fallback keyword bridge keys on `is_dark` and ignores
  `theme.syntax` entirely (C:2019-2023, Sh:79-88), so any light theme is
  honest on grammar'd files and shows an unrelated Nord/Solarized palette
  on everything else. **Recommendation: yes, fix it here.** It is ~30
  lines (a `From<&theme::SyntaxColors>` mapping the eight token classes
  the bridge models), it is what makes *any* custom theme — not just this
  one — actually honest, and without it two of the six shots per variant
  in §4.1 will show colours Tom did not choose. Shipping a designed light
  theme on top of an undesigned fallback would be exactly the kind of
  half-landed work this repo's standards forbid.
