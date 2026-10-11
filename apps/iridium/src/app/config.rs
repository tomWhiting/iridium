//! The user's `[keys]`, brought into a terminal session.
//!
//! [`iridium_config`] decides what the file *says*; this module decides what
//! this face does about it. Until it existed, `apps/iridium` referenced
//! `iridium_config` exactly twice, both `::theme` — so **the terminal face did
//! not read `[keys]` at all**, and a binding somebody wrote worked in the
//! desktop app and silently did nothing here.
//!
//! The terminal writes a separate generated reference and offers live reload.
//! See `config_actions` and `binding_reference`; user settings are never overwritten.
//!
//! # A refused binding costs the binding, not the file
//!
//! [`iridium_config::install`] drops one refused binding at a time and retries,
//! so a single typo does not cost every other line. What it cannot do is decide
//! what a *face's* error means, which is why the mapping onto [`Refusal`] is
//! here.
//!
//! # ⚠️ What this face cannot yet show
//!
//! The desktop opens a tab holding every problem. This face has one message
//! line and no surface for a report, so what is shown is the count and the path
//! — enough to know something was rejected and where to look, and honestly less
//! than the desktop gives. It is **not** silence: a file with a bad line in it
//! says so on the way in.

use std::path::Path;

use iridium_config::{Problem, Refusal};
use iridium_editor::KeyBinding;
use iridium_editor::workspace::{FaceSetupError, Workspace};

/// Pushes the user's key bindings, returning what had to be dropped.
///
/// [`Workspace::push_keymap`] validates against the registry and leaves the stack
/// untouched when it refuses, which is exactly what [`iridium_config::install`]
/// requires of it — so a refused binding is dropped and the rest go in.
pub(super) fn install_user_bindings<T>(
    workspace: &mut Workspace<T>,
    bindings: Vec<KeyBinding>,
) -> Vec<Problem> {
    iridium_config::install(bindings, |layer| {
        workspace.push_keymap(layer).map_err(|error| match error {
            FaceSetupError::Keymap(error) => Refusal::Keymap(error),
            FaceSetupError::Registry(error) => Refusal::Face(error.to_string()),
        })
    })
}

/// The one line the message strip shows, or `None` when there is nothing wrong.
///
/// Names the file as well as the count, because a count with nowhere to go is
/// just a number — and in this face the place to go is the file itself.
pub(super) fn summary(problems: &[Problem], path: Option<&Path>) -> Option<String> {
    let count = problems.len();
    if count == 0 {
        return None;
    }
    let plural = if count == 1 { "" } else { "s" };
    // The problems are real even when the path is not knowable — a
    // configuration directory this process cannot name still produced them, and
    // saying "in <nothing>" would read as a bug in the message.
    Some(path.map_or_else(
        || format!("{count} problem{plural} in your configuration"),
        |path| format!("{count} problem{plural} in {}", path.display()),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::path::Path;

    use iridium_config::Problem;
    use iridium_editor::commands::builtin::EDIT_DELETE_TO_LINE_END;
    use iridium_editor::{Editor, KeyBinding, KeyCode, KeyPress, Modifiers};

    use super::{install_user_bindings, summary};

    /// What the face's stack resolves `key` to, pressed with no modifiers.
    fn resolves_to(editor: &Editor, key: KeyCode) -> Option<iridium_editor::CommandId> {
        editor
            .keymap()
            .exact_match(&[KeyPress::new(key, Modifiers::none())], None)
            .and_then(iridium_editor::KeyBinding::command)
            .cloned()
    }

    /// ⭐ **The reason this module exists.** A line from the file reaches the
    /// kernel's stack and changes what a key does in *this* face.
    ///
    /// Asserted by resolving the key rather than by inspecting the stack: a
    /// layer that is present and outranked would satisfy the second and fail
    /// the user.
    #[test]
    fn a_binding_from_the_file_changes_what_a_key_does() {
        let mut editor = iridium_editor::workspace::Workspace::<()>::new(
            iridium_editor::EditorConfig::default(),
            iridium_editor::Theme::default(),
        );
        editor.open("", "test", None).expect("root tab");
        assert_eq!(
            resolves_to(editor.active_editor().expect("active"), KeyCode::F9),
            None,
            "F9 must start unbound, or this test would agree with itself"
        );

        let binding =
            KeyBinding::parse("f9", EDIT_DELETE_TO_LINE_END).expect("the fixture is a sequence");
        let problems = install_user_bindings(&mut editor, vec![binding]);
        assert!(
            problems.is_empty(),
            "a valid binding must not be refused: {problems:?}"
        );

        assert_eq!(
            resolves_to(editor.active_editor().expect("active"), KeyCode::F9),
            Some(EDIT_DELETE_TO_LINE_END),
            "the user's binding did not reach the terminal face's stack"
        );
    }

    /// A binding naming a command nobody registered is dropped, and said to be.
    ///
    /// ⚠️ The other bindings survive it. A face that threw the file away on one
    /// bad line would punish the whole configuration for a typo.
    #[test]
    fn an_unknown_command_costs_its_own_binding_and_no_other() {
        let mut editor = iridium_editor::workspace::Workspace::<()>::new(
            iridium_editor::EditorConfig::default(),
            iridium_editor::Theme::default(),
        );
        editor.open("", "test", None).expect("root tab");
        let good = KeyBinding::parse("f9", EDIT_DELETE_TO_LINE_END)
            .expect("the fixture is a key sequence");
        let bad = KeyBinding::parse("f10", iridium_editor::CommandId::new("nobody.registered"))
            // A command nobody registered is exactly what a typo in the file
            // produces, and is the case the drop-and-retry loop exists for.
            .expect("the fixture is a key sequence");

        let problems = install_user_bindings(&mut editor, vec![good, bad]);
        assert_eq!(
            problems.len(),
            1,
            "exactly the bad binding should be refused"
        );

        assert_eq!(
            resolves_to(editor.active_editor().expect("active"), KeyCode::F9),
            Some(EDIT_DELETE_TO_LINE_END),
            "a good binding was lost along with the bad one"
        );
        assert_eq!(
            resolves_to(editor.active_editor().expect("active"), KeyCode::F10),
            None,
            "the refused binding must not be installed"
        );
    }

    /// Nothing wrong says nothing at all.
    #[test]
    fn a_clean_file_produces_no_message() {
        assert_eq!(summary(&[], Some(Path::new("/tmp/config.toml"))), None);
    }

    /// ⛔ A problem must reach the strip. This is the assertion that stops the
    /// reporting path becoming a channel that can only carry success.
    #[test]
    fn a_problem_names_its_count_and_the_file() {
        let problems = vec![Problem::keys("something is wrong".to_owned())];
        let text = summary(&problems, Some(Path::new("/tmp/config.toml")))
            .expect("one problem must produce a message");
        assert!(text.contains('1'), "the count must be in it: {text}");
        assert!(
            text.contains("config.toml"),
            "the file must be in it: {text}"
        );
    }

    /// And the plural is right, because "1 problems" reads as a bug.
    #[test]
    fn two_problems_are_pluralised() {
        let problems = vec![
            Problem::keys("one".to_owned()),
            Problem::keys("two".to_owned()),
        ];
        let text = summary(&problems, None).expect("two problems must produce a message");
        assert!(text.contains("2 problems"), "{text}");
    }
}
