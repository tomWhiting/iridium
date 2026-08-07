# #63 step 5 — admitting the grammarless languages

**Live state. Written as the work proceeds.**

## The ruling, and where I departed from it

Tom ruled in "the seven grammarless languages" — `diff`, `gitcommit`, `gomod`,
`gowork`, `jsdoc`, `markdown-inline`, `regex` — on my own recommendation.

**Four are being admitted. Three are not, and Tom has been told why.** The
recommendation changed because two facts turned up that were not checked when
it was made:

1. `jsdoc`, `markdown-inline` and `regex` declare **no `path_suffixes` at
   all**. No file can ever open as one.
2. **`QueryKind::Injections` has no consumer outside tests** — verified by
   grep across `crates/` and `apps/`. Iridium does not process injections.

Those three exist *only* to be injected: jsdoc into a comment, regex into a
string literal, markdown-inline into a markdown block. With no suffix, no
grammar and no injection processing, listing them would add three `Language`
values that nothing can produce and nothing can consume — present in
`Language::all()`, in `COUNT`, in every test table, and in any language picker.

They go in the same commit that implements injections. That is one line each,
which is the point of the registry.

## What the four buy, with no grammar linked

| language | claims | `Ctrl+/` |
| --- | --- | --- |
| `diff` | `.diff`, `.patch` | — (no `line_comments`) |
| `gitcommit` | `COMMIT_EDITMSG`, `MERGE_MSG`, `TAG_EDITMSG`, `NOTES_EDITMSG`, `EDIT_DESCRIPTION` | `#` |
| `gomod` | `go.mod` | `//` |
| `gowork` | `go.work` | `//` |

⚠️ **This paragraph originally read "Plus brackets and auto-pairs from each
manifest."** That was banked and never delivered — checked 8 Aug: the manifest
reader deserializes seven keys and `brackets` is not among them, so the bracket
tables `gitcommit`, `gomod` and `gowork` carry are on disk and unread, exactly
like the rest. The editor's auto-pair table is six hard-coded characters, the
same in every language. `docs/design/AUTO-PAIR-MAP.md` has the ground, the
seven divergences and the slices.

None of them highlights, and that is the expected state — `grammar()` returning
`None` is exactly what the registry work made legal, and `query::compile`
already resolves a grammarless language to the same routine `Absent` as a
language shipping no `.scm`.

### The suffix hazard, checked rather than assumed

`gomod` claims the bare suffix `mod`, and Rust has `mod.rs`. **Safe**:
`entry_claims` (`suffix.rs:130`) matches only on an exact file name or on the
entry preceded by a literal `.`, so `mod` claims `go.mod` and a file named
exactly `mod`, never `mod.rs`. `no_extension_names_two_languages` guards this
independently, and it passes.

## The table restructure

`KNOWN_ABSENCES` and `PINNED` are **replaced by one `SHIPS` table** of
language → kinds shipped, in `iridium-lang/src/query/tests.rs`.

Two tables describing one fact is one table too many, and the split had a hole:
a language could lose one file and gain another and the absence *count* would
still balance. With the four admitted, `KNOWN_ABSENCES` would also have grown
to 23 rows.

`SHIPS` stays hand-written. Every other test in that file compares the
generated table against the directory it was generated from — both sides read
the same source, so they catch a mangled table but never a file that quietly
vanished in a vendor refresh. `SHIPS` is the third opinion.

Four tests over it, each with a different oracle:

- `every_language_ships_exactly_what_the_table_says` — walks `SHIPS`.
- `the_absences_are_exactly_the_ones_the_table_accounts_for` — walks the
  *registry*, so a language missing from `SHIPS` entirely is caught. Not
  redundant with the above; they fail on different mistakes.
- `the_table_names_every_language_exactly_once` — count and coverage.
- `the_table_agrees_with_the_files_on_disk` — unchanged, compares to disk.

## What ships, measured off disk

```
awl         highlights indents
bash        highlights brackets textobjects indents injections
c/cpp/css/go/javascript/markdown/python/rust/tsx/typescript   all six
diff        highlights injections
gitcommit   highlights injections
gomod       highlights injections
gowork      highlights injections
json        highlights brackets textobjects indents outline
yaml        highlights brackets textobjects injections outline
```

## ⚠️ `cargo test` is fail-fast at the TARGET level

The first workspace run reported **one** failure and looked nearly done. It had
never reached `iridium-syntax` at all — `iridium-editor` failed and cargo
stopped. Re-running with `--no-fail-fast` showed **four**, three of them in the
crate that never ran.

**`--no-fail-fast` is now in the battery script** for all three test gates.
Without it, a run that stops early reads exactly like a run that got further
than it did.

## Everything that had to change, and why each was wrong

Every one is the same class: a test written when every language necessarily had
a grammar, whose claim quietly became false rather than obviously so.

| test | was | now |
| --- | --- | --- |
| `comment_manifest_tests::the_table_covers_every_language_exactly_once` | `EXPECTED.len() == COUNT` | four rows added |
| `every_language_comments_the_way_the_table_says` | resolved every row | branches on the all-`None` row — `diff` has no comment syntax in either direction, the first such language, and `resolved()` panics on exactly that |
| `query::tests::every_language_ships_the_highlights_query…` | every language compiles highlights to `Some` | scoped to languages with a grammar, plus a vacuity guard |
| `query::tests::every_embedded_query_compiles_against_its_grammar` | `Ok(None)` implied no source | `Ok(None)` implies no source **or** no grammar; still rejects a language with both that got nothing |
| `tree::tests::every_language_can_own_a_tree` | every language owns a tree | scoped to grammars, **plus a new test** that a grammarless language is *refused* with an error naming it, rather than handed a tree that silently parses to nothing |
| `textobject_tests::every_language_answers_every_combination…` | `expect("every language has a grammar")` | skips a language that cannot own a tree |

Two new tests were added rather than only relaxing old ones, because relaxing
alone would have left the grammarless path untested:

- `a_grammarless_language_is_refused_a_tree_rather_than_given_an_empty_one`
- `a_language_with_no_grammar_reports_absence_rather_than_an_error`

Both carry vacuity guards, as do the two scoped tests. Four skips were
introduced here; every one of them can fail if it ever skips everything.

## Status

- `cargo test -p iridium-lang`: **61 passed, 0 failed.**
- `cargo test -p iridium-syntax`: **110 passed, 0 failed.**
- Eight-gate battery in flight.
