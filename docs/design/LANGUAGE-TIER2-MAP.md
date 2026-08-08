# #88 — language extensions, tier 2: ground re-verified and the build map

**Tier 1 is already built. `IN-FLIGHT-languages.md` §1 and §4 describe a
codebase that no longer exists — read this instead, then that for the tier
pricing and L-0..L-8.**

Ground measured 8 Aug 2026 at commit `c90608a5`. Every claim below was produced
by a command in this repository, not carried over from the earlier map.

---

## 1. What tier 1 actually delivered

The design map's central proposal was *"`Language` stops being an enum and
becomes a handle issued by a registry."* **That has happened.**

| the map assumed | measured now |
| --- | --- |
| `pub enum Language` with 13 variants | ⭐ **no `pub enum Language` exists anywhere in `crates/`** |
| the type lives in `iridium-syntax`, with a hand-maintained stub in `iridium-editor` | a dedicated **`iridium-lang`** crate, identity only, depending on nothing that parses |
| a fixed 2-D `COMPILED` array | still fixed — see §3, it is the one real obstacle |
| four hard-coded tables | manifests read from the vendored tree (`iridium-lang/src/manifest/`) |

`iridium-lang/src/lib.rs:98` — `pub struct Language(u16)`. `:78` —
`include!(concat!(env!("OUT_DIR"), "/language_table.rs"))`. The registry is
**generated at build time** by `crates/iridium-lang/build.rs`, which emits
`LANGUAGE_IDS`, one `pub const` per language, and `ALL`.

So the tier-1 half of L-0 is done. **What remains under #88 is tier 2 and only
tier 2: making that registry able to grow at runtime, and loading wasm
grammars into it.**

## 2. ⭐ THE FINDING THAT MAKES THIS TRACTABLE — nothing gets renumbered

Static languages occupy indices `0..Language::COUNT`, assigned by `build.rs` in
declaration order and pinned by
`indices_agree_with_the_declared_order` (`iridium-syntax/src/lib.rs:113`). A
runtime-loaded extension can only ever be **appended**, at `COUNT` and beyond.

**Therefore every existing `Language::RUST`-style constant keeps its index, and
no serialized value changes meaning.** Tier 2 is an *extension* of the index
space, not a renumbering of it. That is the difference between a contained
change and one that touches every persisted document.

⚠️ **This holds only while the static table is generated first and the runtime
table appended after.** Any future scheme that interleaves them — sorting the
merged set by id, say — silently breaks it. Written here because it is an
invariant that lives in two files and is enforced by neither.

## 3. The seams tier 2 must open — measured, and there are exactly three

### 3a. ⛔ `COMPILED` is a fixed 2-D array — the only hard blocker

`iridium-syntax/src/query/mod.rs:71`:

```rust
static COMPILED: [[OnceLock<Compiled>; KIND_COUNT]; LANGUAGE_COUNT] = …
//  :87
let slot: &'static OnceLock<Compiled> = &COMPILED[language.index()][kind.index()];
```

with `LANGUAGE_COUNT = Language::COUNT` (`:51`). A runtime language's index is
`>= COUNT`, so **this indexes out of bounds and panics** — in a crate whose
CLAUDE.md bans panics outside tests.

⭐ **It is the ONLY production site in the workspace that indexes a fixed-size
array by language index.** Swept: the only other `.index()`-into-an-array use is
`iridium-syntax/src/lib.rs:116`, which is inside a `#[cfg(test)]` module, and
every remaining `.index()` hit in the tree belongs to a *different* `index()` —
the highlight cache's and `QueryKind`'s. That narrowness is the good news here.

The shape that fits: keep the static array as the fast path for
`index() < COUNT`, and spill runtime entries into a lock-guarded side table.
Static lookups stay a bare array index with no lock at all, which matters
because this is on the highlight path.

### 3b. `grammar()` is a match on the id, not a table

`iridium-syntax/src/grammar.rs:79` — fourteen arms of `"rust" => …` plus
`_ => return None`. Tier 2 needs it to consult the runtime table **before**
falling through to `None`, and to return a wasm-backed
`tree_sitter::Language`. The existing arms are untouched; a linked grammar
must keep winning over an installed one, or an extension can shadow a built-in.

### 3c. `id()` returns `&'static str`, and a loaded extension's id is not static

`Language::id()` reads `LANGUAGE_IDS[self.index()]`, a `&'static str` baked in
by `build.rs`. A runtime id is a `String` read from a `config.toml`.

**Leak it at registration** (`Box::leak`) rather than widening `id()` to a
borrowed lifetime. The alternative ripples the lifetime through `Serialize`,
`Debug`, every test message and every caller. The leak is bounded by the number
of installed extensions, happens once at load, and never grows during a session
— which is the case where leaking is honest rather than lazy. ⚠️ It does mean
**an extension cannot be unloaded and its id reclaimed within a process**; if
hot-reload is ever wanted, this is the decision to revisit.

## 4. The dependency, still not enabled

`Cargo.toml:54` pins `tree-sitter = "0.26.3"` with default features, and
**`Cargo.lock` contains no `wasmtime`** — the `wasm` feature is off. Turning it
on is the 6.6 MiB / 90-crate cost Tom accepted under L-0, and it is a one-line
change that should land **on its own**, with the binary size measured before
and after on this box, so the number in `IN-FLIGHT-languages.md` §3 is
confirmed on the real workspace rather than on the two toy crates it was
measured with.

## 5. ⭐ NEW DECISION — L-9: what happens to a document saved under an
extension that is later removed?

Not in the original L-0..L-8, and tier 2 creates it.

`Language`'s `Deserialize` (`iridium-lang/src/lib.rs`) goes through
`from_id` and **refuses an identifier no language claims** — deliberately, so a
typo in a config is an error where it is written. Today that is only reachable
by hand-editing a file.

⛔ **Under tier 2 it becomes reachable by ordinary means: uninstall an
extension, reopen a workspace, and a persisted `Language` no longer resolves.**
The current behaviour would refuse the whole deserialization — so one removed
extension can fail the load of a session file that also describes fifteen
unrelated documents.

Three answers, and this needs a ruling before persistence touches extensions:

1. **Refuse, as today.** Honest, and hostile: removing one extension loses the
   session.
2. **Resolve to no-language, and report.** The document opens as plain text and
   a diagnostic names the missing extension. Matches the existing failure rule
   from #59 — *a bad extension never stops the editor, a mistake costs its own
   line* — which is the rule this crate is already meant to follow.
3. **Preserve the unknown id opaquely** so it re-binds if the extension comes
   back. Best behaviour, most machinery, and it re-opens the "unknown value
   round-trips silently" objection the current doc comment argues against.

**Recommendation: 2.** It is the rule already written for extensions elsewhere
in this codebase, and 3 can be added later without changing what 2 does.

## 6. Build order

Each step is independently landable and gated on the nine-gate battery.

1. **Enable tree-sitter's `wasm` feature alone.** Measure workspace binary size
   and clean-build time before and after; record both. Nothing uses it yet — the
   point is to price it on the real tree and confirm the battery stays green
   with 90 more crates in the graph.
2. **Break `COMPILED`'s fixed sizing** (§3a) with the static fast path intact
   and a spillover for `index() >= COUNT`. No loader yet, so nothing can reach
   the spillover — pin it with a test that registers a synthetic entry.
3. **The runtime registry in `iridium-lang`**: append-only, id leaked at
   registration (§3c), `all()` and `from_id` consulting both halves. Pin §2's
   invariant with a test that asserts static indices are unchanged after a
   runtime registration.
4. **The loader** — read `<dir>/<name>/{config.toml, grammar.wasm, *.scm}`,
   reusing #59's failure rules verbatim. Global directory only.
   ⛔ **NOT the project-local path — that is L-3 and it is still Tom's**, and it
   is the code-execution question, so it must not ride in on this step.
5. **`grammar()` consults the runtime table** (§3b), linked grammars winning.
6. **ABI version check at load (L-7)** — report a mismatch, never crash on it.
7. **L-9's answer** wired into persistence.

⚠️ Steps 1–3 change no behaviour any user can observe. **Nothing is
user-visible until step 4**, which is the first step that needs L-3 answered
even to be scoped, because "which directories do we scan" is its subject.

## 7. Still Tom's, and what each one blocks

| | question | blocks |
| --- | --- | --- |
| **L-3** | may a *project* carry a grammar (`.iridium/languages/`)? Opening a repository would load its wasm. | step 4's scope |
| **L-9** | document saved under a since-removed extension (§5) | step 7 |
| **L-1** | do the built-ins become extensions too, or stay statically linked and fast? | nothing — can ride the build |
| **L-5** | browser parity | nothing — the browser already runs wasm grammars |

L-1 and L-5 were already ruled "can ride the build". **L-3 and L-9 cannot** —
both decide behaviour a user sees, and L-3 is a security boundary.

## 8. What this map does not do

It does not price step 1 — that is step 1's own job, and the 6.6 MiB figure in
`IN-FLIGHT-languages.md` §3 was measured on **two scratch crates**, not on this
workspace. ⭐ Quoting it as this workspace's cost would be citing a figure whose
population is a different program. It is a good estimate and it is not a
measurement of the thing being changed.
