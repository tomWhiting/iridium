# Iridium Development Guidelines

## CRITICAL: CODING STANDARDS

This codebase runs mission-critical infrastructure for financial, legal, and healthcare settings.

**REQUIREMENTS:**
- NO LAZY CODE: Every implementation must be complete and robust
- NO SHORTCUTS: Handle all edge cases, no partial implementations
- NO DEVIATING FROM PLAN: Follow agreed approach; raise concerns before changing direction
- NO DEFERRED WORK: Set work is NOT optional, you must not defer tasks "for later"
- PRODUCTION READY: All code deployable immediately
- STABLE: All error cases handled, inputs validated
- PERFORMANT: Consider memory, complexity, efficiency

**Standard:** Would you trust this code with patient records, financial transactions, or legal documents? If not, it's not ready.

## What Iridium is

A GPU-accelerated text editor in Rust and wgpu. One kernel (`iridium-editor`) sits behind three faces — a terminal
app, a native desktop app and a WebGPU web component — and renders in the same frame it receives input. Nothing may
block a frame: parsing, directory reads and project search all run off the frame thread and are collected by it.

## Project structure

```text
apps/
  iridium/            # The terminal editor
  iridium-desktop/    # The native desktop editor (winit + wgpu)
crates/
  iridium-editor/     # The kernel: document, history, editor state, rendering, input
  iridium-syntax/     # Syntax highlighting and code intelligence via tree-sitter
  iridium-lang/       # Which language a document is in, with no parser attached
  iridium-bindings/   # napi (Node) and wasm-bindgen (web) bindings
  iridium-tui/        # The terminal face's surface and driver
  iridium-panel/      # The row vocabulary every face's panels are composed in
  iridium-config/     # The user's configuration file for the native faces
  iridium-file/       # Reading and writing files, atomically
  iridium-explorer/   # A non-blocking filesystem hierarchy for tree views
  iridium-tree/       # Expansion state and a virtualised row projection
  iridium-search/     # Bounded project content search
  iridium-query/      # jq filters resolved to JSON/JSONL source selections
packages/@iridium/    # TypeScript packages: core, syntax-worker
examples/             # web and web-component hosts (Vite)
```

## Tech stack

- **Rust 1.97.1** pinned in `rust-toolchain.toml` (edition 2024, MSRV 1.85), with the `wasm32-unknown-unknown` target
- **wgpu 28** - GPU rendering
- **glyphon 0.10** - text rendering; **cosmic-text 0.15** through glyphon's re-export, no direct dependency
- **winit 0.30** - desktop windowing
- **ropey 2.0** - rope; line operations use `LineType::LF_CR`
- **tree-sitter 0.26** - parsing, natively and through web-tree-sitter on the web
- **napi-rs 3** and **wasm-bindgen** - bindings

## Commands

```bash
gate check            # portable: no home paths, no literal ports, every platform branch has a twin
gate compiles         # check + clippy (workspace, kernel, syntax, wasm)
gate tested           # compiles + nextest (workspace, kernel, syntax, lang); must pass before main moves
gate web              # tsc and bun tests for the TypeScript packages and the web example
cargo lint            # clippy --workspace --all-targets --all-features
cargo nextest run --workspace --all-features
cargo fmt --all       # formatting is applied, never checked
```

The gate's checks are named in `gatefile` and configured in `.config/gate.toml`.

## Code style

- Clippy pedantic and nursery are on for the whole workspace; warnings are errors in the gate
- No `todo!()`, `unimplemented!()` or `dbg!()`; `unwrap()`, `expect()` and `panic!()` warn
- All public items are documented (`missing_docs = "warn"`)
- 4-space indent, 100-column lines
- No absolute home paths and no literal ports in code; ports come from `ports.json`

## Architecture notes

- **Command-sourced**: all mutations go through reversible `Command` objects
- **Undo tree**: branches are preserved; work is never lost
- **Performance targets**: 120fps, sub-8ms input latency
- **Background parsing**: faces call `parse_in_background` before opening a document and `poll_syntax` at the
  start of each frame; anything that must read the structure as it now is calls `settle_syntax`
- **ropey 2.0 API**: use `len_lines(LINE_TYPE)`, `line(idx, LINE_TYPE)`, `line_to_byte_idx()`, `byte_to_line_idx()`
  with `LineType::LF_CR`

## Workspace dependencies

Shared dependencies are defined in the root `Cargo.toml` under `[workspace.dependencies]`. Individual crates use
`dependency.workspace = true` to inherit them.

<!-- MANUAL ADDITIONS START -->

## BANNED PORTS - DO NOT USE

**NEVER RUN SERVERS ON THESE PORTS:**
- 3000
- 3030
- 8000
- 8080

These are always in use. Everything tries to use port 3000. The ports this repository uses are in `ports.json`
(12223, 12224 and 8899); add new ones there.

<!-- MANUAL ADDITIONS END -->
