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

### 1.3 ⚠️ The seven divergences, each with its symptom

Not a list of nice-to-haves. Each line is a thing the editor does today that
the language says it should not.

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
6. **Markdown: `*` is not paired and the manifest says it should be.** Typing
   `*` for emphasis does not close. The one divergence in the *other*
   direction — a pairing the language wants and does not get.
7. **Every language's multi-character openers are unreachable.** The table is
   keyed by `char`, so Python's `"""`, `f"` and `rb'`, Rust's `r#"`, the
   `/* */` that six languages declare, and bash's `do` / `then` / `in` cannot
   be expressed in it at all.

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

### S-2 — additions

Markdown's `*`. Divergence 6, and the only one that makes a pairing appear
where none was. One character, one language, and a user typing `*text*` in
Markdown is the case it helps. **Wants a ruling** — it is the only slice
where someone could reasonably prefer today's behaviour.

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
  this as ruling-free under #66 and will build it unless told otherwise.
- **B-2 — S-2, Markdown's `*`.** Recommend **yes**, but it is the one slice
  with taste in it, so it is asked rather than assumed.
- **B-3 — S-3 and S-5 order.** Recommend **S-5 before S-3**: `autoclose_before`
  is a smaller change that improves every language, where multi-char openers
  mostly serve Python and Rust raw strings.
- **B-4 — S-4 at all?** Recommend **not yet**. It is the only slice that puts
  a syntax query on the typing path, and the benchmark claim that typing never
  parses is worth more than `/*` behaving perfectly inside a string literal.

⚠️ **None of B-1..B-4 blocks the doc corrections**, which are unambiguous:
three module docs currently state that auto-pairs come from the manifest, and
they do not. Those are being corrected in the same commit as this map, so no
reader is told a capability exists before it does.
