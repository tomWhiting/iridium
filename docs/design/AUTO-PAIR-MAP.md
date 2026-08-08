# Auto-pair map — the brackets every manifest declares and nothing reads

**Ground verified 8 Aug 2026.** Every figure below was produced by a command
in the transcript beside it; nothing here is quoted from another document
without being re-checked against the code.

## The three load-bearing facts, up front

1. **The editor's auto-pair table is six characters, hard-coded, identical in
   every language.** `pair_close` (`input/keyboard/behaviors.rs:33-42`):
   `(`→`)`, `[`→`]`, `{`→`}`, and `"`, `'`, `` ` `` closing with themselves.
2. **Every vendored manifest declares its own `brackets` table, and
   seventeen of the twenty do.** They disagree with the hard-coded six in
   ways that are visible on ordinary lines — most sharply, **Rust does not
   pair `'`**, and Iridium is a Rust editor.
3. **`brackets` and `autoclose_before` are not deserialized at all.**
   `Manifest`'s `Fields` (`iridium-lang/src/manifest/schema.rs:48-88`) reads
   seven keys — `name`, `grammar`, `path_suffixes`, `line_comments`,
   `block_comment`, `documentation_comment`, `hidden`. The bracket data is on
   disk, shipped, and unread.

## The sentence this came out of

`iridium-lang/src/manifest/mod.rs:16-19`, the reader's own module doc:

> This module is deliberately only the *reader*. Nothing here changes what
> Iridium believes about a language; **the hard-coded tables are replaced one
> at a time, each against a test asserting the manifest agrees with what it
> replaces.**

Comment tokens were replaced that way (#66). **Brackets were not**, and the
half of that sentence that matters here is the test: *asserting the manifest
agrees with what it replaces.* For brackets it does **not** agree, in seven
named ways. That disagreement is the defect; the replacement is the fix.

Two other documents already bank the capability as delivered:

- `iridium-syntax/src/grammar.rs:21-23` — *"File association, comment tokens,
  indent rules and auto-pairs all come from the vendored manifest."*
- `iridium-lang/src/languages/languages.txt:29-31` — the same sentence, as the
  justification for admitting the grammarless languages.
- `docs/IN-FLIGHT-grammarless.md:37` — *"Plus brackets and auto-pairs from each
  manifest. `gitcommit` carries six."*

None of that is true today.

---

## 1. Verified ground

### 1.1 What the editor does

`auto_pair_edit_for` (`behaviors.rs:371-414`), per cursor, in order:

| step | behaviour |
| --- | --- |
| non-collapsed + opener | wrap the selection, preserving orientation |
| non-collapsed + closer | replace, like ordinary typing |
| collapsed + closer already at caret | skip over it |
| collapsed + opener | insert the pair, caret between — **unless** the character is a quote and the character before the caret is a word character (`don't`, `it's`) |

The tables it consults, all `const fn` over `char`:

- `pair_close` — the six openers above.
- `bracket_close` — `(`, `[`, `{` only; used by Enter's bracket-block expansion.
- `is_quote` — `"`, `'`, `` ` ``.
- `is_pair_closer` — `)`, `]`, `}` plus the three quotes; the skip-over set.

The only configuration is `EditorConfig::auto_pairs`, a single `bool`
(`docs/CONFIG.md:52`). **There is no per-language path of any kind** — not a
parameter, not a lookup, not a stub.

### 1.2 What the manifests declare

⚠️ **The first extraction below was wrong and is corrected in §1.2a.** It was
taken with a regular expression, and `\{[^}]*\}` terminates on the `}` inside
`end = "}"` — so every `{`→`}` row was truncated and silently dropped, and no
row's `close` flag was read at all. §1.2a is the same data through
`tomllib`. The table here is kept because its *starts* are accurate and it is
what the divergence list was first written against; **read §1.2a for the
facts that matter.** A citation chain is not evidence; only the parse is.

Extracted from `crates/iridium-lang/src/languages/queries/*/config.toml`:

| language | `brackets` openers | `autoclose_before` |
| --- | --- | --- |
| rust | `{` `r#"` `r##"` `r###"` `[` `(` `<` `"` `/*` | `;:.,=}])>` |
| c, cpp | `{` `[` `(` `"` `'` `/*` | `;:.,=}])>` |
| go | `{` `[` `(` `"` `'` `` ` `` `/*` | `;:.,=}])>` |
| javascript, typescript, tsx | `{` `[` `(` `<` `"` `'` `` ` `` `/*` | `;:.,=}])>` |
| python | `f"` `f'` `b"` `b'` `u"` `u'` `r"` `r'` `rb"` `rb'` `t"` `t'` `"""` `'''` `{` `[` `(` `"` `'` | `;:.,=}])>` |
| json, jsonc | `{` `[` `(` `"` | `,]}` |
| yaml | `{` `[` `"` `'` | `,]}` |
| css | `{` `[` `(` `"` `'` | `;:.,=}])>` |
| markdown | `{` `[` `(` `<` `"` `'` `` ` `` `*` | `;:.,=}])>` |
| bash | `[` `(` `{` `"` `'` `do` `then` `then` `then` `in` | `}])` |
| gitcommit | `(` `` ` `` `"` `'` `{` `[` | — |
| gomod, gowork | `(` | `)` |
| regex | `(` `{` `[` | `)]}` |
| jsdoc | `{` `[` | `]}` |
| awl, diff, markdown-inline | none declared | — |

Each entry also carries `end`, `close` (whether to auto-close at all),
`newline` (whether Enter expands the block), and often
`not_in = ["string", "comment"]`.

### 1.2a The same data, parsed rather than matched — **this is the authority**

`close` defaults to `true` when the key is absent (17 rows rely on that).
"Auto-close set" below is every row whose `start` and `end` are each exactly
one character and whose `close` is not `false`.

| language | auto-close set | declared but `close = false` | multi-character rows |
| --- | --- | --- | --- |
| rust | `{` `[` `(` `"` | `<` | `r#"` `r##"` `r###"` `/*` |
| c, cpp | `{` `[` `(` `"` `'` | — | `/*` |
| go | `{` `[` `(` `"` `'` `` ` `` | — | `/*` |
| javascript, typescript, tsx | `{` `[` `(` `"` `'` `` ` `` | `<` | `/*` |
| python | `{` `[` `(` `"` `'` | — | `f"` `f'` `b"` `b'` `u"` `u'` `r"` `r'` `rb"` `rb'` `t"` `t'` `"""` `'''` |
| json, jsonc | `{` `[` `(` `"` | — | — |
| yaml | `{` `[` `"` `'` | — | — |
| css | `{` `[` `(` `"` `'` | — | — |
| markdown | `{` `[` `(` `<` | `"` `'` `` ` `` `*` | — |
| bash | `{` `[` `(` `"` `'` | `do` `then` `in` | `do` `then` `in` |
| gitcommit | `{` `[` `(` `"` `'` `` ` `` | — | — |
| gomod, gowork | `(` | — | — |
| regex | `{` `[` `(` | — | — |
| jsdoc | `{` `[` | — | — |
| diff | **empty list** | — | — |
| awl, markdown-inline | **no `brackets` key at all** | — | — |

⚠️ `diff` declaring an **empty** list and `awl` declaring **no key** are
different states and are treated differently below: an empty declaration is a
language saying "pair nothing", an absent one is a language that has not said.

### 1.3 ⚠️ The divergences, each with its symptom

Each line is a thing the editor does today that the language says it should
not. ⭐ **Every single-character divergence is a subtraction** — once the data
is parsed correctly, there is no single-character pairing a language wants and
does not get. That is what makes the first slice clean.

1. ⭐ **Rust: `'` is paired and must not be.** Rust's table has no single
   quote — lifetimes are why. Typing `<` then `'` in `fn f<'a>` yields
   `<''`, caret between; `a` gives `<'a'`; the stray quote has to be deleted
   by hand. `char_before` is `<`, not a word character, so the existing
   apostrophe guard does not fire. **This is the daily one, in the language
   this editor is written in.**
2. **Rust: `` ` `` is paired and is not a Rust delimiter at all.** It appears
   only inside doc comments.
3. **JSON: `'` and `` ` `` are paired.** Neither is ever valid JSON.
4. **YAML: `(` and `` ` `` are paired.** YAML's table declares neither.
5. **`gomod` / `gowork`: five of the six are paired.** Their tables declare
   `(` and nothing else.
6. ⚠️ **Markdown wants *fewer* pairings, not more** — corrected from this
   map's first draft, which claimed Markdown wanted `*` paired. It does not:
   `*` is declared `close = false`, and so are `"`, `'` and `` ` ``. Markdown
   auto-closes `{ [ ( <` and nothing else. Prose is the reason — an
   apostrophe in Markdown is an apostrophe. The editor pairs all three quotes
   there today.
7. **`diff` declares an empty bracket list and gets all six.**
8. **Every language's multi-character openers are unreachable.** The table is
   keyed by `char`, so Python's `"""`, `f"` and `rb'`, Rust's `r#"`, and the
   `/* */` six languages declare cannot be expressed in it at all. (bash's
   `do`/`then`/`in` are `close = false` — they are matching hints, not
   auto-close rows, so they are not part of this.)
9. **Markdown declares `<`→`>` with `close = true`** and the editor never
   pairs `<`. The **one** single-character addition anywhere in the set, and
   it is deliberately out of the first slice — see S-2.

⚠️ **The apostrophe guard is a proxy, and this is where it diverges.** The
`is_word_char` check exists to keep `don't` from becoming `don''t`. It is a
proxy for *"this language does not pair `'` here"*, and it agrees with that
target in prose. It disagrees in Rust after `<`, `&`, or a space — every
lifetime position there is.

### 1.4 What is *not* wrong

Stated so the fix is not oversold:

- The **skip-over**, **selection-wrap** and **Enter expansion** mechanics are
  correct and language-independent; nothing below changes them.
- `bracket_close`'s three-bracket restriction for Enter expansion agrees with
  the manifests' `newline = true` rows for every language checked.
- `<` is declared by Rust, JS, TS, TSX and Markdown with `close = false` — do
  not auto-close. The editor never pairs `<`, so it agrees **by accident**.
  A wire-up must keep agreeing, which means honouring `close`, not just
  reading `start`.

---

## 2. The slices, priced

### S-1 — subtraction only, single characters, no scope constraints

For each language, drop the openers its manifest does not declare. Fixes
divergences 1-5. Adds nothing, so no user ever sees a pairing appear where
none was.

- **Data**: `brackets` deserialized; keep only rows whose `start` is one
  `char` and whose `close` is not `false`; intersect with today's six.
- **Kernel**: `pair_close` and friends take the active language's set instead
  of matching on a literal. The set is a `Copy` bitmask over six characters,
  or a small `&'static` slice — either is cheaper than the current `match`.
- **Fallback**: a language with no manifest, or a buffer with no language,
  keeps today's six. Nothing regresses to *less* than today's behaviour.
- **Test**: the `#66` idiom the reader's module doc names — assert per
  language that the derived set equals a hand-written expectation, and
  separately that Rust's set excludes `'`.
- **Ruling needed: none I can see.** #66 already ruled the manifests are the
  source of truth for language facts, and every change is the removal of a
  pairing the language declares it does not want.

### S-2 — the one addition

Markdown's `<`→`>`, divergence 9 — the only single-character pairing any
language declares that the editor does not offer. One character, one language.
**Wants a ruling**, because it is the only slice that makes a pairing appear
where none was, and because `<` in Markdown is as often a literal `<` as it is
an autolink.

⚠️ This slice was originally written as "Markdown's `*`", from the broken
extraction. Markdown declares `*` with `close = false`; it does not want it.

### S-3 — multi-character openers

Python's triple quotes and string prefixes, Rust's `r#"`, the six languages'
`/* */`. Divergence 7. This is a different data structure (longest-match over
the text before the caret, not a `char` lookup) and a different skip-over
rule, so it is genuinely its own piece of work rather than more of S-1.

### S-4 — `not_in` scope constraints

`not_in = ["string", "comment"]` needs to know what the caret is inside,
which means a syntax query at type time. The kernel now has a retained tree
(`SyntaxState`) and a windowed span cache, so the information exists — but
this is the first typing-path consumer of it, and "typing never parses" is a
claim with a benchmark behind it. **Price it before building it.**

### S-6 — Enter expansion follows `newline`

⭐ **Found after S-1 landed, by checking a claim S-1 left half-made.** §1.4
said `bracket_close`'s fixed three "agrees with the manifests' `newline = true`
rows *for every language checked*". Checking all of them, it does not:

| language | expands per manifest | expanded before |
| --- | --- | --- |
| `gitcommit`, `regex`, `jsdoc`, `diff` | **nothing** | `{` `[` `(` |
| `json`, `jsonc`, `yaml` | `{` `[` | `{` `[` `(` |
| `gomod`, `gowork` | `(` | `{` `[` `(` |
| `bash` | `{` `(` | `{` `[` `(` |
| everything else | `{` `[` `(` | same |

The commit-message row is the one worth naming: pressing Enter between `(` and
`)` in a commit message grew a three-line indented block out of a parenthesis
in prose.

⭐ **Pure subtraction again, and provably so**: every language's `newline`
set is a subset of `{ [ ( <`, and `<` is never paired, so no language gains an
expansion. Same argument as S-1, same ruling (#66).

⚠️ `close` and `newline` are **independent flags**, which the vendored data
makes plain: Rust declares `<`→`>` as `close = false, newline = true` — *do
not type the closer for me, but do expand the block if I typed it myself.*
`close` defaults to true, `newline` defaults to false, and the manifests are
written expecting exactly that asymmetry.

### S-5 — `autoclose_before`

Only auto-close when the character *after* the caret is in the language's set
(or the line ends). The editor has no equivalent, so today it closes
unconditionally — typing `(` before an identifier gives `(|)identifier`.
Every manifest that declares brackets declares this too, and the sets differ
per language (`;:.,=}])>` for C-likes, `,]}` for JSON and YAML, `)` for
`gomod`). Independent of S-1..S-4; a clean slice on its own.

---

## 3. Decisions for the owner

- **B-1 — S-1 now?** Recommend **yes**: pure subtraction, manifest-sourced,
  and it removes a stray quote from every lifetime a Rust user types. I read
  this as ruling-free under #66 and built it — see §4.
- **B-2 — S-2, Markdown's `<`.** Recommend **no** for now: `<` in Markdown is
  a literal at least as often as it is an autolink, and it is the only
  addition in the whole set. Asked rather than assumed either way.
- **B-3 — S-3 and S-5 order.** Recommend **S-5 before S-3**: `autoclose_before`
  is a smaller change that improves every language, where multi-char openers
  mostly serve Python and Rust raw strings.
- **B-4 — S-4 at all?** Recommend **not yet**. It is the only slice that puts
  a syntax query on the typing path, and the benchmark claim that typing never
  parses is worth more than `/*` behaving perfectly inside a string literal.
- **B-5 — S-5's semantics, which the data does not give.** ⚠️ The manifests
  declare the character *sets* but not what happens at **end of line** or
  **before whitespace**, and neither appears in any set. If they do not
  implicitly permit closing, then typing `(` at the end of a line would stop
  auto-closing — the single most common case — so they almost certainly must;
  every declared set is punctuation only, which is itself evidence the feature
  assumes whitespace is handled outside it. **But that is an inference, not a
  reading**, and Zed's implementation is not vendored here to check against.
  Recommend: treat end-of-line and whitespace as permitting, state it in the
  code as an assumption, and build S-5. Asked rather than assumed, because
  this map has already shipped one wrong claim from a convenient inference
  (§1.2) and the correction is what made it useful.

⚠️ **None of B-1..B-4 blocks the doc corrections**, which are unambiguous:
three module docs currently state that auto-pairs come from the manifest, and
they do not. Those were corrected in the same commit as this map, so no
reader is told a capability exists before it does.

---

## 4. S-1 — LANDED

- **`iridium-lang`**: `Bracket` deserialized, with `close` defaulting to
  `true` — seventeen rows across the tree omit the key, and reading it as
  `false` would have disabled `{` in every C-like language.
  `Manifest::auto_close_pairs` returns `Option<impl Iterator<Item = (char, char)>>`:
  `None` for a manifest with no `brackets` key, `Some(empty)` for one that
  declares an empty table.
- **`iridium-editor`**: `AutoPairs`, a one-byte bitmask over the six pairs
  this editor can type, built once per keystroke in `handle_char_input` and
  again in `backspace_edits_with_pairs`. `pair_close` and `is_pair_closer`
  are gone; `is_quote` stays, because the apostrophe guard asks what kind of
  character `'` is, not whether this language pairs it.
- **The skip-over and backspace sets are narrowed too**, deliberately: in
  Rust `'a'` is a character literal the user typed, and stepping over the
  closing quote or eating both halves on one backspace would drop input.

### The red proof

The tests were written after the fix compiled, so red was demonstrated by
restoring the old behaviour exactly — `AutoPairs::for_document` returning
`Self::ALL`, which is what `pair_close` was — and re-running:

```
rust pairs '\'' and its manifest does not declare it
  left: "''"
 right: "'"
```

**4 of the 7 new behaviour tests went red**; the other 3 pass both ways by
design — they assert the pairs each manifest *does* declare still work, so
the suite cannot be satisfied by an implementation that merely stops pairing.

### Gates

Nine green: **2,525 passed, 0 failed** (2,514 before, plus 11 here).

### What S-1 deliberately did not do

- `<`→`>` stays unpaired everywhere, including Markdown, which declares it —
  that is S-2 and B-2.
- ⚠️ **No `not_in` handling.** A `"` inside a comment still pairs. That was
  true before and is unchanged.
- ~~Enter's bracket-block expansion still uses its own three-bracket table.
  Every manifest's `newline = true` rows agree with it for the languages
  checked~~ — **this claim was wrong**, and checking it properly produced S-6
  below. "For the languages checked" was doing load-bearing work in a sentence
  that read like a clearance.

## 5. S-6 — LANDED

`AutoPairs` became `PairRules`, carrying two masks: which pairs close, and
which Enter expands. `bracket_close` is gone; `Manifest::block_expand_pairs`
reads `newline` the way `auto_close_pairs` reads `close`, through one shared
filter so the two cannot drift.

- **The silence rule is shared**: no language, no manifest, no `brackets` key
  → both masks are the old fixed sets. `awl` is in that group.
- **`expands` is intersected with the three brackets.** No manifest sets
  `newline` on a quote, so the intersection removes nothing today; it is there
  so one appearing in a vendor refresh is a decision rather than a surprise.
- **`BRACKET_MASK` names positions in `PAIRS` by hand**, so a test asserts it
  selects exactly `(`, `[`, `{` — reordering that array would otherwise start
  expanding quote blocks silently.

### The red proof

Same method: restore the old behaviour verbatim (`expands: BRACKET_MASK`
unconditionally) and re-run. **2 of the 4 new tests went red** —
`a_commit_message_does_not_expand_a_bracket_block` (`fix (\n    \n)` where a
plain break was wanted) and `json_expands_only_the_brackets_it_marks_newline`.
The other 2 assert the expansions each language *keeps*, and pass both ways by
design.

### Gates

Nine green: **2,530 passed, 0 failed** (2,525 after S-1, plus 5 here).

---

## 6. Rulings of 8 Aug, and the question they came with

Tom ruled on the outstanding decisions and asked a further one that changes
the shape of the rest.

### The rulings

- **B-2 — Markdown's `<`: NO.** Recommendation taken. `<` stays unpaired.
- **B-5 — S-5's semantics: BUILD IT**, treating end-of-line and whitespace as
  permitting, with the assumption stated in the code as the recommendation
  said. ⚠️ It remains an *inference* from the shape of the declared sets, not
  a reading of a specification, and the code must say so — if a language ever
  ships a set where that inference is wrong, the comment is what leads someone
  to it.
- **B-4 — S-4: SUPERSEDED.** It was "recommend not yet", on the grounds that
  a syntax query on the typing path costs more than `/*` behaving perfectly
  inside a string. **Tom has asked for exactly that behaviour**, so the
  question is no longer *whether* but *how cheaply*. The rest of this section
  is that.

### 6.1 The ask

> Currently all of our auto-pairing just goes ahead on any document — curly
> braces auto-close and all that. Shouldn't that be syntax dependent for the
> language, and could we make that configurable through tree-sitter queries?

**Yes, and most of the configuration already exists and is already being
ignored.** Every vendored manifest that declares brackets also declares
`not_in` per row — overwhelmingly `["string", "comment"]`. Nothing reads it.
That is the same shape as the original finding behind this map: the data is
vendored, per-language, and inert.

So "make it configurable" is not a new mechanism. It is S-4, plus the
question of what the caret consults.

### 6.2 The crux: node kinds are not portable, captures are

Two ways to answer "is the caret inside a string or a comment", and the
difference is the whole design.

**(a) Walk the node kinds.** `descendant_for_byte_range(n, n)` then climb,
comparing `node.kind()`. Cheap — O(depth), no query execution, no
allocation — and needs no query file at all.

⚠️ **But node kinds are grammar-specific and unnormalised.** Rust says
`string_literal` and `raw_string_literal`; JSON says `string`; Python has a
dozen prefixed forms; comments are `line_comment`, `block_comment`, `comment`
depending on the grammar. `not_in = ["string", "comment"]` names *neither* of
those vocabularies. Matching kinds against it means a per-grammar translation
table — which is precisely the hard-coded barrage this project exists to
avoid, and it would have to be extended by hand for AWL and every future
language.

**(b) Use the highlight captures.** `highlights.scm` already normalises every
grammar's spelling into `string`, `comment`, `keyword` and the rest — that is
what a highlight query *is*, and it is the layer the ecosystem already
maintains per language. `not_in`'s vocabulary is that vocabulary. A new
language ships `config.toml` + `highlights.scm`, which it must ship anyway to
be highlighted at all, and it gets scope-aware auto-pairing **for free, with
no new file kind and no Iridium change**.

**Recommend (b).** It is the only one of the two that answers Tom's
"configurable through tree-sitter queries" honestly: the query is the one
already vendored, and the per-language configuration is the `not_in` line.

### 6.3 Making it cheap enough for the typing path

The benchmark claim that **typing never parses** is not negotiable, and (b)
sounds more expensive than (a). It need not be.

1. **Consult the spans already computed.** The caret is, essentially always,
   inside the viewport, and the viewport's highlight spans are already
   resolved and cached for rendering. Looking up the span covering the caret
   is a binary search over a sorted slice. **Zero parses, zero queries.**
2. **Fall back to a byte-ranged query** only when the caret is not covered —
   a `QueryCursor` with `set_byte_range` over the caret's node, not the
   document.
3. **Never call `sync()` from the typing path.** `note_edit` already keeps the
   retained tree edited in nanoseconds; a reparse stays where it is.

⚠️ The kernel has **no** "what scope covers byte N" accessor today —
`SyntaxState` exposes `tree()` and `sync()` and nothing else, and the only
span-shifting code lives in `iridium-bindings`. That accessor is the actual
work of S-4, and it belongs in the kernel so all three faces share one answer.

### 6.4 The policy question the data cannot answer

Spans go stale between an edit and the next highlight pass, and a document may
have no language, no grammar, or an unparsable tree. So the scope at the caret
is sometimes simply **unknown**, and that is not an edge case — it is every
keystroke in an unsupported file.

**B-6 — what does unknown mean?**
- *Fail open* (auto-close anyway, today's behaviour) — recommend **this**. The
  failure mode is an unwanted bracket inside a string, which one keystroke
  fixes. The other direction's failure mode is "my brackets stopped working",
  in a file the user cannot diagnose.
- *Fail closed* (no auto-close when unsure) — cleaner in theory, and it would
  make every grammarless file silently lose a feature.

**B-7 — does `not_in` suppress the closing character, or the pairing
entirely?** Inside a string, typing `(` should insert `(` alone. But the same
question applies to *skip-over* (typing `)` when `)` is next) and to Enter
expansion. Recommend: `not_in` gates **insertion of the close** only; skip-over
and Enter expansion follow the same scope test but are separate slices, so a
half-built S-4 cannot leave the two disagreeing.

**B-8 — the AWL question.** `awl` declares **no brackets at all** (§1.2), so
none of this reaches it until its manifest does. Worth knowing before it is
mistaken for a bug: scope-aware auto-pairing will look like it does nothing in
AWL, correctly.

### 6.5 Order

S-5 first (ruled, small, improves every language), then S-4 built on the
already-computed spans. S-3 after. S-2 is closed as "no".
