# The fallback keyword palette ignores the theme

**A ruling-free defect found inside #31, which is otherwise blocked on eight
of Tom's decisions. Verified in the code, not taken from the map.**

---

## What #31 actually is, corrected

I recorded #31 in `SESSION-STATE.md` as "unblocked, but carries Tom's UI
taste". **That was too generous and is now corrected.** `docs/design/LIGHT-THEME-MAP.md`
already carries the whole design — three finished candidate variants
(`theme/classic.rs`: platinum, paper, monochrome), transcribed field by field,
with a screenshot harness to render them — and §5 lists **D-1 through D-8**
awaiting a ruling. It is blocked exactly the way L-0..L-8 blocks the web track.

⚠️ So with #35 closed, **the unblocked column is empty**, not "#31".

The map also records a sequencing constraint — "this change must sequence
behind the context menu" — which **has since cleared**: the context menu is #28
and is closed. What has not cleared is D-1..D-8.

## The map is stale in one place, and it matters

Its load-bearing fact 1 says `theme::SyntaxColors::light()` "carries at least
two outright defects: `attribute` and `error` are the same colour, and
`operator`/`punctuation` are `#000000` while plain `foreground` is `#333333`".

**Both are already fixed** at `theme/colors.rs:262-285`: `attribute` is now
`001080` and `operator`/`punctuation` are `333333`, each with a comment saying
why. A citation chain is not evidence; only the code is.

## The defect that is still live, and has no ruling in it

Load-bearing fact 2 **is** true, and it is a correctness bug rather than a
matter of taste.

`FrameCompositor::set_theme` (`render/compositor/settings.rs:102-115`):

```rust
if theme.is_dark {
    self.highlighter.set_dark_theme();
} else {
    self.highlighter.set_light_theme();
}
self.theme = theme;
```

It selects the fallback keyword bridge's palette **on `theme.is_dark` alone and
never consults `theme.syntax`**. The bridge has its own type of the same name
— `render::simple_highlight::SyntaxColors` — whose two presets are an unrelated
Nord/Solarized hybrid hard-coded at `simple_highlight.rs:60-90`.

⭐ **Stated as the proxy it is:** `is_dark` stands in for *"which syntax palette
should the fallback paint with"*. It agrees with its target on exactly two
inputs — the two built-in themes, for which the hard-coded presets were
hand-matched. **Name where they diverge:** every other theme. A user's JSON
theme, any of the three classic-Mac candidates, or `Theme::dark()` with one
colour changed all set `theme.syntax` and get a fallback palette that ignores
it. Two light themes with completely different syntax colours are painted
identically on any document the bridge is holding.

### Which documents the bridge is holding

Not an edge case. The bridge paints whenever the face reports a language but no
spans have resolved — every document with no grammar, and every grammar'd
document in the window between opening it and its spans landing. So the same
file changes palette as it loads, and a plain-text file never uses the theme's
colours at all.

**This is wrong in the dark theme today**, not only in some future light one.
It is why it can be fixed without D-1.

## The change

`impl From<&Theme> for simple_highlight::SyntaxColors`, and `set_theme` uses it
instead of branching.

⚠️ **From `&Theme`, not from `&theme::SyntaxColors` as the map prices it.** The
bridge's palette carries a `text` field — the colour for unclassified tokens —
and `theme::SyntaxColors` has no such field. It has to come from
`theme.editor.foreground`, so the conversion needs the whole theme. Taking the
map's signature would have left `text` on a hard-coded default, which is the
same defect one field smaller.

Field mapping, 8 of the bridge's fields against 14 of the theme's:

| bridge | theme |
| --- | --- |
| `text` | `editor.foreground` |
| `keyword` | `syntax.keyword` |
| `string` | `syntax.string` |
| `number` | `syntax.number` |
| `comment` | `syntax.comment` |
| `type_name` | `syntax.type_name` |
| `function` | `syntax.function` |
| `punctuation` | `syntax.punctuation` |

The theme's other six (`variable`, `operator`, `property`, `constant`, `tag`,
`attribute`) have no bridge counterpart — the bridge's `TokenType` has eight
variants and classifies by keyword table and character class, so it cannot tell
a variable from a property. Dropping them is not a loss of information; there
is no token the bridge could paint with them.

## Proof

A pixel test in the existing `retained_shaping` harness, which already has
`ActiveLanguageNoSpans` — a source that reports a language and resolves no
spans, which is precisely the state that puts the bridge in charge.

Two themes identical except for `syntax.keyword`, each composed cold. Today
both are `is_dark`, both take `set_dark_theme()`, and the frames are
byte-identical: red. The oracle is that they must differ.
