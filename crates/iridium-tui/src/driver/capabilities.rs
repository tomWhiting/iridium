//! What the terminal can actually do, and how that is found out.
//!
//! Three questions are answered here, and each one has a fallback that has to
//! be correct rather than theoretical, because the terminals that answer
//! nothing are exactly the ones a text editor still has to work on.
//!
//! # How much colour
//!
//! The environment gives a first answer and a runtime query refines it.
//! `COLORTERM=truecolor` (or `24bit`) is the convention every terminal that
//! supports 24-bit colour follows; a `TERM` naming `direct` or `truecolor`
//! says the same thing through terminfo; a `TERM` naming `256color` asks for
//! the indexed palette; anything else, including no `TERM` at all, gets the
//! sixteen basic colours, which every terminal since the VT100 can show.
//! `TERM=dumb` is not a terminal this face can drive, and it is answered with
//! the most conservative capabilities in the type rather than with an
//! optimistic guess.
//!
//! The runtime check is the one from `termina`'s own `detect-features`
//! example: set a 24-bit background, ask the terminal to report its graphic
//! rendition back with DECRQSS, and see whether the colour survived. A
//! terminal that quantised it answers with something else, and the depth
//! drops to the palette. This can only *narrow* what the environment claimed:
//! `COLORTERM` is set by the terminal itself, and a terminal that both sets it
//! and fails the probe has told us the truth twice over in the second answer.
//!
//! # Whether the kitty keyboard protocol is available
//!
//! Not knowable from the environment at all, and assumed absent until the
//! terminal reports its flags. That is the correct default: the legacy
//! encoding works everywhere, and `crate::input` treats it as a first-class
//! path rather than a degraded one. Pushing enhancement flags at a terminal
//! that does not understand them is harmless, but *believing* they were
//! accepted is not, because the driver would then have to pop them on exit and
//! the pop would be as ignored as the push.
//!
//! Which flags to push is settled by `docs/TERMINAL-STACK.md` and pinned by a
//! test in `crate::input`: see [`KEYBOARD_ENHANCEMENT_FLAGS`], and note which
//! flag is deliberately absent from it.
//!
//! # Whether synchronized output is available
//!
//! Assumed **present** until the terminal says otherwise, which is the
//! opposite default from the other two, for a reason. An unrecognised DEC
//! private mode is ignored by every VT-compatible terminal — that is what the
//! private-mode namespace is for — so emitting BSU/ESU at a terminal that does
//! not implement it costs eight bytes a frame and changes nothing. Not
//! emitting it at a terminal that does implement it costs a torn frame on
//! every scroll. The query therefore exists to turn the feature *off* when a
//! terminal explicitly reports the mode as not recognised.

use std::io;

use termina::escape::csi::{
    Csi, DecModeSetting, DecPrivateMode, DecPrivateModeCode, Device, Keyboard, KittyKeyboardFlags,
    Mode, Sgr,
};
use termina::escape::dcs::{Dcs, DcsRequest, DcsResponse};
use termina::style::RgbColor;

use crate::cell::ColorDepth;
use crate::input::CapabilityReply;

#[cfg(test)]
mod tests;

/// The keyboard-enhancement flags the driver pushes when the terminal has one.
///
/// `REPORT_ALL_KEYS_AS_ESCAPE_CODES` is deliberately **not** among them. It
/// puts every printable key into CSI-u form, where the sequence names the key
/// rather than the character it produced, and `terminput` 0.5.15 emits an
/// alternate codepoint only for ASCII letters. With it set, typing `!` inserts
/// `1`. See `docs/TERMINAL-STACK.md` and the test
/// `report_all_keys_costs_the_shifted_character` in `crate::input`.
pub const KEYBOARD_ENHANCEMENT_FLAGS: KittyKeyboardFlags =
    KittyKeyboardFlags::DISAMBIGUATE_ESCAPE_CODES
        .union(KittyKeyboardFlags::REPORT_EVENT_TYPES)
        .union(KittyKeyboardFlags::REPORT_ALTERNATE_KEYS);

/// The 24-bit colour the truecolor probe sets and looks for in the reply.
///
/// Any colour outside the 216-entry cube and the grey ramp would do; a
/// terminal that quantises to the palette cannot answer with it. This one is
/// the value `termina`'s `detect-features` example uses, so a terminal known
/// to pass that example passes this.
pub const PROBE_COLOR: RgbColor = RgbColor::new(150, 150, 150);

/// What the terminal being driven can show and report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// How much colour the terminal can display. Damage is computed and
    /// styles are emitted at this depth.
    pub color_depth: ColorDepth,

    /// Whether the terminal implements the kitty keyboard protocol.
    ///
    /// When this is false, `Ctrl+Shift+`-style chords and key releases cannot
    /// reach the editor at all: the bytes a legacy terminal sends do not carry
    /// them. That is a property of the terminal, not a gap to work around.
    pub keyboard_enhancement: bool,

    /// Whether the terminal implements synchronized output (DEC mode 2026).
    ///
    /// A frame is wrapped in BSU/ESU when it does, which makes the repaint
    /// atomic and stops a fast scroll from showing half of two frames.
    pub synchronized_output: bool,
}

impl Capabilities {
    /// The least a terminal can do and still be driven at all.
    ///
    /// Sixteen colours, no keyboard protocol, no synchronized output. This is
    /// what an unrecognised or absent `TERM` gets, and what negotiation falls
    /// back to when nothing answers.
    pub const CONSERVATIVE: Self = Self {
        color_depth: ColorDepth::Ansi16,
        keyboard_enhancement: false,
        synchronized_output: false,
    };

    /// The capabilities implied by the environment, before any query.
    ///
    /// `colorterm` and `term` are the values of `COLORTERM` and `TERM`, or
    /// `None` when the variable is unset. Taking them as arguments rather than
    /// reading the environment is what makes this testable; see
    /// [`Self::from_environment`] for the reading half.
    #[must_use]
    pub fn from_env(colorterm: Option<&str>, term: Option<&str>) -> Self {
        if term.is_some_and(|term| term.trim().eq_ignore_ascii_case("dumb")) {
            return Self::CONSERVATIVE;
        }
        Self {
            color_depth: color_depth_from_env(colorterm, term),
            keyboard_enhancement: false,
            synchronized_output: true,
        }
    }

    /// The capabilities implied by this process's environment.
    ///
    /// The whole of the reading of the environment is these two lookups; every
    /// decision made from them is in [`Self::from_env`].
    #[must_use]
    pub fn from_environment() -> Self {
        let colorterm = std::env::var("COLORTERM").ok();
        let term = std::env::var("TERM").ok();
        Self::from_env(colorterm.as_deref(), term.as_deref())
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::CONSERVATIVE
    }
}

/// How much colour the environment claims the terminal can show.
///
/// See the module documentation for the rules and why each is what it is.
#[must_use]
pub fn color_depth_from_env(colorterm: Option<&str>, term: Option<&str>) -> ColorDepth {
    let colorterm = colorterm.unwrap_or_default().trim();
    if colorterm.eq_ignore_ascii_case("truecolor") || colorterm.eq_ignore_ascii_case("24bit") {
        return ColorDepth::TrueColor;
    }
    let term = term.unwrap_or_default().to_ascii_lowercase();
    if term.contains("truecolor") || term.contains("direct") {
        ColorDepth::TrueColor
    } else if term.contains("256color") {
        ColorDepth::Ansi256
    } else {
        ColorDepth::Ansi16
    }
}

/// How much colour a depth can express, for comparing two of them.
const fn richness(depth: ColorDepth) -> u8 {
    match depth {
        ColorDepth::Ansi16 => 0,
        ColorDepth::Ansi256 => 1,
        ColorDepth::TrueColor => 2,
    }
}

/// The poorer of two colour depths.
const fn narrower(left: ColorDepth, right: ColorDepth) -> ColorDepth {
    if richness(left) <= richness(right) {
        left
    } else {
        right
    }
}

/// Asking the terminal what it can do, and reading the answers.
///
/// The queries are written once, the replies arrive as ordinary events on the
/// same stream as user input, and the primary device-attributes reply is the
/// sentinel that says every earlier query has been answered — a terminal
/// answers in order, and every terminal answers `DA1`. Nothing here does any
/// I/O beyond writing the queries into a sink the caller provides, so the
/// whole state machine can be driven by handing it replies a test built.
///
/// Replies that arrive later, unsolicited, are still meaningful: a terminal
/// multiplexer can change what is underneath it while the editor runs. Feeding
/// them back in is therefore always safe, and [`Self::apply`] stays correct
/// after [`Self::is_complete`] is true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiation {
    /// What the environment claimed, narrowed by every reply so far.
    capabilities: Capabilities,
    /// What the environment claimed, kept so a colour reply can only narrow.
    baseline_depth: ColorDepth,
    /// Whether the device-attributes sentinel has arrived.
    complete: bool,
}

impl Negotiation {
    /// A negotiation starting from what the environment claimed.
    #[must_use]
    pub const fn new(baseline: Capabilities) -> Self {
        Self {
            capabilities: baseline,
            baseline_depth: baseline.color_depth,
            complete: false,
        }
    }

    /// Writes every query, ending with the device-attributes sentinel.
    ///
    /// The order is load-bearing: a terminal answers queries in the order it
    /// received them, so the sentinel must be last for its arrival to mean
    /// that the others have been answered or refused.
    ///
    /// # Errors
    ///
    /// Returns any error from the sink.
    pub fn write_queries<W: io::Write>(out: &mut W) -> io::Result<()> {
        write!(
            out,
            "{}{}{}{}{}",
            Csi::Keyboard(Keyboard::QueryFlags),
            Csi::Mode(Mode::QueryDecPrivateMode(DecPrivateMode::Code(
                DecPrivateModeCode::SynchronizedOutput
            ))),
            Csi::Sgr(Sgr::Background(PROBE_COLOR.into())),
            Dcs::Request(DcsRequest::GraphicRendition),
            Csi::Sgr(Sgr::Reset),
        )?;
        write!(
            out,
            "{}",
            Csi::Device(Device::RequestPrimaryDeviceAttributes)
        )
    }

    /// Folds one reply in, and reports whether negotiation is now complete.
    ///
    /// A reply the negotiation did not ask for is ignored rather than treated
    /// as an answer to something else; the editor shares this stream with
    /// whatever else queries the terminal.
    pub fn apply(&mut self, reply: &CapabilityReply) -> bool {
        match reply {
            CapabilityReply::Csi(Csi::Keyboard(Keyboard::ReportFlags(_))) => {
                self.capabilities.keyboard_enhancement = true;
            },
            CapabilityReply::Csi(Csi::Mode(Mode::ReportDecPrivateMode {
                mode: DecPrivateMode::Code(DecPrivateModeCode::SynchronizedOutput),
                setting,
            })) => {
                // The question is whether the mode can be *used*, which is
                // narrower than whether it is recognised. `Set` and `Reset`
                // say it can be turned on and off; `PermanentlySet` says it is
                // always on, which is also fine, because then the BSU and ESU
                // are the harmless no-ops. `PermanentlyReset` says it can
                // never be turned on, and `NotRecognized` says the terminal
                // has never heard of it.
                self.capabilities.synchronized_output = matches!(
                    setting,
                    DecModeSetting::Set | DecModeSetting::Reset | DecModeSetting::PermanentlySet
                );
            },
            CapabilityReply::Dcs(Dcs::Response {
                value: DcsResponse::GraphicRendition(rendition),
                ..
            }) => {
                let survived = rendition.contains(&Sgr::Background(PROBE_COLOR.into()));
                let probed = if survived {
                    ColorDepth::TrueColor
                } else {
                    ColorDepth::Ansi256
                };
                self.capabilities.color_depth = narrower(self.baseline_depth, probed);
            },
            CapabilityReply::Csi(Csi::Device(Device::DeviceAttributes(()))) => {
                self.complete = true;
            },
            CapabilityReply::Csi(_) | CapabilityReply::Osc(_) | CapabilityReply::Dcs(_) => {},
        }
        self.complete
    }

    /// Whether the device-attributes sentinel has arrived.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    /// What the terminal has been shown to do.
    ///
    /// Meaningful before completion too: it is the environment's answer with
    /// every reply so far folded in, which is exactly what a driver should use
    /// when the terminal stops answering.
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
}
