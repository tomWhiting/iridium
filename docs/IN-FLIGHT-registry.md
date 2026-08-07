# The language registry — design map, ground verified 7 Aug 2026

**Task #63.** Tom ruled out the shortcut on 7 Aug: *"I really don't want like
a hard coated barrage."* AWL becomes data, not a fourteenth enum variant, and
this is the change that makes that possible.

This is the *how*. The *why* and the tier analysis are in
`docs/IN-FLIGHT-languages.md`; the AWL specifics are in
`docs/IN-FLIGHT-awl-build.md`.

**Tier 1**, per L-0: nothing hard-coded, rebuild to add a language. No new
dependency, no wasmtime, no 6.6 MiB. Tier 2 (drop a folder in, *no* rebuild)
stays a separate decision, and this work is its unavoidable groundwork either
way.

---

## 1. What is actually closed — counted, not guessed

The closed-enum surface is far smaller than the enum's 200-odd usages suggest.
Almost every use is `Language::Rust` in a test, which a registry does not
disturb. The load-bearing sites are **five**:

| # | site | what it assumes |
| --- | --- | --- |
| 1 | `iridium-lang/src/lib.rs` | `Language` is an enum of 13 variants; `COUNT = 13`; `all()` returns `&'static [Self; 13]`; `index()` is a match |
| 2 | `iridium-lang/src/query/embedded.rs` | exhaustive match over `(Language, QueryKind)` — 78 pairings, **no wildcard arm** |
| 3 | `iridium-syntax/src/grammar.rs` | exhaustive match `Language -> tree_sitter::Language`, **total**, cannot return `None` |
| 4 | `iridium-syntax/src/query/mod.rs` | `static COMPILED: [[OnceLock<Compiled>; KIND_COUNT]; Language::COUNT]` — a **const-sized static array** |
| 5 | `iridium-syntax/src/folding/language.rs` | `const fn for_language` — an exhaustive match returning foldable node kinds |

Only **two** exhaustive matches on `Language` exist outside `iridium-lang`
(sites 3 and 5). Verified:

```
$ grep -rn "match language" --include="*.rs" crates/ apps/ | grep -v test
crates/iridium-syntax/src/grammar.rs:22
crates/iridium-syntax/src/folding/language.rs:22
```

`Language::COUNT` has exactly two consumers outside the crate — site 4 and one
test. That is the whole job.

## 2. The shape: `Language` becomes an index into a static registry

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Language(u16);
```

A `u16` index into `&'static [LanguageDef]`, where a `LanguageDef` carries the
id, the parsed manifest, and the six query sources. `Copy`, `Eq` and `Hash`
survive unchanged, so the ~200 call sites that merely *hold* a `Language` do
not care.

What changes on the type:

| today | after |
| --- | --- |
| `COUNT: usize = 13` | `COUNT: usize = LANGUAGE_IDS.len()` — **still a `const`** |
| `all() -> &'static [Self; 13]` | `all() -> &'static [Self]` — a slice, not an iterator |
| `index() -> usize` (a match) | `index() -> usize` (the `u16`) — unchanged contract |
| `id() -> &'static str` (a match) | an index into `LANGUAGE_IDS` |
| `from_id` (a match with aliases) | a scan of `LANGUAGE_IDS`, plus the alias table |

### `COUNT` stays a constant, which deletes half the work

The first draft of this map had `COUNT` becoming `fn count() -> usize`, and
site 4's `COMPILED` array therefore becoming a runtime-sized
`LazyLock<Box<[…]>>`. **Both were unnecessary.** Tier 1 means *rebuild to add
a language*, so the list is still fully known at compile time — it has simply
moved from an enum to a generated table. `LANGUAGE_IDS.len()` is
const-evaluable, so:

```rust
pub const COUNT: usize = LANGUAGE_IDS.len();
```

**Site 4 therefore needs no change whatsoever.** `COMPILED` stays a plain
const-sized static array of `OnceLock`s, indexed on the read path with no lock
and no allocation. The performance characteristic that made it worth writing
that way is preserved exactly.

Returning `&'static [Self]` from `all()` rather than an iterator matters for
the same reason: every existing `for &language in Language::all()` and
`.iter()` call keeps compiling. What is lost is the array type's structural
guarantee that `all()` has exactly `COUNT` entries, which becomes a test —
both are generated from one list, so it catches a `build.rs` defect rather
than a human one.

### `Debug` must be written, not derived

`#[derive(Debug)]` on a newtype prints `Language(17)`. Test failure messages
throughout use `{language:?}` and would become unreadable, so `Debug` is
implemented to print the id.

`index()` keeping its meaning is what lets site 4 stay an indexed cache rather
than becoming a `HashMap` on a hot path.

### The 200 call sites do not have to change

`Language::Rust` appears ~200 times, nearly all in tests. Rewriting them is
the bulk of the risk in this change, and it is avoidable: associated constants
can carry the *same spelling* as the variants they replace.

```rust
#[expect(non_upper_case_globals, reason = "these replace enum variants of the same name")]
impl Language {
    pub const Rust: Self = Self(0);
    // ...
}
```

`Language::Rust` then still compiles, unedited, everywhere — **including in
patterns**, because a newtype over `u16` with derived `PartialEq`/`Eq` is
structural-match, so constants are legal in `match` arms. What changes is only
that such a match now needs a wildcard arm, which is precisely the change
sites 3 and 5 need anyway.

**Where the indices come from.** Writing `Self(0)` by hand would be the
hard-coding this work removes, so the constants are generated. That requires
`build.rs` to know the language list, which today lives in Rust
(`manifest/embedded.rs::SOURCES`) where a build script cannot read it. So the
list moves to a data file beside the vendored tree, hand-edited and
diff-visible — preserving the property §5 protects — and `build.rs` emits the
ids, the constants and the query table from that one source.

### Serde stays wire-compatible, and this must be tested

Today `#[serde(rename_all = "lowercase")]` on the enum produces exactly what
`id()` returns for all thirteen — `Tsx` → `"tsx"`, `Cpp` → `"cpp"`,
`JavaScript` → `"javascript"`. So a hand-written `Serialize` emitting `id()`
and a `Deserialize` going through `from_id` is **byte-identical on the wire**,
which matters because a `Language` is serialized into saved configuration and
crosses into TypeScript.

An unknown id must keep *failing* to deserialize rather than being preserved
as an opaque string. Preserving it would let a configuration name a language
that does not exist and have that survive a round trip.

## 3. `grammar()` becomes fallible — the one real semantic change

Site 3 is the only thing here that genuinely cannot be data: a statically
linked grammar is a C symbol, and a symbol either got linked or it did not.
So the registry has entries whose grammar is not present, and:

```rust
pub fn grammar(language: Language) -> Option<tree_sitter::Language>
```

That signature change is the heart of #63. It admits the state the current
type system forbids: **a language Iridium knows about but cannot parse.** That
is not a defect, it is the whole point — it is what lets a manifest, its
comment tokens and its file associations be useful before, or without, a
grammar being linked.

Every caller of `grammar()` must then degrade rather than fail. The one that
matters is `query::compile` (site 4), which today calls `grammar(language)`
unconditionally; with no grammar there is nothing to compile a query against,
so the slot resolves to `Compiled::Absent` — the same routine absence it
already models for a language that ships no `.scm` of that kind. **No new
error variant is needed**, which is the sign the model was already right.

## 4. ⚠️ The `jsonc` trap — a live regression if this is done naively

Every vendored manifest names its own grammar. Checked:

```
jsonc/config.toml:  grammar = "jsonc"
diff/config.toml:   grammar = "diff"
gomod/config.toml:  grammar = "gomod"
```

Iridium links **none** of those. Today `.jsonc` still highlights, because
`suffix.rs` carries a hand-written `MANIFEST_ALIASES` entry mapping it onto
`Language::Json` — the resolution of #61, and a decision Tom took.

Under a naive registry `.jsonc` resolves to a *jsonc* language whose declared
grammar is absent, and **highlighting silently stops**. It would look like the
file simply was not supported, and nothing would fail.

### The resolution: keep the alias, do not admit `jsonc` as a language

The first answer here was "add a borrow-another-language's-grammar field to
the registry". Comparing the two manifests directly shows that is machinery
for nothing:

| | `json` | `jsonc` |
| --- | --- | --- |
| `line_comments` | `["// "]` | `["// "]` |
| `brackets` | 4 rows, identical | 4 rows, identical |
| `tab_size` | 2 | 2 |

Everything Iridium reads from a manifest is **the same in both**. So admitting
`jsonc` as its own language gains no observable behaviour, and costs
highlighting unless a borrow mechanism is built to carry it — machinery whose
only user would be a language indistinguishable from the one it borrows from.

Keeping `MANIFEST_ALIASES` exactly as it is preserves all of it: `.jsonc`,
`tsconfig.json`, `bun.lock`, `devcontainer.json` and `pyrightconfig.json`
continue to resolve to `Language::Json`, highlight through the JSON grammar,
and comment with `//`. That is #61's and #62's rulings unchanged, which is the
point — those were Tom's calls and this work has no business quietly revising
them.

**So R-1 admits seven, not eight.** `jsonc` stays an alias. A test must still
pin that `.jsonc` and `tsconfig.json` resolve to JSON, because the failure
mode is silent: nothing errors, the file just stops highlighting.

## 5. What generates the registry — scan the *files*, hand-list the *languages*

The first draft of this section said "scan the directory" and was wrong, in a
way worth recording because the argument against it is already written down
in this repo. `manifest/embedded.rs` names its 21 `include_str!` entries by
hand and says why:

> Named one by one rather than globbed: a build script that walked the
> directory would make the set of languages depend on what happens to be on
> disk, and a vendor refresh that dropped a directory would then remove a
> language without any diff saying so.

That reasoning is sound and a wholesale scan reverses it. But a hand table is
also genuinely unworkable for the *queries*: 21 languages × 6 kinds is 126
entries, and `include_str!` is a compile error on a missing file, so "does
this language ship `brackets.scm`?" cannot be probed from inside a macro. It
has to be answered by something that looks at the directory.

**The two questions are different and get different answers:**

| question | answered by | why |
| --- | --- | --- |
| *Which languages exist?* | the hand-written list, as today | a language appearing or vanishing must show up in a diff |
| *Which `.scm` files does each ship?* | a `build.rs` scan | 126 entries, and the answer is a fact about the directory |

So `build.rs` scans `src/languages/queries/` and emits **only the query
table**, keyed by directory name, with `cargo:rerun-if-changed` on the tree.
The language set stays exactly where it is. It runs on the host, so the wasm
target is unaffected.

Adding AWL is then: drop the directory in, add **one line** to the language
list. That is data, not a barrage — and it is what tier 1 means by "rebuild
to add a language". The thing Tom objected to was a new enum variant plus
five match arms plus six query arms plus a count bump, which is a different
animal entirely.

**The residual cost, named:** a language listed but whose directory has lost
its files degrades to "ships no queries" rather than failing to build.
**What catches it** is the discipline already used for the comment table in
#66: a **hand-written expectation table**, deliberately *not* derived from the
scan, asserting that specific languages ship specific kinds — Rust all six,
YAML no `indents.scm`, JSON no `injections.scm`, Bash no `outline.scm`. A
derived table would agree with any scan, including a broken one.

## 6. The grammarless languages — admit seven of the eight

`diff`, `gitcommit`, `gomod`, `gowork`, `jsdoc`, `markdown-inline` and `regex`
are vendored, carry manifests, and are unreachable today because there is no
`Language` variant for them. (`jsonc` is the eighth and stays an alias — §4.)

Once `grammar()` is fallible they cost nothing to admit, and they bring real
behaviour with them: `.diff`/`.patch`, `go.mod`, `go.work` and git commit
messages get file association and correct comment tokens. Comment toggling
keys off the manifest, not the grammar.

⚠️ **This sentence also claimed indent rules and auto-pairs.** Corrected
8 Aug: neither is manifest-driven. The manifests declare `brackets` and
`autoclose_before`; the reader deserializes neither. See
`docs/design/AUTO-PAIR-MAP.md`.

**No file association collides.** Computed across all 21 manifests rather than
eyeballed — not one `path_suffixes` entry is claimed by two languages, so
admitting the seven introduces no ambiguity for `suffix.rs` to resolve:

```
diff:            diff, patch
gitcommit:       COMMIT_EDITMSG, MERGE_MSG, TAG_EDITMSG, NOTES_EDITMSG, EDIT_DESCRIPTION
gomod:           mod
gowork:          work
jsdoc / markdown-inline / regex:  none — they are injected, never chosen
```

The one worth reasoning through is `gomod`'s bare `mod`, because Rust is full
of files called `mod.rs`. It is safe under `entry_claims`' rule: `mod.rs`
neither equals `mod` nor ends with `.mod`, so it never reaches gomod, while
`go.mod` ends with `.mod` and does. The rule's leading dot is doing the work,
which is what it was written for.

Three of them (`jsdoc`, `markdown-inline`, `regex`) set `hidden = true` — they
exist to be injected into another language, not chosen. The manifest schema
already reads that field and a test already pins that exactly three are
hidden. They belong in the registry but must not appear in a language picker.

## 7. File-by-file

1. **`iridium-lang/build.rs`** (new) — scan, emit the table, `rerun-if-changed`.
2. **`iridium-lang/src/registry.rs`** (new) — `LanguageDef`, the static slice,
   lookup by id and by index, the borrowed-grammar field.
3. **`iridium-lang/src/lib.rs`** — `Language` becomes the newtype; `count()`,
   `all()`, `id()`, `from_id()`, `index()` re-expressed against the registry;
   hand-written `Serialize`/`Deserialize`.
4. **`iridium-lang/src/query/embedded.rs`** — the 78-arm match becomes a table
   read. The module doc explaining why the match is exhaustive is replaced by
   the doc explaining what the scan gives up and what pins it instead.
5. **`iridium-lang/src/suffix.rs`** — `MANIFEST_ALIASES` moves onto the
   registry as the borrow field.
6. **`iridium-syntax/src/grammar.rs`** — match on `id()`, return `Option`.
7. **`iridium-syntax/src/query/mod.rs`** — **no change.** `Language::COUNT`
   stays a `const`, so the cache stays a const-sized array. See §2.
8. **`iridium-syntax/src/folding/language.rs`** — `for_language` loses `const`
   and matches on `id()`. (L-6 asks whether folds should come from the
   queries instead; **out of scope here**, filed, not done.)
9. **`iridium-bindings/src/lib.rs:303`** — `Language::all()` now yields values,
   not references.
10. **Tests** — `Language::COUNT` → `count()`; the `EXPECTED` comment table
    grows from 13 to 21 rows.

## 7a. What has landed so far

**Steps 1–2 are done and green** (58 → 60 tests in `iridium-lang`, wasm check
clean):

- **`crates/iridium-lang/build.rs`** — scans `src/languages/queries/`, emits
  `QUERY_SOURCES` into `OUT_DIR`. **146 `include_str!` entries across 21
  language directories**, which matches the 146 `.scm` files counted
  independently in the previous session, so nothing was dropped.
- **`query/embedded.rs`** — the 78-arm exhaustive match is gone; `source()` is
  two lookups against the generated table. It lost `const`, which nothing
  depended on.
- **`query/tests.rs`** — `PINNED`, a hand-written table of what four specific
  languages ship, plus a check that a pinned `highlights.scm` carries real
  s-expression text rather than an empty string a generated table would
  happily hold.
- **`src/languages/languages.txt`** — the hand-edited language list, 20 rows.
  Read by `build.rs`, which emits `LANGUAGE_IDS` and the
  `Language::<Name>` constants. **Not included by anything yet**: the
  constants collide with the enum variants of the same name, so the file is
  generated but dark until step 3 flips the type.

`languages.txt` starts with the **current 13**, not all 20. Admitting the seven
is step 5 and is kept separate on purpose: the moment they appear in that file
`Language::all()` grows and every test that iterates it sees them, which would
tangle a behaviour change into what is otherwise a pure refactor.

### Step 3 — `Language` is a newtype, and it cost almost nothing

- **`iridium-lang`: 60 tests pass with zero test edits.** The generated
  constants preserved every `Language::Rust` call site, in expressions and in
  patterns alike, exactly as §2 predicted.
- **`iridium-syntax`: 104 tests pass.** `grammar()` returns `Option`,
  `SyntaxTree::new` reports `SyntaxError::UnsupportedLanguage` — a variant that
  already existed and had no user until now — and the fold table matches on the
  identifier with an empty default row.
- **`query/mod.rs` needed no change at all**, as predicted once `COUNT` stayed
  a constant.

**The one thing that cascaded** was `const`. `FoldableNodeTypes::for_language`
cannot be `const` while it compares strings, and that propagated up a chain of
four:

```
FoldableNodeTypes::for_language  (iridium-syntax)
  -> FoldDetector::new
    -> FoldCache::new
      -> Folds::new                (iridium-editor, syntax feature only)
```

Nothing constructs any of them in a constant, so the cost is a handful of
string comparisons once per cache rather than once per node. The last link is
the tidiest outcome of the four: `Folds::new`'s feature-*off* twin was already
non-`const`, so the pair now agrees where it previously did not.

**Measured: the generated table costs zero wasm bytes.** The previous session
established that unreferenced `include_str!` is discarded; this change puts all
146 into a *single static*, which is the case that might not have been. It is
still discarded — the release wasm contains none of the query text, none of the
`.scm` file names, and not even the manifest field names:

```
attribute_item  absent    highlights.scm  absent
impl_item       absent    path_suffixes   absent
```

### ⚠️ The index space changed, and it is safe only because of where it is used

The old enum's order was `Rust=0, Python=1, TypeScript=2, …`. The generated
registry sorts by id, so it is now `bash=0, c=1, cpp=2, …`. **Every language's
`index()` changed.**

That is safe here, and the reason should be stated rather than assumed:
`Language::index()` has exactly **one** consumer in the workspace —
`COMPILED[language.index()][kind.index()]` in `iridium-syntax`, a process-local
cache built fresh on every run. Checked by grep, not by memory.

It would **not** be safe if an index were ever serialized, sent across the
TypeScript boundary, or written to a configuration. Nothing does: the wire form
is the identifier, via `Serialize`. Anyone tempted to persist `index()` should
read this paragraph first — it is a build-order artefact, not an identity.

### Gate results

| gate | result |
| --- | --- |
| `cargo test --workspace --all-features` | **2344 passed, 0 failed** — the 2341 baseline plus the 3 tests added here |
| `cargo test -p iridium-editor --no-default-features` | **1002 passed, 0 failed** |
| `cargo test -p iridium-editor --no-default-features --features syntax` | **1097 passed, 0 failed** |
| `cargo check -p iridium-bindings --features web --target wasm32-unknown-unknown` | clean |
| `cargo fmt --all --check` | clean |

**Clippy caught one thing worth recording**, because it is a property of
*generated* code and will bite again. `redundant_pub_crate` rejected
`pub(crate) static QUERY_SOURCES`: that table is included into
`query/embedded`, a **private** module, where `pub` is already unreachable
from outside the crate and the `(crate)` adds nothing.

`LANGUAGE_IDS` is `pub(crate)` and does *not* trip the lint, because it is
included into the crate root where the restriction is real. So `build.rs`
emits different visibility for the two tables, and that asymmetry is correct
rather than an oversight — the comment beside it says so, since it otherwise
reads like a slip someone would "fix".

The parser-free kernel passing is the one that matters most for this change:
it is the configuration that carries no tree-sitter at all, and therefore the
one where a language existing without a grammar has to be ordinary rather than
exceptional.

### Still to do

Steps 4–6: `has_grammar` is exported but nothing outside `iridium-syntax` calls
it yet; the seven languages are not admitted; AWL is not vendored.

## 8. Build order

1. `registry.rs` + `build.rs`, with `Language` still the enum, registry unused.
   Nothing observable changes; the scan is testable on its own.
2. The hand-written expectation table, red-proven against a deliberately
   broken scan.
3. `Language` becomes the newtype. Everything downstream compiles or does not.
4. `grammar()` → `Option`; `COMPILED` → runtime-sized.
5. Admit the eight grammarless languages, **with the `.jsonc` test first**.
6. Then AWL (#68) is a directory and a `cc` build step, touching no Rust
   beyond the extern declaration.

## 9. Rulings — ✅ ALL THREE RULED IN, 7 Aug 2026

> Tom: *"Yeah, so look I'll go with your recommendations on everything there
> and. Yeah, so record those as decisions and then let's get going."*

**R-1 ✅ admit the eight grammarless languages.** `.diff`, `.patch`, `go.mod`,
`go.work` and git commit messages get file association and comment tokens.

**R-2 ✅ scan the query files, hand-list the languages.** The trade in §5 is
taken knowingly: a language whose files went missing degrades quietly instead
of failing the build, and `KNOWN_ABSENCES` is what catches it.

**R-3 ✅ leave the fold-kind table alone here.** L-6 stays a separate
decision; this work does not touch how folds are determined.

The original wording of each, for the record:

- **R-1 — the eight grammarless languages.** §6 recommends admitting them, so
  `.diff`, `go.mod` and git commit messages get comment tokens and file
  association. Costs nothing; the alternative is they stay vendored and dead.
- **R-2 — the build.rs trade.** §5: the *language set* stays hand-listed so a
  language cannot appear or vanish without a diff; only the *per-language
  query files* are scanned, because that half is 126 entries and is a fact
  about the directory. Adding a language becomes one line plus a folder.
  Recommended; the residual cost is that a language whose files went missing
  degrades quietly instead of failing the build, which a hand-written
  expectation table catches.
- **R-3 — L-6, fold kinds.** Currently a hand-maintained table `iridium-syntax`
  invented; the manifests do not carry it. Recommend leaving it alone here and
  deciding separately whether folds come from `outline.scm` instead.

None of these block starting — steps 1 and 2 are true whichever way they go.
