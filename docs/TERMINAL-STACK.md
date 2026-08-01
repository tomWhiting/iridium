# Terminal stack — verified API facts

Decision (30 Jul 2026, Tom's call): **`termina` 0.3.3 + `terminput` 0.5.15 +
`terminput-termina` 0.3.1**.

Everything here was verified by compiling — and where noted, *running* — spikes
against these crates on this toolchain (Rust 1.97.1) on 30 Jul 2026. It is
proven, not guessed. Do not re-derive it; correct it if a version bump proves
it wrong.

**Re-verified 1 Aug 2026.** Both spikes were rebuilt from source on the same
toolchain (rustc 1.97.1, `8bab26f4f`): `termina 0.3.3`, `terminput 0.5.15` and
`terminput-termina 0.3.1` all compile clean. The versions have not moved and
the facts below still hold, so the terminal face can be written against them
without a further spike.

## D1 is answered — modal *and* non-modal, and the kernel already does both

`PLAN.md` Phase 4 says "Decision D1 (modal vs non-modal keymaps) lands before
this phase starts." It has landed: Tom's call (31 Jul) was that he wants
near-Vim modes *and* the flexibility of everything else, and that turns out not
to be a trade at all, because the resolver was built for it.

`KeyBinding` (`commands/binding.rs`) carries `mode: Option<ModeName>` — the mode
a binding is scoped to, `None` meaning every mode — and `enters_mode:
Option<ModeName>`, the mode the binding switches into when it fires. Mode
transitions are **data in the keymap, not commands in the kernel**;
`KeymapResolver` (`commands/resolver.rs`) holds the active mode, discards a
pending prefix across a mode change, and its own documentation works Vim's `d2w`
through counts and mode transitions as the motivating example.

So modal and non-modal are the same machinery: non-modal is a keymap where
nothing sets a mode; Vim is a layer whose bindings are mode-scoped; a mode-free
binding stays live in every mode while a mode-scoped one outranks it for the
same sequence. They compose on the existing layer stack.

**The honest limit:** what exists is the machinery. No Vim keymap has been
authored — no normal/insert/visual binding set, no operator-pending grammar.
That is real work, but it is writing a keymap against a resolver that already
handles counts, captures and mode transitions; it is not building a modal
editor. Phase 4 is not blocked on it.

## termina 0.3.3

- Crate: `termina = { version = "0.3", features = ["event-stream"] }`
  `github.com/helix-editor/termina`, by Michael Davis (Helix maintainer).
  MIT OR MPL-2.0. 8,379 lines. edition 2021, rust-version 1.71.
  Norn pins the identical version in `norn-cli` and `norn-tui`.
- Dep tree is 19 packages total: bitflags, parking_lot, rustix (std/stdio/termios/event,
  no libc on the hot path), signal-hook, optional futures-core. Compiles in ~4s.
- Self-described as *"based on termwiz's terminal API, but Termina keeps feature setup
  outside the terminal type and mirrors crossterm's synchronous event-reader shape for
  `poll` and `read`."*

## No competing layout model — this is why it was chosen

`grep 'pub struct (Surface|Screen|Cell|Buffer)'` returns **nothing**. `Terminal` is
`pub trait Terminal: io::Write`. It is a pure VT manipulation library: it gives events in
and escape sequences out. Iridium's kernel keeps sole ownership of layout geometry
(~4,750 lines of pure layout logic). termwiz was rejected precisely because its
damage-tracked surface model would duplicate and compete with that.

## Verified public API

```rust
use termina::{PlatformTerminal, Terminal, PlatformHandle, EventReader, Event};
use termina::escape::csi::{Csi, DecPrivateMode, DecPrivateModeCode, Mode};
use termina::escape::osc::{Osc, Selection};

let mut term = PlatformTerminal::new()?;          // io::Result<PlatformTerminal>
term.set_panic_hook(|handle: &mut PlatformHandle| { /* write cleanup sequences */ });
let dims = term.get_dimensions()?;                 // io::Result<WindowSize>
```

`trait Terminal: io::Write` methods (all verified present):
- `enter_raw_mode() -> io::Result<()>`
- `enter_cooked_mode() -> io::Result<()>` — restores the termios state captured at open
- `get_dimensions() -> io::Result<WindowSize>`
- `event_reader() -> EventReader` — cloneable
- `poll<F: Fn(&Event) -> bool>(filter, timeout: Option<Duration>) -> io::Result<bool>`
  **`timeout: None` blocks** → zero-CPU idle event loop falls straight out, no poll hack.
- `read<F: Fn(&Event) -> bool>(filter) -> io::Result<Event>`
- `set_panic_hook(impl Fn(&mut PlatformHandle) + Send + Sync + 'static)`
  Receives a stdout handle; termina restores platform mode as if `enter_cooked_mode` ran
  **after** the hook. This covers the worst TUI failure mode (terminal left in raw mode
  after a panic) as a library guarantee rather than by our own discipline. The hook is
  where we must undo *application*-level state: leave the alternate screen, disable
  bracketed paste, pop keyboard-enhancement flags, re-show the cursor.
- Unix `PlatformTerminal` also has a `Drop` impl (terminal/unix.rs:237) that restores.

Escape sequences are values implementing `Display` — write them with `write!`:

```rust
Csi::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(
    DecPrivateModeCode::ClearAndEnableAlternateScreen)))   // alt screen on
Csi::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(
    DecPrivateModeCode::SynchronizedOutput)))              // tear-free repaint
Csi::Mode(Mode::SetDecPrivateMode(DecPrivateMode::Code(
    DecPrivateModeCode::BracketedPaste)))                  // paste as one op
Osc::SetSelection(Selection::CLIPBOARD, "text")            // OSC 52, ships own base64
```
Use `Mode::ResetDecPrivateMode` to turn each off. All four verified to compile and format.

Event model lives in `termina::event`: `Event`, `KeyEvent`, `Modifiers` (bitflags, u8),
`KeyEventState`, `KeyCode`. Kitty keyboard protocol is supported
(`KeyboardEnhancementFlags`, incl. `REPORT_ASSOCIATED_TEXT`) — this is the only way
Ctrl+Shift+Z is distinguishable from Ctrl+Z at all. There is a `detect-features` example
for capability negotiation, and `escape/csi.rs` is 2,399 lines of typed CSI modelling.

## Fork posture

MIT lets us fork freely; 8.4K lines with a clean module split (escape/, event/, parse/,
terminal/, style) makes it genuinely tractable. **Do not fork yet.** The things that would
tempt a fork — capability negotiation and a damage/cell layer — belong in `iridium-tui`
anyway, since termina deliberately omits them. The real fork signal is needing to change
VT *parsing* behaviour, or an upstream stall on a protocol change we need. Flag it if hit.

## Caveats to carry

- Pre-1.0 (0.3.x): API churn is a real risk. Mitigated by pinning the same version Norn
  already uses — one terminal stack across the whole ablative estate beats two.
- Lower-level than crossterm: we emit escape sequences ourselves. Acceptable because they
  are *typed* values, not string literals, and because the kernel must own layout anyway.

## terminput 0.5.15

`terminput` is the reason the input path can be tested without a pty. It ships
a parser **and an encoder**, so a test can synthesise the exact bytes a real
terminal would send, feed them through the same parse used in production, and
assert on the resulting kernel command. No pty, no timing, no flake.

`no_std` by default with an `alloc` requirement; the `std` feature (on by
default) is what enables the parser and encoder modules. 

```rust
use terminput::{Encoding, Event, KeyCode, KeyEvent, KeyModifiers, KittyFlags};

// Parse: an associated function on Event, not a free function.
let event: Option<Event> = Event::parse_from(b"\x1b[A").ok().flatten();
// => Some(Key(KeyEvent { code: Up, .. }))

// Encode: 16 bytes of buffer is stated by the crate to be always sufficient.
let ev = Event::Key(KeyEvent::new(KeyCode::Char('z')).modifiers(KeyModifiers::CTRL));
let mut buf = [0u8; 16];
let n = ev.encode(&mut buf, Encoding::Xterm)?;                  // 1 byte:  [26]
let n = ev.encode(&mut buf, Encoding::Kitty(KittyFlags::all()))?; // 8 bytes: ESC [ 1 2 2 ; 5 u
```

Those two encodings are the whole kitty-protocol argument in one line, and both
outputs above were **observed, not predicted**. Under legacy xterm, Ctrl+Z is
the single byte `26`, and a real terminal sends that same byte for Ctrl+Shift+Z,
so the two can never be told apart. Under the kitty protocol it is `ESC[122;5u`,
carrying the base keycode and a modifier bitmask, so they can. Any editor that
wants Ctrl+Shift+something as a distinct binding needs the kitty protocol; there
is no cleverness that recovers it from the legacy stream.

**Correction (1 Aug), found while building the input adapter.** The sentence
above originally said the two were "byte-identical". That is true of the *wire*
but not of terminput's encoder: `encode(Ctrl+Shift+Z, Encoding::Xterm)` does not
produce byte 26, it **returns an error** — the legacy encoding simply has no
form for that chord. All three facts are now pinned by
`ctrl_shift_z_is_indistinguishable_from_ctrl_z_under_xterm_only` in
`iridium-tui`: Ctrl+Z encodes to `[26]`, Ctrl+Shift+Z fails to encode, and the
byte `26` parses back as Ctrl+Z with shift absent.

Three further legacy collisions were found and are pinned by the same test file,
because each is a fact about terminals rather than a gap to paper over: under
xterm `Ctrl+I` is Tab, `Ctrl+M` is Enter and `Ctrl+Backspace` is `Ctrl+H` (all
resolved in favour of the named key, which is what the byte has meant since the
VT100); Super/Hyper/Meta do not exist in the legacy stream at all, so `Super+P`
arrives as a bare `p`; and key *releases* and *repeats* are kitty-only.

## Which keyboard-enhancement flags the driver must push

Established while building the adapter and pinned by
`report_all_keys_costs_the_shifted_character`. Push:

```text
DISAMBIGUATE_ESCAPE_CODES | REPORT_EVENT_TYPES | REPORT_ALTERNATE_KEYS
```

and **not** `REPORT_ALL_KEYS_AS_ESCAPE_CODES`. That last flag puts every
printable key into CSI-u form, where the sequence names the *key* rather than
the character produced — and terminput 0.5.15 emits an alternate codepoint only
for ASCII letters, ignoring the associated-text field entirely. The consequence
is concrete: with it set, typing `!` would insert `1`. Leaving printable keys in
their legacy form avoids the whole class.

## Upstream bug to carry (terminput 0.5.15)

The **kitty encoder mis-encodes PageUp/PageDown**: it appends its `u` terminator
to the legacy form, emitting `ESC[5~u`, which terminput's own parser then
rejects. Harmless to Iridium — we only ever *parse*, and both protocols use the
same `CSI 5 ~` bytes for those keys — so the affected tests write the bytes
literally and say why. Worth reporting upstream.

`Encoding` has exactly two variants — `Xterm` and `Kitty(KittyFlags)` — so
tests can pin behaviour under both, which is what a legacy-fallback path
requires.

## terminput-termina 0.3.1

The bridge. Free functions in both directions, **behind a default-on
`termina_0_3` feature** — with `default-features = false` the crate compiles to
nothing at all and the conversions silently vanish. Keep the default on.

```rust
terminput_termina::to_terminput(termina_event) -> Result<Event, UnsupportedEvent>
terminput_termina::to_termina(terminput_event) -> Result<termina::Event, UnsupportedEvent>
```

`to_terminput` covers focus in/out, key, mouse, paste and resize. It returns
`UnsupportedEvent` for raw `Dcs`/`Csi`/`Osc` events, which is correct: those are
capability replies, not user input, and they belong to the terminal-negotiation
layer in `iridium-tui`, not to the kernel's input path. **Do not discard that
error** — it is the signal that a capability reply arrived, and swallowing it is
how feature detection silently stops working.

Verified round-trip: `termina::Event::Key(Ctrl+'a')` converts to
`terminput::Event::Key(KeyEvent { code: Char('a'), modifiers: CTRL, kind: Press })`.

## How the three compose

```text
  terminal bytes
        │
        ▼
  termina::EventReader        VT parsing, raw mode, panic-safe restore
        │  termina::Event
        ▼
  terminput_termina::to_terminput     one small, testable conversion
        │  terminput::Event
        ▼
  iridium-tui input adapter   terminput::Event -> iridium KeyCode + Modifiers
        │
        ▼
  kernel command registry     platform-neutral, already exists
```

Two conversions, each independently testable, and only the last one is ours.
The kernel's `KeyCode`/`Modifiers` types are already platform-neutral, so the
adapter is a pure mapping function with no state — which means the whole input
path above the terminal can be tested with `Event::encode` and zero I/O.
