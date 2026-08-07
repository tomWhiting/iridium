# #80 — the search path asked a byte what only a character can answer

Found 8 Aug 2026. Two defects, one root, both proven red before the fix.

## The route in

`motions::is_word_char` carries this sentence:

> This is the single word-character definition shared by word motions,
> word-wise deletion, and the auto-pair quote suppression.

A claim of uniqueness is checkable, and it was false. Two other definitions
exist:

| where | shape | agrees? |
| --- | --- | --- |
| `input/mouse.rs:600` | `\|c: char\| c.is_alphanumeric() \|\| c == '_'` | yes — a copy, free to drift |
| `search/find.rs:355` | `const fn(byte: u8) -> byte.is_ascii_alphanumeric() \|\| byte == b'_'` | **no** |

The third is not a copy. It answers a different question — *is this byte an
ASCII word byte* — and stands in for *is this character a word character*. The
two agree for exactly as long as the text is ASCII.

⭐ Same shape as #77 and #78: **a proxy that agrees with its target on the
examined set.** The examined set here was every test in the file, all ASCII.

## A — the panic

`find_literal_matches`, case-sensitive branch, `find.rs:229`:

```rust
let mut start = 0;
while let Some(offset) = text[start..].find(query) {
    …
    start = match_start + 1;   // ⚠️ one BYTE
}
```

`start` indexes `text` directly on the next iteration. When the match begins on
a multi-byte character, `match_start + 1` is inside it and the slice panics:

```
thread '…' panicked at crates/iridium-editor/src/search/find.rs:230:42:
start byte index 1 is not a char boundary; it is inside 'é' (bytes 0..2 of string)
```

Case-sensitive search for `é` in `ééé` crashed the editor. Not an edge: any
accented, Cyrillic, Greek or CJK query with case-sensitivity on.

⚠️ **The sharpest part of this one.** Twelve lines below, the case-insensitive
path already advances correctly, *with a comment explaining why*:

```rust
// Advance to the next character boundary after the match start so
// overlapping matches are found and `find_at` stays boundary-aligned.
search_from = match_start + text[match_start..].chars().next().map_or(1, char::len_utf8);
```

Someone met this exact hazard, understood it, wrote it down — and hardened the
branch they were looking at. The branch above it kept the `+ 1`. **A fix
applied to the path you are standing on is not a fix to the defect.** When a
hazard is found, the question to ask is *where else does this shape appear*,
and the answer is often three lines away.

The fix is the same step, so the two paths now read identically. On ASCII
`len_utf8` is 1, so the overlapping-match semantics are unchanged —
`find_case_insensitive_overlapping_matches_preserved` and
`find_all_case_sensitive` both still pass untouched.

## B — the over-match

`is_word_boundary` read one byte on each side:

```rust
let prev_char = bytes.get(start - 1).copied().unwrap_or(b' ');
!Self::is_word_char(prev_char)
```

Every byte of a non-ASCII UTF-8 character is `>= 0x80`, so no non-ASCII
neighbour is ever an ASCII alphanumeric, and every one of them read as a word
*boundary*. Whole-word search for `na` reported a match inside `naïve`.

The error is **one-directional**: over-matching, never under-matching (no byte
of a multi-byte character can be ASCII alphanumeric, so the reverse is
unreachable). That is why it survived — it never hid a result the user was
looking for, it only offered ones that were not there. A defect that only ever
gives you *more* is the kind nobody reports.

`replace.rs:152` re-applies the same boundary check before expanding a regex
replacement, so replace-all inherited it and would write into the middle of a
word. In a legal or financial document that is silent corruption, which is why
the replace path gets its own test rather than trusting the shared function.

## The fix

```rust
pub(super) fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let before = text.get(..start).and_then(|head| head.chars().next_back());
    let after = text.get(end..).and_then(|tail| tail.chars().next());

    !before.is_some_and(motions::is_word_char) && !after.is_some_and(motions::is_word_char)
}
```

`get` rather than indexing: an offset that cannot slice `text` yields no
neighbour and reads as a boundary, which is what `unwrap_or(b' ')` did with the
same inputs. Every caller passes offsets that came out of a matcher, so the
case is unreachable rather than merely tolerated — but the function is
`pub(super)` and takes raw offsets, and a panic is not an acceptable answer to
a bad one.

The mouse handler's private closure is now `motions::is_word_char` too. That is
not tidying: it is what makes the sentence in `motions.rs` true, and the
sentence is what led here. A definition that claims to be the only one has to
be the only one, or the claim is worse than no claim at all — it stops the next
reader looking.

## The red proof

```
find_case_sensitive_multibyte_query_resumes_on_a_character_boundary   … panicked (the slice)
whole_word_reads_a_non_ascii_neighbour_as_part_of_the_word            … left: 1, right: 0
```

Plus `replace_all_whole_word_spares_a_word_that_only_looks_standalone_in_bytes`,
which pins both directions in one document: `"naïve na"` → `"naïve NA"`.

## Gates

All nine green. **2,533 passed, 0 failed** (2,530 before; three new tests).

## What this did not touch

- **`wasm.rs:2259 char_class`** — the web face hand-rolls a three-class
  (whitespace / word / punctuation) predicate for word motion. It is
  character-wise and its word class is `is_alphanumeric() || '_'`, so it
  *agrees* with `motions::is_word_char` today. It is a fourth definition of the
  same idea living in a different crate and a different language's face, and it
  is exactly the shape #38 closed for `pixel_to_index`. Worth its own item;
  not a defect today.
- **`search/find.rs` is 783 lines and `search/replace.rs` 878**, both against
  the 500-line bar. Tests are inline in each. Noted, not acted on.
