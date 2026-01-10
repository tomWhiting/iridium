# Iridium Coding Standards

## Dependencies

### Adding Dependencies

Always use `cargo add` to add new dependencies:

```bash
cargo add ropey
cargo add wgpu --features webgpu
cargo add tokio --features full
```

Never manually edit `Cargo.toml` for dependency additions. `cargo add` ensures proper formatting and version resolution.

### Version Policy

- Use the **latest stable versions** of all dependencies
- Before starting work, verify dependencies are current: `cargo outdated`
- Update dependencies regularly: `cargo update`
- Pin major versions only when necessary for stability

When adding a dependency, verify it's the latest:

```bash
cargo add <crate>  # Gets latest by default
cargo search <crate>  # Check what's available
```

---

## Project Structure

### Modular Organization

Organize code into logical modules using folders:

```
src/
├── lib.rs              # Crate root, public API exports
├── editor/
│   ├── mod.rs          # Module declarations only
│   ├── core.rs         # Core editor state and logic
│   ├── cursor.rs       # Cursor and selection handling
│   └── operations.rs   # Edit operations (insert, delete, etc.)
├── buffer/
│   ├── mod.rs
│   ├── rope.rs         # Rope data structure wrapper
│   └── history.rs      # Undo/redo stack
├── render/
│   ├── mod.rs
│   ├── pipeline.rs     # wgpu render pipeline
│   ├── text.rs         # Text rendering with glyphon
│   └── viewport.rs     # Viewport and scrolling
├── syntax/
│   ├── mod.rs
│   ├── highlight.rs    # Syntax highlighting
│   └── parser.rs       # Tree-sitter integration
└── lsp/
    ├── mod.rs
    ├── client.rs       # LSP client implementation
    └── protocol.rs     # Message handling
```

### mod.rs Files

Keep `mod.rs` files **lean**. They should contain:

- Module declarations (`mod`, `pub mod`)
- Re-exports (`pub use`)
- Minimal glue code if absolutely necessary

```rust
// Good mod.rs
mod core;
mod cursor;
mod operations;

pub use core::Editor;
pub use cursor::{Cursor, Selection};
pub use operations::EditOperation;
```

```rust
// Bad mod.rs - don't put implementation here
mod cursor;

pub struct Editor {
    // 500 lines of implementation...
}

impl Editor {
    // More implementation...
}
```

All implementation belongs in named files, not `mod.rs`.

---

## File Size Limits

### Guidelines

- **Target**: Under 800 lines per file
- **Hard limit**: Under 1000 lines per file

If a file approaches these limits, split it:

1. Identify logical boundaries (related functions, a sub-feature)
2. Extract to a new file in the same module
3. Re-export from `mod.rs` if needed for public API

### What Counts

- Lines include code, comments, and whitespace
- Tests at the bottom of a file count toward the limit
- Consider moving tests to a separate `tests/` directory for large modules

---

## Semantic Organization

### File Naming

Files should have clear, semantic names that describe their contents:

```
// Good
cursor.rs           # Cursor state and movement
selection.rs        # Selection handling
highlight.rs        # Syntax highlighting
viewport.rs         # Viewport and scroll management

// Bad
utils.rs            # Too vague
helpers.rs          # What does it help?
misc.rs             # Catch-all smell
types.rs            # Usually means "I didn't know where to put these"
```

### One Responsibility Per File

Each file should have a clear, single responsibility:

- `cursor.rs` - cursor position, movement, multi-cursor management
- `selection.rs` - selection ranges, selection modes
- `clipboard.rs` - cut, copy, paste operations

Don't combine unrelated functionality just because files are small.

### Grouping Related Code

Keep related items together within a file:

```rust
// Group by functionality, not by item type

// Cursor position
pub struct Position { ... }
impl Position { ... }

// Cursor state
pub struct Cursor { ... }
impl Cursor { ... }

// Multi-cursor handling
pub struct MultiCursor { ... }
impl MultiCursor { ... }
```

Not:

```rust
// Don't group all structs, then all impls

pub struct Position { ... }
pub struct Cursor { ... }
pub struct MultiCursor { ... }

impl Position { ... }
impl Cursor { ... }
impl MultiCursor { ... }
```

---

## Rust Best Practices

### Error Handling

- Use `Result` and `?` for fallible operations
- Create domain-specific error types
- Add context with `.context()` or `.with_context()` (from `anyhow`)
- No `unwrap()` or `expect()` in library code

### Documentation

- Document public API with `///` doc comments
- Include examples in doc comments for complex functions
- Use `//!` for module-level documentation at the top of files

### Testing

- Unit tests go in the same file with `#[cfg(test)]` module
- Integration tests go in `tests/` directory
- Keep test code to the same quality standards as production code

### Formatting

- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Configure editor to format on save

---

## Summary Checklist

Before submitting code:

- [ ] Dependencies added with `cargo add`
- [ ] Dependencies are latest versions
- [ ] Code is organized in logical module folders
- [ ] `mod.rs` files contain only declarations and re-exports
- [ ] No file exceeds 1000 lines (target: under 800)
- [ ] Files are semantically named and focused
- [ ] `cargo fmt` passes
- [ ] `cargo clippy` passes
- [ ] Tests are included for new functionality
