# #84 — the browser compared against an id it had typed out itself

Found 8 Aug 2026, by checking a uniqueness claim. Closed the same tick.

## The route in

`input/keyboard/actions/mod.rs:10` said:

> The table is the only place an id string is tied to behaviour, and it names
> the ids through the constants in `crate::commands::builtin`, so there is no
> second transcription of the id text to drift.

⭐ **Rule D — a claim of uniqueness is checkable, so check it.** This one is
false three ways, and only the third matters:

1. **`workspace/dispatch.rs:53`** ties the five `workspace.*` ids to
   behaviour, in this same crate.
2. **Every face's host-command dispatch** ties the `builtin::host` ids to
   behaviour — `app/host_commands.rs` in the desktop shell, and its
   counterparts in the terminal and the browser. That is not an accident; it
   is what a host command *is*.
3. **The browser face re-typed the id text.** `element/index.ts:220` and
   `:224` compared `request.command === "palette.open"` and
   `=== "history.togglePanel"` against literals written on the TypeScript
   side, and `examples/web/src/App.tsx:224,226` did the same.

(1) and (2) name the constants, so a rename breaks their build. That is the
property the sentence was reaching for — and it is *naming the constant*, not
being alone. The sentence's actual claim, "only place", was what stopped
anyone looking for (3).

## The divergence case

Rename `PALETTE_OPEN` in the kernel:

| | what happens |
|---|---|
| desktop face | fails to compile |
| terminal face | fails to compile |
| workspace dispatch | fails to compile |
| **browser face** | **compiles, type-checks, passes all 104 tests** |

And `Ctrl+K` in the browser resolves to a host command, gets consumed, matches
neither literal, and falls through to the `iridium-host-command` event — which
this element dispatches for exactly the case of "a command I do not claim". So
nothing opens, nothing is logged, and the keypress is gone. A dead key with an
alibi.

Nothing was wrong *today* — both literals still matched. This is a fix to the
mechanism, not to an observed symptom, and the write-up says so rather than
dressing it up as a live bug.

## The fix

The kernel already claims this ground. `commands/builtin/host.rs:5`:

> the kernel cannot draw a UI, but it can — and must — own the **identity** of
> the action, so that one id and one key sequence mean the same thing in every
> face.

It owned the identity for every consumer that could name a Rust constant, and
for no other. So the ids now cross the boundary as data:

- **`crates/iridium-bindings/src/host_commands.rs`** — `HostCommandIds`, three
  named fields read from `PALETTE_OPEN`, `HISTORY_TOGGLE_PANEL` and
  `EXPLORER_TOGGLE_PANEL`. Not gated on `feature = "web"`, for the reason
  `palette.rs` is not: it is a plain function over kernel constants, so
  `cargo test` reaches it on the host target.
- **`wasm.rs`** — a free `hostCommandIds()` adapter. Free rather than a method
  because the ids are the same before any editor exists.
- **`controller/index.ts`** — reads and parses it once in `doCreate`, beside
  the existing `sanitizePixelRatio` read, and exposes `editor.hostCommands`.
- **`element/index.ts`**, **`Iridium.tsx`**, **`App.tsx`** — compare against
  those instead of literals.

**Named fields rather than a list**, deliberately. A face does not want *the
ids*; it wants "the one that means open the palette". A bare list would push
the naming back into TypeScript, which is the transcription being removed.

**`explorerTogglePanel` is exported although a browser has no directory.** A
face that cannot implement a command still has to *recognise* it: the kernel
binds `Ctrl+Alt+E` regardless, so the chord is consumed either way, and a face
that cannot name the command cannot say why nothing happened.

## The oracle, proven red

`every_host_command_the_kernel_names_is_exported` compares the exported ids as
a set against `HOST`'s. Set equality, so it fails in both directions — a host
command added and not exported (a browser that cannot recognise a chord the
kernel consumes), and a field left behind after one is retired. Duplicates
collapse in a set, so it also catches two fields carrying one id without a
separate test.

`the_ids_serialize_under_the_names_typescript_reads` pins the exact JSON. That
is what makes the `{}` fallback in the wasm adapter unreachable rather than
merely unlikely — and the fallback *logs*, because an empty object reaches
TypeScript as three `undefined`s, every comparison against which fails, and the
palette would simply stop opening with nothing said.

**Proven red**: with `palette_open` set to `"palette.show"` — what a rename
leaves behind — both tests fail, naming the divergence:

```
left:  {"explorer.togglePanel", "history.togglePanel", "palette.show"}
right: {"explorer.togglePanel", "history.togglePanel", "palette.open"}
```

## What this does not close

⚠️ **Nothing stops a future face writing a fresh literal.** The transcription
is gone from the three that had it; the *possibility* is a property of any
untyped boundary. The guard against re-introduction is the corrected comment
in `actions/mod.rs`, which now names the hazard instead of denying it.

⚠️ **The TypeScript half still has no test of its own** — the 104 suites are
pure state machines and `element/index.ts` needs a DOM. That gap is the
baton's standing note on the web face, and `#43` owns it. What changed here is
that the browser no longer holds a *value* that can be wrong; it holds a
comparison against one the kernel supplies.

⚠️ **`pkg/` is gitignored**, so the bundle is not committed. It was rebuilt on
this box (`wasm-pack build crates/iridium-bindings --target web --features web
--no-default-features`, exit 0, `hostCommandIds` present in both the `.js` and
the `.d.ts`). Anyone else running the demo must rebuild, or `doCreate` throws
on a missing export.

## Gates

All nine green. **2,535 passed, 0 failed** — up two, which are the two above.
TypeScript: `deno check src/element/index.ts` clean, `bun test src/` 104
passed, and `tsc --noEmit` clean in `examples/web`, which is what actually
compiles the `IridiumHandle` change.

## The other claim this sweep corrected

`file_tree/panel.rs:346` said `requery` was "the only place `Self::pattern`
changes". `keys.rs:148` also writes it — `clear_query` sets `Unfiltered`. No
defect: it writes the one value that needs no query to know, and drops the
query text in the same breath, so the two cannot disagree. The comment now
says that instead of denying the second write site, because a reader auditing
query/pattern consistency would have stopped after finding one.
