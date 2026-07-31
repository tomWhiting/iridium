# Tree-sitter in the browser — the spike, priced

The plan's §4.5 named three ways to give the web face syntax-node navigation, and
recommended taking (a) now and spending a timeboxed spike pricing (c) honestly.
This is that price. It was produced by research against the locked dependency
tree — source inspection plus disposable direct `clang` probes — not by a build.

**Verdict: viable, not dead, and costly. Roughly 2–3 engineer-weeks.**

**The recommendation stands: keep the JavaScript worker.** Nothing here changes
the §4.5 conclusion, and option (b) — reimplementing navigation in TypeScript —
remains the one to avoid, because forking the semantics of the most-used verb
into a second language is exactly what the one-kernel-three-faces architecture
exists to prevent.

## What was verified directly, in this repository

These four facts were re-checked by hand against `Cargo.lock` and the registry
sources, not taken from the report:

- The lock is `tree-sitter 0.26.3` and `tree-sitter-language 0.1.6`.
- `tree-sitter 0.26.3`'s `binding_rust/build.rs` **does** have a
  `target.starts_with("wasm32-unknown")` branch (line 31), and panics unless
  `DEP_TREE_SITTER_LANGUAGE_WASM_HEADERS` is set by the language crate.
- `tree-sitter-rust 0.24.0`'s build script contains the string `wasm` **zero**
  times — it has no wasm branch and cannot consume that mini-sysroot.
- `tree-sitter-md 0.5.2`'s build script **does** reference those headers.

So the shape of the blocker is confirmed: the Tree-sitter *runtime* gained
wasm32 support upstream in September 2025, but most of our *grammar* crates
never adopted the build-script template that makes it usable.

## The blocker chain

1. **Iridium's own wiring stops the attempt before it is a browser feature.**
   The browser builds `--no-default-features --features web`, which does not
   include `syntax`; `wasm.rs` unconditionally imports `syntax_stubs`, which is
   only compiled when `iridium-editor/syntax` is *off*. Enabling syntax is a
   source change, not a feature-list edit. There is also no language setter on
   the browser binding today.
2. **Ten of twelve grammar crates have no wasm branch** — `rust`, `python`,
   `typescript`, `go`, `json`, `yaml`, `bash`, `c`, `cpp`, `javascript`. Only
   `md` and `css` consume the mini-sysroot. First failure is
   `'stdlib.h' file not found`.
3. **A wasm-capable clang plus the locked headers still isn't enough.** A direct
   probe with LLVM 20.1.7 compiled **21 of 25** parser/scanner units. The four
   failures: Python wants `UINT8_MAX`, Markdown wants `wchar.h`, Bash wants
   `isdigit`/`strcmp`, C++ wants `wchar_t`/`static_assert`. `tree-sitter-language`
   0.1.7 adds `UINT8_MAX` and none of the rest.
4. **wasi-sdk is a compile aid, not the answer.** It targets WASI, not
   `wasm32-unknown-unknown`; linking wasi-libc wholesale risks leaving
   `wasi_snapshot_preview1` imports in a module that must have none. Switching
   the shipped artifact to `wasm32-wasip1` is not a shortcut either — browsers
   don't supply those imports and wasm-bindgen's documented browser target is
   `wasm32-unknown-unknown`.
5. **The allocator is the production blocker.** The upstream wasm `stdlib.c`
   initialises its allocator pointers only inside `reset_heap`, which no native
   Rust parser path calls, and caps the heap at 4 MiB. This is still true on the
   latest 0.26.11. Upstream issue #5530 (open, reporting a browser panic on
   0.26.8) is the same diagnosis. The fix is to omit that `stdlib.c` and export
   Rust-backed `malloc`/`calloc`/`realloc`/`free`, preferably with
   `TREE_SITTER_REUSE_ALLOCATOR` so `set_allocator` governs scanners too.
6. **Integration remains after it links** — conditional real/stub imports, a
   language API on `WebEditor`, a decision about replacing the worker's highlight
   spans, and the loss of off-main-thread parsing unless a Rust wasm worker is
   introduced.

## Size

The thirteen grammars measure roughly **12.5 MiB raw / 1.25 MiB gzip / 0.85 MiB
Brotli** as standalone artifacts (C++ alone is several MB). Added to the current
3.26 MiB browser wasm that is an upper bound of about **15.8 MiB raw / 1.66 MiB
Brotli** before dead-stripping and `wasm-opt`.

That is *not* necessarily a transfer regression, and this is the strongest
argument on the other side: the generated TypeScript **already** embeds the same
gzip-compressed grammars as base64 (~1.48 MiB for the full 19-language file), so
replacing the worker could be transfer-neutral or slightly smaller. The real
cost is that all thirteen become one monolithic compile/cache unit instead of
separately decoded language modules.

These figures are computed from cached standalone artifacts, not from a linked
build. **The final linked size is unknown** and must be measured.

## If it is ever funded

A five-day gate before committing to the rest: hermetic LLVM wasm toolchain,
complete custom headers, Rust-backed allocator; prove JSON first, then the hard
scanners (Bash, C++, Markdown); inspect the final import graph and reject any
`wasi_snapshot_preview1` or stray `env` libc imports; then a real
`wasm-pack build --target web` browser test doing an actual expand/shrink.
Only on a clean gate proceed to the further 5–10 days for all thirteen with CI,
incremental-edit tests and main-thread latency profiling. Do not embed all
thirteen unconditionally — tier them until measured size and latency are
acceptable.

## What remains unknown

Honestly held open, in descending importance:

- The final wasm-bindgen link/import graph after the allocator is replaced. No
  `cargo`/`wasm-pack` build was run, so linker surprises remain possible.
- The linked size after LTO, dead-stripping and `wasm-opt`.
- Main-thread parse/query latency on large files — precisely the risk the
  current worker exists to avoid.
- Whether we would accept maintaining forked grammar build scripts, or require
  the fixes upstream first.
- No primary source was found validating this exact thirteen-grammar
  configuration in a full browser wasm-bindgen application. Upstream has proved
  *one* parser under `wasm-pack test --node` (PR #5158, January 2026); nobody
  ships what we would be shipping.
