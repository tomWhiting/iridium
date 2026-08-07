//! Installing the user's bindings, one refusal at a time.
//!
//! # Why this is a loop rather than a call
//!
//! The kernel validates a keymap layer *whole*: [`Keymap`] goes in, the first
//! [`KeymapError`] comes out, and the stack is left exactly as it was. That is
//! the right contract for the kernel — a half-applied keymap would be worse
//! than none — but handed straight to a user it means one mistyped command id
//! costs every binding in the file, and the twenty that were fine disappear
//! with no indication that they were fine.
//!
//! So the refusal is read, the binding it names is dropped, the refusal is
//! *recorded*, and the rest are offered again. Every error the kernel reports
//! names the offending binding by the same round-trippable display form the
//! binding renders itself in, so identifying it is a comparison rather than a
//! guess. The loop shrinks the set every pass, so it terminates in at most one
//! pass per binding.
//!
//! # What is never done for the user
//!
//! A bare chord bound here strands every longer sequence starting with it —
//! bind `ctrl+k` and the default keymap's `ctrl+k …` chords become
//! unreachable, which the kernel reports. There are two ways out: drop the new
//! binding, or suppress the stranded ones.
//!
//! **The stranded ones are never suppressed automatically.** Doing it would
//! mean an unrelated line silently switching off a default chord — the file
//! would no longer say what the editor does, and the chord would have stopped
//! working for a reason that appears nowhere. The new binding is dropped
//! instead, and the report quotes the exact line to add if suppressing them is
//! what was wanted.

use core::fmt;

use iridium_editor::{KeyBinding, Keymap, KeymapError};

use crate::keys::{KEYMAP_NAME, keymap};
use crate::problem::Problem;

/// Why a face refused a layer.
///
/// Two cases because only one of them can be acted on. A [`Self::Keymap`]
/// refusal names a binding, so that binding can be dropped and the rest
/// offered again; anything else the face reports is about the face, names no
/// binding, and leaves nothing to drop.
///
/// The distinction is drawn by the *face* rather than guessed at here, because
/// only the face knows what its own error type means. Guessing — treating
/// every refusal as a keymap one and shedding bindings until the error changed
/// — would report innocent bindings as broken and stop only when there were
/// none left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The keymap was refused, on the terms the kernel states.
    Keymap(KeymapError),
    /// The face refused for a reason of its own.
    Face(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Keymap(error) => write!(formatter, "{error}"),
            Self::Face(message) => formatter.write_str(message),
        }
    }
}

impl From<KeymapError> for Refusal {
    fn from(error: KeymapError) -> Self {
        Self::Keymap(error)
    }
}

/// Pushes the user's bindings, dropping and reporting each one refused.
///
/// `push` is whatever the face uses to install a layer —
/// [`Workspace::push_keymap`](iridium_editor::Workspace::push_keymap) or
/// [`Editor::push_keymap`](iridium_editor::Editor::push_keymap) — with its
/// error mapped onto [`Refusal`]. It **must** leave the stack untouched when
/// it refuses, which both of those do; a `push` that half-applied would make
/// each retry stack another copy.
///
/// Returns one [`Problem`] per refused binding. An empty result means every
/// binding is installed.
pub fn install(
    bindings: Vec<KeyBinding>,
    mut push: impl FnMut(Keymap) -> Result<(), Refusal>,
) -> Vec<Problem> {
    let mut remaining = bindings;
    let mut problems = Vec::new();

    while !remaining.is_empty() {
        let Err(refused) = push(keymap(remaining.clone())) else {
            return problems;
        };

        let Refusal::Keymap(error) = &refused else {
            problems.push(Problem::keys(format!(
                "none of the bindings could be applied: {refused}"
            )));
            return problems;
        };

        let Some(index) = culprit(error, &remaining) else {
            // The kernel refused for a reason that names no binding of ours.
            // Nothing can be dropped to make progress, so the layer is
            // abandoned whole and said to be abandoned whole — the one outcome
            // where the user's keys are all gone, and the only one where
            // pretending otherwise would be a lie.
            problems.push(Problem::keys(format!(
                "none of the bindings could be applied: {error}"
            )));
            return problems;
        };

        let dropped = remaining.remove(index);
        problems.push(refusal(&dropped, error));
    }

    problems
}

/// The report for one refused binding, with the line that would fix it.
fn refusal(refused: &KeyBinding, error: &KeymapError) -> Problem {
    let sequence = refused.display_sequence();
    let remedy = match error {
        KeymapError::CrossLayerShadowedSequence {
            shadowed_keymap,
            shadowed,
            ..
        } if shadowed_keymap != KEYMAP_NAME => format!(
            " — to keep it, also unbind the sequence it strands by adding `\"{shadowed}\" = \"\"`"
        ),
        _ => String::new(),
    };
    Problem::keys(format!("`{sequence}` was refused: {error}{remedy}"))
}

/// Which of `bindings` the kernel is complaining about, if any of them.
///
/// The comparison is against
/// [`KeyBinding::display_sequence`](iridium_editor::KeyBinding::display_sequence),
/// which is the same function that produced the text in the error — not a
/// second rendering that would agree with it until one of them changed.
fn culprit(error: &KeymapError, bindings: &[KeyBinding]) -> Option<usize> {
    match error {
        KeymapError::UnknownCommand { keymap, id } if keymap == KEYMAP_NAME => bindings
            .iter()
            .position(|binding| binding.command().is_some_and(|bound| bound.as_str() == id)),

        // Both halves are ours, and the one that can never fire is the longer
        // one. Dropping the prefix instead would silently change what the
        // shorter chord does, which is not what a report about the longer one
        // should cause.
        KeymapError::ShadowedSequence {
            keymap, shadowed, ..
        } if keymap == KEYMAP_NAME => position_of(bindings, shadowed),

        // Across layers, either half may be ours. When the stranded binding is
        // ours it is the one that can never fire; when the *prefix* is ours,
        // it is the one stranding somebody else's, and it goes instead —
        // see the module documentation for why the stranded chord is not
        // suppressed on the user's behalf.
        KeymapError::CrossLayerShadowedSequence {
            shadowed_keymap,
            shadowed,
            prefix_keymap,
            prefix,
        } => {
            if shadowed_keymap == KEYMAP_NAME {
                position_of(bindings, shadowed)
            } else if prefix_keymap == KEYMAP_NAME {
                position_of(bindings, prefix)
            } else {
                None
            }
        },

        // `EmptySequence` and `UnparsableStroke` are both produced while
        // *building* a binding, which happened in `crate::keys` and reported
        // there. A binding that exists has a non-empty, parsed sequence by
        // construction, so neither can name one of these — and answering
        // `None` says so rather than dropping an innocent binding to see
        // whether the error goes away.
        _ => None,
    }
}

/// The index of the binding whose display form is `sequence`.
fn position_of(bindings: &[KeyBinding], sequence: &str) -> Option<usize> {
    bindings
        .iter()
        .position(|binding| binding.display_sequence() == sequence)
}

#[cfg(test)]
mod tests {
    use iridium_editor::{CommandId, KeyBinding, Keymap, KeymapError};

    use super::{Refusal, install};
    use crate::keys::KEYMAP_NAME;

    /// A binding of `sequence` to `command`, for a fixture.
    fn bind(sequence: &str, command: &str) -> KeyBinding {
        KeyBinding::parse(sequence, CommandId::new(command.to_owned()))
            .expect("the fixture sequence parses")
    }

    /// A refusal on the kernel's terms, which is what a real keymap push
    /// produces and what all but one of these fixtures return.
    fn refused(error: KeymapError) -> Result<(), Refusal> {
        Err(Refusal::Keymap(error))
    }

    #[test]
    fn bindings_that_are_accepted_produce_no_problems() {
        let problems = install(vec![bind("ctrl+alt+j", "a.one")], |_: Keymap| Ok(()));
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn nothing_to_install_pushes_nothing() {
        let mut pushes = 0;
        let problems = install(Vec::new(), |_| {
            pushes += 1;
            Ok(())
        });
        assert!(problems.is_empty());
        assert_eq!(pushes, 0, "an empty layer is not worth pushing");
    }

    /// **The rule this module exists for.** One unknown command id costs one
    /// binding, not the file.
    #[test]
    fn one_unknown_command_costs_one_binding() {
        let bindings = vec![
            bind("ctrl+alt+j", "a.real"),
            bind("ctrl+alt+k", "a.typo"),
            bind("ctrl+alt+l", "a.other"),
        ];
        let mut installed: Vec<String> = Vec::new();
        let problems = install(bindings, |layer| {
            if layer
                .bindings()
                .iter()
                .any(|binding| binding.command().is_some_and(|id| id.as_str() == "a.typo"))
            {
                return refused(KeymapError::UnknownCommand {
                    keymap: KEYMAP_NAME.to_owned(),
                    id: "a.typo".to_owned(),
                });
            }
            installed = layer
                .bindings()
                .iter()
                .map(KeyBinding::display_sequence)
                .collect();
            Ok(())
        });

        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].detail.contains("ctrl+alt+k"), "{problems:?}");
        assert_eq!(
            installed,
            vec!["ctrl+alt+j".to_owned(), "ctrl+alt+l".to_owned()],
            "the other two bindings are installed"
        );
    }

    #[test]
    fn every_refused_binding_is_reported_not_just_the_first() {
        let bindings = vec![
            bind("ctrl+alt+j", "nosuch.one"),
            bind("ctrl+alt+k", "nosuch.two"),
            bind("ctrl+alt+l", "a.real"),
        ];
        let problems = install(bindings, |layer| {
            for binding in layer.bindings() {
                let id = binding.command().map_or("", CommandId::as_str);
                if id.starts_with("nosuch.") {
                    return refused(KeymapError::UnknownCommand {
                        keymap: KEYMAP_NAME.to_owned(),
                        id: id.to_owned(),
                    });
                }
            }
            Ok(())
        });
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn a_binding_stranding_a_default_chord_is_dropped_and_the_fix_is_quoted() {
        let bindings = vec![bind("ctrl+k", "a.palette")];
        let problems = install(bindings, |layer| {
            if layer.is_empty() {
                return Ok(());
            }
            refused(KeymapError::CrossLayerShadowedSequence {
                prefix_keymap: KEYMAP_NAME.to_owned(),
                prefix: "ctrl+k".to_owned(),
                shadowed_keymap: "default".to_owned(),
                shadowed: "ctrl+k ctrl+d".to_owned(),
            })
        });
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].detail.contains("`\"ctrl+k ctrl+d\" = \"\"`"),
            "the report quotes the line that would keep the binding: {problems:?}"
        );
    }

    #[test]
    fn the_longer_of_two_of_our_own_sequences_is_the_one_dropped() {
        // Only the longer one can never fire; dropping the prefix would change
        // what the shorter chord does, which nothing asked for.
        let bindings = vec![
            bind("ctrl+alt+k", "a.short"),
            bind("ctrl+alt+k f", "a.long"),
        ];
        let mut survived = Vec::new();
        let problems = install(bindings, |layer| {
            if layer.len() > 1 {
                return refused(KeymapError::ShadowedSequence {
                    keymap: KEYMAP_NAME.to_owned(),
                    prefix: "ctrl+alt+k".to_owned(),
                    shadowed: "ctrl+alt+k f".to_owned(),
                });
            }
            survived = layer
                .bindings()
                .iter()
                .map(KeyBinding::display_sequence)
                .collect();
            Ok(())
        });
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(survived, vec!["ctrl+alt+k".to_owned()]);
    }

    #[test]
    fn a_refusal_naming_no_binding_of_ours_abandons_the_layer_and_says_so() {
        // Nothing can be dropped to make progress, so the loop must stop
        // rather than shed bindings one at a time until the error changes.
        let bindings = vec![bind("ctrl+alt+j", "a.one"), bind("ctrl+alt+k", "a.two")];
        let mut pushes = 0;
        let problems = install(bindings, |_| {
            pushes += 1;
            refused(KeymapError::UnknownCommand {
                keymap: "somebody-elses-layer".to_owned(),
                id: "a.one".to_owned(),
            })
        });
        assert_eq!(pushes, 1, "it stops instead of retrying");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].detail.contains("none of the bindings"),
            "{problems:?}"
        );
    }

    #[test]
    fn a_refusal_that_is_not_about_the_keymap_stops_rather_than_shedding_bindings() {
        // A face can refuse for reasons of its own that name no binding.
        // Dropping bindings until such an error changed would report every one
        // of them as broken and stop only when there were none left.
        let bindings = vec![bind("ctrl+alt+j", "a.one"), bind("ctrl+alt+k", "a.two")];
        let mut pushes = 0;
        let problems = install(bindings, |_| {
            pushes += 1;
            Err(Refusal::Face("the face was already broken".to_owned()))
        });
        assert_eq!(pushes, 1, "it stops instead of retrying");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].detail.contains("the face was already broken"),
            "the face's own words reach the report: {problems:?}"
        );
    }

    #[test]
    fn a_refusal_that_names_every_binding_terminates_rather_than_looping() {
        // The guarantee that makes this a loop at all: each pass removes one
        // binding, so the worst case is one pass per binding and then an empty
        // set, which is not pushed.
        let bindings = vec![bind("ctrl+alt+j", "a.one"), bind("ctrl+alt+k", "a.two")];
        let problems = install(bindings, |layer| {
            let id = layer
                .bindings()
                .first()
                .and_then(KeyBinding::command)
                .map_or(String::new(), ToString::to_string);
            refused(KeymapError::UnknownCommand {
                keymap: KEYMAP_NAME.to_owned(),
                id,
            })
        });
        assert_eq!(problems.len(), 2, "{problems:?}");
    }
}
