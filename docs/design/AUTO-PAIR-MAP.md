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

---

## 7. S-5 — LANDED, 8 Aug 2026

`autoclose_before` is read. Typing `(` in front of an identifier now gives
`(identifier`, not `()identifier`.

### The shape

- **`iridium-lang`** — `Fields::autoclose_before: Option<String>` and
  `Manifest::autoclose_before() -> Option<&str>`. `None` and `Some("")` are
  kept apart for the same reason `brackets` keeps them apart, and the
  distinction is reached by a **different set of manifests**: `gitcommit`
  declares six brackets and no `autoclose_before` at all.
- **`iridium-editor`** — `PairRules` gains `close_before: Option<&'static str>`
  and `permits_close_before(next: Option<char>)`. A borrowed `&'static str`
  rather than a set: manifests live for the process, the sets are at most nine
  characters, and a nine-character scan on the path that inserts one character
  is not worth an allocation per keystroke.
- **One call site**, in the collapsed-opener branch of `auto_pair_edit_for`,
  after the apostrophe guard.

### The assumption, and where it is written

⚠️ **End of line and whitespace permit the close.** This is Tom's ruling B-5 and
it is an *inference*: no vendored set contains a space, a tab or anything
standing for a line ending, so read literally every set forbids closing at the
end of a line — which is where a bracket is most often typed. The reading taken
is that these sets constrain what a closer may be **displaced in front of**, and
empty space displaces nothing.

It is stated as an assumption in three places, deliberately: the doc comment on
`Manifest::autoclose_before`, the doc comment on `PairRules::permits_close_before`,
and a test. The test is the one that matters —
`no_autoclose_before_set_contains_whitespace_or_is_empty` asserts the *premise*,
so a vendor refresh that adds whitespace to a set fails loudly rather than
silently making the inference wrong.

### What it deliberately does not gate

Written into `auto_pair_char_edits`'s own contract rather than left to be
rediscovered:

- **Selection wrap.** The character after a selection is an accident of where
  the selection ended, and the user asked for the wrap by selecting. Refusing it
  would turn an explicit request into a typed-over selection — it would destroy
  the text.
- **Skip-over.** It fires on typing a *closer*, and asks what is already at the
  caret, not what follows it.
- **Backspace pair deletion.** Same reason: it acts on a pair that exists.

### The red proof

The gate was removed — the three-line `permits_close_before` call, nothing else
— and the suite re-run. **2 of the 7 new behaviour tests went red:**
`a_pair_does_not_close_in_front_of_a_character_the_language_did_not_list`
(`left: "()identifier"`) and `each_language_uses_its_own_autoclose_before_set`.

The other 5 pass both ways **by design**, and that is the point: they assert
what S-5 must *keep* doing — closing at end of line, closing after whitespace,
closing in front of a listed character, closing everywhere for a language that
listed nothing, and wrapping a selection regardless. Without them the suite
could be satisfied by an implementation that simply stopped auto-closing.

`each_language_uses_its_own_autoclose_before_set` is the one that proves the set
is read from the manifest rather than fixed: a semicolon permits in Rust and
refuses in JSON, a comma is the reverse, and `go.mod` permits only `)`.

### Gates

Nine green: **2,554 / 1,068 / 1,164, 0 failed** — 2,545 before, plus seven
behaviour tests and two manifest guards.

### Next

**S-4** remains blocked on **B-6** and **B-7** (§6.4), both put to Tom on 8 Aug.
S-3 after that. S-2 is closed as "no".

---

## 8. B-6 and B-7 — RULED, 8 Aug 2026

Tom: *"I'll go with your recommendations there."* Both recommendations from §6.4
are now the decisions, so **S-4 is unblocked**.

### B-6 — an unknown scope means *fail open*

When the scope at the caret cannot be determined — no grammar, no language, an
unparsable tree, or spans that have not caught up with the last keystroke —
auto-pairing behaves **exactly as it does today**. It does not suppress.

The argument, recorded because the opposite reads as tidier: the failure mode of
failing open is one unwanted bracket inside a string, which a single keystroke
undoes and which the user can see. The failure mode of failing closed is
"brackets stopped working", in a file whose grammar the user has no way to
inspect, with no error and nothing to search for. **A feature that silently
switches itself off is worse than one that occasionally over-fires.**

⚠️ This makes *unknown* the same as *not in a suppressed scope*, which means
S-4 can never be verified by "it stopped pairing" alone — a test that asserts
suppression must first assert the scope was actually resolved, or it passes
against a build where the resolver returns `None` for everything. **That is the
trap in this slice.** It is the same shape as the atlas probe in #87: assert the
premise, not only the conclusion.

### B-7 — `not_in` gates the insertion of the closer, and nothing else

Inside a string or a comment, typing `(` inserts `(` alone.

Skip-over and backspace pair-deletion are **not** gated, matching S-5's
boundary (§7) and for the same reason: both act on a pair that is already in the
document, and the question they ask is what is *at* the caret, not what scope it
sits in. A closer this editor never inserted is a character the user typed, and
stepping over it or eating it would drop input.

Enter expansion (S-6's `newline`) is likewise untouched — it is a separate flag
on a separate slice, and folding it in here would let a half-built S-4 leave the
two disagreeing about what a scope means.

### What is still to build

The design in §6.2–6.3 stands unchanged: **highlight captures, not node kinds.**
The work is:

1. **A scope-at-byte accessor in the kernel.** `SyntaxState` exposes only
   `tree()` and `sync()`; there is no way to ask what covers byte N. This is the
   actual work of S-4.
2. Read `not_in` off the manifest — it is declared everywhere and read by
   nothing.
3. Resolve against the **already-computed viewport spans** (a binary search, not
   a parse). ⚠️ **Never call `sync()` from the typing path.**

**B-8 stands as recorded:** `awl` declares no brackets, so scope-aware pairing
will correctly appear to do nothing there. Worth knowing before it is filed as a
bug.

---

## 9. S-4 — ground verified, 8 Aug 2026. Build plan.

Everything below was read, not remembered.

### The vocabulary matches exactly, and that is the whole argument for captures

Parsed from every vendored manifest, `not_in` uses **two values and no others**:

| value | occurrences |
| --- | --- |
| `string` | 60 |
| `comment` | 45 |

`HighlightType` (`iridium-syntax/src/highlight/capture.rs:17`) carries `String`,
`StringEscape`, `Comment` and `CommentDoc`, and `from_capture` already folds
`string.literal` and `string.special` into `String`. So the manifests' vocabulary
is a **subset** of the capture vocabulary — no translation table, nothing
per-grammar, which is exactly what node kinds would have required.

### ⚠️ The trap: `String` is not the only string, and `Comment` is not the only comment

A naive `highlight == HighlightType::String` test lets a pair through on an
**escape sequence inside a string** (`StringEscape`), and `== Comment` lets one
through inside a **doc comment** (`CommentDoc`). Both are still inside the thing
the manifest named. The map must be:

- `"string"` → `String` **or** `StringEscape`
- `"comment"` → `Comment` **or** `CommentDoc`

Written here because the failure is invisible: pairing works everywhere it
should, and quietly still fires on `"\n|"` and inside `/// `.

### What exists to resolve a scope

- **`Highlighter::spans_in_range(&self, tree, source, range) -> Vec<HighlightSpan>`**
  (`highlight/highlighter.rs:138`) — a byte window via
  `QueryCursor::set_byte_range`. ⚠️ Its own doc says `source` must be the
  **whole** text the tree was parsed from; a windowed `&str` mis-colours and
  mis-evaluates predicates.
- **`HighlightSpan { start, end, highlight }`**, `Ord` by `(start, end)`.
- **`SyntaxState::tree()`** returns the retained tree **without parsing**.
  `sync()` parses. The typing path may call the first and must never call the
  second.

### The open question this leaves — decide before building

`note_edit` applies an edit to the retained tree in nanoseconds and marks it
dirty; it does **not** reparse. So `tree()` on the typing path is a tree whose
ranges have been shifted but whose *structure* is one or more edits stale.
Querying it gives an answer that is right almost always and wrong just after a
keystroke that changed which scope the caret is in — opening a quote, most
obviously.

Under **B-6 (fail open)** that is the benign direction: a stale tree that has not
yet seen the opening quote reports "not in a string", and the pair fires as it
does today. The user's next keystroke lands on a synced tree. So the staleness
costs at most one over-fire, never a suppression, which is the trade B-6 already
chose.

**This is worth stating in the code**, not just here: the correctness of S-4 on
the typing path rests on B-6, and someone later "fixing" the staleness by calling
`sync()` would put a full reparse on every keystroke.

### Build order

1. `HighlightType::is_within(scope: &str) -> bool` in `iridium-syntax`, with the
   two-into-four mapping above and a test naming all four variants.
2. `Bracket::not_in` deserialized in `iridium-lang`, and a `Manifest` accessor.
   ⚠️ Same `None` vs `Some(empty)` discipline as `brackets` and
   `autoclose_before` — 17 rows omit `close`, and this key is absent far more
   often than present.
3. A scope-at-byte accessor on `SyntaxState`, over `tree()` and
   `spans_in_range`, returning `Option<HighlightType>` — `None` when there is no
   tree, no language, or no covering span. **`None` means fail open.**
4. `PairRules` grows the suppressed-scope set per pair; `auto_pair_edit_for`
   consults it in the collapsed-opener branch only (§8, B-7).
5. ⚠️ **Every suppression test must first assert the scope resolved.** Because
   unknown and not-suppressed are the same answer, a test that only asserts "it
   did not pair" passes against a resolver returning `None` for everything.

### 9.1 S-4a — the data half, LANDED 8 Aug 2026

Steps 1 and 2 of the build order. **Gates: all nine green, 2,568 / 1,068 /
1,164, 0 failed** (2,559 before, plus five capture tests and four manifest
tests).

- **`HighlightType::is_within(scope) -> bool`** — the two-into-four mapping,
  with a test naming all four variants and one asserting an unrecognised scope
  contains nothing (the fail-open direction, so a vendor refresh introducing a
  third value suppresses nothing rather than everywhere).
- **`Bracket::not_in`** deserialized, defaulting to empty — ⚠️ **absent means
  "nowhere", not "everywhere"**; the opposite would switch auto-closing off for
  every language that has not said.
- **`Manifest::pairs_suppressed_in(scope)`**, built on the same
  `single_char_pairs` filter as its two siblings, so the three cannot drift on
  what counts as a pair. A row with `close = false` is not an auto-close rule
  and so cannot be suppressed in one; a test asserts the suppressed set is a
  **subset** of the closing set.

### 9.2 ⚠️ Why S-4b is a separate slice — the cost, named

The editor half needs the caret's scope, and **the typing path cannot see the
syntax tree**. `handle_char_input` takes `(&Document, &CursorState,
&EditorConfig)`; `CommandContext` (`input/keyboard/actions/mod.rs:65`) carries
those three plus the event and args, and *deliberately* no syntax state — the
same reason the AST verbs went through `KeyResult::Ast` rather than widening it.
The tree lives on `EditorState::syntax`, one level above.

So S-4b is a threading change through keyboard dispatch, not a leaf edit, and it
is priced as its own slice rather than smuggled into this one. **What has to be
decided first is where the scope is resolved**, and there are two honest shapes:

- **(a) Resolve above, pass down an `Option<HighlightType>`.** The caret's byte
  is known wherever the cursor is, so `Editor` can resolve once per keystroke
  and hand the answer in as a `Copy` value. `CommandContext` grows one field of
  a type that already exists, and `behaviors` stays free of syntax entirely.
- **(b) Pass a resolver.** More general, and it would let a future verb ask
  about a byte other than the caret's — at the cost of a trait object or a
  lifetime on the context.

**Recommend (a).** The only question this slice asks is about the caret, the
answer is one `Copy` enum, and (b)'s generality is speculative. If a later verb
needs arbitrary bytes, (a) does not block it — the resolver still exists on
`SyntaxState`, and that verb can take `&Editor` as the AST verbs already do.

⚠️ Still to build in S-4b, unchanged from §9: the scope-at-byte accessor on
`SyntaxState` over `tree()` and `spans_in_range` (**never `sync()`**), the
suppressed-scope masks on `PairRules`, and the gate in the collapsed-opener
branch only. **And every suppression test must first assert the scope
resolved** — under B-6, unknown and not-suppressed are the same answer.

### 9.3 S-4b — the editor half, LANDED 8 Aug 2026

Steps 3, 4 and 5 of the build order. **Gates: all nine green, 2,591 / 1,070 /
1,185, 0 failed** (2,568 / 1,068 / 1,164 before).

#### ⚠️ Shape (a) was recommended in §9.2 and is **wrong**. Built (b)-shaped.

§9.2 priced "(a) resolve above, pass down an `Option<HighlightType>`" and
recommended it because "the only question this slice asks is about the caret".
That sentence contains the error: there is not *a* caret. `auto_pair_char_edits`
maps over `cursor.all_selections()`, and multi-cursor routinely puts one caret
inside a string literal and another in code in the same edit — select every
occurrence of a token that appears both in a string and as an identifier, which
is an ordinary thing to do.

One resolved answer for all of them means the string caret's answer is applied
to the code caret. That **suppresses a pair the language permits**, which is the
fail-*closed* direction ruling B-6 explicitly rejected. So (a) is not a cheaper
approximation of (b); it is a violation of a ruling already taken.

What landed is (b)'s shape without (b)'s price:

- **`CaretScopes` is a concrete borrowed value, not a trait object.** No `dyn`,
  no new lifetime — `CommandContext<'a>` already had one, and the resolver is
  one more `&'a` field beside `document` and `cursor`.
- **It is asked per caret**, in `auto_pair_edit_for`, so each caret gets its own
  answer.
- **It is lazy.** `Document::text` materialises the whole rope into a `String`;
  resolving eagerly per keystroke would put an O(n) copy on every arrow key. A
  `OnceCell` takes the text on the first question and never if none is asked —
  and `PairRules::has_any_suppression` is a bitmask test that stops the question
  ever being asked in a language whose manifest suppresses nothing, which is
  most of them.

#### ⭐ Rule L — when several rules produce the same refusal, a test of one must rule out the others

`auto_pair_edit_for` declines to insert a pair for **four** independent reasons:
the scope did not resolve (B-6 fail-open), `autoclose_before` (S-5), the
apostrophe-after-a-word-character rule, and now `not_in`. All four produce the
identical observable: one character where two would have gone.

The first draft of `scope_suppression_tests.rs` was green at every position and
was testing almost nothing. Two of its three carets sat in front of an ordinary
letter, which `autoclose_before` refuses on its own; the third was preceded by
a word character, which the apostrophe rule refuses on its own. **Removing the
whole `not_in` gate would have left those tests green.**

The fix is not a better assertion, it is a fixture that eliminates the
alternatives, and the elimination itself has to be a test:

- `the_fixture_resolves_to_the_three_scopes_the_tests_assume` — the tree parses
  and each caret resolves to the specific scope named.
- `every_caret_is_one_autoclose_before_permits` — types `(`, which Go never
  suppresses, at all three carets and requires it to pair.
- `every_caret_is_one_the_apostrophe_rule_permits` — types the same `'` at the
  same three carets with **no language set**, where no manifest is read at all,
  and requires it to pair.

Only then does "the quote did not pair" mean `not_in`. This is Rule J's family —
assert the premise — but sharper: the premise is not just "the probe was set up
right", it is **"nothing else could have produced this outcome."**

Red proof, with the gate removed: exactly the four suppression tests fail and
the eight guarding everything else stay green.

#### What landed, concretely

| where | what |
| --- | --- |
| `iridium-lang` `schema.rs` | `Manifest::suppression_scopes()` — the *vocabulary*, not the rules |
| `iridium-lang` `tests.rs` | ⭐ the ratchet: no manifest may name a `not_in` value the editor cannot translate |
| `editor/ast/scope.rs` | `CaretScopes` — lazy, per-byte, never parses |
| `editor/ast/state.rs` | a `Highlighter` beside the tree; `SyntaxState::caret_scopes` |
| `syntax_stubs.rs` | `spans_in_range` and `HighlightType::is_within`, mirroring the real ones |
| `behaviors.rs` | `SUPPRESSIBLE_SCOPES`, the per-scope masks, `has_any_suppression`, `suppressed_in`, the gate |
| `actions/mod.rs` | `CommandContext::scopes` |
| `keyboard/mod.rs`, `dispatch.rs` | one more parameter on `handle_key`, `dispatch_key`, `run_command` |
| `editor/core.rs`, `wasm.rs` | the six call sites, all asking the kernel rather than assuming |

#### Two smaller decisions, recorded rather than buried

- **`SUPPRESSIBLE_SCOPES` is a two-element array in the editor, and that is a
  hard-coded vocabulary.** It is defensible only because it is *checked*:
  `not_in_names_only_the_two_scopes_the_editor_can_translate` fails the build if
  any vendored manifest names a third. Without that test the array would be
  exactly the "hard-coded barrage" this project keeps refusing — a rule read
  from data, matched against a fixed list, and silently dropped on the floor
  when it does not match. Teaching the editor a third scope is one entry there
  and one arm in `HighlightType::is_within`.
- **The web face asks `state.syntax.caret_scopes(...)` rather than passing
  `CaretScopes::none()`.** It answers `none` today, for a reason that is true of
  the *build* (the browser compiles the kernel with `syntax` off) and not of the
  call. Writing the conclusion into six call sites would be a fact with a shelf
  life; the day a tree reaches that face, `not_in` starts applying by itself.
- **`KeyboardHandler::run_command` carries an `#[expect(too_many_arguments)]`**
  with its reason: its five state parameters are exactly `CommandContext`'s
  fields, and a public input struct would put a second name on the same set —
  one the kernel builds, one every caller builds — while `handle_key` beside it
  takes the same five and sits inside the limit.

#### ▶ What is left of the auto-pair map

**S-3** — multi-character openers (`"""`, `r#"`, `/*`). ⚠️ Note the interaction
this slice makes visible: `Manifest::pairs_suppressed_in` reports only
single-character rows, so a language's *effective* suppressed set today is a
subset of what its manifest declares. Rust declares `not_in` on four rows —
`r#"`, `r##"`, `r###"`, `/*` (all multi-character), `<` (`close = false`, so
not an auto-close rule at all) and `"` — and only the last of those reaches
`PairRules`. Nothing is wrong: each excluded row is excluded for a reason
already argued (S-3 for the multi-character ones, B-7's "a row that does not
close cannot be suppressed from closing" for `<`). It is worth knowing when
reading a manifest and wondering why a rule seems not to fire.

---

## 10. S-3 — multi-character openers. Ground verified 8 Aug 2026.

Everything below was parsed out of the vendored manifests, not remembered.

### 10.1 The whole data set

**Eight languages, twenty-four rows with `close = true` and more than one
character on a side, eighteen distinct openers.**

| language | rows |
| --- | --- |
| `python` | 14 — `f" f' b" b' u" u' r" r' rb" rb' t" t'` → the matching quote; `"""`→`"""`; `'''`→`'''` |
| `rust` | 4 — `r#"`→`"#`, `r##"`→`"##`, `r###"`→`"###`, `/*`→`" */"` |
| `c`, `cpp`, `go`, `javascript`, `typescript`, `tsx` | 1 each — `/*`→`" */"` |

Everything else in the tree with a multi-character side carries
`close = false` and is therefore not an auto-close rule at all: bash's
`do`→`done`, `then`→`fi`/`else`/`elif`, `in`→`esac`. Those are matching rules
for navigation, and S-3 must not start typing `done` for anybody.

⚠️ **The `/*` closer has a leading space**, `" */"`, in all seven languages that
declare it. That is deliberate in the source data: typing `/*` is meant to give
`/*| */`, not `/*|*/`. Nothing here should trim it.

### 10.2 ⭐ What Python's twelve prefix rows are actually for

`f"` → `"` looks like a no-op: the closer is the same `"` the single-character
row already inserts, so why declare it?

**Because the apostrophe rule refuses it.** `auto_pair_edit_for` keeps a quote
single when the character before the caret is a word character, so that `don't`
stays `don't`. `f` is a word character. So today, in Python, typing `f"` gives
`f"` with no closing quote — and the same for `rb"`, `t'`, and the other ten.

That is the entire user-visible payload of S-3 for Python, and it is a bigger
one than the triple quotes. It also fixes the ordering question before it is
asked: **the multi-character match must be tried before the apostrophe rule**,
or the twelve rows stay dead.

### 10.3 The matching rule

At the moment the last character of an opener is typed, the text **before** the
caret must be checked. For each declared opener whose final character is the
one being typed, the opener matches if the text before the caret **ends with**
the rest of it. **Longest match wins.**

Longest-match is load-bearing, not defensive: at `rb|` typing `"`, both `b"`
(text ends with `b`) and `rb"` (text ends with `rb`) match, and only the longer
is right. At `r##|` typing `"`, `r#"` does **not** match — `"r##"` ends with
`"##"`, not `"r#"` — so the three Rust raw-string widths separate correctly by
this rule alone, with no special casing.

⚠️ **A false match is possible and mostly harmless.** Typing `"` after any
identifier ending in `b`, `f`, `r`, `t` or `u` matches a Python prefix row —
`verb"`, `def"`. The closer those rows declare is the same `"` the
single-character row would have inserted, so the outcome is identical; the only
difference is that the pair now fires where the apostrophe rule used to refuse
it, which is exactly the intent. The rows whose closer *differs* (`"""`, `'''`,
`r#"`, `/*`) all need at least one non-word character in their prefix, so they
cannot be reached by ordinary identifier text.

### 10.4 Skip-over — the part the manifests do not specify

⚠️ This is S-3's B-5: the data declares the delimiters and says nothing about
what typing them again should do.

**The existing single-character rule already does the right thing for `"""`.**
`"""abc|"""` typing `"` steps over one quote, three times, and the string
closes. Making a multi-character closer skip *as a whole* would break that:
the first `"` would jump all three and the user's next two keystrokes would
open a new pair. Muscle memory types three quotes.

**But it is wrong for Rust's `"#`.** `r#"abc|"#` typing `"` steps over the
quote, then typing `#` inserts a second one — `r#"abc"#|#`. The `#` is not a
single-character closer, so nothing steps over it.

The rule that fixes the second without touching the first is a
**generalisation of the existing one, not a second rule beside it**:

> Typing `c` steps over the character at the caret when `c` equals that
> character **and** the text before the caret, plus `c`, ends a closer this
> language declares.

A single-character closer satisfies this with an empty prefix, so today's
behaviour is unchanged by construction. `"#` satisfies it in two steps: `"`
alone is a declared closer in Rust, then `"` + `#` is `"#`. And a bare `#`
typed anywhere else — before a `#[derive]`, say — does not, because the
character before the caret is not a `"`.

⚠️ **What this does not fix, stated rather than discovered later**: `" */"`
cannot be stepped over from the caret position the insertion leaves. `/*| */`
typing `*` finds a space at the caret, not a `*`. Reaching the `*` means typing
the space first, which nobody does. The case is left alone because a user does
not type a closer the editor already wrote; it is recorded so the next reader
knows it was considered.

### 10.5 Where the data lives

`PairRules` is `Copy` and built once per keystroke, so it cannot own a `Vec` of
borrowed pairs. It gains **`Option<&'static Manifest>`** instead —
`Language::manifest` already returns `&'static`, so this is one pointer, and it
keeps `iridium_lang` the owner of the rules rather than copying them into a
second shape. The multi-character walk then happens only when it can matter:
the openers' final characters become triggers, and the walk is skipped
entirely for a language declaring no multi-character rows, which is fourteen
of the twenty-two.

⚠️ **`*` and `#` must become triggers.** `handle_char_input` enters auto-pair
handling only when `PairRules::is_trigger(c)` says so, and today that is the
six single characters. `/*` ends in `*`, which is in no pair — so without this,
the `/*` rows can never fire however correct the matching is. This is the one
place where S-3 changes a *reachability* condition rather than a decision.

### 10.6 Order inside `auto_pair_edit_for`

The collapsed-caret branch becomes, in order:

1. **Skip-over** (§10.4's generalisation).
2. **Multi-character opener**, longest match — subject to `autoclose_before`
   and `not_in`, exactly as the single-character path is.
3. **The apostrophe rule** — after 2, so Python's prefix rows survive it.
4. **Single-character opener**, unchanged.

### 10.7 Build order

1. `iridium-lang`: a manifest accessor for the multi-character closing rows,
   with the same `None`/`Some(empty)` discipline as its three siblings, and the
   `close = false` filter that keeps bash's `done` out.
2. `PairRules`: hold the manifest; the trigger set grows to include every
   opener's final character; longest-match lookup.
3. `auto_pair_edit_for`: the multi-character insertion branch, in the order
   above.
4. The generalised skip-over.
5. Backspace: an empty multi-character pair around the caret collapses in one
   keystroke, the way `(|)` already does.

⚠️ Each step needs its own red proof, and §9.3's **Rule L** applies with full
force here: by the time S-3 lands there are *five* independent reasons a pair
may not appear, and the apostrophe rule is the one that will silently make a
Python test pass for the wrong reason.

### 10.8 Three rulings the build raised, 8 Aug 2026

The agent that built steps 4 and 5 implemented §10.7 as written and then said
where it thought the spec was wrong, rather than quietly choosing. It was right
on the first and it is the more interesting of the three.

#### ⭐ B-8 — §10.7 STEP 5 IS WRONG AS WRITTEN. Backspace restores the buffer, it does not collapse the pair.

§10.7 step 5 says *"an empty multi-character pair around the caret collapses in
one keystroke, the way `(|)` already does."* Implemented literally, that gives:

```text
print(f"|")   backspace   →   print()
```

**taking the `f` the user typed before the editor had done anything.** At
`print(f|)` typing `"` inserts exactly two characters; one backspace then
removes three. That breaks the identity `(|)` has and that every user relies on
without naming: **backspace immediately after an auto-pair puts you back where
you were.**

**The rule is therefore: delete the closer the editor wrote, plus the single
character the user typed to trigger it — never the prefix that was already in
the buffer.**

| before the keystroke | after typing | correct backspace | "whole pair" would give |
| --- | --- | --- | --- |
| `\|` | `(\|)` | `\|` | `\|` — agrees |
| `print(f\|)` | `print(f"\|")` | `print(f\|)` | `print(\|)` ❌ eats the `f` |
| `r#\|` | `r#"\|"#` | `r#\|` | `\|` ❌ eats `r#` |
| `""\|` | `"""\|"""` | `""\|` | `\|` ❌ eats two quotes |

⭐ **Why this was worth catching, in the terms this file already uses.** "Collapse
the whole pair" and "undo what the insertion wrote" are **a proxy and its
target that agree on every single-character opener** — because there the opener
*is* the trigger character — **and diverge on exactly the case S-3 exists to
add.** The examined set at the time the spec was written contained nothing that
could tell them apart. That is the Proxy Law with the divergent case named, and
the spec is the thing that was wrong, not the implementation of it.

⚠️ The tidiness objection is real and loses: `r#|` is not valid Rust, so a
whole-pair collapse leaves a cleaner buffer. But `print(f|)` leaves a bare `f`
and that is *exactly what the user had*, which is the property that matters —
and tidiness is not even reachable, because nothing can know how much of a
prefix the user wanted gone. One more backspace is cheap; a re-typed character
the editor ate is not.

#### B-9 — skip-over reads `multi_char_close_pairs()`, not `close_rules()`. Blessed.

Markdown declares `<` → `>` with `close = true`. §10.4's literal words — *"ends
a closer this language declares"* — would make `>` a skip-over character there,
stepping over a `>` **this editor never wrote**, because `mask_of` drops `<`/`>`
pending S-2. The agent read the rule as *declares **and would insert*** and
restricted the multi-character half accordingly.

That is right, and the general form is worth stating: **a skip-over may only
step over a character the editor would itself have written.** Anything else
silently discards a keystroke. It also keeps step 4 from pre-empting S-2.

#### B-10 — the ` */` false positive is accepted, and must be pinned as accepted

`/` is now a trigger in the seven block-comment languages, so at `a *|/ b`
typing `/` steps over, because `a *` + `/` ends ` */`. It is the same accepted
class as the `"#` one at §10.3: narrow, reachable only from text that already
has the closer's shape.

⚠️ **It must be pinned by a test that names it as accepted**, the way
`a_hash_after_any_quote_steps_over_one_that_is_already_there` does. Rule F: an
unpinned known case is a comment claiming something nothing checks, and it will
drift the first time the trigger set moves.

#### 🔴 A figure this file got wrong, corrected

§10.5 says *"thirteen of the twenty-one"*, and the same figure is repeated in
`crates/iridium-lang/src/manifest/schema.rs`'s doc comment on
`multi_char_close_pairs`. **Measured: there are 22 vendored `config.toml`
manifests, 8 declare a multi-character closing row, and 14 do not.** Both places
need the correction; the doc comments in `multi_char_pairs.rs` already use the
right figures, so the tree currently disagrees with itself.

### 10.9 What the adversarial verifier found — and B-10 is REVERSED

Three agents verified the built slice: a Rule-L auditor, a correctness
adversary, and a gate runner. **All nine gates pass — 2,644 / 1,111 / 1,230,
zero failed.** The Rule-L audit came back clean: every fixture column
re-derived from the raw text rather than the comment, no off-by-one anywhere,
and no test green for the wrong reason. **The correctness adversary returned
defects, and the slice is not committable until they are fixed.**

#### 🔴 DEFECT 1 — backspace destroys user-typed characters. B-8 is the fix.

`longest_pair_between` matches a row on `before.ends_with(row.start())` alone,
then `empty_pair_around` deletes `row.start().chars().count()` characters to the
left. Nothing checks those characters were ever part of a pair.

```text
Python:  print(verb"|")   Backspace   →   print(ve)
```

`before` ends with `rb"`, because the last three characters of `print(verb"` are
`r`, `b`, `"`. Longest wins, so **`rb` is eaten out of the user's identifier.**
Same shape at `it'|'` → `i`. And it is reachable in two keystrokes entirely
inside tested behaviour, because `a_quote_after_an_identifier_ending_in_a_prefix_letter_now_pairs`
asserts that typing `"` at `print(verb|)` produces exactly that buffer: **type a
quote, change your mind, press Backspace, lose two characters of your variable
name.**

⭐ **§10.3's harmlessness argument does not transfer, and that is the whole
lesson.** On the *insert* path a false prefix match is harmless because the
contested rows declare the same closer, so both write the same text. On the
*delete* path the **opener's length alone decides how much is destroyed**. The
argument was sound where it was made and was carried to a path where its
premise does not hold.

⚠️ **1b — the two paths disagree about whether a row exists.** B-7 exempts the
backspace collapse from `not_in`; insertion is *not* exempt. So in Python
`# verb"|"` inside a comment, `insertion_for` refused the `rb"` row (it declares
`not_in = ["string", "comment"]`) and the plain `"` row wrote the pair — then
backspace applies `rb"` anyway and gives `# ve`. **A pair the multi-character
rule was forbidden to create is destroyed by the multi-character rule.**

**B-8 fixes both by construction**, which is why it is worth landing exactly as
§10.8 states it: the bounded rule never consults the opener's length at all, so
there is no over-claim to get wrong and no second opinion about which rows
exist. That the ruling and the defect arrived independently and have one fix is
the strongest evidence either of them is right.

#### 🔴 B-10 IS REVERSED — `/` must NOT be a trigger

An hour ago this file accepted the ` */` false positive and asked only that it
be pinned. **That was wrong, and the adversary's example is what shows it:**

```text
JavaScript:  const s = "a *|/b";   type /   →   nothing happens
```

`before` ends with `" *"`, the character at the caret is `/`, so the keystroke
is discarded. Skip-over is exempt from `not_in`, so being inside a string
literal does not rescue it.

⭐ **The decisive argument is one this file already contains and I failed to
apply.** §10.4 records that `" */"` **cannot be stepped over from the caret its
own insertion leaves** — `/*| */` typing `*` finds a space at the caret, not a
`*`, and reaching the `*` would mean typing the space first, which nobody does.

So for the block-comment rows, generalised skip-over is **unreachable in the
case it exists to serve and reachable only in false-positive cases.** It buys
exactly nothing and costs a swallowed keystroke. Accepting it was trading a real
harm for a benefit that does not exist.

**The rule: a closer joins the skip-over trigger set only if it is reachable
from the caret its own insertion leaves.** A closer whose first character is a
space never is, because stepping proceeds character by character from position
zero. That drops `/` from the trigger set and leaves `#` in it — `r#"|"#` has
`"` at the caret, which is the closer's first character, so `"#` is genuinely
reachable and its false positive stays accepted and pinned, as B-10 originally
said of it.

⚠️ **Silently discarding a keystroke is a worse failure than writing an
unwanted character**, because the user's correction for the second is Backspace
and their correction for the first is to wonder whether the keyboard is broken.
That asymmetry should govern any future skip-over question.

### 10.10 B-11 — the delete was bounded on the left and not on the right. And the law behind all three.

🔴 **§10.9's claim that "B-8 fixes both by construction" is FALSE for any closer longer
than one character, and this file must stop saying it.** The same claim sits in
`multi_char_pairs.rs`'s module doc and must go from there too.

B-8 bounded the *left* side to one character — the trigger the user typed — and that part
holds. The *right* side is still sized by `closer.chars().count()`, **with nothing checking
the editor ever wrote that closer.**

#### The case, in Rust, on the shipped path

```rust
fn main() {
    /* keep */
}
```

Caret after `keep`. Type `/` — no longer a trigger, so it is written plainly:
`/* keep/ */`. Now type `*`:

- skip-over declines (the character at the caret is a space)
- `insertion_for` finds the opener `/*`, `autoclose_before` permits — and **`not_in` refuses,
  because the caret is inside a comment and Rust declares `/*` with
  `not_in = ["string", "comment"]`**
- so one character is written and **no closer**

Buffer: `/* keep/*| */`. One Backspace, and the collapse matches `/*` behind and `" */"` in
front, and deletes four characters:

```text
    /* keep/          ← the */ that closed the enclosing comment is GONE
```

**The rest of the file is now commented out**, from one typed character and one Backspace.
The single-character branch would have deleted exactly one. The multi-character rule is
purely additive harm here.

⭐ **Why it survived a fix that was aimed at exactly this class.** All three reproductions
the fix was verified against — `print(verb"|")`, `it'|'`, `# verb"|"` — are **Python, where
every prefix row's closer is a single `"` or `'`.** For a one-character closer the
multi-character answer and the single-character answer are byte-identical, so the
disagreement cannot be observed. **The verification set could not distinguish the fix from
its absence on the right-hand side.** That is the Proxy Law applied to a *test suite* rather
than to code, and it is the third time this slice has produced it.

#### ⭐⭐ THE LAW, since three separate defects in one slice are the same sentence

> **A reversal must be sized by what the editor would have written — never by what the
> manifest declares.**

- **B-9** — a skip-over may only step over a character the editor would itself have written.
  (Markdown declares `<`→`>`; the editor does not write it; stepping would eat a keystroke.)
- **B-8** — backspace's *left* bound is the one character the user typed, not the opener the
  manifest declares.
- **B-11** — backspace's *right* bound is the closer **the editor actually wrote**, not the
  closer the manifest declares.

Every one of the three read a declaration as evidence of an action. The manifest says what
*may* happen; only the insertion path says what *did*.

#### The ruling

**B-7 is narrowed, not reversed.** It still stands that `not_in` gates the insertion of a
closer and does not gate the single-character collapse. But **the multi-character collapse
must consult `not_in` at the caret**, because it is the only thing that can tell a closer the
editor wrote from a closer that was already in the buffer. Suppressed row → decline, and fall
through to the single-character answer.

⚠️ `autoclose_before` deliberately is **not** re-checked. At collapse time the closer's
presence has been *observed*, and `autoclose_before` would be evaluated against a different
character than it saw at insertion. `not_in` is the gate that could have refused this row at
this position, and it is the one that must be asked.

⚠️ **This needs a test where `not_in` is live**, and no existing harness can host it: the S-3
section of `scope_suppression_tests.rs` has three insertion tests and **no backspace test**,
while `multi_char_backspace_tests.rs` runs on a harness where `CaretScopes::none()` makes
`not_in` unreachable by construction. **The one path where the disagreement can appear is the
one path with no test on it** — which is why the defect was invisible, and fixing the
coverage hole is part of the fix, not a follow-up.
