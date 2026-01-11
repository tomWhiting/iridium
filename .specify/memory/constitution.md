<!--
SYNC IMPACT REPORT
==================
Version change: N/A (initial) → 1.0.0
Modified principles: N/A (initial constitution)
Added sections:
  - Core Principles (10 principles from PRINCIPLES.md)
  - Technical Standards (from CODING_STANDARDS.md)
  - Development Workflow (completion requirements)
  - Governance
Templates requiring updates:
  - .specify/templates/plan-template.md: ✅ Constitution Check section compatible
  - .specify/templates/spec-template.md: ✅ Requirements/Success Criteria sections compatible
  - .specify/templates/tasks-template.md: ✅ Phase structure compatible
Follow-up TODOs: None
-->

# Iridium Constitution

## Core Principles

### I. Consistency Is On Us

The system MUST behave consistently. Always. This is not the user's job.

If a user presses a key and something unexpected happens, that's our fault. If the same action produces different results in different contexts, that's our fault. If Ctrl+S means save, it means save every single time. Users MUST NOT have to "learn our quirks."

The user expresses their intent. We execute it faithfully. That's the contract.

**Rationale**: A high-performance editor lives or dies by predictability. When users are working with large files under time pressure, they cannot afford to second-guess whether their input will behave as expected.

### II. Contract, Not Coercion

There is a difference between rules you agree to and rules forced upon you.

**Contract** (acceptable):
- Rust's type system is strict, but you chose Rust
- WebGPU's shader model has constraints, but you chose GPU programming
- The API requires content as a string—you agreed to that interface

**Coercion** (unacceptable):
- The system says "this text can't be selected" when the user says it should be
- Reaching into someone's mental model and telling them it's wrong
- Silently "fixing" user-provided content because we think we know better

When a user types something, we don't get to disagree. We render their intent faithfully. The user defines what things mean. We make sure those meanings are applied consistently.

### III. Trust Users, Don't Give Them Guns

Users can make bad choices that affect themselves—let them. That's their right.

But they MUST NOT be able to make catastrophic, irreversible mistakes that destroy everything. The distinction:

- **Their problem**: You opened a 500MB log file and it's slow. Your choice, your consequence. Allowed.
- **Our problem**: You accidentally deleted your entire buffer with a misclick and lost unsaved work. Catastrophic, irreversible. MUST be prevented or recoverable.

Expose power. Guard against annihilation. Create an environment where it's easy to make the right decision.

### IV. Expose All Controls, Make Defaults Excellent

Every parameter MUST be available. Advanced users need access to everything—key bindings, rendering options, theme internals, all of it.

But using defaults MUST just work, beautifully. A user who touches nothing MUST have an excellent experience. A user who customizes everything MUST have access to everything.

No hidden state. No mystery behaviours. If a setting is exposed, changing it changes behaviour. No silent overrides. No "actually we ignore that during startup."

### V. No Silent Failures

If something is broken, it MUST be visibly broken.

No swallowing GPU errors. No "it just didn't render." No mysterious blank canvases. If the system fails, it fails loudly, with context, with actionable information.

A user MUST NOT wonder "did that work?" They MUST know.

- GPU initialization failures MUST report device capabilities and what's missing
- Shader compilation errors MUST surface with line numbers and context
- Parse errors MUST identify the problematic location in the file

We don't pretend things are fine when they're not. We don't simulate success.

### VI. Automation Over Gatekeeping

Instead of preventing actions, enable users to define consequences.

**Gatekeeping** (don't do this):
- "You can't open files larger than 10MB."
- "That operation isn't allowed in this mode."
- The system deciding what's permitted.

**Automation** (do this):
- "When file size exceeds threshold, switch to virtualized rendering."
- "When frame rate drops below 60fps, emit a performance warning."
- The user defining what happens in response to events.

Expose the event stream. Let users write handlers against real events.

### VII. Low-Level Primitives Over Opinionated Wrappers

Choose the lower abstraction. Build up, not down.

- wgpu directly, not through abstraction layers
- Explicit render passes, not "just call render()"
- Direct buffer management, not magic data binding

Opinionated wrappers feel faster at first, then trap you. They've answered questions you hadn't asked yet. Primitives require more upfront work, then set you free.

This component IS the primitive. Framework bindings (React, Vue, Svelte) are thin layers that don't hide the core API.

### VIII. No Lazy Code

- No placeholders, no mock implementations, no "// TODO: implement later" that ships incomplete
- No `todo!()` macros in shipped code
- No `unimplemented!()` in any code path that can be reached
- No placeholder functions that defer implementation
- Every line of code MUST be real, working code that does what it claims

**Rationale**: Placeholders become permanent. Unfinished code creates false confidence. Ship working code or don't ship.

### IX. Make It Easy to Have Fun

Like Apple: the internals may be complicated, but using it feels effortless.

The goal isn't simplicity—it's ease. A high-performance GPU-accelerated editor with Rust/WASM internals CAN be easy to use if designed with care. We design with care.

Opening a file and seeing it render MUST feel instant. Typing MUST feel responsive. Scrolling MUST feel smooth. The path of least resistance MUST be the beautiful path.

### X. Build With Love

We're not building this because someone's paying us. We're building it because we want it to exist.

Like the chef who cooks because he loves food: we take our time, we get it right, we care about details others won't notice. Every frame rendered. Every interaction polished. Every edge case handled.

This doesn't mean slow. It means intentional.

## Technical Standards

### Dependencies

- MUST use `cargo add` to add new dependencies—never manually edit Cargo.toml for additions
- MUST use latest stable versions of all dependencies
- MUST verify dependencies are current with `cargo outdated` before starting work
- MUST update dependencies regularly with `cargo update`
- Pin major versions only when necessary for stability with documented justification

### Project Structure

- Code MUST be organized into logical modules using folders
- `mod.rs` files MUST contain only module declarations and re-exports—no implementation
- All implementation belongs in named files, not `mod.rs`

### File Size Limits

- **Target**: Under 800 lines per file
- **Hard limit**: Under 1000 lines per file
- If a file approaches these limits, it MUST be split along logical boundaries
- Tests count toward file size limits

### Code Quality

- Files MUST have clear, semantic names that describe their contents
- Each file MUST have a single, clear responsibility
- No `utils.rs`, `helpers.rs`, `misc.rs`, or `types.rs` catch-all files
- `cargo fmt` MUST pass before commit
- `cargo clippy` MUST pass with no warnings
- All public API MUST be documented with `///` doc comments

## Development Workflow

### Complete Features, No Half Measures

There is no "MVP version" of a feature that ships incomplete with the intention of finishing it later.

When we implement a feature, we implement it fully:
- If we add undo/redo, it handles all edge cases, grouping, persistence, and the full undo tree
- If we add selection, it supports all selection modes, keyboard and mouse, with proper rendering
- If we add syntax highlighting, it's fast, accurate, and handles incremental updates

We build incrementally—feature by feature—but each feature reaches production quality before moving to the next. We do not accumulate technical debt under the guise of "good enough for now."

**The bar is not "does it work?" but "would this be acceptable in a professional tool?"**

### Specification Adherence

- Specs MUST be complete before implementation begins
- Plans MUST be thorough before tasks are generated
- Tasks MUST be fully enumerated before coding starts
- Implementation MUST NOT deviate from spec without explicit discussion and approval
- No backing away from agreed scope
- No deferring features to "later" without explicit approval
- No leaving things out because they're complex

### Complexity Is Acceptable

If the correct solution is complex, we implement the complex solution.

- We do not simplify at the cost of correctness
- We do not cut corners because something is hard
- We work through difficult problems systematically
- If something takes longer than expected, we take longer—not shortcuts

### Quality Gates

- All code MUST compile without warnings
- All tests MUST pass before merge
- Performance benchmarks MUST meet specified targets
- Documentation MUST match implementation

## Governance

This constitution supersedes all other practices. When in conflict, these principles win.

### Amendment Process

1. Proposed changes MUST be documented with rationale
2. Changes MUST be reviewed against existing principles for conflicts
3. Version MUST be incremented according to semantic versioning:
   - MAJOR: Principle removals or incompatible redefinitions
   - MINOR: New principles or materially expanded guidance
   - PATCH: Clarifications, wording, non-semantic refinements

### Compliance

- All PRs and reviews MUST verify compliance with these principles
- Complexity MUST be justified against Principle VII (Low-Level Primitives)
- Silent failures MUST be treated as bugs per Principle V
- Incomplete implementations MUST be treated as bugs per Principle VIII

### Priority Order

When principles conflict, they are ranked:

1. **Consistency** (non-negotiable)
2. **No silent failures** (trust requires honesty)
3. **Contract, not coercion** (respect user intent)
4. **Trust users / don't give guns** (power with guardrails)
5. **Automation over gatekeeping** (enable, don't restrict)
6. **Expose all controls / excellent defaults** (access and ease)
7. **Easy to have fun** (the feel)
8. **Low-level primitives** (technical choice)
9. **Build with love** (always, but doesn't override safety)

### Reference Documents

- `docs/VISION.md` - Project vision, goals, and scope
- `docs/CONTEXT.md` - Technical background and decisions
- `docs/PRINCIPLES.md` - Expanded principle rationale
- `docs/CODING_STANDARDS.md` - Detailed coding conventions

**Version**: 1.0.0 | **Ratified**: 2026-01-11 | **Last Amended**: 2026-01-11
