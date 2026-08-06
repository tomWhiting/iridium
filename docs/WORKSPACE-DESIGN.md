# Workspace — tabs and self-nesting groups

**Status:** design map, ground verified 2026-08-06. Tom ruled tabs in at
00:50Z and asked for *"different ways of organizing a workspace or
workspaces… almost like self-nesting groups"*. This reverses the v1
no-tabs decision and unblocks step 1 of the sidebar plan.

---

## Ground — what the kernel actually has today

Verified by reading, not assumed:

- **There is no workspace, no buffer list, and no document id.**
  `grep` for `struct Workspace`, `mod workspace`, `DocumentId`,
  `struct BufferId` across `crates/` returns nothing.
- **`Editor` owns exactly one document**, by value:
  `Editor { state: EditorState, .. }` (`editor/core.rs:347`), and
  `EditorState` (`core.rs:131`) bundles `document`, `cursor`, `history`,
  `theme`, `config`, `viewport`, `scroll_line`, `scroll_x`, `has_focus`,
  `read_only`, `fold_state`, `syntax`, `search`.
- `Editor::new(EditorConfig)` (`core.rs:387`) is the only constructor
  that takes configuration.
- `crates/iridium-tree` exists, is dependency-free, and is generic over a
  `TreeSource`.

## The one decision that shapes everything else

`EditorState`'s thirteen fields split cleanly into two groups, and the
split is not a judgement call — it falls out of what a second open
document would need its own copy of:

| per document | per window |
|---|---|
| `document`, `cursor`, `history`, `fold_state`, `syntax`, `search`, `scroll_line`, `scroll_x` | `theme`, `config`, `viewport`, `has_focus` |

`read_only` is per document (a specific file is read-only), and is
currently in the same struct as `theme`, which is not.

**Two ways to act on that, and the choice is deliberate:**

- **(a) Split `EditorState`.** Correct in the long run, and a large
  invasive change: every one of the ~60 command dispatch arms and every
  face touches `state`. High risk of a regression in code that is
  currently green across 1958 tests.
- **(b) A workspace that owns N whole `Editor`s.** Zero changes to
  `Editor`. The cost is that `theme`, `config` and `viewport` are stored
  once per open document instead of once per window, and must be kept in
  step when they change.

**Take (b), and be honest about the cost rather than hiding it.** The
duplication is bounded by the number of open documents, and `Theme` and
`EditorConfig` are small value types — this is bytes, not a design flaw.
What it genuinely costs is a *synchronisation obligation*: setting the
theme must set it on every open editor, not just the active one, or a
tab switch would change the colours. That obligation is discharged in one
place (`Workspace::set_theme`, `set_config`, `resize`) and is testable —
which is exactly what makes it acceptable where scattering it would not
be.

⇒ **The named divergence** (proxy law): *the active editor's theme* is a
proxy for *the workspace's theme*, and they diverge the moment a second
document is opened after a theme change. The synchronisation methods and
their tests exist to close that case, and the test asserts it directly.

## The model

```
Workspace {
    documents: HashMap<DocumentId, Editor>,   // the buffers themselves
    nodes:     HashMap<NodeId, Node>,          // the organisation
    roots:     Vec<NodeId>,
    active:    Option<NodeId>,
}

Node = Group { name: String, children: Vec<NodeId> }
     | Tab   { document: DocumentId, title: String }
```

Two id spaces, deliberately. A `DocumentId` names a buffer; a `NodeId`
names a *place* in the organisation. They are separate because **the same
document can appear in two groups** — which is the whole point of "different
ways of organising a workspace": a file can be in both "current work" and
"the auth refactor" without being opened twice or having two histories.
Collapsing the two ids would make that impossible and would only be
noticed after the API had callers.

Groups nest without limit. A group's children are groups or tabs in any
mixture, so "a group of tabs inside a group" needs no special case.

## Where the tree crate comes in

`Workspace` implements `iridium_tree::TreeSource` with `Id = NodeId`:

- `children(None)` → `roots`
- `children(Some(group))` → that group's children
- `children(Some(tab))` → empty
- `has_children(tab)` → `false`; `has_children(group)` → whether it has any

Then the sidebar, the tab strip's overflow menu and any future outline
view are all `Tree<Workspace>` — expansion, selection, keyboard movement
and virtualised windowing come from the crate that already has 34 tests
covering them. **This is the reuse Tom asked for, and it is why the tree
crate was written generic over a source rather than over files.**

`iridium-editor` therefore gains a dependency on `iridium-tree`. That is
a compile-time edge and costs nothing at runtime — unlike a second wasm
module, which would put JSON and a JavaScript hop between the tree and
the editor on every frame.

## Ordering

1. `DocumentId` / `NodeId` (newtypes over a monotonic `u64`, never
   reused — a stale id must fail to resolve rather than silently name a
   different document).
2. `Workspace` with documents only: open, close, activate, `active()`,
   `active_mut()`. No grouping yet. **This alone delivers tabs.**
3. The node tree: create group, rename, move, remove; the invariants
   (no cycles, no orphans, removing a group re-parents or removes its
   children by an explicit rule, not by accident).
4. `TreeSource` impl + the projection tests.
5. Synchronisation: `set_theme`, `set_config`, `resize` across all open
   editors, with the divergence test named above.
6. Commands: `workspace.nextTab`, `previousTab`, `closeTab`,
   `newGroup`, … and bindings.
7. Faces.

Steps 1–5 are pure `cargo test`.

## Invariants the tests must pin

- Closing the active tab activates a *neighbour*, deterministically —
  next sibling, else previous, else the parent group, else nothing.
- A closed document's `Editor` is dropped only when no node references
  it; two tabs on one document keep it alive until both close.
- An id is never reused, so a stale `NodeId` resolves to `None`.
- Removing a group removes its subtree from the organisation but does
  **not** drop documents still referenced elsewhere.
- No node is its own ancestor — `move_node` must refuse a move into its
  own subtree rather than building a cycle that later hangs a traversal.
