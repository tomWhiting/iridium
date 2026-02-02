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


GPU-accelerated text editor built with Rust and wgpu.

## Project Structure

```text
crates/
  iridium-editor/     # Core editor (document, history, rendering, input)
  iridium-syntax/     # Syntax highlighting via tree-sitter
  iridium-bindings/   # TypeScript/WASM bindings via napi-rs
```

## Tech Stack

- **Rust 1.85+** (edition 2024)
- **wgpu 28.0** - GPU rendering
- **glyphon 0.10** - Text rendering
- **cosmic-text 0.16** - Text shaping
- **ropey 2.0** - Rope data structure (uses `LineType::LF_CR` for line operations)
- **tree-sitter 0.26** - Syntax parsing
- **napi-rs 3.x** - TypeScript/Node bindings

## Commands

```bash
# Build
cargo build

# Check (fast compilation check)
cargo check

# Test
cargo test

# Clippy (linting)
cargo clippy

# Format
cargo fmt

# Benchmarks
cargo bench
```

## Code Style

- Follow aggressive clippy lints (pedantic, nursery enabled)
- No `todo!()`, `unimplemented!()`, or `dbg!()` in production code
- Warn on `unwrap()`, `expect()`, `panic!()`
- All public items require documentation (`missing_docs = "warn"`)
- Tab width: 4 spaces, max line width: 100

## Architecture Notes

- **Command-sourced**: All mutations go through reversible `Command` objects
- **Undo tree**: Branches preserved, never lose work
- **Performance targets**: 120fps, sub-8ms input latency
- **ropey 2.0 API**: Use `len_lines(LINE_TYPE)`, `line(idx, LINE_TYPE)`, `line_to_byte_idx()`, `byte_to_line_idx()` with `LineType::LF_CR`

## Workspace Dependencies

Shared dependencies are defined in root `Cargo.toml` under `[workspace.dependencies]`. Individual crates use `feature.workspace = true` to inherit them.

<!-- MANUAL ADDITIONS START -->

## BANNED PORTS - DO NOT USE

**NEVER RUN SERVERS ON THESE PORTS:**
- 3000
- 3030
- 8000
- 8080

These are always in use. Everything tries to use port 3000. Use obscure ports like 12223, 14567, etc.

<!-- MANUAL ADDITIONS END -->

## Active Technologies
- Rust 1.85+ (edition 2024), TypeScript 5.7+ + wgpu 28.0, glyphon 0.10, ropey 2.0, tree-sitter 0.26, web-tree-sitter 0.25.6 (002-viewport-syntax-rendering)
- N/A (in-memory data structures only) (002-viewport-syntax-rendering)

## Recent Changes
- 002-viewport-syntax-rendering: Added Rust 1.85+ (edition 2024), TypeScript 5.7+ + wgpu 28.0, glyphon 0.10, ropey 2.0, tree-sitter 0.26, web-tree-sitter 0.25.6
