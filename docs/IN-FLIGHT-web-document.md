# #43 — extracting `WebDocument`, and the two things in the way

Map only. **Nothing here has been compiled or run** — the box sat at load
36.28 against 10 cores for this window, and the map is what the window was
good for. Every claim below is source read at a named line.

## The shape is already decided, elsewhere

The desktop face solved this in #46 and the answer should be **mirrored, not
reinvented**. `apps/iridium-desktop/src/app/state.rs:59`:

```rust
pub(super) struct DesktopDocument {
    pub(super) scroll_y: f32,          // physical px; the kernel's is in lines
    pub(super) file: Option<TextFile>, // the kernel does no I/O
    pub(super) syntax: HighlightCache, // keyed to the compositor's seam
}
```

Three properties of that design carry straight over, and the doc comment
states all three deliberately:

1. **The kernel `Editor` is not in it.** `Workspace` owns the editors —
   `workspace.active_editor()`, `workspace.editor(document)` — and the payload
   is only the *face's* per-document extras (`state.rs:210,241,246,251`).
2. **It rides in `Workspace<T>`** rather than a map the face keeps, because the
   rule for when it goes away — when the last tab onto its document closes — is
   already implemented and tested there.
3. ⭐ **"None of which duplicate anything the kernel holds."** That is the rule
   the web face currently breaks, twice. See below.

So `WebDocument` is *not* "move nine of `WebEditor`'s sixteen fields into a
struct". It is: `Workspace<WebDocument>` owns the editors, and `WebDocument`
holds only what the browser face needs per document that the kernel does not
already have.

## `WebEditor`'s sixteen fields, classified

`crates/iridium-bindings/src/wasm.rs:180`.

| field | scope | note |
| --- | --- | --- |
| `editor` | → `Workspace` | the kernel document; not payload |
| `scroll_y` | **document** | the direct analogue of `DesktopDocument::scroll_y` |
| `ts_highlights` | **document** | worker-delivered spans |
| `use_ts_highlights` | **document** | whether this document has them |
| `span_index` | **document** | derived from `ts_highlights` |
| `highlight_generation` | **document** | staleness counter for the above |
| `fold_state` | **document**, ⚠️ duplicate | see below |
| `fold_syntax` | **document**, ⚠️ duplicate | see below |
| `pending_edit` | **document** | edit tracking for incremental reparse |
| `surface` | app | the canvas |
| `compositor` | app | the renderer |
| `needs_redraw` | app | frame-level |
| `pending_clipboard_text` | app | transient |
| `pending_host_command` | app | transient |
| `palette_mru` | app | recency is deliberately cross-document |
| `keyboard_handler` | ⚠️ **straddles** | see below |

## Blocker 1 — four fields are one concept

`ts_highlights`, `use_ts_highlights`, `span_index` and `highlight_generation`
are a single cache with a hand-maintained invariant between its parts. The
desktop face collapsed the equivalent into one `HighlightCache`.

⭐ This is #42's shape exactly — several fields whose agreement nothing
enforces — and #42's lesson applies: the invariant may well hold today, but it
holds *by circumstance*, and a reader of any one field cannot tell which is
authoritative.

**Do this first, and separately.** Collapsing four fields into a
`WebHighlightCache` is independently verifiable, needs no decision from anyone,
and shrinks the thing #43 then has to move from eight document fields to five.

## Blocker 2 — the folds are a genuine duplicate, and it is a known defect

`fold_state` and `fold_syntax` duplicate state the kernel already owns. This is
not new: it is the plan's finding 3 — `FoldState::update_regions` is called
only from `set_content` and `set_language`, never from
`apply_command_internal`, so **the kernel's folds go stale after every edit**,
and the web face is unaffected *precisely because* it drives its own copy from
`wasm.rs`.

So the web face is currently correct **because** it duplicates. Moving that
duplicate into `WebDocument` would bless it — and would violate the one rule
`DesktopDocument`'s design states outright.

**Recommendation:** do not carry the fold fields into `WebDocument`. Fix the
kernel refresh first (the plan already schedules this as §4.1 step 4, where it
falls out of `SyntaxState` sharing one retained tree), then let the web face
read the kernel's folds like the desktop face does. If that ordering is not
acceptable, the fallback is to move them and record explicitly that
`WebDocument` knowingly holds a duplicate and why — but that is the worse
answer and should be a deliberate choice, not a side effect of this task.

## Blocker 3 — `KeyboardHandler` is half app, half document

`crates/iridium-editor/src/input/keyboard/mod.rs:111-143`. It holds:

- `keymap: KeymapStack` — **app-wide** configuration
- `key_hints: KeyHintIndex` — derived from the keymap, **app-wide**
- `resolver: KeymapResolver` — the pending-sequence state machine
- `sticky_columns` + `sticky_state` — **per-focus**, per document

The resolver's own doc comment settles which side it is on: *"per-focus state
with the same lifetime as the sticky columns below, and must be abandoned by
the same host events that invalidate them."*

⭐ **Name the circumstance in which each wrong answer fails.** One handler per
document duplicates the keymap stack per tab — memory, plus a config reload
that reaches some tabs and not others. One handler shared across documents
carries sticky columns between them — switch tabs, press ↓, and the cursor
lands on the column you left in *the other file*. Neither is acceptable, so the
handler has to be split rather than assigned.

This one is not #43's to solve, and #43 must not pretend it is. The narrow
version — leave `keyboard_handler` on `WebEditor` for now, since the web face
has exactly one document today — is correct and honest, provided the limit is
written down where #44 will find it. **#44 (wasm exports for the workspace) is
where it becomes load-bearing**, because that is the commit that makes two
documents possible.

## Order

1. **`WebHighlightCache`** — collapse the four highlight fields. No decision
   needed, independently verifiable.
2. **Kernel fold refresh** — plan §4.1 step 4, so the face can stop
   duplicating. Or the recorded fallback.
3. **`WebDocument` + `Workspace<WebDocument>`** — carrying `scroll_y`,
   the cache, and `pending_edit`.
4. **Split `KeyboardHandler`** — app config from per-focus state. Belongs with
   #44, and #44 cannot ship two documents without it.

## The constraint over all of it

`wasm.rs` is gated on `all(feature = "web", target_arch = "wasm32")`, runs to
**3,160 lines**, and contains **zero** `#[cfg(test)]` blocks. The only gate
that compiles it is `cargo check … --target wasm32-unknown-unknown`, and
nothing anywhere executes it.

⭐ So every step above is verifiable only by the compiler, which is exactly why
step 1 goes first: a field collapse is a change the compiler *can* fully check,
because it enumerates every reader. Steps 2 and 3 move behaviour, and behaviour
is the thing nothing here can test. That asymmetry should drive the ordering,
not convenience.
