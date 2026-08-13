# Who writes `config.toml`, and what is in it — design map

**Ground verified 13 Aug 2026 against `d22baa93`. Nothing below is recalled.**

This closes the item `apps/iridium/src/app/config.rs` logged and deliberately
did not guess: *"The terminal layer's own chords being absent from the written
file is a real and separate defect, and needs a ruling rather than a guess."*

The ground turned out worse than that note recorded, and one fact turned out to
make the fix much cheaper than it looks. Both are below.

---

## 1. The ask this is measured against

`crates/iridium-config/src/template_keys.rs` quotes it at the top of the file:

> "You had to add the key bindings yourself and you don't know what they are…
> You need to have them all already listed there because you couldn't possibly
> expect somebody to know the name of all the key bindings."

So the test is not "is the file well-formed". It is **can a person find every
binding by opening the file**.

---

## 2. What is actually true today

### 2.1 ⛔ The terminal binary never writes the file at all

Verified: `apps/iridium/src/app/mod.rs:162` calls
`iridium_config::user_config_path()` and `UserConfig::read()`. There is no call
to `create_if_absent` anywhere under `apps/iridium/`. The only two callers are
both in the desktop app — `src/run.rs:92` (startup) and
`src/app/host_commands.rs:221` (*Edit Configuration*).

**Consequence:** somebody who installs both faces and only ever runs `iridium`
has **no `config.toml` on disk**, and therefore no listing of anything. The ask
fails completely for them, not partially.

This is not what the note in `config.rs` describes. That note reasoned about
*contents*; the gap is *existence*.

### 2.2 The two faces' own command vocabularies genuinely differ

Read out of the two `commands` modules:

| only `apps/iridium` (terminal) | only `apps/iridium-desktop` | both |
| --- | --- | --- |
| `app.quit`, `file.reload`, `fold.all`, `fold.none`, `fold.toggle`, `goto.line` | `commands.list`, `config.edit`, `file.new`, `file.open`, `file.saveAs`, `project.open`, `project.set` | `file.save`, `file.saveForce` |

Six terminal-only and seven desktop-only commands, plus their chords, are
invisible to the other face's writer.

### 2.3 The panel layers each face can see

| layer | lives in | reachable from `iridium-config` |
| --- | --- | --- |
| kernel default keymap | `iridium-editor` | ✅ direct dependency |
| file explorer | `iridium-panel` | ✅ direct dependency |
| desktop face layer, palette, undo tree, right-click menu | `apps/iridium-desktop` | ❌ handed in via `FaceKeys` |
| terminal face layer, palette, undo tree, search | `apps/iridium` + `crates/iridium-tui` | ❌ **handed in by nobody** |

`crates/iridium-tui`'s three `keymap.rs` files each say so in their own module
docs: *"this face hands no `FaceKeys` to `iridium_config` yet, so nothing lists
these."* That is ~39 chords with no listing anywhere.

### 2.4 ⭐ Every generated line is commented out

`template_keys.rs`, "⛔ Commented out, not live": every binding is emitted behind
`# `, deliberately, so that a file written last year cannot pin last year's
defaults.

**This is the fact that decides the shape of the fix.** A commented line is
never installed, so a file that lists *both* faces' bindings makes **neither
face report a problem**. The obvious objection to one complete file — "the
desktop would complain about `app.quit`" — is not real. It only becomes real if
a user uncomments a line their face does not have, and being told that is
correct behaviour, not a defect.

### 2.5 Why the edge cannot simply be reversed

`iridium-config` cannot depend on a face: every face depends on it. And
`iridium-desktop` must not depend on `iridium-tui` — that would pull `termina`,
`terminput` and `terminput-termina` into a GPU application to read three tables.

---

## 3. The rulings

### D-1 — Does the terminal binary become a writer?

**Recommendation: yes, and it is not really optional.** §2.1 is a plain defect:
a user of one of the two shipped faces gets no configuration file. Whatever is
ruled for D-2, `apps/iridium` must call `create_if_absent` at startup exactly as
`iridium-desktop/src/run.rs:92` does.

The reason the previous seat held off is stated in `config.rs`: two writers
would make the file's contents depend on which binary ran first. That reason is
sound and is exactly what D-2 answers — it is an argument for making the two
writers produce the *same* file, not for having only one.

*Revert cost: one call site in `apps/iridium/src/app/mod.rs`.*

### D-2 — Do both writers produce the same file?

**A. Yes — every layer moves below both faces.** A new crate (`iridium-keys`)
depending on `iridium-editor` alone holds every panel's `default_keymap()` and
every face's own layer. `iridium-config` reads them all directly; `FaceKeys` and
its three ratchets are deleted. The file is byte-identical whoever writes it.

- Cost: seven `keymap.rs` files move crates, plus the two faces' own layers,
  which drags their command *metadata* down too (the generator needs titles).
  Every `keymap_tests.rs` reaches across a crate boundary. Large.
- ⚠️ It contradicts a recorded ruling —
  `commands/builtin/panel/mod.rs`: *"the kernel owns the names… and the panel
  owns the keys, so that a face with different screens is not forced to pretend
  it has the explorer's."* Locality of a table beside the panel it drives is
  what #117 leaned on seven times.

**B. Yes, for panels only.** The four face-specific *panel* keymaps move down
(they name kernel ids only, so they move cleanly); each face's own top layer
stays handed in via `FaceKeys`. The file then differs between writers by exactly
one section — thirteen commands, §2.2.

- Cost: four files move. The `FaceKeys` mechanism survives for the one thing it
  was actually written for.
- Price: the file still is not identical, and cannot claim to be complete.

**C. No — each writer writes what it can see, and the file says so.** Add a
header line naming the face that wrote the file and stating plainly that the
other face's own chords are not listed here.

- Cost: one paragraph in the template.
- Price: the ask is answered with an apology. A user still cannot find
  `fold.toggle` by opening the file the desktop wrote.

**Recommendation: B.** It closes the ~39 unlisted panel chords, which is the
bulk of the gap, at four file moves; it leaves the residue at thirteen
face-bound commands, which are the ones genuinely *about* a particular face and
the only ones a heading can honestly explain. A goes further than the ask needs
and costs a recorded architectural ruling to get there.

*Revert cost for B: four `keymap.rs` files move back and four `use` lines change.
No behaviour depends on where the table lives.*

### D-3 — If B, where do the four panel tables live?

Not `iridium-panel` — that crate is the file explorer and its row vocabulary,
and a terminal search overlay's table is a category error there. A new
`crates/iridium-keys` depending on `iridium-editor` alone, one module per panel,
is the honest home. `iridium-config` gains it as a dependency.

---

## 4. What is not in scope here

* `config.reload` on the terminal face — logged for #102, still true, still not
  this.
* Whether the `~shift` spelling stays in the generated block. Open with Tom,
  nothing waiting on it; the recommendation remains to leave it, because the
  prettified form would parse into a stricter chord and fail silently.
