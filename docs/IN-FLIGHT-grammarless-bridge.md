# A file with no grammar wears another language's keywords

**Found 8 Aug, by asking what an exported function with no callers was for.**
Everything below was **run**, not read.

## The sentence that pointed at it

`crates/iridium-syntax/src/grammar.rs:92-99` documents `has_grammar` as

> The question a caller usually wants answered before deciding a syntax-driven
> feature is available. Asking this is better than constructing a `SyntaxTree`
> and discarding the error […]

`SyntaxTree::new`'s own docs repeat it. A `grep` over the workspace finds
`has_grammar` **re-exported at `iridium-syntax`'s crate root and called by
nothing outside the crate** — only its own tests. An exported answer that
nobody asks is either dead API or a missing wire-up; this was the second.

## What the registry says will happen

`crates/iridium-lang/src/languages/languages.txt`, on the four languages
ruling R-1 admitted with no grammar linked — `diff`, `gitcommit`, `gomod`,
`gowork`:

> They **will not highlight** until a grammar is linked, and that is the
> expected state rather than a defect.

## What actually happened

They highlighted. In another language's colours.

Three verified links:

1. **`WindowedSpanCache::refresh_windowed`** (`windowed.rs:162`) set
   `self.language_active = state.syntax.language().is_some()`.
2. **`Highlighter::try_new(language)`** answers `None` for a grammarless
   language — there is no grammar to compile a query against — so the cache
   takes its "the query did not compile" degradation path: entry empty,
   **`language_active` still true**.
3. **The compositor** (`render/compositor/shaping.rs:234` and `:280`) reads
   exactly that pair:

   ```rust
   if let Some(rich_spans) = highlights.resolve(&context) { … }
   else if highlights.language_active() {
       // The bridge: a language is set but its spans are not here
       // this frame — the keyword highlighter colors until they are.
       let spans = self.highlighter.highlight_flat(&self.cpu_visible_content);
   ```

`self.highlighter` is `SimpleHighlighter`, whose `is_keyword`
(`render/simple_highlight.rs:396`) is the **union of the Rust, JavaScript,
TypeScript and Python keyword sets**.

⚠️ **The worst case is the one `git commit` opens.** A commit message is
prose, and the union contains `for`, `in`, `as`, `if`, `else`, `match`,
`move`, `type`, `new`, `from`, `try`, `with`, `where`, `while`, `case`,
`class`, `default`, `super`, `self`, `return`, `use`, `void`. An ordinary
sentence comes out speckled in keyword colour. A `.diff`, a `go.mod` and a
`go.work` get the same treatment.

## ⭐ The divergence

`language().is_some()` is a **proxy** for *"spans are owed but have not
arrived this frame"* — which is the only thing the bridge is for. It agrees
with its target for every language that has a grammar, including the one the
degradation path was written for (a grammar whose bundled `highlights.scm`
fails to compile: spans really are owed, and really are late).

It **diverges for a language with no grammar at all**: nothing is owed, ever.
The bridge is not a bridge, it is the permanent state.

Both states leave the cache entry empty, and both make `try_new` answer
`None`. ⭐ **Only `has_grammar` tells them apart** — which is precisely what
its own documentation says it is for, and why it having no caller was the
symptom rather than a tidy-up.

## The rule was already written, in two places

This needed no ruling because nobody had to choose anything. The rule is
already stated, twice, by the code that was breaking it:

- `render/compositor/highlight.rs:63-64`, on `HighlightSource::language_active`:
  *"a file without a grammar must never wear another language's keyword
  colors."*
- `span_index/windowed.rs`, on `refresh_windowed`: *"the face renders the
  plain foreground, because a file without a grammar must not wear another
  language's keyword colours."*

Both sentences say "grammar". Both were implemented as "language".

## The fix

One line, plus the docs that had it wrong:

```rust
self.language_active = state.syntax.language().is_some_and(has_grammar);
```

`has_grammar` is re-exported through `crate::syntax` alongside the rest of the
`iridium_syntax` surface the kernel already forwards. `span_index` is
`#[cfg(feature = "syntax")]` (`lib.rs:68-69`), so no stub counterpart is
needed and the parser-free kernel is untouched.

**Both native faces inherit it**, because both read this one cache — the
parser-tax map's R2 ruling. The terminal face has no keyword bridge at all
(no `HighlightSource` impl in `iridium-tui`), so it already rendered these
files plain; the desktop face is where the colours were wrong, and it now
agrees with the terminal. The web face answers `language_active` from its own
notion of a set language, which its host owns through `set_syntax_enabled` —
deliberately out of scope, and documented as such on the trait.

## The red test, and what red looked like

`a_language_with_no_grammar_is_reported_inactive`, in
`span_index/windowed.rs` — all four grammarless languages, each with a
plausible document.

```
thread '…::a_language_with_no_grammar_is_reported_inactive' panicked at
crates/iridium-editor/src/span_index/windowed.rs:494:13:
diff has no grammar: there is nothing to bridge to, and a file without a
grammar must never wear another language's keyword colours
```

⭐ It failed on the **second** assertion, not the first: `index().is_none()`
already passed. The spans genuinely never exist — so the bridge really was
the only thing painting those files, and the fix removes colour that came
from nowhere rather than colour that came from somewhere better.

## Status — LANDED

- [x] `has_grammar`'s zero callers confirmed by grep across `crates/` and `apps/`.
- [x] The three-link chain read at named lines, not inferred.
- [x] Red test over all four grammarless languages, proven red first.
- [x] One-line fix; `has_grammar` re-exported through `crate::syntax`.
- [x] Field, accessor and `refresh_windowed` docs corrected — each stated the
      grammar rule while implementing the language rule.
- [x] **Nine gates green**: 2,514 passed, 0 failed.

## What this leaves behind

**The web face's `language_active` is still `true` unconditionally**
(`wasm.rs:3079`). That is documented as the host's business — its grammars
live in a JavaScript worker the kernel cannot interrogate — so it is not the
same defect. But the *symptom* is available there too: a browser host that
opens a `.diff` and leaves `set_syntax_enabled` on gets the keyword bridge for
the same reason. Naming it rather than fixing it, because fixing it means
deciding what the TypeScript side knows about grammar linkage, which is L-0
territory.
