# AWL — the implementation plan, ground verified 7 Aug 2026

**Task #68.** Tom asked to see AWL highlighting today. Answered him honestly
that it is not wired in yet, flagged the trade below, and he has not objected.

## ⛔ RULED 7 Aug 06:57 — TOM SAID NO TO THE HARD-CODED VARIANT

> *"Yeah, look I really don't want I really don't want like a hard coated
> barrage so yeah if you can. Yeah, if you can get get get to work on all of
> that."*

He was offered the fast path — AWL as a 14th enum variant, visible today,
deleted later — and **turned it down**. So:

- **Step 4 below (the `Language::Awl` enum variant) is CANCELLED.**
- **#63, the registry, comes first.** `Language` stops being a closed enum,
  `COMPILED` stops being a fixed array indexed by `Language::index()`, and
  `query/embedded.rs` stops being an exhaustive match. AWL then falls out of it
  as data rather than as a variant.
- Everything else in this file **still stands and is still the plan** — the
  vendoring, the `cc` build step, the manifest, the queries, the capture check,
  the install. None of that was contingent on the enum.
- Sequence: registry (#63) → then steps 1, 2, 3, 5–8 here.

He also asked, in the same message: **"What about our tree component? Do we
have that ready to go?"** — the `iridium-tree` crate. Unanswered; check its
state and report. It is a separate question from AWL.

Ground in `docs/IN-FLIGHT-awl.md`. This file is the *how*.

---

## 1. The source is already on the box

`/Users/tom/Developer/ablative/extensions/tree-sitter-awl`, git rev **`8d4d63e`**
— exactly the mirror pin Vesper named, so it is the gate-protected copy.

```
src/parser.c              266,189 B, #define LANGUAGE_VERSION 15
src/tree_sitter/          alloc.h  array.h  parser.h
src/grammar.json, node-types.json
queries/highlights.scm  indents.scm  folds.scm     <- NO textobjects, NO brackets
awl.wasm                  24 KB, prebuilt — not needed for the native path
```

**No `scanner.c`.** ABI 15 loads on this workspace's tree-sitter 0.26.3. There
is no published AWL crate, so the `links = "tree-sitter"` collision cannot arise.

## 2. The C build step — copy chiron, do not invent

`dev-ops/chiron/crates/syntax/build.rs` already does exactly this. Its shape,
verbatim in substance:

```rust
fn compile_grammar(name: &str, dir: &str) {
    let src_dir = std::path::Path::new(dir);
    let mut c_config = cc::Build::new();
    c_config
        .std("c11")
        .include(src_dir)
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable");
    let parser_path = src_dir.join("parser.c");
    c_config.file(&parser_path);
    println!("cargo:rerun-if-changed={}", parser_path.display());
    // scanner.c only if it exists — AWL has none
    c_config.compile(name);
}
```

- Build dep: **`cc = "1.2"`** in `crates/iridium-syntax/Cargo.toml`.
- Chiron's grammar dir layout is `grammars/<name>/{parser.c, tree_sitter/}`.
  Mirror it: **`crates/iridium-syntax/grammars/awl/`**. C source belongs with
  the crate that compiles it, not with `iridium-lang`, which must stay
  parser-free.

Declaring it to Rust (chiron `crates/syntax/src/language.rs:13-35`):

```rust
use tree_sitter_language::LanguageFn;
unsafe extern "C" {
    fn tree_sitter_awl() -> *const ();
}
pub const AWL: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_awl) };
```

### ⚠️ `unsafe_code = "warn"` and the clippy gate is `-D warnings`

Workspace `Cargo.toml:139`. The extern block **will fail the gate** unless it
carries an explicit `#[expect(unsafe_code, reason = "...")]` with a real
justification — FFI to a statically linked vendored grammar, the only way to
reach a C symbol, with the ABI version asserted by the parser test.

Check whether **`tree-sitter-language`** is already a resolvable dependency; the
published grammar crates pull it, but `iridium-syntax` may not name it directly.
If not, add it.

## 3. Files to change, in order

1. **Vendor** `parser.c` + `src/tree_sitter/*.h` → `crates/iridium-syntax/grammars/awl/`.
2. **Queries + manifest** → `crates/iridium-lang/src/languages/queries/awl/`:
   `highlights.scm`, `indents.scm`, `folds.scm`, and a new `config.toml`:
   ```toml
   name = "AWL"
   grammar = "awl"
   path_suffixes = ["awl"]
   line_comments = ["// "]
   ```
   No `block_comment`, no `documentation_comment` — AWL has no block pair, so
   the derivation rules already tested give `block_comment() == None` correctly.
3. **`crates/iridium-syntax/build.rs`** (new) + `cc` build-dep.
4. **`Language::Awl`**: `COUNT` 13 → **14**, plus arms in `id()` (`"awl"`),
   `from_id()`, `all()`, `index()` (→ 13).
5. **`grammar.rs`**: an `Language::Awl => AWL.into()` arm.
6. **`iridium-lang/src/manifest/embedded.rs`**: add `("awl", include_str!(...))`
   to `SOURCES`.
7. **`iridium-lang/src/query/embedded.rs`**: AWL arms. **Highlights and Indents
   present; TextObjects, Brackets, Injections, Outline → `None`.** This is the
   exhaustive `(Language, QueryKind)` match with no wildcard, so it must be
   spelled out. AWL also ships a `folds.scm` there is **no `QueryKind` for** —
   leave it vendored and unread, and note it; naming a seventh kind is #63's
   business.
8. **`iridium-lang/src/query/tests.rs`**: `KNOWN_ABSENCES` gains **four** AWL
   rows. `every_language_ships_the_highlights_query_the_highlighter_requires`
   still passes (AWL has highlights).

## 4. ⚠️ Tests that use `"awl"` as the *unknown language* sentinel

These were written when AWL was guaranteed not to resolve. **They will break
the moment AWL exists**, and each needs a different never-a-language id
(suggest `"nonesuch"`):

| file | test |
| --- | --- |
| `iridium-lang/src/suffix.rs` | `an_unclaimed_extension_resolves_to_nothing` — asserts `language_for_extension("awl") == None` |
| `iridium-lang/src/suffix.rs` | `a_name_nothing_claims_resolves_to_nothing` — includes `"hello.awl"` |
| `iridium-lang/src/manifest/tests.rs` | `by_id_says_no_to_a_language_that_was_never_vendored` — asserts `by_id("awl").is_none()` |
| `iridium-editor/.../comment_manifest_tests.rs` | `an_unknown_language_still_falls_back_to_the_configured_token` — uses `document_in("awl")` |
| `iridium-editor/.../comment_manifest_tests.rs` | `an_unknown_language_with_no_configured_token_has_no_comment_syntax` — same |
| `iridium-editor/.../comment_tests.rs` | `an_unrecognised_language_falls_back_to_config_token` — `doc_with_language("value", "awl")` |

Also: `comment_manifest_tests::EXPECTED` and
`the_table_covers_every_language_exactly_once` assert `EXPECTED.len() ==
Language::COUNT`, so **`EXPECTED` gains an `("awl", Some("//"), None)` row**.

And `iridium-syntax`'s
`the_vendored_text_objects_carry_exactly_the_five_documented_captures` iterates
`Language::all()` and expects **every** language to ship textobjects — AWL does
not. **That test will panic on `.expect("every language ships textobjects")`**
and must learn to skip languages with no textobjects query.

## 5. Highlight captures AWL uses — check the mapping

```
@comment  @comment.documentation  @comment.documentation.workflow
@constant  @constant.builtin  @function  @function.builtin  @function.method
@keyword  @label  @namespace  @number  @number.float  @operator
@string  @string.special  @type  @type.builtin  @type.definition
@variable.parameter
```

Iridium maps captures to `HighlightType` by **prefix** (`name.starts_with(...)`
— that is how `property.json_key` became `Property`). Verify `@label`,
`@namespace`, `@operator` and `@constant` all land somewhere sensible rather
than falling through to a default. `@comment.documentation` should reach
`Comment`.

**Preserve:** AWL's `///` and `//!` are **distinct grammar tokens**
(`doc_comment` vs `comment`), not a naming convention — the opposite of Rust,
whose manifest lists all three as line-comment tokens. The highlights query
already splits them.

## 6. Install

**Only** `apps/iridium-desktop/bundle/install.sh` — never a hand-rolled
sequence. Iridium was **not running** at 06:55 (checked with `pgrep -x`), so a
swap is safe; **re-check before swapping**, and never kill pids 99844 / 31161.

## 7. Standing caveat to carry into the manifest

The AWL tree-sitter grammar is a **presentation layer only**. `crates/aion-awl`
in the aion repo is the sole parser used for diagnostics. Never treat a
successful grammar parse as "valid AWL".
