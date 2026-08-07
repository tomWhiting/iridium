# Iridium as manifold's editor organ — source-verified read

Produced 8 Aug 2026 for Waffles, who asked for a receipts-bearing read rather
than a remembered one, ahead of manifold's build plan hardening. Their design
record: `ablative/docs/design/face-substrate/DESIGN-DRAFT-r1.md` — §3 for the
three-organ composition, §12 item 5 for the editor ruling.

**The ruling being priced:** *"The editor is a component bound to a resource
provider. First provider: files in the estate's repositories (surfaced through
the repo services), read-first. Record-native documents follow as a second
provider. The editor never grows its own storage."* Plus the composition's
attention ladder — dot → pill → card → full window, same identity throughout.

Paths below are relative to `libs/iridium`. Everything carries a line number
because a remembered fact is not a fact.

---

## 0. A premise I could not confirm

§11 records *"Iridium: editor arc landed (highlighting proven live)"*, and the
brief said "proven live on 4179".

`4179` appears **nowhere** in this tree — searched `*.md`, `*.json`, `*.ts`,
`*.toml`, `node_modules` excluded, zero hits. The only port this repo
configures is **12223** (`examples/web/vite.config.ts:39`), and that is a Vite
dev server, which is banned here by standing instruction (run-once builds
only; frame serves). So 4179 is a manifold fact, not an iridium one.

⚠️ **"Highlighting" splits in two, and the split governs everything
downstream.**

- **Native** (desktop, terminal): real tree-sitter in Rust.
- **Browser**: the wasm build compiles with the `syntax` feature **off** and
  takes spans from a JavaScript worker (`packages/@iridium/syntax-worker`)
  over a protocol that carries spans and nothing else — no node identity, no
  kinds, no parent links.

So in the browser there is **no parse tree**, and everything derived from one
— structural selection, syntax folding, node navigation — is native-only. If
what is live on 4179 is the web build, "highlighting proven live" means the
worker path is proven, not that iridium's parser runs in the page.

---

## 1. Capability inventory

**Kernel** `crates/iridium-editor`, ~63,600 lines.

- **99 built-in commands** (`commands/builtin/table.rs`), **66 default key
  bindings** (`commands/default_keymap.rs:65`). Every action is a named,
  addressable value; a test asserts the registry and the action table cover
  each other exactly.
- Command-sourced: every mutation is a reversible `Command`. Undo is a
  **tree** with branch navigation and jump-to-node, not a stack.
- Multi-cursor throughout. Search/replace with regex, case and whole-word
  modes. Folding. A fuzzy command palette whose ranking lives in Rust
  specifically so the faces cannot disagree about it.

**Three faces, one kernel, all live:** desktop (winit + wgpu, ~21,000 lines —
tab strip, file tree, context menu, settings), terminal (~12,500), web (wasm +
~9,500 lines of TypeScript).

**Gates:** a nine-command battery. Last green run **2,533 tests, 0 failed**.

**Languages:** **23 vendored manifests**
(`crates/iridium-lang/src/manifest/embedded.rs`), read from upstream
`config.toml` rather than hard-coded — comment tokens, bracket tables and
extensions all come from the file.

**Grammars linked natively: 14** — `crates/iridium-syntax/src/grammar.rs:122`,
written as an asserted list rather than derived from the dispatch table,
*because a derived list would agree with that table however wrong it was*:
awl, rust, python, typescript, javascript, tsx, go, json, yaml, markdown, css,
bash, c, cpp. AWL is vendored in-tree (`crates/iridium-syntax/build.rs:39`);
the rest come from crates.

---

## 2. Does anything assume it owns the page?

**No — and it is better prepared than anyone building it intended.** All
receipts under `packages/@iridium/core/src/`:

| concern | receipt |
| --- | --- |
| encapsulation | custom element attaches a **shadow root**, `element/index.ts:54` |
| sizing | `:host { width:100%; height:100% }`, `element/index.ts:97-103` — fills its slot, never reaches for the viewport |
| keyboard | listener on the **canvas**, `controller/index.ts:653` |
| blur / clipboard | canvas, `:665`, `:674-676` |
| mouse / wheel | canvas, `:689-691`, `:693` |
| the one global | `document` `mouseup` at `:692` — required to catch a drag released outside the element — removed at `:699` |
| resize | `ResizeObserver` on the element, `:713`; disconnected `:731` |
| display scale | `matchMedia` with a paired `removeEventListener`, `:767` |
| teardown | `destroy()` `:1894` drains a cleanup array, cancels the frame loop, terminates the worker, de-registers the canvas; `disconnectedCallback` calls it, `element/index.ts:69` |
| double mount | `WeakMap<HTMLCanvasElement, IridiumEditor>` `:409` makes re-init idempotent |

Nobody built this for a frame composition. It behaves as if they had.

---

## 3. The resource-provider seam

**The kernel does no I/O at all.** `crates/iridium-editor` has no dependency
on `iridium-file`; only the apps do (`apps/iridium-desktop/Cargo.toml:38`,
`apps/iridium/Cargo.toml:36`). The desktop face says it where it defines its
per-document payload: *"the file is this face's business because the kernel
does no I/O"* (`apps/iridium-desktop/src/app/state.rs:50`).

⭐ **"The editor never grows its own storage" is not a change to iridium. It
is already true, structurally, enforced by crate boundaries.**

What already exists:

- **Multi-document generic over the face's payload.** `Workspace<P>` — the
  kernel owns tabs, ordering, and the rule for when a document's state dies
  (when the last tab onto it closes); the face supplies `P`. Desktop's is
  `DesktopDocument { scroll_y, file, syntax }`
  (`apps/iridium-desktop/src/app/state.rs:59`). **A provider handle is exactly
  what goes in `P`.** Landed and tested (#41, #46, #47, #48).
- **Content in, incrementally.** `applyRemoteEdit(start_byte, old_end_byte,
  text)` — `crates/iridium-bindings/src/wasm.rs:907`. Validated for inversion,
  bounds and UTF-8 character boundaries; document, history and cursors
  untouched on error. A provider pushing a change from elsewhere has a real
  path.
- **Content in, wholesale.** `setContent(string)`.
- **Edits out.** `onChange(content: string)` —
  `packages/@iridium/core/src/controller/index.ts:324` — plus `takeLastEdit`
  for the span.

⚠️ **Objection.** The outbound callback is a whole-document string copy per
change while the inbound path is incremental. Fine for one editor on a page,
wrong for a composition where a provider may be watching several. The span
information exists; it is just not what the public callback hands you. A
provider seam should consume `takeLastEdit`'s span and treat `onChange`'s
string as the fallback, not the interface.

---

## 4. The gates, in the order they should worry someone

**G1 — no parse tree in the browser.** §0 above. Closing it means either
getting tree-sitter to compile for `wasm32` (no one in the ecosystem ships
that configuration) or reimplementing the semantics in TypeScript, which forks
the behaviour of the most-used feature into a second language permanently.
Neither is a sprint. **Decide before the build plan hardens.**

**G2 — the web face is single-document.** Tabs exist in the kernel and in the
desktop face. The web face has none: three pieces outstanding, all blocked —
extract a web document type (#43), wasm exports for the workspace (#44), the
TypeScript tab strip (#45). A component bound to a provider wants more than
one open thing on day one. ⭐ **This is manifold's card-scaling analogue: the
rung looks reachable until you ask it to hold two.**

**G3 — cost per mounted instance, which the attention ladder multiplies.**

- The render loop is an **unconditional `requestAnimationFrame` chain**
  (`controller/index.ts:849-860`). It paints only every 500 ms (cursor blink),
  but the callback runs every frame, forever, per instance — **including one
  that is currently a dot in the graph**. Nothing checks visibility or focus.
- **One syntax Worker per editor** (`:1908`), each with its own web-tree-sitter
  instance.
- **One GPU surface per canvas** (`WebSurface::from_canvas`, `wasm.rs:352`).
  Browsers cap concurrent WebGPU contexts; that cap is unmeasured here.

Ten mounted editors is ten GPU surfaces, ten workers and ten frame loops for
nine things nobody is looking at. **The ladder's lower rungs need a cheap
representation that is not a live editor.**

**G4 — a pill cannot be 200 px tall.** `element/index.ts:101` sets
`min-height: 200px` on `:host`. One line, but it is a floor written into the
component and the ladder's bottom two rungs sit below it.

**G5 — `readonly` is decorative, and the first provider is read-first.** The
custom element lists `readonly` in `observedAttributes`
(`element/index.ts:49`) and documents it (`:14`) — and there is **no case for
it** in `attributeChangedCallback` (`:74-92`), no other reference in the file,
and no read at mount. The capability is real two layers down
(`IridiumEditor.setReadOnly` `controller/index.ts:1738` →
`WebEditor::setReadOnly` `wasm.rs:1266`), and `IridiumEditorOptions` has no
field for it. **Setting `readonly` on `<iridium-editor>` today does nothing.**

---

## 5. Absence claims, with the searches that failed

- No occurrence of `4179` in the tree (four file types, `node_modules`
  excluded).
- **No provider abstraction of any kind** — no trait, no interface, no
  directory named for one. `applyRemoteEdit` is a transport-shaped hole
  someone left open, not a provider seam.
- **No attention-state, LOD or visibility concept in the web face.** Searched
  the controller and the element for visibility or intersection handling;
  none. The render loop's unconditionality is the evidence.
- **No test can reach the web face's Rust.** `wasm.rs` is gated
  `#[cfg(all(feature = "web", target_arch = "wasm32"))]`
  (`crates/iridium-bindings/src/lib.rs:88`) and the wasm gate is a `check`,
  which compiles without running a test — see
  `docs/IN-FLIGHT-stale-highlights-on-load.md` for a defect closed there this
  morning with no red test available. The crate's own answer is extraction,
  with a landed pattern: `web_span_index` and `web_highlight_cache` are gated
  `any(target_arch = "wasm32", test)` precisely so tests reach them
  (`lib.rs:90-106`). Worth a line in manifold's brief.

---

## 6. The summary worth standing behind

The **kernel** is in better shape for this ruling than the ruling assumes. It
already refuses to own storage, and multi-document is already generic over a
face-supplied payload — which is where a provider handle goes, with no new
concept required.

Every gate is in the **web face**, and they cluster: no parse tree, no second
document, and a per-instance cost model that assumes one editor filling a
page. **G1 and G2 want deciding before the build plan hardens. G3 wants
measuring. G4 and G5 are afternoons.**

---

# Round 2 — Waffles' two challenges, answered at source

Waffles accepted §0–§3 and G4/G5, adopted the `onChange` objection as written,
and ruled two things that change the shape:

- **Sequencing:** the editor's first slice is read-first viewing and plain
  editing on the proven worker path. Structural features are explicitly out of
  scope until G1 is decided, so G1 leaves the critical path without being
  papered over.
- **Resources bind to attention state.** A dot or a pill acquires nothing — no
  GPU surface, no worker, no frame loop. A card gets a static preview at most.
  Only the full-window state mounts a live editor. **This makes G3 a
  measurement rather than a design problem**: ten editors is one live editor
  and nine cheap representations, and the WebGPU context cap becomes a number
  someone measures once.

Then two challenges back.

## G1 — "is the worker an oracle?" Yes, and I was wrong to leave it off.

Waffles' question: the syntax worker already runs web-tree-sitter, so a real
parse tree exists browser-side, stranded behind a spans-only protocol. Is
widening that protocol a third path between compiling Rust syntax to `wasm32`
and forking semantics in TypeScript?

**Verified: the tree is real and retained.**
`packages/@iridium/syntax-worker/src/worker.ts:22` — `private tree: Tree |
null`, held across messages, edited incrementally (`:150 this.tree.edit(...)`,
`:160`/`:219 parser.parse(content, oldTree)`). Not parse-and-discard.

⭐ **The framing matters and I had it wrong.** The plan priced
"reimplement navigation in TypeScript *through the worker*" and rejected it
for good reason. Waffles' framing is different: **the worker answers about
structure; Rust keeps the semantics.** That is an oracle, not a fork, and the
distinction was never drawn.

Three findings, two lowering the price and one relocating it:

- **↓ The protocol is a clean discriminated union with request ids**
  (`packages/@iridium/core/src/worker/protocol.ts:39-58`). New message types
  are structurally free.
- **↓ The UTF-16 ↔ UTF-8 boundary is already solved and tested.** tree-sitter
  speaks UTF-16 code units, the rope speaks UTF-8 bytes, and
  `packages/@iridium/syntax-worker/src/encoding.ts` exists for exactly that
  conversion with `encoding.test.ts` behind it. A structural payload crosses
  the boundary the spans already cross. **This is normally the expensive,
  bug-farming part of such a bridge and it is already paid for.**
- **→ The real cost is a node abstraction, not protocol vocabulary.** The
  kernel's structural walks are already pure and editor-free —
  `editor/ast/walk.rs:23` is `type Walk = for<'tree> fn(Node<'tree>,
  &Range<usize>) -> Option<Range<usize>>`, and `expand.rs` takes `root:
  Node<'_>` — but they are bound to `tree_sitter::Node` **concretely**. The
  work is making `walk.rs` and `expand.rs` generic over a node abstraction so
  the same functions run over a mirrored tree. Bounded, mechanical, one small
  pure module — and afterwards there is still exactly **one** implementation
  of the semantics.

**The design objection that decides the shape: do not make it request/response
on the keystroke path.** A postMessage round trip against an 8 ms budget, for
the most-pressed verb, forks the *feel* even when the semantics agree. Push a
**structure digest** after each parse instead — a flat
`(start, end, kind, parent index)` array over the same window the highlight
path already covers — and the kernel answers synchronously in Rust over the
mirror. Typing never parses; navigation never awaits.

**The degradation path already exists.** The worker drops its tree on error
(`worker.ts:127`, `:177`, `:267` — *"tree.edit() may already have mutated the
old tree; drop the pair"*), so the oracle can be transiently absent — and the
kernel's structural verbs already document exactly that case: *"When the
document has no language set, or the tree has not parsed, each of these does
nothing at all. That is not an error…"* (`input/keyboard/types.rs:246-249`).

**Verdict: G1 moves from "decide before build" to "widen when structural
features arrive"** — with the caveat that the widening's first task is a node
abstraction in the kernel, not a message type.

## G2 — the dissolution holds for tabs and fails for identity

Waffles' argument: in this composition the arrangement *is* the tab strip, so
one document per instance and the kernel's tabs stay a desktop concern.

**Agreed for tabs.** The composition is a better tab strip than a tab strip.

**Not agreed for what tabs were carrying.** `workspace/model.rs:177`: *"two
tabs share one buffer, one undo history and one set of cursors."* That is not
a tab feature — it is the rule that makes two views onto one file behave like
one file. Dissolve tabs and two mounted components onto one resource get **two
buffers, two undo histories, two cursor sets**. Not stale — **divergent**.
Both write back; the provider has two writers claiming one resource.

Two ways out, both real, and the brief should pick one:

- **(a) One live editor per resource, guaranteed by the slot.** The
  attention-state ruling nearly gives this for free: only full-window mounts a
  live editor, so divergence needs two full windows onto one file — a user
  action that can be refused or redirected ("already open here"). An
  arrangement rule, not code. Residue: genuinely nil.
- **(b) The provider owns convergence.** Single writer, fans changes to every
  bound instance through `applyRemoteEdit` (exists, validated, exactly this
  shape), each instance's history local. **The better architecture for a
  record-native provider.**

⚠️ **(b) has a consequence to rule on rather than discover: undo stops being
global for a document.** Two windows onto one file, each with its own undo
tree, both fed by the provider — undo in one can resurrect text the other just
deleted. The kernel's shared-history rule exists precisely to prevent that.
Survivable if chosen; a bug report if not.

**So: G2 dissolves as a tab-strip concern and survives as a document-identity
concern.**

## ⭐ What this exchange is worth keeping for

Twice in one night a source read caught what no list held — G5's decorative
`readonly` (found while reading for a different question) and the worker's
retained tree (found because someone challenged an absence claim instead of
accepting it). **An absence claim carries the search that failed, and the
search that failed is the thing worth challenging.**
