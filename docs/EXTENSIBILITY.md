# Extensibility — where Iridium already stands, and what is missing

A thinking note, not a plan. Nothing here is scheduled; Tom's instruction stands
that the extension protocol must not block the faces.

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

Streaming is where liminal genuinely earns its keep, because a participant can
resume mid-stream — the notebook pill and LSP diagnostics are the motivating
cases.

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
