# Extensible language support — design map

**Raised by Tom, 7 Aug 2026:** *"Can we make sure that our language support is
extensible? I don't want to be hyper-specialising and hard coding in keywords.
We want a Zed-like extensibility system. We've got our own language that we work
with, AWL, and we've got tree-sitter packages for that."*

This map verifies the ground, prices the paths, and numbers the decisions. It
does not change any code. **Nothing here should be built before Tom has ruled.**

Everything below was checked by reading the code and the dependency manifests,
not recalled.

---

## 1. How closed it actually is

### The Rust side is fully static

`Language` (`crates/iridium-lang/src/lib.rs`) is a **closed enum of 13
variants** with:

- a `COUNT` associated constant,
- an `index()` that is a *numeric slot*, not a label,
- `all()` returning `&'static [Self; COUNT]`.

That index is load-bearing. `crates/iridium-syntax/src/query/mod.rs:71` declares

```rust
static COMPILED: [[OnceLock<Compiled>; KIND_COUNT]; LANGUAGE_COUNT] = ...;
```

and looks queries up by `COMPILED[language.index()][kind.index()]`. **A language
set that can grow at runtime cannot be a fixed-size array indexed by an enum
discriminant.** This is the single most structural blocker, and it is not
cosmetic — the array's type is derived from `Language::COUNT`.

Four further tables match exhaustively over the enum, each of which would have
to become data an extension supplies:

| File | What it hard-codes | Rows |
| --- | --- | --- |
| `iridium-syntax/src/query/embedded.rs` | Every `.scm` file, `include_str!`'d | 78 pairings |
| `iridium-syntax/src/grammar.rs` | The tree-sitter grammar per language | 13 |
| `iridium-syntax/src/folding/language.rs` | Foldable / comment / import node kinds | 13 |
| `iridium-editor/src/input/keyboard/comments.rs` | Line token and block pair | 13 |

Plus `Language::id`, `from_id`, `from_extension` and `extensions` in
`iridium-lang` itself, and `Language::all()` in `iridium-bindings/src/lib.rs`.

The grammars themselves are **thirteen statically linked Rust crates**
(`Cargo.toml:54-66`, `tree-sitter-rust`, `tree-sitter-python`, …). Adding AWL
today means editing `Cargo.toml`, editing five tables, and recompiling Iridium.

`embedded.rs`'s module doc argues the static design deliberately: *"a face
running in a browser, a terminal or an editor pane has no guaranteed filesystem,
and a query read at runtime is a query that can go missing after install."*
That reasoning is sound and does not disappear — it becomes an argument about
**what ships in the box**, not about whether anything can be added to it.

### The web side is already dynamic — and nobody said so

This is the find that changes the shape of the work.

`packages/@iridium/syntax-worker/src/worker.ts:13` imports `decodeGrammar` and
`AVAILABLE_LANGUAGES` from `@iridium-editor/core/syntax`. That resolves to
`packages/@iridium/core/src/syntax/grammars.gen.ts` — **tree-sitter grammars
compiled to WebAssembly and base64-embedded**, loaded into `web-tree-sitter` at
runtime.

And `packages/tree-sitter-builder/build.ts` — 2 tracked files, already written —
**clones grammar repos and compiles them to wasm**, extracting `highlights.scm`
alongside. Its `.cache/` holds **19 already-built `.wasm` grammars** (1–5 MB
each, 138 MB total, untracked) including several the Rust side has never heard
of: html, java, lua, ruby, toml, zig.

So the browser face does not need extensibility built — **it needs it exposed**.
The static half is the *native* half, which is the reverse of the assumption in
`embedded.rs`'s doc.

### Zed's own manifests are already vendored, and unread

`crates/iridium-syntax/src/languages/queries/` holds **21 directories, 768 KB**,
lifted from Zed. Eight are unreachable today because no grammar is registered
for them: `diff`, `gitcommit`, `gomod`, `gowork`, `jsdoc`, `jsonc`,
`markdown-inline`, `regex`.

Each directory carries a `config.toml`. Here is `json/config.toml` in full:

```toml
name = "JSON"
grammar = "json"
path_suffixes = ["json", "flake.lock"]
line_comments = ["// "]
autoclose_before = ",]}"
brackets = [
    { start = "{", end = "}", close = true, surround = true, newline = true },
    ...
]
tab_size = 2
```

**Nothing in the repository reads these files.** Verified: the only `config.toml`
matches in Rust source are the user-settings file from #59 and one comment.

That manifest already carries, as data, what three of the four hard-coded tables
carry as code: the extension list (`path_suffixes`), the comment token
(`line_comments`), and the bracket pairs. It does **not** carry fold node kinds.

---

## 2. What chiron actually does — read, because Tom pointed at it

Tom named `chiron/crates/syntax` and `chiron/crates/lsp` as the source. Both
were read. The honest summary is that **chiron does not have runtime
extensibility either**, and saying otherwise would send this whole design the
wrong way.

`chiron/crates/syntax` is the same shape Iridium's was ported from: a **closed
`Language` enum**, an `include_str!` query table (`query/embedded.rs`, 952
lines), and grammars resolved by exhaustive match. It is simply *wider* — 28
grammar dependencies against Iridium's 13, covering cypher, glsl, nu, proto,
sequel, toml, tsquery, zig, gomod, gowork, jsdoc, gitcommit and more. Searched
for `read_to_string`, `fs::read`, `dlopen`, `libloading` and `wasm` across the
crate: **no runtime loading of anything.**

Two things there are worth taking regardless of which tier is chosen.

**The `build.rs` vendoring trick, which is the answer to the problem AWL will
hit.** `chiron/crates/syntax/build.rs` compiles tree-sitter grammars **from C
source** with `cc`, and `language.rs` declares them via `extern "C"` +
`LanguageFn::from_raw`. Its stated reason:

> *"These grammars are vendored because their published crates have incompatible
> `tree-sitter` version constraints (they depend on tree-sitter ~0.20 but we use
> 0.26)."*

Three grammars — just, typst, wgsl — are carried this way. This matters directly:
a private grammar like AWL almost certainly has no published crate at all, and
even if it did, `links = "tree-sitter"` means two tree-sitter versions cannot
coexist in one dependency graph. **Vendoring the generated `parser.c` sidesteps
both problems entirely, and it is already proven in Tom's own codebase.**

**`chiron/crates/lsp/src/extension/`** is an extension system, but for language
*servers*, not grammars: an `LspExtension` async trait with defaulted no-op
methods, implemented in-process and registered alongside real servers. It is a
**compile-time seam** — a clean interface with a default implementation — not a
plugin loader. That is a real and reusable pattern, and it is a different thing
from "drop a folder in and restart".

So chiron's answer to "don't hyper-specialise" is: *breadth, data-driven
queries, and clean seams* — not runtime plugins. That is a legitimate answer and
it is much cheaper than the alternative. It should be on the table as tier 1
below rather than skipped past.

---

## 3. The path, and what it costs

There are **three tiers** here, not two, and they cost very different amounts.
"Extensible" could honestly mean any of them, which is why L-0 below is the
first thing to rule on.

| | What it means | Adding AWL | Rebuild Iridium? |
| --- | --- | --- | --- |
| **Tier 1** | Nothing hard-coded: grammars vendored as C source, queries and per-language behaviour as data files. What chiron does. | drop in `parser.c` + `.scm` files + a manifest, add one `build.rs` line | yes |
| **Tier 2** | Tier 1, plus runtime loading of wasm-compiled grammars from a directory | drop a folder in, restart | no |
| **Tier 3** | Tier 2, plus extensions that carry LSP servers, themes and commands | install an extension | no |

Tier 1 removes every hard-coded table and is a **prerequisite for both others** —
the enum has to stop being an enum either way. Tier 2 adds a runtime loader and
a dependency. Tier 3 is a product, not a feature.

**The work is largely shared.** Whichever tier is chosen, `Language` stops being
a closed enum, `COMPILED` stops being a fixed array, and the four hard-coded
tables become manifest fields. Tier 1 alone gets Tom AWL highlighting without a
new dependency; tier 2 is a strictly additive second step once the registry
exists.

### If tier 2 or 3: the mechanism

`tree-sitter 0.26` — the version already in `Cargo.lock` — ships a
`wasm` feature:

```toml
wasm = ["std", "wasmtime-c-api"]
```

That is the mechanism Zed uses: grammars compiled to WebAssembly, loaded at
runtime through wasmtime, sandboxed. No `dlopen`, no native ABI matching, no
per-platform grammar builds. **The capability exists in the dependency we
already have.** It is not currently enabled.

### The rejected alternative, named so it stays rejected

**Loading native dynamic libraries** (`.so` / `.dylib` / `.dll`) is the other way
to make grammars pluggable, and it should not be chosen:

- The grammar must be compiled against a matching tree-sitter ABI, and a
  mismatch is a segfault rather than an error.
- It is unsandboxed: an extension gets the editor's full process rights, in a
  program that CLAUDE.md scopes to financial, legal and healthcare settings.
- It needs a separate build per platform and architecture, so AWL would need
  four artefacts instead of one.

Wasm has none of those properties. Its cost is speed: a wasm grammar parses
slower than a natively compiled one. Zed accepts that trade for every language
it does not ship in the box.

### The measured cost of tier 2

Measured, not estimated. Two minimal binaries, identical but for the feature
flag, both `--release` with `strip = true`, built 7 Aug 2026:

| | packages | stripped binary | clean build |
| --- | --- | --- | --- |
| `tree-sitter = "0.26"` | 25 | **341,488 B** (334 KiB) | 24 s |
| `tree-sitter` + `wasm` | 115 | **7,280,512 B** (6.94 MiB) | 3 m 42 s |

**Enabling the `wasm` feature costs 6.6 MiB of binary and pulls 90 further
crates** — the whole of wasmtime and cranelift, a JIT compiler. Clean build time
goes up roughly ninefold on this measurement, and `target/` for the toy crate
went from 62 MB to 524 MB.

That is the honest price of tier 2 on every native face, paid whether or not
anyone ever installs an extension. It is not disqualifying — Zed pays it — but
it is a real number and it is much larger than tier 1's, which is zero.

*(The scratch crates resolved `tree-sitter 0.26.11` rather than the workspace's
pinned `0.26.3`, since `"0.26.3"` is a caret range. The `wasm` feature is
present in both; the size delta is not sensitive to the patch version.)*

---

## 4. What an extension would be

Proposed shape, deliberately close to what is already vendored so that the 21
Zed manifests work nearly unchanged and an existing Zed language extension is a
short port rather than a rewrite:

```
<languages-dir>/awl/
  config.toml        # Zed's schema, plus what it lacks (see L-6)
  grammar.wasm       # the tree-sitter grammar, compiled once
  highlights.scm
  brackets.scm       # optional
  indents.scm        # optional
  injections.scm     # optional
  outline.scm        # optional
  textobjects.scm    # optional
```

`Language` stops being an enum and becomes a **handle issued by a registry** —
an opaque id whose `index()` is a registry slot rather than a discriminant.
`COMPILED` stops being a fixed 2-D array and becomes per-language state owned by
the registry entry. Everything downstream that today matches on the enum instead
*asks the entry*.

The failure rules are already written and should be reused verbatim from
`iridium-config` (#59), because they are the same problem: **a bad extension
never stops the editor; a mistake costs its own line; a refused extension is
reported, never swallowed; no extensions is not a problem.**

---

## 5. Decisions for Tom — L-0 to L-8

**L-0 — Which tier? This is the one that decides everything else.**
Tier 1 (nothing hard-coded, rebuild to add a language — what chiron does) adds
**no dependency and no bytes**, and gets AWL highlighting. Tier 2 (drop a folder
in, no rebuild) costs a **measured 6.6 MiB and 90 crates** on every native face,
paid whether or not anyone installs an extension. Tier 3 is a product.

The recommendation is **build tier 1 now and decide tier 2 afterwards.** Tier 1
is unavoidable groundwork for tier 2 — the enum has to stop being an enum either
way — it is useful the day it lands, and it defers a 6.6 MiB decision until
there is a second person who needs to add a language without a compiler.

**L-1 — Do the thirteen built-in languages become extensions too?**
*One path:* everything is an extension, built-ins just ship in the box and load
from a bundled directory. Simplest to reason about, one loader, no privileged
tier — but every built-in language then parses through wasm and gets slower.
*Two paths:* built-ins stay statically linked and fast; extensions go through
wasm. Keeps today's performance, at the cost of two code paths for one concept
and a permanent second-class tier for anything Tom adds — including AWL.

**L-2 — Native grammar host: wasmtime, confirmed?**
The map recommends yes and rejects `dlopen` for the reasons in §2. Naming it as
a decision rather than an assumption.

**L-3 — Where do extensions live?**
`~/.config/iridium/languages/<name>/`, beside the `config.toml` from #59, is the
obvious answer. Open: does a *project* also get to carry languages
(`.iridium/languages/`), so a repository can bring its own grammar without a
global install? That is useful and it is also a code-execution vector — opening
someone's repository would load their wasm.

**L-4 — Manifest schema: adopt Zed's `config.toml` or define our own?**
Adopting it means the 21 vendored directories become extensions with almost no
edit, and any existing Zed language extension is a short port. It also means
carrying fields Iridium has no use for. Defining our own is cleaner and throws
that compatibility away.

**L-5 — Browser parity.**
The browser already runs wasm grammars; what it lacks is a way to *add* one.
Options: extensions bundled at build time only (what happens today, honestly
documented); or a runtime load from a URL or a user-picked file, which is a real
feature and a real security surface.

**L-6 — Fold rules and comment tokens.**
Zed's `config.toml` carries `line_comments` and `brackets` but **not** foldable
node kinds — Iridium's `folding/language.rs` invented that table. Either add a
field to the manifest, or derive folds from the vendored `outline.scm` /
`brackets.scm`, which would remove a hand-maintained table entirely. Worth
checking whether the queries can answer it before adding a field.

**L-7 — Grammar/ABI version checking.**
A grammar is compiled against a tree-sitter language ABI version. One built
against a different one must be *reported*, not crashed on. Needs a version
check at load and a stated support range. Applies to tier 2; under tier 1 the
compiler enforces it.

**L-8 — How does AWL's grammar get in, concretely?**
Under tier 1 the answer is chiron's: vendor the generated `parser.c` (and
`scanner.c` if it has one) under `crates/iridium-syntax/grammars/awl/`, compile
it in `build.rs` with `cc`, declare it with `LanguageFn::from_raw`. That avoids
both the "no published crate" problem and the `links = "tree-sitter"` collision
that stops two tree-sitter versions coexisting. It is proven in Tom's own
codebase. Under tier 2 it is a `.wasm` in a folder.

Whichever, this needs the answer to: **is AWL's tree-sitter package public or
internal, and does it ship `highlights.scm` and friends or only the grammar?**

---

## 6. Two side findings, filed separately

**A 21.9 MB tracked file that nothing imports.** There are two copies of the
generated grammar bundle:

| Path | Size | Modified | Imported? |
| --- | --- | --- | --- |
| `crates/iridium-bindings/ts/syntax/grammars.gen.ts` | **21.9 MB** | 14 Jan | **no** |
| `packages/@iridium/core/src/syntax/grammars.gen.ts` | 2.1 MB | 30 Jul | yes |

Both are git-tracked. `queries.ts` is byte-identical between them; the other
three files differ. Only the `packages/` copy is reached — by
`syntax-worker/src/worker.ts`. The `crates/` copy is where
`tree-sitter-builder/build.ts` still writes (`build.ts:88`), so the builder's
output goes to the dead one. That is the same two-copies-of-one-decision shape
as the two `Language` enums, and it is 21.9 MB of it in git history.

**Disk.** The working tree is **36,287,864 KB** — up from 20,335,244 KB at lane
open, almost entirely `target/`. 35,097,016 KB free on the volume.

---

## 7. What this map does not do

It does not remove the enum, add a dependency, or touch a table. The build order
follows the rulings, not the other way round — the same discipline as the
context-menu map: ground verified, paths priced, decisions numbered, then build.

**Open question put to Tom:** is AWL's tree-sitter package public or internal,
and does it ship `highlights.scm` and friends, or only the grammar? That decides
whether "drop it in" means one file or seven.
