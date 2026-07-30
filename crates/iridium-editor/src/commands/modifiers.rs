//! Tri-state modifier requirements for a key binding.

use serde::{Deserialize, Serialize};

use crate::input::Modifiers;

/// What a binding requires of one modifier key.
///
/// Exact matching on a [`Modifiers`] value cannot express the dispatch this
/// editor already implements: today `Backspace` deletes backwards whatever
/// `Shift`, `Alt` or `Meta` are doing, `Ctrl+C` copies with or without `Shift`,
/// and `Ctrl+Alt+Up` adds a cursor only while `AltGraph` is *not* active. Those
/// are three different requirements on one modifier — hold, don't care, and must
/// not hold — so a binding states each modifier independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModifierState {
    /// The modifier must be held for the binding to match.
    Required,
    /// The modifier must not be held for the binding to match.
    ///
    /// The default, so a binding written as "just these modifiers" means exactly
    /// that: a keymap author who names `ctrl` gets `Ctrl` and nothing else,
    /// rather than a binding that also fires under `Ctrl+Alt` or `AltGr`.
    #[default]
    Forbidden,
    /// The modifier is ignored; the binding matches whether or not it is held.
    Any,
}

impl ModifierState {
    /// Returns `true` for [`Self::Forbidden`].
    ///
    /// Used to omit default fields when serializing a binding.
    #[must_use]
    pub const fn is_forbidden(&self) -> bool {
        matches!(self, Self::Forbidden)
    }

    /// Returns `true` when `held` satisfies this requirement.
    #[must_use]
    pub const fn accepts(self, held: bool) -> bool {
        match self {
            Self::Required => held,
            Self::Forbidden => !held,
            Self::Any => true,
        }
    }

    /// Returns `true` when this requirement constrains the modifier at all.
    #[must_use]
    pub const fn is_constraint(self) -> bool {
        !matches!(self, Self::Any)
    }

    /// [`Self::Required`] when `held`, [`Self::Forbidden`] otherwise.
    #[must_use]
    pub const fn exact(held: bool) -> Self {
        if held {
            Self::Required
        } else {
            Self::Forbidden
        }
    }
}

/// Returns `true` when one modifier value can satisfy both requirements.
const fn states_overlap(left: ModifierState, right: ModifierState) -> bool {
    !matches!(
        (left, right),
        (ModifierState::Required, ModifierState::Forbidden)
            | (ModifierState::Forbidden, ModifierState::Required)
    )
}

/// The modifier requirements of one element of a key sequence.
///
/// Every field defaults to [`ModifierState::Forbidden`], which means a
/// default-constructed pattern matches only a bare keypress and — importantly —
/// that **`AltGraph` is excluded unless a binding opts in**. On many non-US
/// layouts `AltGr` is reported as `Ctrl+Alt` held together while composing a
/// character; the `Ctrl+Alt` add-cursor chords must not fire on it. Making
/// `Forbidden` the default means that guard is the safe default for every new
/// binding instead of something each author must remember.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ModifierPattern {
    /// Requirement on `Shift`.
    #[serde(default, skip_serializing_if = "ModifierState::is_forbidden")]
    pub shift: ModifierState,
    /// Requirement on `Control` (`Cmd` on macOS hosts that map it there).
    #[serde(default, skip_serializing_if = "ModifierState::is_forbidden")]
    pub ctrl: ModifierState,
    /// Requirement on `Alt` / `Option`.
    #[serde(default, skip_serializing_if = "ModifierState::is_forbidden")]
    pub alt: ModifierState,
    /// Requirement on `Meta` / `Win` / `Cmd`.
    #[serde(default, skip_serializing_if = "ModifierState::is_forbidden")]
    pub meta: ModifierState,
    /// Requirement on `AltGraph`.
    ///
    /// See the type-level note: this defaults to
    /// [`ModifierState::Forbidden`], which is the `AltGr` guard.
    #[serde(default, skip_serializing_if = "ModifierState::is_forbidden")]
    pub alt_graph: ModifierState,
}

impl ModifierPattern {
    /// No modifier may be held.
    pub const NONE: Self = Self {
        shift: ModifierState::Forbidden,
        ctrl: ModifierState::Forbidden,
        alt: ModifierState::Forbidden,
        meta: ModifierState::Forbidden,
        alt_graph: ModifierState::Forbidden,
    };

    /// Every modifier is ignored; matches any modifier combination.
    pub const ANY: Self = Self {
        shift: ModifierState::Any,
        ctrl: ModifierState::Any,
        alt: ModifierState::Any,
        meta: ModifierState::Any,
        alt_graph: ModifierState::Any,
    };

    /// Builds a pattern from explicit per-modifier requirements.
    #[must_use]
    pub const fn new(
        shift: ModifierState,
        ctrl: ModifierState,
        alt: ModifierState,
        meta: ModifierState,
        alt_graph: ModifierState,
    ) -> Self {
        Self {
            shift,
            ctrl,
            alt,
            meta,
            alt_graph,
        }
    }

    /// Builds the pattern that matches `modifiers` and nothing else.
    ///
    /// Every modifier becomes [`ModifierState::Required`] or
    /// [`ModifierState::Forbidden`] according to `modifiers`, so a host that
    /// captured a chord from the user can turn it straight into a binding.
    #[must_use]
    pub const fn exact(modifiers: Modifiers) -> Self {
        Self {
            shift: ModifierState::exact(modifiers.shift),
            ctrl: ModifierState::exact(modifiers.ctrl),
            alt: ModifierState::exact(modifiers.alt),
            meta: ModifierState::exact(modifiers.meta),
            alt_graph: ModifierState::exact(modifiers.alt_graph),
        }
    }

    /// Returns a copy with the `Shift` requirement replaced.
    #[must_use]
    pub const fn with_shift(mut self, state: ModifierState) -> Self {
        self.shift = state;
        self
    }

    /// Returns a copy with the `Control` requirement replaced.
    #[must_use]
    pub const fn with_ctrl(mut self, state: ModifierState) -> Self {
        self.ctrl = state;
        self
    }

    /// Returns a copy with the `Alt` requirement replaced.
    #[must_use]
    pub const fn with_alt(mut self, state: ModifierState) -> Self {
        self.alt = state;
        self
    }

    /// Returns a copy with the `Meta` requirement replaced.
    #[must_use]
    pub const fn with_meta(mut self, state: ModifierState) -> Self {
        self.meta = state;
        self
    }

    /// Returns a copy with the `AltGraph` requirement replaced.
    #[must_use]
    pub const fn with_alt_graph(mut self, state: ModifierState) -> Self {
        self.alt_graph = state;
        self
    }

    /// Returns `true` when `modifiers` satisfies every requirement.
    #[must_use]
    pub const fn matches(&self, modifiers: Modifiers) -> bool {
        self.shift.accepts(modifiers.shift)
            && self.ctrl.accepts(modifiers.ctrl)
            && self.alt.accepts(modifiers.alt)
            && self.meta.accepts(modifiers.meta)
            && self.alt_graph.accepts(modifiers.alt_graph)
    }

    /// Returns `true` when some modifier combination satisfies both patterns.
    ///
    /// Two patterns are incompatible only where one [`ModifierState::Required`]
    /// meets a [`ModifierState::Forbidden`]. Used by
    /// [`Keymap::validate`](super::Keymap::validate) to decide whether a shorter
    /// binding can actually shadow a longer one, rather than assuming any two
    /// bindings on the same key collide.
    #[must_use]
    pub const fn overlaps(&self, other: &Self) -> bool {
        states_overlap(self.shift, other.shift)
            && states_overlap(self.ctrl, other.ctrl)
            && states_overlap(self.alt, other.alt)
            && states_overlap(self.meta, other.meta)
            && states_overlap(self.alt_graph, other.alt_graph)
    }

    /// How many modifiers this pattern constrains *at all*, from 0 to 5.
    ///
    /// [`ModifierState::Forbidden`] counts, because forbidding a modifier is as
    /// much of a constraint as requiring one: `ModifierPattern::NONE` scores 5
    /// even though it requires nothing. That makes this the **weaker** half of
    /// the tie-break — see [`Self::required_count`], which is consulted first.
    #[must_use]
    pub const fn specificity(&self) -> u8 {
        self.shift.is_constraint() as u8
            + self.ctrl.is_constraint() as u8
            + self.alt.is_constraint() as u8
            + self.meta.is_constraint() as u8
            + self.alt_graph.is_constraint() as u8
    }

    /// How many modifiers this pattern *requires* to be held, from 0 to 5.
    ///
    /// This is the primary tie-break when several bindings in one layer match a
    /// keypress, and it is what actually makes `Ctrl+Alt+Up` (two required
    /// modifiers, add a cursor) beat the looser `Up` (none required, move the
    /// caret) without either binding having to know about the other.
    ///
    /// [`Self::specificity`] alone cannot express that: it counts
    /// [`ModifierState::Forbidden`] too, so a fully exact pattern written with
    /// [`Self::exact`] scores 5 regardless of how many modifiers it holds, and an
    /// exact `Up` binding would tie with the add-cursor chord and fall through to
    /// insertion order. Ranking by required modifiers first and total constraints
    /// second means "more modifiers held wins", which is the model a keymap
    /// author has in mind.
    #[must_use]
    pub const fn required_count(&self) -> u8 {
        matches!(self.shift, ModifierState::Required) as u8
            + matches!(self.ctrl, ModifierState::Required) as u8
            + matches!(self.alt, ModifierState::Required) as u8
            + matches!(self.meta, ModifierState::Required) as u8
            + matches!(self.alt_graph, ModifierState::Required) as u8
    }

    /// The modifier set this pattern requires, ignoring what it forbids.
    ///
    /// The inverse of [`Self::exact`] for a pattern built by it, and what a
    /// chord-capture rebinding UI needs in order to show the user the chord it
    /// just recorded. A pattern with [`ModifierState::Any`] fields loses that
    /// information, so this is only a true inverse for exact patterns —
    /// `ModifierPattern::exact(m).required_modifiers() == m` always holds.
    #[must_use]
    pub const fn required_modifiers(&self) -> Modifiers {
        Modifiers {
            shift: matches!(self.shift, ModifierState::Required),
            ctrl: matches!(self.ctrl, ModifierState::Required),
            alt: matches!(self.alt, ModifierState::Required),
            meta: matches!(self.meta, ModifierState::Required),
            alt_graph: matches!(self.alt_graph, ModifierState::Required),
        }
    }

    /// Sets the named modifier to `state`, returning `false` when `name` is not a
    /// modifier.
    ///
    /// The single place the accepted textual spellings live, so
    /// [`StrokePattern::from_str`](super::StrokePattern) and any future parser
    /// cannot diverge.
    pub(super) fn apply_named(&mut self, name: &str, state: ModifierState) -> bool {
        match name {
            "ctrl" | "control" => self.ctrl = state,
            "shift" => self.shift = state,
            "alt" | "option" => self.alt = state,
            "meta" | "cmd" | "super" | "win" => self.meta = state,
            "altgraph" | "altgr" => self.alt_graph = state,
            _ => return false,
        }
        true
    }

    /// Writes every constrained modifier as a `+`-separated prefix.
    ///
    /// A [`ModifierState::Required`] modifier is written bare, a
    /// [`ModifierState::Any`] one as `~name`, and
    /// [`ModifierState::Forbidden`] is omitted — which is exactly what parsing an
    /// unmentioned modifier yields, so the rendering round-trips through
    /// [`StrokePattern::from_str`](super::StrokePattern).
    pub(super) fn write_display(self, out: &mut String) {
        for (state, name) in [
            (self.ctrl, "ctrl"),
            (self.shift, "shift"),
            (self.alt, "alt"),
            (self.meta, "meta"),
            (self.alt_graph, "altgraph"),
        ] {
            match state {
                ModifierState::Required => {
                    out.push_str(name);
                    out.push('+');
                },
                ModifierState::Any => {
                    out.push('~');
                    out.push_str(name);
                    out.push('+');
                },
                ModifierState::Forbidden => {},
            }
        }
    }
}
