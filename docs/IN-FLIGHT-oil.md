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

## Still to do

2. **`apply.rs`** — executing a plan, re-checking every operation against the
   disk immediately before it runs, because the plan was computed earlier.
   **The temporary must be checked for existence too** — `plan` can only avoid
   colliding with names it knows about, and a real directory may already hold
   one.
3. Panel mode + editable rows.
4. Confirmation view.
5. `⌘O`, and the apply-or-discard prompt on a filter change.
