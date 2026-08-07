# AWL — where it stands

## ✅ LANDED — `6da7b89`, all eight gates green

2347 workspace · 1002 parser-free kernel lib · 1097 kernel+syntax lib · wasm
check · three clippy runs · `fmt --check`.

⚠️ **Two gates failed on the first battery run and the notification said
"exit code 0".** `clippy --workspace --all-features` and
`clippy -p iridium-editor --features syntax` both exited 101 on
`match_same_arms` — the new `namespace | module` arm duplicated the `type`
arm's body. Merged into one pattern, which is the resolution this file already
documents at the top of `from_capture_name`. **The background-task
notification's exit code has now been wrong twice in this stint. Read the
status file.**

Remaining: none for AWL itself. See #70 for the unstyled-capture gap the work
uncovered, and #63 step 5 for the seven grammarless languages.

---

*Everything below is the working record kept during the build.*

## Committed and green

- `6f58c72` — **the registry (#63 step 3)**. `Language` is a newtype over a
  generated table; the closed enum is gone. All eight gates verified green:
  2344 workspace tests, 1002 parser-free kernel, 1097 kernel+syntax, wasm
  check, three clippy runs, `fmt --check`.
- `327e29e` — Tom's three explorer rulings (#58 unblocked).

Design record: `docs/IN-FLIGHT-registry.md`.

## Uncommitted, in progress — AWL is vendored and *nearly* working

`cargo test -p iridium-syntax`: **103 passed, 1 failed.** The one failure is
the one predicted in `docs/IN-FLIGHT-awl-build.md` §4.

### The failure

`crates/iridium-syntax/src/query/tests.rs:118` —
`the_vendored_text_objects_carry_exactly_the_five_documented_captures`.

It iterates `Language::all()` and `.expect(...)`s that **every** language ships
a `textobjects.scm`. **AWL does not** — it ships only `highlights.scm`,
`indents.scm` and `folds.scm`. This is not a defect in the vendoring; it is a
test written when every language happened to ship one.

**Fix:** make it skip a language with no textobjects query, rather than
expecting one. Do *not* weaken what it checks for languages that do ship one —
the five captures (`@function.around/.inside`, `@class.around/.inside`,
`@comment.around`) are still the claim.

### What has been done (all uncommitted)

| file | change |
| --- | --- |
| `crates/iridium-syntax/grammars/awl/parser.c` | vendored, 266,189 B, from `github.com/ablative-io/aion` rev **`8d4d63e`**, ABI 15, **no scanner.c** |
| `crates/iridium-syntax/grammars/awl/tree_sitter/*.h` | `alloc.h`, `array.h`, `parser.h` |
| `crates/iridium-syntax/build.rs` | **NEW** — compiles `VENDORED = ["awl"]` with `cc`, checks for an optional `scanner.c` |
| `crates/iridium-syntax/Cargo.toml` | `tree-sitter-language.workspace` dep; `[build-dependencies] cc.workspace` |
| `Cargo.toml` (workspace) | added `cc = "1.2"`, `tree-sitter-language = "0.1"` — **both already in Cargo.lock**, so nothing new is downloaded |
| `crates/iridium-syntax/src/grammar.rs` | `const AWL: LanguageFn` via `unsafe extern "C" { fn tree_sitter_awl() }`, with the required `#[expect(unsafe_code, reason = …)]`; `"awl" => AWL.into()` arm; `"awl"` added to `GRAMMARS_LINKED_FOR` |
| `crates/iridium-lang/src/languages/queries/awl/` | `highlights.scm`, `indents.scm`, `folds.scm`, and a new `config.toml` |
| `crates/iridium-lang/src/languages/languages.txt` | `awl               Awl` added as the first row |
| `crates/iridium-lang/src/manifest/embedded.rs` | `("awl", include_str!(…))` added to `SOURCES` |

**The `unsafe_code` gate did not fire a warning** — the `#[expect]` was written
up front with a real justification, and clippy has not yet been run on this
tree.

## ✅ DONE since this file was first written

- **The textobjects test is fixed.** It now skips a language shipping no
  `textobjects.scm`, with a `shipped > 0` guard so the capture assertions can
  never pass vacuously. `cargo test -p iridium-syntax`: **104 passed, 0
  failed** — AWL's grammar compiles, links and parses.
- **All six sentinels moved from `"awl"` to `"nonesuch"`.** Verified none
  remain by grep.
- **`EXPECTED` gained its row**: `("awl", Some("//"), None)`.
- The module note on `comment_manifest_tests` now records *why* the sentinel
  moved — picking a plausible-looking name as a never-a-language sentinel is
  what caused this, and `"nonesuch"` is not a language anyone will add.

## ✅ DONE — the full workspace run, and what it caught

The first full-workspace run exited **101**, not 0. Two failures, both in
`iridium-lang/src/query/tests.rs`, both the same cause and both anticipated:
AWL ships only 2 of the 6 known query kinds, so `KNOWN_ABSENCES` gained four
rows (`Brackets`, `TextObjects`, `Injections`, `Outline`) and AWL was added to
`PINNED` from the other side. The test named "…the_three_that_were_never_
vendored" is now `the_only_absences_are_the_ones_written_down`.

⚠️ **The background-task notification reported "exit code 0" for that run.** It
was reading the trailing `echo`, not the test command. The status file said
`a1=101`. **Believe the status file, never the notification.**

### A correction to something recorded earlier

AWL's `folds.scm` was described as "a query kind Iridium has no concept for,
carried harmlessly". True, but not special: **nine** vendored `.scm` kinds have
no `QueryKind` — `overrides`, `embedding`, `runnables`, `imports`, `debugger`,
`redactions`, `structure`, `folds`, `contexts`. Carrying them is the
pre-existing norm for this vendored tree, not something AWL newly exercised.

## ✅ DONE — step 4, the capture mapping, and the real defect it found

Checking AWL's 20 capture names against `HighlightType::from_capture_name`
turned up **`@namespace`, which mapped to nothing**. An unmapped capture
produces no span at all, so the token renders in the plain foreground and
nothing logs, errors or reports it.

It was never an AWL problem: **`@namespace` is used by `awl`, `cpp`, `css` and
`go`**, and had been unstyled in all four.

**Fixed** by mapping `namespace | module → HighlightType::Type`, reusing
`syntax.type_name`. Not a new `HighlightType` variant, because `SyntaxColors`
is a fixed struct of concrete colours — a variant means a new field in every
theme *and* in `iridium-tui`'s palette. `Lifetime` already sets this precedent
in `highlight_to_color`.

**The sweep mattered more than the fix.** A new test,
`every_vendored_capture_maps_to_a_highlight_type`, checks every capture of
every compiled highlights query and found **14 unmapped names across 8
languages**. Four are correct (`_isinstance`, `_issubclass` and `none` are
predicate operands — verified individually against the `#eq?`/`#match?` calls
that use them — and `text` is markdown prose). **Nine are a real gap**:
markdown headings and links, *every* CSS selector, and JSX tags all reach the
screen unstyled. Filed as **#70**, listed in `KNOWN_UNSTYLED_GAP` so the gap is
ratcheted and cannot grow silently, with a companion test asserting both lists
name only captures that are still used *and* still unmapped — so neither list
can rot.

`cargo test -p iridium-syntax`: **107 passed, 0 failed** (was 104).

## The six sentinel tests — now done, kept for the record

`"awl"` was chosen in earlier work as the *never-a-language* sentinel,
precisely because it did not resolve. **It resolves now.** These will fail and
each needs a different non-language id (suggest `"nonesuch"`):

| file | test |
| --- | --- |
| `iridium-lang/src/suffix.rs` | `an_unclaimed_extension_resolves_to_nothing` — `language_for_extension("awl")` |
| `iridium-lang/src/suffix.rs` | `a_name_nothing_claims_resolves_to_nothing` — includes `"hello.awl"` |
| `iridium-lang/src/manifest/tests.rs` | `by_id_says_no_to_a_language_that_was_never_vendored` — `by_id("awl")` |
| `iridium-editor/…/comment_manifest_tests.rs` | `an_unknown_language_still_falls_back_to_the_configured_token` — `document_in("awl")` |
| `iridium-editor/…/comment_manifest_tests.rs` | `an_unknown_language_with_no_configured_token_has_no_comment_syntax` |
| `iridium-editor/…/comment_tests.rs` | `an_unrecognised_language_falls_back_to_config_token` — `doc_with_language("value", "awl")` |

Also `comment_manifest_tests::EXPECTED` asserts `EXPECTED.len() ==
Language::COUNT`, so it **gains a row**: `("awl", Some("//"), None)` — line
comment `//`, no block pair.

Note these did NOT fail in the `iridium-syntax` run above because they live in
other crates. **Run the full workspace before believing this is done.**

## Then

1. Fix the textobjects test to skip languages without one.
2. Fix the six sentinels + the `EXPECTED` row.
3. Full eight-gate battery.
4. Check the highlight capture mapping — AWL uses `@label`, `@namespace`,
   `@operator`, `@constant`, `@comment.documentation`, and Iridium maps
   captures to `HighlightType` **by prefix**. Verify none fall through to a
   default.
5. Install **only** via `apps/iridium-desktop/bundle/install.sh`. Iridium was
   not running at 06:55; **re-check with `pgrep -x` before swapping**, and
   never kill pids 99844 / 31161.

## Deferred, deliberately

**#63 step 5 — admitting the seven grammarless languages** (`diff`,
`gitcommit`, `gomod`, `gowork`, `jsdoc`, `markdown-inline`, `regex`). Tom ruled
it in, and it is *not* on the critical path to AWL. It is a restructure rather
than a line-add: those seven ship only 2–3 query kinds each, so
`KNOWN_ABSENCES` would grow by ~28 rows. The right move is to replace
`KNOWN_ABSENCES` **and** `PINNED` with one hand-written `SHIPS` table of
language → kinds shipped. Still hand-written, so it keeps the not-derived
property that makes it worth having.

`jsonc` stays an alias and is **not** among the seven — see
`docs/IN-FLIGHT-registry.md` §4.
