# Doug — in-flight state at the 12 Aug compaction

Written because the next thing to happen is a `/compact`, and everything below
lived only in the conversation.

## ⛔ THE ONE ACTION IN FLIGHT — the desktop install

**Tom authorised it and is waiting on it.** He quit the app specifically so it
could run (`pgrep -x iridium-desktop` returned nothing before it started).

- Running as a background task, log at
  `<scratchpad>/install.log`, command `./apps/iridium-desktop/bundle/install.sh`.
- It is a **cold release build**, so it takes a long time on a loaded box.

**When it finishes, the verification is not the exit status.** The receipt Tom
was promised, and the one now written into `docs/SESSION-STATE.md`:

```sh
ls -la /Applications/iridium.app/Contents/MacOS/iridium-desktop     # date
strings -a /Applications/iridium.app/Contents/MacOS/iridium-desktop | grep -c 'file.open'
```

⭐ **The grep matters more than the date.** A timestamp says a file was written;
only the string says *this* change is in it. Pre-install measurement, for the
before/after pair: bundle dated **9 Aug 11:55**, and `strings` on it found
`file.save`, `config.edit`, `commands.list` but **zero** occurrences of
`file.open`, `project.open` or `project.set`. That is why Tom's `⌘O` did
nothing — the commands were not in the binary he was running.

If the install failed, say so with its output; do not retry blind, and **never**
kill a running `iridium-desktop` to force it — the installer refuses on purpose.

## What was landed and verified this sitting

- `aa66c00c` — #69 fixed. `8b1b350d` — extension rulings + receipt.
- **Pushed and verified from the remote**: `origin/main = 8b1b350d`,
  `git rev-list --count origin/main..HEAD` = 0, measured at fetch.
- **Ten gates green**, read from `ci.sh`'s own `>>> … OK` lines and its
  `✅ all 10 gates passed`, **not** from its exit status.
- #69: 8 failures in 87 runs before, **0 in 455 after**, loads 39–433.

## Uncommitted at the moment of compaction

`docs/EXTENSIBILITY.md` only — three additions, all recorded and none lost:

1. **§0e riders (Waffles).** (a) The three bundled participants must be
   buildable out-of-tree in principle, compiling against nothing but the
   published surface — otherwise the vocabulary discovered is the in-tree
   dialect and the whole design-pressure argument evaporates; the no-bypass
   rule guards the registry door and *not* shared types or direct imports.
   (b) The falsifier widens: **a poll is an event hook with worse manners** — if
   a participant can only be expressed with a timer, the deferral has already
   failed quietly.
2. **§0f Hermes' liminal ground** — liminal has **no** surface vocabulary, by
   design, so mint ours; contributions travelling as admitted records make
   blocking *structurally inexpressible*; version the vocabulary from day one;
   liminal's in-process mount is **never the fast path**; latency RTT is an
   open debt he named rather than guessed.
3. **A factual correction**: the old claim that a liminal participant "can
   resume mid-stream" was wrong. Resume is at the **record** level; a shed
   subscription feed is **terminal** and must be re-opened, not resumed.

## Owed to people

- **Hermes Crumpet** (`dm:5b70322e-e7a9-451c-91ca-a3dfa7b05bd9`) — a reply.
  He corrected our doc, refused to invent a latency number, and answered the
  surface-vocabulary question that unblocks the irreversible decision.
- **Tom** (`dm:c9255b2a-5731-4d17-8124-e3bfa2224186`) — the install receipt,
  and that Waffles confirmed the 30 July seam stands *and* that the compression
  error was his own, not Tom's misremembering. Tom recalled the ruling more
  accurately than either of us restated it; he should be told plainly.
- **Waffles** (`dm:896955e1-86dd-4d6a-9c25-f3f978b189a9`) — both riders are
  recorded verbatim with attribution.

## Next work, in order

1. Finish and report the install.
2. **#111** New File + a way in to Save As. ⚠️ `save_as` already exists
   (`DesktopApp::save_as`, `Prompt::save_as`, `Answer::SaveAs`) — only the
   command that reaches it for an already-named document is missing. New File
   is genuinely absent.
3. **#112** oil surface: taller, hidden-file filter, **as a sidebar**, and in
   the terminal face — Tom named the directory-open path as shaping the
   terminal design rather than following it.
4. **#113** the pending-operator mechanism, before any modal keymap is authored.
