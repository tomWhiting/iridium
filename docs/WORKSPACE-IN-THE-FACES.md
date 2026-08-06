# Putting the workspace into the faces

The kernel half of tabs and nested groups landed in `d2e78b8` and `04ba052`,
and the wire format in `6cbae2e`. None of it is visible yet. This is the map
for making it so, and it turns on one decision that is easy to get wrong
quietly.

Everything below was read from the source, not inferred.

---

## The problem, stated exactly

`WebEditor` (`crates/iridium-bindings/src/wasm.rs:157`) holds **seventeen**
fields. Ten of them are per-*document*; seven are per-*window*. Today there is
one document, so the distinction has never had to be drawn.

**Per-document — there must be one of each per open buffer:**

| field | line | note |
|---|---|---|
| `editor: Editor` | 158 | the kernel's own state |
| `fold_state: FoldState` | 168 | a **second** fold state; `Editor` has one too |
| `fold_syntax: WebFoldSyntax` | 173 | the parse the folds are read from |
| `scroll_y: f32` | 175 | where this document is scrolled to |
| `ts_highlights: Vec<JsHighlightSpan>` | 177 | spans from the JS worker |
| `use_ts_highlights: bool` | 179 | |
| `span_index: WebSpanIndex` | 181 | the windowed lookup over those spans |
| `highlight_generation: u64` | 187 | the retained-shaping key |
| `read_only: bool` | 194 | a **second** one; `EditorState` has one too |
| `pending_edit: PendingEdit` | 235 | edit accumulated since `takeLastEdit` |

**Per-window — exactly one, no matter how many tabs:**

`surface`, `compositor`, `needs_redraw`, `keyboard_handler`,
`pending_clipboard_text`, `palette_mru`, `pending_host_command`.

The per-window seven are already correct as they stand. The per-document ten
are what have to multiply, and **switching a tab without carrying all ten
across is a silent bug in every direction**: stale fold regions drawn over new
text, a scroll position from the wrong file, highlight spans indexed into a
buffer that no longer has those bytes.

Note the two duplications in that table. `fold_state` and `read_only` exist on
both `WebEditor` and the kernel's `Editor`. That is pre-existing — it is
finding 3 in `docs/PLAN.md`, where `Editor`'s fold regions go stale because
only the web face refreshes its own copy — and multiplying by N tabs makes it
strictly worse. This work should not create ten more of them.

---

## The decision: where do the per-document ten live?

### (a) A second map in the face

`WebEditor` grows `Workspace` plus `HashMap<DocumentId, WebDocument>`, where
`WebDocument` holds the nine non-`Editor` fields. The kernel keeps owning the
`Editor`s; the face keeps owning its own.

**Cost, named:** two maps keyed by the same id, with no mechanism keeping them
in step. Open a tab and the face must remember to insert; close one and it
must remember to remove, but *only* when the last tab onto that document
closed — which is a rule the kernel already implements and the face would have
to reimplement. The failure is a leak on one side and a `None` on the other,
and both look like nothing at all until a user opens their fortieth file.
The TUI face would then implement the same rule a third time.

**In its favour:** zero kernel change. Entirely additive.

### (b) The workspace carries a payload the face defines

`Workspace<T>` stores `(Editor, T)` per document, `T: Default`. The web face
instantiates `Workspace<WebDocument>`, the TUI face `Workspace<TuiDocument>`,
and the kernel's own tests `Workspace<()>` — for which `type Workspace = ...`
keeps every existing call site unchanged.

**Cost, named:** `Workspace` becomes generic, so every mention of the type in
signatures grows a parameter. Concretely that is `model.rs`, `nav.rs`,
`dispatch.rs`, `tree_source.rs`, the three test modules, and
`bindings/src/workspace.rs` — the last of which is the one that stings,
because its functions currently take `&Workspace` and would take
`&Workspace<T>` with `T` unconstrained. There is exactly **one** lifecycle
rule (allocate the payload with the document, drop it when the last tab onto
it closes) and it is written once, in the place that already implements it.

**In its favour:** the ids cannot diverge, because there is one map. Both
faces inherit the rule instead of writing it.

### Recommendation: (b)

The deciding fact is that the rule in question — *drop the payload when the
last tab onto the document closes* — is not a convenience. It is the same rule
that makes `DocumentId` and `NodeId` two spaces rather than one, and it is
already implemented and tested in `Workspace::close`. Option (a) asks two
faces to reimplement it from the outside, using only the return value of
`close`, which tells them whether a *node* went away and not whether a
*document* did.

That is the shape of every bug this codebase has spent the week on: a proxy
that agrees with its target on the examined set. "The tab closed" agrees with
"the buffer closed" in every workspace where no file is open twice — which is
every workspace anyone builds a smoke test from.

---

## What (b) does *not* solve, and must be handled separately

Making the payload generic does nothing about the two **duplicated** fields.
`fold_state` and `read_only` would simply move into `WebDocument` and go on
disagreeing with the kernel's copies, once per tab.

Both should collapse onto the kernel's copy as part of this work, not after:

- **`read_only`** is the cheap one. `EditorState::read_only` already exists and
  `Editor` already gates on it; the web face's own bool is a parallel gate in
  `handle_key_event`. Deleting the field and reading `state().read_only` is
  mechanical, and there is a test to write: a read-only tab must stay read-only
  across a tab switch, which under a per-window bool it would not.

- **`fold_state`** is the one that carries the pre-existing bug with it. It is
  `docs/PLAN.md` finding 3 and step 4 of §4.6 there: `Editor`'s fold regions
  refresh only in `set_content` and `set_language`, never after an edit, so the
  kernel's copy is correct exactly once. The web face works only because it
  drives its own. Collapsing them requires fixing the kernel's refresh first —
  which is worth doing on its own merits and is already planned.

**This is the honest sequencing:** `read_only` collapses inside this work;
`fold_state` does not, and must not be pretended into it. It gets its own
change, before the web face multiplies its copy.

---

## The desktop face is a much smaller job — and Tom asked for it first

Counted from `apps/iridium-desktop/src/app.rs`. `DesktopApp` holds
**nineteen** fields and only **four** are per-document:

| field | line | |
|---|---|---|
| `editor: Editor` | 234 | the kernel's own state |
| `scroll_y: f32` | 238 | where this document is scrolled to |
| `file: Option<TextFile>` | 250 | the path on disk — genuinely per tab |
| `syntax: HighlightCache` | 254 | spans cached per parse generation |

Everything else — shell, modifiers, pointer, clipboard, search, palette,
mru, menu, history, prompt, message, latency, title, failure — is per
window and stays exactly where it is.

Four against the web face's ten, with **no duplicated kernel state at all**:
the desktop face reads `editor.fold_state()` and `editor.state().read_only`
rather than keeping its own. So the whole `read_only` / `fold_state`
collapse above is a web-face problem that the desktop face simply does not
have.

**The friction is the `Option`, not the fields.** `Workspace::active_editor`
answers `Option<&Editor>`, and `self.editor` appears at 58 sites in
`app.rs`. Matching on `None` 58 times would be miserable and would bury the
one interesting question — *what should a window with no tabs even do?*

Two things resolve it together:

1. **The desktop session keeps at least one tab open.** Startup opens the
   named file or an untitled empty buffer, exactly as today; closing the
   last tab opens a fresh untitled one rather than leaving a void. So the
   `None` branch is unreachable in practice — but it is still written, and
   written as an early return, never as an `expect`.
2. **Bind once per entry point, then pass down.** There are on the order of
   ten event handlers (key, mouse, scroll, redraw, resize, …). Each binds
   the active editor and payload once with a `let … else { return }`, and
   the helper methods below take `&mut Editor` as a parameter instead of
   reaching into `self`. That converts most of the 58 field reads into
   local reads, and leaves those helpers testable without a window.

**The borrow that would otherwise bite:** `self.search.handle_key(event,
&mut self.editor)` needs two `&mut` into `self` at once. It works today
because they are disjoint *fields*. It keeps working as
`self.workspace.active_editor_mut()` for the same reason — but only if the
accessor stays a field path. A convenience `fn editor_mut(&mut self)` on
`DesktopApp` would borrow all of `self` and break every such call site, so
there must not be one.

### Desktop build order

- **A.** `DesktopDocument { scroll_y, file, syntax }` as the payload;
  `DesktopApp` swaps four fields for one `Workspace<DesktopDocument>`. No
  visible change — still exactly one tab. This is the whole refactor.
- **B.** Wire the five `workspace.*` commands and open-into-a-tab, so
  ⌘⇧] and ⌘W do something. Still no strip, but the behaviour is live.
- **C.** Draw the strip.

---

## Build order (web)

1. **`Workspace<T>`** with `T: Default`, plus the `Workspace<()>` alias so no
   existing call site moves. One new test: the payload is dropped when the last
   tab onto a document closes, and *kept* when a second tab still shows it.
2. **`bindings/src/workspace.rs` takes `&Workspace<T>`.** Its functions never
   touch the payload, so the parameter is unconstrained and the tests are
   unchanged.
3. **Collapse `WebEditor::read_only`** onto `EditorState::read_only`. Red test
   first: read-only must survive a tab switch.
4. **Extract `WebDocument`** from `WebEditor` — the nine remaining per-document
   fields, moved wholesale, with `WebEditor` reaching through to the active one.
   This is where the 3,149-line `wasm.rs` should shed a file rather than grow;
   `WebDocument` and its accessors belong in their own module.
5. **The wasm exports**: `workspaceSnapshot`, `activateNode`, `closeNode`,
   `renameNode`, `moveNode`, `createGroup`, `openDocument`, and routing
   `workspace.*` host commands into `bindings::workspace::run_command`. Thin
   adapters only — the logic is already tested in step 2's module.
6. **TypeScript**: the controller surface, then a framework-free tab-strip
   state machine in `@iridium/core` beside the palette's, then the React
   component.
7. **The TUI face** instantiates `Workspace<TuiDocument>` and gets the same
   five commands for free.

Steps 1–3 are pure `cargo test` and land independently of any face.

---

## The one open question for Tom

Already asked, already answered by default: **a new file opens into the group
you are currently in**, not at the top level. One line if that is wrong.

Nothing else here needs a ruling — "however you want to do that" covers the
rest, and the cost of (b) over (a) is paid in one afternoon of type
parameters rather than in a class of bug that only appears once someone has
forty files open.
