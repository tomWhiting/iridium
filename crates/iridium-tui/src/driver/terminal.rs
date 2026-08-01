//! The one part that owns a real terminal.
//!
//! Everything here needs a tty, so everything that could be decided without
//! one has been moved out: [`super::frame`] decides what bytes a repaint is,
//! [`super::lifecycle`] decides what entering and leaving the terminal are,
//! and [`super::capabilities`] decides what the terminal can do. What is left
//! is opening the device, flipping termios, blocking on `poll`, and routing
//! each event to one of those three. That is the untestable surface, and it is
//! deliberately this thin.

use std::io::{self, Write as _};
use std::sync::Arc;
use std::time::{Duration, Instant};

use termina::{Event, PlatformTerminal, Terminal as _};
use terminput::UnsupportedEvent;

use super::capabilities::{Capabilities, Negotiation};
use super::frame::{CursorState, Frame, apply_resize, write_frame};
use super::lifecycle::{TerminalState, write_setup, write_teardown};
use crate::cell::Surface;
use crate::input::{TerminalEvent, TerminalInput, from_termina};

/// How long start-up waits for the terminal to answer its capability queries.
///
/// A terminal answers a device-attributes request in well under a millisecond;
/// one that has not answered in this long is not going to, and the environment
/// has already given an answer that works. The cost of being wrong in this
/// direction is a session without the kitty keyboard protocol, which is the
/// documented legacy path. The cost of waiting longer is start-up latency,
/// which the plan's feel gate measures.
pub const NEGOTIATION_TIMEOUT: Duration = Duration::from_millis(100);

/// One thing that happened, after capability replies have been dealt with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverEvent {
    /// Something the user did. Ready for the keymap, or for the frame layer in
    /// the case of a mouse event.
    Input(TerminalInput),

    /// The terminal was resized and the surface has already been resized to
    /// match, which invalidates it: the next frame is a full repaint.
    Resize {
        /// The new width in cells.
        columns: usize,
        /// The new height in cells.
        rows: usize,
    },

    /// An event the VT parser could not name — a key with no key code at all.
    ///
    /// Surfaced rather than swallowed. It carries nothing to act on, but a
    /// caller that sees a stream of these is looking at a terminal sending
    /// something this stack does not understand, and silence would make that
    /// look like a dead keyboard.
    Unrecognised,
}

/// The terminal, the surface, and the loop that connects them.
///
/// Dropping a `Driver` leaves the terminal exactly as it was found, and so
/// does a panic: the same teardown runs from [`Drop`] and from the panic hook
/// `termina` installs, driven by the same [`TerminalState`].
#[derive(Debug)]
pub struct Driver {
    /// The terminal device.
    terminal: PlatformTerminal,
    /// What has been turned on, shared with the panic hook.
    state: Arc<TerminalState>,
    /// The capability negotiation, kept so that late replies still count.
    negotiation: Negotiation,
    /// The front/back cell buffers the frame layer draws into.
    surface: Surface,
    /// Where the caret should be left at the end of the next frame.
    cursor: CursorState,
}

impl Driver {
    /// Opens the terminal, negotiates with it, and enters it.
    ///
    /// The panic hook is installed **before** raw mode is entered and before
    /// any mode is set, so there is no window in which a panic could leave the
    /// terminal changed and unrestored.
    ///
    /// # Errors
    ///
    /// Returns any error from opening the terminal, entering raw mode, reading
    /// its dimensions, or writing the set-up sequences.
    pub fn open() -> io::Result<Self> {
        let mut terminal = PlatformTerminal::new()?;
        let state = Arc::new(TerminalState::new());

        let hook_state = Arc::clone(&state);
        terminal.set_panic_hook(move |handle| {
            // The process is already unwinding, so there is nowhere to report
            // a failure to write; leaving the terminal as unrestored as
            // possible is the only remaining goal. `termina` restores termios
            // itself once this returns.
            let _ = write_teardown(handle, &hook_state);
        });

        terminal.enter_raw_mode()?;

        let negotiation = negotiate(&mut terminal)?;
        let capabilities = negotiation.capabilities();

        write_setup(&mut terminal, capabilities, &state)?;
        terminal.flush()?;

        let size = terminal.get_dimensions()?;
        let surface = Surface::new(
            usize::from(size.cols),
            usize::from(size.rows),
            capabilities.color_depth,
        );

        Ok(Self {
            terminal,
            state,
            negotiation,
            surface,
            cursor: CursorState::Hidden,
        })
    }

    /// What the terminal was shown to be able to do.
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.negotiation.capabilities()
    }

    /// The buffers a frame is drawn into.
    #[must_use]
    pub const fn surface(&self) -> &Surface {
        &self.surface
    }

    /// The buffers a frame is drawn into, mutably.
    pub const fn surface_mut(&mut self) -> &mut Surface {
        &mut self.surface
    }

    /// Where the caret will be left at the end of the next frame.
    #[must_use]
    pub const fn cursor(&self) -> CursorState {
        self.cursor
    }

    /// Sets where the caret is left at the end of the next frame.
    pub const fn set_cursor(&mut self, cursor: CursorState) {
        self.cursor = cursor;
    }

    /// Declares that nothing on screen can be trusted, forcing a full repaint.
    ///
    /// For the moments when something other than this driver wrote to the
    /// terminal: a resume from suspend, a subprocess that took the screen.
    pub const fn invalidate(&mut self) {
        self.surface.invalidate();
    }

    /// Writes the damage between the two buffers and commits it.
    ///
    /// The surface is committed only after the bytes have reached the
    /// terminal, so a failed write leaves the damage still owed and the next
    /// call will try again.
    ///
    /// # Errors
    ///
    /// Returns any error from writing or flushing.
    pub fn present(&mut self) -> io::Result<()> {
        let damage = self.surface.damage();
        let frame = Frame {
            damage: &damage,
            capabilities: self.negotiation.capabilities(),
            cursor: self.cursor,
        };
        write_frame(&mut self.terminal, &frame, &self.state)?;
        self.terminal.flush()?;
        self.surface.commit();
        Ok(())
    }

    /// Blocks until something happens, and returns it.
    ///
    /// `termina`'s `poll` with no timeout blocks, so an idle editor uses no
    /// CPU at all: there is no interval to tune and no sleep to get wrong.
    ///
    /// Capability replies never come back from here. They are folded into the
    /// negotiation instead — a terminal can change underneath a multiplexer,
    /// and a reply that arrives an hour into a session is as meaningful as one
    /// that arrives at start-up — and a depth change re-paints the screen,
    /// because what is on it was painted in the old depth.
    ///
    /// # Errors
    ///
    /// Returns any error from polling or reading the terminal.
    pub fn next_event(&mut self) -> io::Result<DriverEvent> {
        loop {
            self.terminal.poll(|_| true, None)?;
            let event = self.terminal.read(|_| true)?;
            if let Some(driver_event) = self.classify(event) {
                return Ok(driver_event);
            }
        }
    }

    /// Sorts one terminal event, returning `None` for one that was consumed by
    /// capability negotiation.
    fn classify(&mut self, event: Event) -> Option<DriverEvent> {
        match from_termina(event) {
            Ok(TerminalEvent::Capability(reply)) => {
                let before = self.negotiation.capabilities();
                self.negotiation.apply(&reply);
                let after = self.negotiation.capabilities();
                if before.color_depth != after.color_depth {
                    self.surface.set_depth(after.color_depth);
                }
                None
            },
            Ok(TerminalEvent::Input(TerminalInput::Resize { rows, cols })) => {
                apply_resize(&mut self.surface, cols, rows);
                Some(DriverEvent::Resize {
                    columns: self.surface.width(),
                    rows: self.surface.height(),
                })
            },
            Ok(TerminalEvent::Input(input)) => Some(DriverEvent::Input(input)),
            Err(UnsupportedEvent(_)) => Some(DriverEvent::Unrecognised),
        }
    }

    /// Leaves the terminal, restoring everything this driver turned on.
    ///
    /// Calling this is optional — [`Drop`] does the same work — but it is the
    /// only way to see an error from it.
    ///
    /// # Errors
    ///
    /// Returns the first error from writing the teardown sequences or from
    /// restoring cooked mode, after attempting all of them.
    pub fn close(&mut self) -> io::Result<()> {
        let teardown = write_teardown(&mut self.terminal, &self.state);
        let cooked = self.terminal.enter_cooked_mode();
        teardown.and(cooked)
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        // There is nowhere to report an error from a destructor, and teardown
        // is already best-effort internally: every step is attempted even when
        // an earlier one fails. It is idempotent, so an explicit `close`
        // followed by this writes nothing.
        let _ = self.close();
    }
}

/// Asks the terminal what it can do, and waits a bounded time for the answer.
///
/// The queries go out in one write and the replies come back on the same
/// stream as user input, so the filter here matters: only escape responses are
/// consumed, and a key pressed during start-up stays buffered in the event
/// reader for the main loop to read.
fn negotiate(terminal: &mut PlatformTerminal) -> io::Result<Negotiation> {
    let mut negotiation = Negotiation::new(Capabilities::from_environment());
    Negotiation::write_queries(terminal)?;
    terminal.flush()?;

    let started = Instant::now();
    loop {
        // Subtracting rather than adding to an `Instant` keeps this total:
        // `Instant + Duration` panics on overflow, and a panic here would land
        // with the terminal already in raw mode.
        let Some(remaining) = NEGOTIATION_TIMEOUT.checked_sub(started.elapsed()) else {
            return Ok(negotiation);
        };
        if !terminal.poll(Event::is_escape, Some(remaining))? {
            return Ok(negotiation);
        }
        let event = terminal.read(Event::is_escape)?;
        if let Ok(TerminalEvent::Capability(reply)) = from_termina(event) {
            let complete = negotiation.apply(&reply);
            if complete {
                return Ok(negotiation);
            }
        }
    }
}
