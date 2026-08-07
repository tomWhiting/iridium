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

## Step 3 — DONE. `buffer.rs`, 29 tests (and 2 more on `plan`)

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

A **trailing `/` is the only way to say "a folder"**, so it is read and
stripped here and the plan only ever sees names. Stripped for existing rows
too: what a path is on disk is not the buffer's to change.

The rows are **not public**. Every editing method maintains one invariant — a
row's parent index names a row that comes before it — and a caller reaching
into the vector would break it silently.

### The check that came out of writing the tests

`plan` computes every path by joining the **drawn** parent chain. A row that
came from the tree separately carries where it **really** is. Nothing checked
that those agreed, and the whole safety of the feature rested on it.

They do agree for both row sources, and the tests above prove the derivation
that makes them agree. But the failure mode if they ever stopped is not a
tidy one, and the red run printed it exactly:

```
rename /project/src/main.rs → /project/src/renamed.rs
```

— for a row that is really `/project/src/deep/main.rs`. That is not a move.
It is an operation against **a different file**: it renames whatever happens
to be sitting at that path and leaves the row's own file untouched.

So `plan` now refuses a row whose drawn parent is not the folder its origin
path lives in, and refuses an existing row nested under one the user typed
(a file already on disk cannot live in a folder that does not exist yet).
Ten lines, and it converts an entire family of derivation bugs — a depth gap,
an off-by-one in the index shifting, a future caller reaching past `rows()` —
from silent damage into a refusal.

### The two hazards the handoff named, and what proves them

**Dedenting.** Three tests, and the load-bearing one goes end to end: a row
drawn after a two-level subtree is renamed, and the operation must name
`/project/loose.rs`, not a path inside the folder the walk was last in.

The derivation was rewritten from a lookup table indexed by depth to a stack
of `(depth, index)` unwound to the nearest shallower row. The table was
correct for contiguous depths — which is all either row source produces — but
it answers a gap by silently attaching the row to *nothing*, and the stack
answers it by attaching to the nearest shallower row. With the check above,
either way now ends in a refusal rather than in an operation.

**The index shifting.** The oracle is deliberately not an index: each row's
parent is read out **by name**, and an insertion or removal must leave every
other row's parent name exactly as it was. An off-by-one reads as a row that
changed folders, which is what it would be.

The strongest of them needs no oracle at all: type a row, then remove it, and
the buffer must be *indistinguishable from the one that loaded*.

**Removing takes the subtree.** `remove_typed` was removing one row, which
would leave its children with a parent index naming whichever row landed in
that slot. It removes the row and everything drawn inside it. That can never
take an existing file's row: rows loaded from the tree are parented by the
indentation they arrived with, and nothing reparents them under something that
was never on disk.

## Step 3b — DONE. The panel actually feeds the buffer, 8 tests

`Buffer::load` took `SourceRow`s that nothing built. `FileExplorer::buffer()`
now builds them, and the claim the design map made — *both row sources hand
over the same pair* — stopped being an assertion and became a function with
two branches and one shared tail.

Tested against real directories on the real reader thread. **Ruling 4 is not
implemented anywhere**: a file the query took off the screen is not in the
buffer, so nothing downstream has to be told to leave it alone. Ruling 8's
other half is the same — a match drawn under its folder is nested under it,
so a filtered rename lands inside `widgets/`, not beside it.

### The oracle that looked stronger than it was

The plan itself was used as the oracle: `plan` refuses a row whose drawn
folder is not the folder it lives in, so an untouched real project planning
`Ok` says the derivation agreed with the filesystem.

That was checked rather than believed, by flattening every depth by one and
re-running — and `a_real_project_is_nested_the_way_it_really_is` **passed
it**. A row wrongly given *no* folder becomes its own target, so its subtree
still resolves and the plan still says `Ok`. The proxy agreed with its target
on the examined set; the divergence is a row promoted to root.

Two things came out of that:

- `the_root_is_the_only_row_with_no_folder_above_it` exists for that case,
  and the module doc says so rather than leaving the hole unnamed.
- **`plan` was treating *any* parentless row as the mount point.** Only the
  first row may be. A later one is a row whose nesting could not be worked
  out, and calling it "the root" — while refusing to rename it *because* it
  is the root — is a message about a row the user can plainly see is not.
  Not a data-loss bug: such a row is its own target, so paths below it stay
  correct. A lying message, fixed because it lies.

## Step 4 — DONE (the content half). `confirm.rs`, 13 tests

Ruling 7 fully determines what a confirmation *says*; only how you reach it
waits on a key. So the content was built and tested, and the panel mode that
shows it lands with the keys.

Operations are listed **in the order they will happen**, because the order is
where the surprises are: a rename cycle routed through a temporary looks like
three renames and two of them name a file nobody typed. Someone about to
press `y` should see that.

### Nothing is hidden without saying so

A panel has a fixed number of rows and a plan has no fixed length. The failure
to avoid is a list that *looks* complete and is not — a confirmation that
quietly drops the delete at the bottom is worse than showing none, because it
was read and trusted. When the list is cut, the last row says how many are
missing.

That is the module's load-bearing test, and it was proven by making
`extend_capped` truncate silently: two tests go red.

### Two smaller decisions

- **Paths are read against the folder on screen.** Twenty absolute paths down
  a narrow panel are unreadable and every one repeats the same prefix. A path
  that is *not* under the root is shown whole rather than as a fragment of
  itself — the one rendering of a destructive operation that must never
  happen.
- **The three verbs reuse `change_added` / `change_modified` /
  `change_deleted`.** Not thrift: a delete that reads the same as a rename is
  the one distinction in this panel worth a colour, and the theme has already
  drawn it for the same three ideas in the gutter.

Refusals get the same treatment, and are named rather than numbered — "row 3"
is a number somebody has to count to.

## Step 4b — HALF DONE. `mode.rs`, 19 tests

The **state machine landed**; the **keys did not**, because they are the part
that is ruled on.

`Mode` is `Browse | Edit(Editing) | Confirm(Confirming)`, and the buffer lives
*inside* the variants that have one. That shape is the point: an
`Option<Buffer>` beside a `mode` field makes "browsing while holding a buffer"
and "editing while holding none" both representable, and the second is a crash
or a silently empty session. Here returning to `Browse` drops the buffer by
construction rather than by a line somebody has to remember.

⚠️ **The one rule:** a dirty buffer is never dropped without being asked about.
Every exit runs through `leave`, which reports `Leaving::Unsaved` and changes
nothing while there are unapplied edits. `discard` is the only thing that loses
work and is named for it. That is also what makes step 5 a *use* of this module
rather than a second implementation — a filter change is just another caller of
`leave`.

Four decisions worth naming, each with a test:

- **A second `begin_edit` is ignored, not a reload.** A second press of the edit
  key must not replace a buffer full of work with the rows as they are on disk.
- **`Confirm` hands out no mutable buffer.** It holds one, for going back — but
  a key that mutated it would leave the displayed plan describing something the
  apply no longer does.
- **The plan is computed once, at confirm time, and kept.** Recomputing it when
  the screen draws or when the apply runs is exactly the window a confirmation
  exists to close.
- **`applied` refuses to run from anywhere but a confirmation**, so an apply
  cannot be recorded for a plan nobody confirmed. It does the same thing to the
  mode as `discard` and must not be interchangeable with it.

⚠️ **`mod mode;` carries a scoped `#[allow(dead_code)]`** naming this blocker.
It is self-removing: the moment `keys` calls in, the lint stops firing. The
alternative was to guess the three keys and wire it anyway — the worse trade,
because the logic is where a mistake loses somebody's files and it is finished
either way, while a guessed binding is a table row that gets rewritten the
moment the ruling lands.

⭐ A fixture note, because it cost a red run: **row 0 must be the folder the
panel is showing.** The planner reads the drawn indentation as the tree, so a
second depth-0 row is a sibling of the root with nowhere to live and every plan
comes back `Refused`.

## Still to do
4b-keys. Wiring `mode` into `keys` and drawing editable rows. **Needs the three
    rulings below** — the logic behind it is done.
5. `⌘O`, and the apply-or-discard prompt on a filter change. `leave` already
    provides the whole mechanism.

### Blocked on Tom (asked, twice)

1. **What enters edit mode?** Every plain character goes to the filter, so it
   cannot be a letter. `Tab` is unbound here.
2. **How is a row marked deleted, and how is one created?** `Ctrl+D` to strike
   through; `Ctrl+N` is taken by *move down*, so "new row" needs another key.
3. **What applies?** `⌘S` reads as "save this buffer", which is the oil idea.

Told him I will take my own suggestions on any he does not care about.
