//! Rendering a key sequence for a human to read.
//!
//! This is deliberately **not** [`KeyBinding::display_sequence`](crate::commands::KeyBinding::display_sequence),
//! and the difference is the whole reason this module exists.
//!
//! That form is *round-trippable*: it must parse back to the same pattern, so it
//! spells a [`ModifierState::Any`] modifier as `~name` and omits only
//! [`ModifierState::Forbidden`]. The default keymap uses `Any` heavily — every
//! `Ctrl`+letter chord ignores `AltGraph`, and *Select All* ignores `Shift` too —
//! so the round-trippable form of the key that selects all is
//! `ctrl+~shift+~altgraph+a`. Showing that in a palette would be worse than
//! showing nothing.
//!
//! A label answers a different question: *which keys does a person hold down?*
//! Only [`ModifierState::Required`] modifiers are keys a person holds, so only
//! those appear. The result is `Ctrl+A`, and it is exactly the keypress
//! [`KeymapStack::binding_is_reachable`](crate::commands::KeymapStack::binding_is_reachable)
//! verified would run the command.

use crate::commands::{ModifierPattern, ModifierState, StrokePattern};
use crate::input::KeyCode;

/// How a key sequence is spelled for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KeyLabelStyle {
    /// Named modifiers joined by `+`, chords by a space: `Ctrl+K Ctrl+D`.
    #[default]
    Portable,
    /// macOS glyphs, unseparated: `⌘K ⌘D`.
    ///
    /// The kernel's `ctrl` renders as `⌘` because every face in this repository
    /// forwards the macOS Command key as `ctrl` — see the key translation in the
    /// web controller, which sends `e.metaKey || e.ctrlKey` as `ctrl` and never
    /// forwards `meta` at all. A future face that maps Command to the kernel's
    /// `meta` instead would need a third style rather than a change here.
    MacGlyphs,
}

impl KeyLabelStyle {
    /// The separator between the modifiers and key within one chord.
    const fn within_chord(self) -> &'static str {
        match self {
            Self::Portable => "+",
            // macOS convention runs the glyphs together: ⌥⇧⌘K.
            Self::MacGlyphs => "",
        }
    }

    /// How this style spells each modifier, in the order it lists them.
    ///
    /// Both orders put the outermost physical modifier first and the key last,
    /// which is the macOS convention (`⌃⌥⇧⌘`) and reads the same way in the
    /// portable form.
    const fn modifier_names(self) -> [(&'static str, ModifierSlot); 5] {
        match self {
            Self::Portable => [
                ("Ctrl", ModifierSlot::Ctrl),
                ("Alt", ModifierSlot::Alt),
                ("Shift", ModifierSlot::Shift),
                ("Meta", ModifierSlot::Meta),
                ("AltGr", ModifierSlot::AltGraph),
            ],
            Self::MacGlyphs => [
                ("⌃", ModifierSlot::Meta),
                ("⌥", ModifierSlot::Alt),
                ("⇧", ModifierSlot::Shift),
                ("⌘", ModifierSlot::Ctrl),
                ("AltGr", ModifierSlot::AltGraph),
            ],
        }
    }
}

/// Which field of a [`ModifierPattern`] a label entry refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModifierSlot {
    Shift,
    Ctrl,
    Alt,
    Meta,
    AltGraph,
}

impl ModifierSlot {
    /// Reads this slot out of `pattern`.
    const fn state_of(self, pattern: ModifierPattern) -> ModifierState {
        match self {
            Self::Shift => pattern.shift,
            Self::Ctrl => pattern.ctrl,
            Self::Alt => pattern.alt,
            Self::Meta => pattern.meta,
            Self::AltGraph => pattern.alt_graph,
        }
    }
}

/// The placeholder shown for a stroke that matches any character.
///
/// A wildcard stroke has no single key to name — that is the point of it — so the
/// label shows the shape of the input the user must supply.
const WILDCARD_PLACEHOLDER: &str = "\u{2039}char\u{203a}";

/// Renders `strokes` in `style`, chords separated by a space.
pub(super) fn render(strokes: &[StrokePattern], style: KeyLabelStyle) -> String {
    let mut out = String::new();
    for (index, stroke) in strokes.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        write_stroke(&mut out, stroke, style);
    }
    out
}

/// Appends one chord to `out`.
fn write_stroke(out: &mut String, stroke: &StrokePattern, style: KeyLabelStyle) {
    for (name, slot) in style.modifier_names() {
        // Only a modifier the binding *requires* is a key the user holds. `Any`
        // and `Forbidden` both mean "do not press this to make it work".
        if matches!(slot.state_of(stroke.modifiers), ModifierState::Required) {
            out.push_str(name);
            out.push_str(style.within_chord());
        }
    }

    if stroke.captures() {
        out.push_str(WILDCARD_PLACEHOLDER);
    } else {
        out.push_str(&key_label(stroke.key));
    }
}

/// The display name of one key.
///
/// Matched exhaustively rather than derived from
/// [`name_of`](crate::commands::name_of), because that table is the *parseable*
/// lowercase form (`pagedown`, `altkey`) and a person reads `Page Down`. An
/// exhaustive match also means a new [`KeyCode`] variant is a compile error here
/// rather than a key that quietly renders as debug output.
fn key_label(key: KeyCode) -> String {
    let named = match key {
        KeyCode::Char(' ') => "Space",
        KeyCode::Char(c) => return c.to_uppercase().to_string(),
        KeyCode::Left => "Left",
        KeyCode::Right => "Right",
        KeyCode::Up => "Up",
        KeyCode::Down => "Down",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::PageUp => "Page Up",
        KeyCode::PageDown => "Page Down",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete => "Delete",
        KeyCode::Enter => "Enter",
        KeyCode::Tab => "Tab",
        KeyCode::Shift => "Shift",
        KeyCode::Control => "Control",
        KeyCode::Alt => "Alt",
        KeyCode::Meta => "Meta",
        KeyCode::Escape => "Esc",
        KeyCode::F1 => "F1",
        KeyCode::F2 => "F2",
        KeyCode::F3 => "F3",
        KeyCode::F4 => "F4",
        KeyCode::F5 => "F5",
        KeyCode::F6 => "F6",
        KeyCode::F7 => "F7",
        KeyCode::F8 => "F8",
        KeyCode::F9 => "F9",
        KeyCode::F10 => "F10",
        KeyCode::F11 => "F11",
        KeyCode::F12 => "F12",
    };
    named.to_owned()
}
