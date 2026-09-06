---
type: design
cluster: iridium-cell-composer
title: Shared cell composer geometry and terminal embedding
---

# Shared cell composer geometry and terminal embedding

## Intention

A compact Norn composer can render, hit-test and edit one Iridium document in its allocated rectangle with correct Unicode and retained undo.

## Problem

Terminal Frame currently clips logical lines and reserves status chrome. Keyboard vertical/character motion is not a shared wrapped grapheme geometry. Painting alone would desynchronize caret and editing; id/revision alone cannot safely cache divergent document clones.

## Solution

Borrowed kernel cell rows, opt-in cell input through the existing command dispatcher, and borrowed terminal frame preparation with optional chrome.

## Decisions

### D1: Scope and opt-in

Add one cell-layout mode for embedded terminal composers. Existing GPU/web/terminal entry points and default EditorConfig.word_wrap remain unchanged. There is no generic extension/softwrap framework, filesystem feature, replay store, new process or serialization identity service.

### D2: Borrow-enforced freshness

CellRowMap borrows the exact Document and caller-supplied FoldState. PreparedCellFrame borrows the Editor and owns that borrowed row-map value. Rendering/query APIs take the prepared value, never a second editor/document; mutation is excluded for the borrow lifetime. Input/pointer methods prepare from their current Editor internally and drop map borrows before consuming a reversible command. A stored id/revision-only map cache is prohibited because Document derives Clone; divergent clones can share both. No full-text clone/hash/equality is a freshness precheck.

### D3: Grapheme-boundary soft wrap

Use greedy extended-grapheme-boundary wrap, not word wrap. Never trim/consume whitespace or move a word backward. At width 10, alpha beta gamma becomes [alpha beta, SPACEgamma]. A line with no final newline that exactly fills its final row produces no phantom content row. A final real line break produces the existing Document final empty line. Word wrapping is an explicitly separate future choice.

### D4: Coordinate types and line endings

Keep Position/Selection columns as existing Unicode-scalar columns and bytes as bytes. Introduce distinct ScreenRow/CellColumn values in the cell-layout module; never overload fold-visual Viewport.first_line or scroll_line. Logical document lines follow Document.line/line_count and LF_CR semantics. CRLF is one document line break and never an editable half-pair through composer input.

### D5: Measurement and degenerate extents

Use the workspace segmentation and width crates already used by the terminal. Preserve existing zero/control-width treatment and tab_width.max(1) compatibility, explicitly documented as an existing rule rather than a new default. Tabs are indivisible atoms with stops from current screen-row origin; wrap before an atom that does not fit a nonempty row, then remeasure there. An atom wider than the entire width occupies one overflow row once; the terminal clips it to blank cells exactly as existing paint_cluster does. Zero columns yields an explicit empty map; zero visible rows hides caret and disables coordinate navigation without affecting editing or history.

### D6: Canonical mapping and affinity

CellPlacement records a logical row-local edge (including row end) plus affinity. At a soft boundary the same Position may be upstream at prior row end or downstream at next row start; canonical default is downstream. Left/Right never stops twice at the same Position solely to change affinity. Hit on a wide/tab continuation cell chooses that atom start; hit after row content chooses that row end with upstream affinity, never trims whitespace. For zero-width atoms, multiple positions share a cell: define canonical hit as the trailing boundary after the maximal zero-width run and do not claim a bijection. place(hit(cell)) returns that canonical atom/edge; hit(place(p)) equals p only for canonical visible atom boundaries with the same affinity. Invalid scalar-interior placements are typed errors, while hidden folded positions yield an explicit non-placement.

### D7: Fold ownership

Borrow actual FoldState and derive visible rows from is_line_hidden, coalescing visibility rather than duplicating nested-fold prefix sums. Existing web FoldState ownership is not migrated. A hidden position is not silently remapped. No fold toggle or dimensions update can leave a valid prepared map alive because its source borrows block mutation.

### D8: Input dispatch and Unicode safety

Cell-mode key and direct-command methods use the existing resolver/action table. A private optional cell context routes geometry-dependent actions; the legacy path remains unchanged. Composer horizontal movement and character deletion use full extended grapheme boundaries; word operations snap in their direction, and destructive ranges include whole graphemes. Reject a host-injected scalar-interior cursor/selection before any mutation, naming line/column. If insertion/paste joins surrounding scalars into a new grapheme, derive normalized post-edit carets in the same compound command so undo restores exact pre-edit selections. All human edit routes reachable in cell mode must preserve this invariant, including cut/replacement, auto-pairs and multicursor operations; no byte/scalar interior deletion is silently accepted.

### D9: Motion and transient state

Visual Up/Down/Page and Home/End use the same cell rows as paint/hit. Home uses the visible row start; End uses the complete visible row end, including spaces. Document and word verbs retain their document meanings except grapheme-safe boundaries. Sticky preferences are per-cursor CELL columns; store affinity and layout options only in transient input state, validated against owner cursor state and geometry inputs. Any resize/tab/fold change or nonvertical motion discards obsolete preferences; history is not cleared. Page amount is the supplied visible text rows; zero height performs no geometry navigation. Existing non-cell dispatch keeps its current page/navigation behavior.

### D10: Commands, paste and undo

Keep one live Editor. Geometry sync is immutable and never calls set_content/state_mut merely to render or resize. Cell paste is one existing reversible multi-cursor insertion/replacement transaction with original bytes and selection. Preserve auto-pair provenance and undo grouping/branches; never simulate paste as keys. Affinity is not serialized in Position/CursorState/Command/UndoTree and is recomputed after replay. Host text changes remain explicit document commands, not a repeated synchronization reset.

### D11: Frame/caret/chrome boundary

Prepare one local frame for the allocated CellBuffer extent. None chrome takes zero cells; Some uses existing status/search/sidebar/gutter rules. Extent mismatch is a typed pre-write error. Logical end-of-row placement can equal the text width; terminal caret output must expose a trailing-edge flag and a bounded physical cell at the final visible atom start (or last available blank for non-glyph space), rather than pretend that edge is an ordinary interior hit. Zero extent has no caret. This keeps a caret visible without inventing a final empty content row or splitting a wide glyph. Norn applies its parent rectangle origin once. CellFrameError belongs to the terminal face and wraps kernel CellLayoutError. PreparedCellFrame retains all visible cursor hints using Editor::cell_cursor_affinity; primary compatibility API is preserved.

### D12: Host ownership

Norn owns the TTY/ANSI lifecycle, focus, parent layout, scroll intent and send-vs-newline keymaps. Iridium reports host commands via existing registry/keymap dispatch; it never submits a prompt itself. Raw terminal paste remains one Paste event; release remains non-editing. Prepared hit queries are read-only; actual pointer selection changes use the fresh live cell-pointer method. No Iridium driver is embedded in Norn.

### D13: Performance and error evidence

Visible cell geometry is measured once on preparation. Hidden or zero-width line boundary metadata is measured once on explicit placement demand and reused for that borrowed map lifetime; untouched hidden lines are not scanned. Repeated paint/hit/caret queries reuse those measurements. Build the borrowed map only for geometry-dependent input, not ordinary characters/copy/host commands. Do not copy the entire document into an identity key, invent workload caps, or implement a general retained-row cache. Measure the initial composer cost against the repository stated sub-8ms input/120fps intent and report actual results; timings are not asserted without measurement. Frame syntax-cache freshness additionally retains one immutable Rope snapshot and document id, comparing shared instance identity in O(1). This is separate from borrowed row geometry. The retained snapshot may require copying an affected rope path on the next edit; R4 must measure that copy-on-write cost.

### D14: Atomic host cell replacement and gesture-isolated undo

replace_cell_range validates the exact current cursor and typed range before any mutation, clamping, event or transient reset; replacement text is unchanged. EndOfReplacement chooses the next canonical grapheme boundary including CRLF joins, while Exact validates the complete desired post-edit CursorState without rounding. Internal preparation uses rope-shared scratch state and one synchronous commit. Rejections preserve text/revision/cursor/history/branches/events. True text-and-cursor no-op does not reset grouping. Cursor-only transactions are recorded without content events. New host transactions and opt-in cell paste isolate grouping on both sides without changing configured timeout or discarding branches; ordinary key and legacy paste/apply semantics remain unchanged. Context-complete reversible edits protect adjacent CR/LF from an inverse that otherwise derives its end from inserted text alone. One existing cell normalization authority is shared; no public staged token, parallel editor, full-document identity copy or generic transaction framework.

### D15: Host ownership of the primary terminal cursor

Frame::render_cells remains compatible and delegates with paint_primary=true. The explicit render_cells_with_primary_caret(prepared, buffer, paint_primary) uses the same exact borrowed frame and extent check, and false omits only primary caret paint. The primary remains in layout metadata for host hardware-cursor placement. The prepared visible-carets list starts with primary only when primary_caret is Some; skip that single list entry, never every equal position. Secondary carets, including those coincident with the primary or first-visible when primary is offscreen, still paint. Text, syntax, selection/search layers, wide-grapheme continuation cells, chrome and error-before-write contract remain unchanged. No second frame, underlay reconstruction, editor mutation, terminal ownership or Norn-specific rendering logic is introduced.

## Structure

- `Cargo.lock`
- `crates/iridium-editor/Cargo.toml`
- `crates/iridium-editor/benches/cell_composer.rs`
- `crates/iridium-editor/src/cell_layout/clusters.rs`
- `crates/iridium-editor/src/cell_layout/mapping.rs`
- `crates/iridium-editor/src/cell_layout/mod.rs`
- `crates/iridium-editor/src/cell_layout/rows.rs`
- `crates/iridium-editor/src/cell_layout/tests.rs`
- `crates/iridium-editor/src/cell_layout/types.rs`
- `crates/iridium-editor/src/editor/cell_input.rs`
- `crates/iridium-editor/src/editor/cell_replacement.rs`
- `crates/iridium-editor/src/editor/cell_transaction_tests.rs`
- `crates/iridium-editor/src/editor/command_apply.rs`
- `crates/iridium-editor/src/editor/core.rs`
- `crates/iridium-editor/src/editor/history_nav.rs`
- `crates/iridium-editor/src/editor/mod.rs`
- `crates/iridium-editor/src/history/undo_tree/isolated.rs`
- `crates/iridium-editor/src/history/undo_tree/mod.rs`
- `crates/iridium-editor/src/input/keyboard/actions/mod.rs`
- `crates/iridium-editor/src/input/keyboard/actions/run.rs`
- `crates/iridium-editor/src/input/keyboard/backspace_pairs.rs`
- `crates/iridium-editor/src/input/keyboard/cell_actions.rs`
- `crates/iridium-editor/src/input/keyboard/cell_edits.rs`
- `crates/iridium-editor/src/input/keyboard/cell_input.rs`
- `crates/iridium-editor/src/input/keyboard/cell_navigation.rs`
- `crates/iridium-editor/src/input/keyboard/cell_tests.rs`
- `crates/iridium-editor/src/input/keyboard/dispatch.rs`
- `crates/iridium-editor/src/input/keyboard/editing/intents.rs`
- `crates/iridium-editor/src/input/keyboard/edits.rs`
- `crates/iridium-editor/src/input/keyboard/handler.rs`
- `crates/iridium-editor/src/input/keyboard/mod.rs`
- `crates/iridium-editor/src/input/keyboard/navigation.rs`
- `crates/iridium-editor/src/lib.rs`
- `crates/iridium-editor/tests/cell_composer.rs`
- `crates/iridium-editor/tests/cell_transactions.rs`
- `crates/iridium-tui/Cargo.toml`
- `crates/iridium-tui/benches/cell_composer.rs`
- `crates/iridium-tui/src/frame/cell_geometry.rs`
- `crates/iridium-tui/src/frame/cell_options.rs`
- `crates/iridium-tui/src/frame/cell_render.rs`
- `crates/iridium-tui/src/frame/embedding_tests.rs`
- `crates/iridium-tui/src/frame/geometry.rs`
- `crates/iridium-tui/src/frame/line.rs`
- `crates/iridium-tui/src/frame/mod.rs`
- `crates/iridium-tui/src/frame/prepared_cells.rs`
- `crates/iridium-tui/src/frame/render.rs`
- `crates/iridium-tui/src/frame/text.rs`
- `crates/iridium-tui/src/frame/types.rs`
- `crates/iridium-tui/src/frame/wrap_tests.rs`
- `crates/iridium-tui/tests/cell_composer.rs`

## Constraints

- **B1** — Norn implementation, same-session daemon/attach, transcript storage, filesystem editor panels, file tabs or conflict resolution.
- **B2** — Word-wrap algorithms, generic plugin/extension framework, retained global layout cache, CRDT, new document identity infrastructure.
- **B3** — Changing GPU/web/default terminal renderer layout or legacy serialized Position/Viewport contracts.
- **B4** — Starting Iridium terminal driver/TTY lifecycle, model requests, builds, installation or release during authoring.
