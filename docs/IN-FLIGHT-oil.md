# #58 — the editable oil buffer

**Design map. Ground verified against the code, not taken from the task note.**

## Tom's rulings (all in hand)

1. **Enter on a folder unfolds.** Shipped; re-rooting is not a thing this panel
   does.
2. **A popover, not a tab.** *"more just the oil like buffer in like a window
   kind of thing like a command palette style thing, reachable from
   anywhere."* So the editable half edits the popover's own rows in place.
3. **The explorer may take ⌘O.** Verified: `Cmd+O` does not exist and never
   has — `open_file` has exactly two callers, a drop and the explorer.
4. Editable always, filtered or not; the filter scopes the diff.
5. **Rows diff by stable `NodeId`, never by name.**
6. Changing the filter while dirty prompts apply-or-discard.
7. A confirmation listing the actual operations, by name, before disk.
8. Filtering keeps the hierarchy.
9. Sigil not mode key: plain is fuzzy, leading `/` is regex.

## Ground facts, each checked

**`NodeId` non-reuse is guaranteed by the type, and its doc names this exact
feature.** `crates/iridium-explorer/src/node.rs:16` — *"resolved to
delete-plus-create would destroy a large file's contents and write them back
rather than moving it — so an id that could be reused for a different file is
the one thing this type must never permit."* The invariant ruling 5 depends on
was built deliberately, and says so.

**Nothing in this repo creates, renames or deletes a user's file.** Grepped
across `crates/` and `apps/`: every `fs::create_dir` / `fs::write` /
`fs::remove` outside `iridium-file` is in a `#[cfg(test)]` fixture. The only
production write path is `iridium-file/src/atomic.rs`, which writes to a
temporary and `fs::rename`s it into place. **This feature is entirely new
ground, and it is the most destructive thing Iridium will be able to do.**

**The existing confirmation cannot express ruling 7.** `prompt::Prompt::Confirm`
is a single line in a strip with a `y`/`n` answer and a `Deed` enum of two
variants, both meaning "discard unsaved work". Ruling 7 needs a *list*.

**The panel is already the right shape to host it.** `FileExplorer` composes
into the same `PanelContent` as the palette and the undo tree, is modal
(`ExplorerOutcome` has no `Ignored` — every key is consumed), and owns its own
selection and scroll. A confirmation is therefore a **state of this panel**,
not a new panel: same chrome, same placement, same focus discipline.

## Shape

`FileExplorer` gains a mode. Browse is what exists today.

```
Browse  ──edit key──▶  Edit  ──apply──▶  Confirm  ──y──▶  apply to disk
   ▲                     │                  │
   └──────esc/discard────┴────────n─────────┘
```

### The diff, which is the part that can destroy data

Rows carry `Option<NodeId>`: `Some` for a row that came from the tree, `None`
for a row the user typed. The diff is then total and unambiguous:

| row | became | operation |
| --- | --- | --- |
| `Some(id)`, name unchanged | — | none |
| `Some(id)`, name changed | | **rename** `id`'s path → new name |
| `Some(id)`, row deleted | | **delete** `id`'s path |
| `None`, name typed | | **create** |
| `Some(id)`, moved under a different parent row | | **move** |

Keyed by id throughout. A name is never used to identify a row — that is the
whole of ruling 5, and the failure it prevents is silent and total.

### Ordering, which is not optional

Operations cannot be applied in the order they were typed. `a→b` and `b→c`
applied in that order destroys the original `b`. Renames must be topologically
ordered, and a cycle (`a→b`, `b→a`) needs a temporary. **Deletes last**, so a
rename out of a directory being deleted still finds its source.

### Refusals, decided before anything is touched

The confirmation is computed from a plan that has already been validated, so
nothing partially applies:

- a create whose target already exists,
- a rename onto an existing path,
- a name containing `/` or a path separator, or equal to `.` or `..`,
- an empty name,
- two rows resolving to the same target path.

## Open — needs Tom, does not block the diff engine

- **Which key enters Edit mode?** The panel currently gives every plain
  printable character to the filter field, so an edit mode cannot be entered by
  typing. Suggest `Tab`, which is unbound here.
- **How is a row marked deleted, and how is one created?** Suggest `Ctrl+D` to
  mark the selected row deleted (struck through, not removed, so it is
  reversible before apply) and `Ctrl+N` for a new sibling row — but `Ctrl+N` is
  currently *move down*, so this needs a different key or a mode-scoped
  rebinding.
- **What applies?** Suggest `⌘S` — it reads as "save this buffer", which is
  exactly the oil mental model.

## Build order

The first two steps are pure logic, need no UI, and are where the danger is —
so they land first and fully tested.

1. **`plan.rs`** — rows → validated `Plan` of operations. Pure, no filesystem.
   Topological ordering, cycle-through-temporary, refusals. Every case tested.
2. **`apply.rs`** — executing a `Plan`. Every operation checked against the
   filesystem immediately before it runs, because the plan was computed
   earlier and the disk may have moved underneath it.
3. Panel mode + editable rows.
4. Confirmation view.
5. `⌘O`, and the apply-or-discard prompt on a filter change.

## Step 1 — DONE. `plan.rs`, 22 tests

**It touches no disk and cannot reach one — it has no access to the tree at
all.** That was a correction made during the build, and it improved the design:

The first draft took a `&FileTree` and looked each row's original name up by
`NodeId`. But `NodeId` has **no public constructor** — deliberately — so that
version could only be tested through a real directory and a worker thread,
which is the last way this logic should be tested. The fix was to notice the
plan does not need ids at all: each row **carries its own origin**, captured
from its node once when the buffer loads.

That is a stronger form of the by-id rule, not a weaker one. The pairing
between a row and what it was is established while the panel still holds the
id, and `plan.rs` has no names to re-match and no tree to consult, so it
cannot get it wrong at any later moment.

### The bug the tests caught

The cycle-breaking was wrong, and the code comment asserted the wrong reason —
it emitted both halves of the temporary immediately, so a two-way swap did
`a→.swap`, then `.swap→b` **while `b` was still occupied**.

It was caught by replacing a bad oracle with a good one. The first check
asserted "no rename targets a path a later rename reads from", which is wrong:
a temporary is *exactly* a path written and then read back, so it forbade the
mechanism it was meant to verify. Replacing it with an **occupancy
simulation** — at the moment each operation runs, is its source there and its
destination free? — caught the real defect immediately.

The fix: only the first half of a cycle-break is emitted; the second goes back
into the pending set and is emitted by the ordinary rule once its destination
is genuinely free.

### What is covered

Untouched buffer plans nothing · rename · delete · create · a struck row that
was never on disk · **renaming a folder does not re-rename its contents** · a
child renamed inside a renamed folder uses the new parent path · `a→b, b→c`
ordering · two-way swap · three-way rotation · deletes last · two rows
claiming one name · a name that is really a path (`../escape`, `sub/f`,
`/abs`, `.`, `..`) · empty and padded names · the root refusing rename and
delete · a row nested under a later row · one bad row refuses the whole buffer
· every refusal reported, not just the first · summary counts and plurals ·
`describe` · delete-then-create with the same name.

`plan.rs` carries `#![cfg_attr(not(test), expect(dead_code, …))]` — scoped to
the non-test build because the tests do use every item. When a caller lands,
that expectation becomes unfulfilled and the build warns, so it removes itself.

## Step 2 — DONE. `apply.rs`, 13 tests

### `std::fs::rename` silently replaces the destination

That is its documented Unix behaviour, and it is the single most dangerous
fact in this feature. The obvious implementation — check `to.exists()`, then
rename — leaves a window in which a rename destroys a file. Small window,
total consequence.

So **every operation uses a primitive that fails when its target exists**
rather than one that checks and then acts:

| operation | primitive | atomic? |
| --- | --- | --- |
| create file | `create_new` | yes |
| create directory | `create_dir` | yes, already fails on an existing path |
| rename | `renamex_np(RENAME_EXCL)` on macOS | yes |
| rename | check-then-rename elsewhere | **no**, and the doc comment says so |

**Proven, not asserted.** Swapping `renamex_np` for `std::fs::rename` makes
**four tests fail**, including
`a_rename_refuses_to_overwrite_rather_than_destroying_what_is_there`. The
platform code is doing real work.

This also *dissolves* the note the design map left for this step. `plan` can
only avoid temporary names it knows about, and a real directory may already
hold one — with a no-replace rename that stops being a check this module must
remember to make and becomes one the kernel refuses. There is a test for it.

### Two other things the tests pin

- **Deleting a symlink removes the link, not its target.** `symlink_metadata`
  rather than `metadata`, because `remove_dir_all` on a followed symlink would
  take the target's contents with it — the difference between removing a
  shortcut and removing somebody's home directory.
- **A failure stops the run and reports how far it got.** No rollback, and
  deliberately: a delete cannot be undone, and undoing half a directory rename
  by guessing turns a bad situation into an unrecoverable one. The caller
  re-reads the directory and shows the truth.

`apply.rs` carries the same self-removing `cfg_attr(not(test), expect(dead_code))`
as `plan.rs`.

## Step 3 — IN PROGRESS, UNCOMMITTED, DOES NOT COMPILE YET

`apps/iridium-desktop/src/file_tree/buffer.rs` is **written but never built**.
Written immediately before a compaction; treat every claim below as intent,
not as verified fact.

### What it is

The bridge between the panel's rows and `plan`, and **where the by-id rule is
actually kept**: each row's origin is captured here, once, from the node it was
drawn from, while the panel still holds its `NodeId`. Nothing downstream can
re-match by name because nothing downstream sees a tree.

Both row sources hand over the same pair — a node and a depth
(`tree.row(i)` gives `{id, depth, has_children, expanded}`, `view.rows[i]`
gives `FilterRow {id, depth, positions, score}`) — so the buffer does not care
which is showing. That is what makes editing work while filtered with no
second code path, and it is how ruling 4 ("the filter scopes the diff") falls
out rather than being implemented.

`SourceRow { depth, name, path, directory }` is deliberately **not** a
`NodeId`: by the time a row reaches the buffer its identity is already resolved
to the name and path it had, and keeping the id would invite something later to
look it up again against a tree that has since moved.

A row's **parent comes from the indentation** — the nearest row above it at a
shallower depth, which is what the eye reads off the screen.

### Known problems, unverified

- **Not declared in `mod.rs`.** Needs `mod buffer;` and a `#[cfg(test)] mod
  buffer_tests;`.
- **`insert_below` has an unused binding** — `Some(row) if into_folder` never
  reads `row`. Will warn under `-D warnings`. Use `Some(_)`.
- Needs the same self-removing
  `#![cfg_attr(not(test), expect(dead_code, …))]` as `plan.rs` and `apply.rs`
  until a caller exists.
- **No tests written at all.** The piece that most needs them is
  `rows_from`'s parent derivation, and specifically **dedenting**: a row at
  depth 1 following a subtree at depth 3 must attach to the last depth-0 row,
  not to something stale. `last_at_depth.truncate(depth)` is meant to do that
  and is unproven.
- `insert_below` and `remove_typed` shift every later `parent` index. That
  arithmetic is unproven and is exactly the sort of thing that silently
  reparents a row into the wrong folder.

## Still to do after step 3
4. Confirmation view.
5. `⌘O`, and the apply-or-discard prompt on a filter change.
