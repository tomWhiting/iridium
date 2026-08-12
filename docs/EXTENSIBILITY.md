# Extensibility — where Iridium already stands, and what is missing

⚠️ **This was a thinking note until 12 Aug 2026. It now carries rulings — see
§0, which is the part that binds.** Everything after §0 is the survey the
rulings were taken against, unchanged except where a ruling supersedes it.

Tom's standing instruction is unchanged and is the root of all of it: **the
extension protocol must not block the faces.**

---

## 0. RULINGS — 12 Aug 2026

Tom ruled on a proposal I put to him after taking the manifold ground from
Waffles. Recorded here rather than in a lane doc because these govern the
kernel and every face, not one stint.

### 0a. The doctrine the rest hangs from — Waffles, ratified

- **The kernel absorbs mechanisms, never features.** "Doesn't care what is
  beyond it" is a *testable property*: the kernel neither reads nor depends on
  anything above it, and keeps working unchanged when everything above is
  replaced wholesale.
- **Opinion lives in composition, and composition is data.** Tiers of
  increasing opinion; the lowest layer is the dumbest. A shipped default keymap
  is an opinion expressed as replaceable data the kernel never reads.
- ⭐ **Strongly opinionated in defaults, strictly unopinionated in the kernel,
  is not a compromise between "more out of the box" and "small core" — it is
  the only architecture that delivers both at once.**
- **The deleted-default test** (sibling of Vesper's deleted-checker). A default
  has become a *coupling* when any of: the kernel behaves differently when it
  is absent; another component addresses the default's internals instead of the
  kernel's doors; replacing it requires touching anything beyond the data that
  declared it.
- **The dimension-cost test.** The cost of shipping the N+1th keymap, mode or
  opinion must not grow with N. The moment adding an opinion requires editing
  the kernel — or editing the *other* opinions — it is a monolith with extra
  steps.
- **Ship as many opinions as wanted; keep every one of them deletable.**

### 0b. Ruled in — three mechanisms

1. **The pending-operator mechanism lands in the kernel *before* any modal
   keymap is authored.** Operator-pending is *deferred selection*: `dw` is
   mechanically `vwd`, and every editing command here is already
   selection-first ("cursors with a selection delete the selection"). So the
   kernel needs one generic mechanism — *the next motion extends a selection
   rather than moving the caret; when it resolves, run this command and
   collapse* — and the whole operator × motion × text-object cross-product
   becomes **data**.
   ⚠️ Authoring the keymap first would bake the cross-product in by
   enumeration, which fails the dimension-cost test: one binding per operator
   per motion, growing multiplicatively. **The scaffolding to refuse is a "Vim
   emulation" module** — that is a feature wearing a kernel's clothes.
   ⭐ Text objects fall out free: `TextObject`/`Variant`/`find`/`jump`/`regions`
   already exist in `iridium-syntax`, so `dif` is operator `delete` × text
   object `function.inside` — the thing NeoVim needs a plugin for.
2. **Modal is available in every face, defaulted on only in the terminal.** The
   mechanism is face-independent, so availability everywhere costs nothing, and
   turning it on for the desktop becomes one config line rather than a port.
3. **Composite commands are data** — an ordered list of command ids with args,
   so "define a new verb out of existing ones" needs no interpreter and does
   not wait on the extension boundary.

### 0c. Ruled in — what ships in the box

Tom's answer to "how far does out-of-the-box go". Every item ships as a
**bundled participant**: registered through the same registry an external
extension would use, with **no private back door**, so "shipped" and
"deletable" are the same sentence.

- Everything the editor has today — selection, multi-cursor, motions, undo
  tree, folding.
- **The oil-style surface, including as a sidebar.** Tom: the surface should be
  insertable *as a sidebar*, not only as a full-window mode — and it belongs in
  the terminal face too, large, especially when opened on a directory. See
  #112.
- **Search and replace with strong regex *and* glob support.** Named as
  important; the engine exists and has had no UI in any face.
- The fuzzy file finder, which he considers already good.
- The tree-sitter surface — highlighting, folding, text objects.

### 0d. The synchronous path — Tom's refinement, and the sharper rule

The rule is **not** "extensions are never in the input path". It is:

> ⭐ **The typing path is never *asked a question*; it only announces.**

- **Key → command resolution is pure data and pure synchrony.** No extension
  participates, ever. This is what makes out-of-process extensions viable at
  all: a participant never has to answer *during* resolution. **The moment a
  plugin gets to decide what a key means, an embedded interpreter becomes
  unavoidable** and the whole boundary collapses.
- **What an extension produces arrives later, as ordinary reversible commands
  through the registry** — diagnostics, formatting, lint marks, git signs. This
  is the NeoVim property worth copying: typing never waits, and everything else
  catches up.
- ⚠️ **The honest exception, stated so it is not discovered as a bug.** Some
  operations legitimately want to block — format-before-write is the obvious
  one, since writing unformatted defeats it. Those are *user-initiated,
  non-typing* commands where a brief visible wait is acceptable. So the line is
  **"never block the typing path"**, and the input path and the command path
  are separately governed.

### 0e. Delegated to this seat, and decided — the event stream waits

Tom left the timing to me. **Decision: the event stream is the next stint, not
this one, and what this stint owes instead is the declarative-surface
vocabulary.**

The reasoning, so it can be argued with:

- **The irreversible decision is not the event stream.** §3 item 1 below
  already names it: the declarative surface vocabulary is *"the decision to take
  before any extension API is published, because it is the one that cannot be
  changed later."* An event stream added later is purely additive; a surface
  vocabulary published wrong is not.
- **An event stream with no consumer is scaffolding**, and scaffolding is a
  last resort. Designing an event vocabulary before anything consumes it means
  guessing which events matter.
- ⭐ **What makes deferring it safe is that the bundled participants become its
  design pressure.** The oil sidebar, the search-and-replace UI and the modal
  keymap are three real consumers. Building them as registry participants first
  means the event vocabulary is **discovered from three cases rather than
  guessed from none** — and the surface vocabulary is discovered the same way.
- **The pre-declared falsifier, so this is not a vibe:** if any bundled
  participant in this stint *cannot* be expressed without an event hook, the
  deferral was wrong and the event stream is built then, not argued about.

⚠️ **The constraint that makes the deferral safe, and it is not optional:**
nothing may bypass the command registry — no private back door for a bundled
participant, however convenient. If that holds, everything above is additive.
If it breaks once, the deferral becomes a trap.

#### Two riders — Waffles, 12 Aug, and the first closes a real hole

**Rider 1 — the participants must be buildable out-of-tree, or the vocabulary
discovered is the wrong one.** Discovery-from-bundled-cases only works if the
bundled participants are built under **external** constraints. In-tree
consumers written by the same hands can quietly lean on shared types, direct
imports and compile-time knowledge — ⚠️ **the no-bypass rule above covers the
registry door and does *not* cover those side channels.** Lean on them and what
gets discovered is *the in-tree dialect*, not the extension language, and the
whole design-pressure argument evaporates without ever tripping a check.

> **The test: each of the three must be buildable out-of-tree in principle,
> compiling against nothing but the published surface.**

That is the deleted-default test applied to participants, and it is the same
law liminal holds as *foreign-participant-specimen-first*.

**Rider 2 — a poll is an event hook with worse manners.** The falsifier fires
not only when a participant cannot be expressed without an event hook, but when
it can be expressed *only* by **polling or a timer**. ⚠️ If one of the three
ships a clock to simulate a subscription, **the deferral has already failed,
quietly** — and quietly is the dangerous part, because a timer looks like
working code rather than like a missing mechanism.

⚠️ Live relevance rather than a hypothetical: #99 records that the oil buffer's
apply runs on the frame thread, and the oil surface is one of the three
participants. That is the exact place a clock would get reached for.

### 0f. Liminal's ground — Hermes, 12 Aug, from `main c3c7c7f`

Asked because the surface vocabulary is the irreversible decision and I would
rather inherit one than mint a second. The answer resolves it, and not the way
I expected.

⭐ **Liminal has NO surface vocabulary, and that is deliberate — record payloads
are opaque to the bus.** The in-flight attachment work at the record layer is
media-shaped (content-addressed blobs), not UI semantics. **Nothing to inherit,
nothing to reconcile later: mint ours.** Three pieces of his ground to build it
on, and the first is a gift:

- ⭐ **(a) "The typing path is never asked a question; it only announces" maps
  exactly onto liminal's hard split between one-way record admission and
  request/response verbs.** If declarative contributions travel as admitted
  records, an extension gets durability, replay and resume for free — and **the
  protocol structurally cannot express an extension blocking a face.** That
  turns Tom's standing instruction from a discipline into a mechanism, which is
  the strongest form it could take.
- **(b) Version-stamp the vocabulary itself from day one.** Published wire
  values are immutable in their world and the release law leans on it. His
  phrasing: *make version a field, not a hope.*
- **(c) Our version-stamped-edit-or-refuse loop is isomorphic to their
  admission-with-typed-refusal.** A proposed edit against a document that moved
  is the same shape as their Precedence refusal family, so proposals travelling
  over liminal inherit typed, replayable refusals rather than us inventing that
  failure surface.

⚠️ **(b) weakens my own argument in §0e and I would rather say so than let it
stand unqualified.** I deferred the event stream partly on the ground that the
surface vocabulary is the one thing that cannot be changed later. A *versioned*
vocabulary is materially less irreversible than an unversioned one. The
deferral still holds — the ordering argument and both riders are untouched —
but the "cannot be changed later" plank is softer than I wrote it, and the
honest restatement is **"expensive to change later, and cheaper if versioned
from the first publication."**

**The in-process question, settled.** Liminal *has* an in-process transport —
proven byte-identical records against the socket mounts — plus an embeddable
server. But it is the same asynchronous record semantics with the socket
removed: ⛔ **nothing in the protocol can express "block the caller until you
answer", in-process or not.** So Waffles' seam stands unmoved: the wasm ABI on
the near side is **orthogonal to liminal**, a separate mechanism that shares our
registry. Use liminal's in-process mount for embedding, testing and
single-process deployment; **never as the fast path.**

**Latency — an open debt, named rather than rounded up.** He declined to hand
over a per-record round trip he has not banked, which is the right refusal.
Structure of the cost so we can reason meanwhile: an admission ack returns
**after the durable append**, so the floor is the store's fsync policy plus a
scheduler hop, and loopback or in-process removes the socket, not the
durability. The figures he does hold are resume-side, not RTT — steady-state
load of a large conversation **102 ms**, marginal replay slope **0.035
ms/record** — which bound how fast a participant *catches up*, not how fast one
echo goes. Median and p99 for loopback and TCP follow when his seat's usage
park lifts. **Design against this in the meantime:** user-visible verbs like a
formatter or a rename are comfortably out-of-process; **anything sub-frame is
not what liminal is for.**

**Shape, for the record.** Four crates with the layering visible:
`liminal-protocol` (the law — wire codec plus client lifecycle state machines,
pure data, no runtime), `liminal-server` (the runtime — a conversation-based
bus on beamr processes, one durable append-only log per conversation via
haematite), `liminal-sdk` (Rust client), and a TS SDK for the browser over
WebSocket. A participant **enrols** to mint a credential, then
**credential-attaches** to a conversation — the binding carries
`conversation_id`, `participant_id`, `generation` and `attach_secret`, and is
generation-stamped so a re-attach supersedes cleanly instead of racing. Every
admission carries a mint-once attempt token: the first commit answers,
re-sends of the same token are absorbed idempotently, and two distinct intents
are two tokens and two commits. Refusals are typed wire values, not dropped
connections.

---

It builds on **Waffles' seam (30 Jul)**, recorded in `SESSION-STATE.md`, which
this note agrees with and does not relitigate: *liminal for anything crossing a
process boundary, the kernel's public API (later a wasm plugin ABI) for anything
that does not — and the boundary is exactly the FEEL-1 line. One command
registry, two attachment mechanisms, exactly one of which is allowed to be
slow.*

What has changed since that was written is that most of the substrate it assumed
now exists and has tests. This note records what that substrate actually gives
us, and — more usefully — the three things it does **not**.

## Four properties already true, which are most of the hard part

**1. Every action is a named, addressable value.** `CommandId` + `CommandMeta` +
the registry, in the *kernel*, not per-face. This is the thing NeoVim has in
`:command` and VS Code in `registerCommand`, and it is normally retrofitted
painfully. An extension that registers a command is not a special case; it is
the same shape as a builtin.

**2. The keymap is data, not code.** A `KeyBinding` carries the mode it is
scoped to and the mode it switches into, and mode transitions are keymap data
rather than kernel commands. So **an extension can ship an entire modal grammar
without executing a single instruction in the input path.** That is a genuinely
unusual property. In NeoVim a plugin's mappings run Lua; here a keymap layer is
inert data the resolver already knows how to interpret, which means it cannot
blow the keystroke budget and cannot crash the editor.

**3. Every mutation is a reversible `Command`.** Waffles' point stands and is
the strongest single guarantee: an out-of-process participant proposes
version-stamped edits which the editor applies through its own reversible
command or refuses if the document moved. The undo tree survives *because the
protocol cannot express bypassing it* — not because extensions are trusted to
behave.

**4. Registration buys discoverability for free.** This one is new since
Waffles wrote the note. The palette ranks over the registry, and `KeyHintIndex`
resolves each command's *actually reachable* binding. So a registered extension
command appears in the palette, with its real keybinding shown, with no work
from the extension author. VS Code needed a `contributes` manifest for this and
NeoVim never got it at all.

## The pattern extension verbs should follow

`KeyResult::Ast(AstRequest)` and `KeyResult::History(HistoryRequest)` are the
model, and they were arrived at for an unrelated reason. The handler **names a
request; the editor performs it.** That is why key and palette invocation are
one path by construction rather than by discipline.

An extension verb should be exactly this: a request the editor performs. It is
what keeps a third invocation route from becoming a third code path — and the
three worst bugs found this year were all a face performing a verb itself
instead of asking the kernel to.

## The three things missing — the actual work

**1. There is no UI extension surface, and it is the hard one.** An extension
can register a command; it cannot draw a panel, a gutter decoration, a hover, or
an inline pill. This fights the one-kernel-three-faces model directly, because
drawing is precisely the part that differs per face. The plausible answer is
that extensions contribute **declarative** surfaces — a decoration is a range
plus a style, a panel is a list of rows — and each face renders that vocabulary
its own way, terminal as cells and web as DOM. Anything richer either forks per
face or drags a rendering model into the kernel. **This is the decision to take
before any extension API is published**, because it is the one that cannot be
changed later.

**2. Nothing is sandboxed or resource-bounded.** The kernel's public API is
currently the extension API, which means "extension" today means "linked Rust
code with full process authority". The wasm plugin ABI in Waffles' seam is what
makes in-process extensions safe to *install* rather than merely to write. Until
that exists, in-process extensibility is a compile-time property, not a
distribution story.

**3. The FEEL-1 line is a real constraint, not a formality.** NeoVim's power
comes substantially from scripting anything *synchronously, in-process*, in Lua.
Out-of-process participants cannot do that without paying round-trip latency on
the keystroke path. So the honest shape is:

| | in-process | out-of-process (liminal) |
|---|---|---|
| mechanism | wasm plugin ABI | liminal participant |
| latency | must fit the frame budget | allowed to be slow |
| good for | verbs, keymaps, text transforms, decorations | LSP, AI, formatters, anything streaming |
| authority | sandboxed, bounded | separate process |

⛔ **CORRECTED 12 Aug 2026 — this paragraph used to say a participant "can
resume mid-stream", and that overstates it.** Hermes, who owns liminal, gave
the real shape and the difference bears on design rather than wording:

**Resume is at the RECORD level, not the stream level.** On a tear you
re-attach; receipt replay settles the binding; `committed_delivery_seq` says
exactly where you were; and mint-once attempt tokens mean re-driving your
uncommitted tail cannot double-commit. So a diagnostics participant that dies
mid-flight reconnects and continues from its committed position with no
duplicates — that part is built and pinned.

⚠️ **What is deliberately *not* resumable:** a subscription feed the server
sheds under backpressure arrives as a typed `SubscribeError` and is **terminal
for that feed**. You re-open; you do not resume. A consumer that assumes
otherwise will hang waiting for a stream that is never coming back.

**The complete consumer obligation list, in his words:** hold your committed
seq; treat a shed as a re-open; and **mint attempt tokens once per record at
staging, never per presentation** — that last one cost a field incident and the
discipline is documented on the request type.

## The honest comparison to NeoVim

Iridium's substrate is *better positioned* than NeoVim's in three respects: one
command registry shared by every face rather than per-UI plumbing; keymaps
including modal grammars expressible as inert data; and an undo tree that a
protocol cannot violate. It is *behind* in one respect that matters enormously —
NeoVim has a synchronous in-process scripting language and a decade of plugins
written against it, and we have neither the ABI nor the ecosystem.

The realistic claim is therefore not "as extensible as NeoVim" but **"extensible
along the axes that survived contact with three faces"**. That is a smaller
claim and a more defensible one, and the declarative-surface decision in item 1
is what determines whether it holds.
