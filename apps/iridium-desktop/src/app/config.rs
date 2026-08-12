//! The user's configuration file, brought into a session.
//!
//! [`iridium_config`] decides what the file *says*; this module decides what
//! the session does about it. Two things, and both are about the half that
//! could not be honoured:
//!
//! - the user's bindings are pushed onto the workspace, one refusal at a time,
//!   so a single bad line does not cost the file;
//! - everything that could not be honoured is written into a tab.
//!
//! # Why a tab, and not just the message strip
//!
//! The strip holds one line. A configuration file is edited several lines at a
//! time, so "3 problems" with only the first shown sends the user round the
//! loop once per mistake — and this face is normally launched from Finder,
//! where standard error goes nowhere anybody will read.
//!
//! A tab is the surface this face already has for showing text somebody needs
//! to read. It opens *behind* the file that was asked for, because the file
//! that was asked for is still what the user came here to see; the strip says
//! the tab is there.

use core::fmt::Write as _;
use std::path::Path;

use iridium_config::{Problem, Refusal};
use iridium_editor::commands::{CommandRegistry, KeyHintIndex};
use iridium_editor::workspace::{FaceSetupError, Workspace};
use iridium_editor::{KeyBinding, KeyLabelStyle};

use super::state::DesktopDocument;

/// The label on the tab holding the report.
pub(super) const PROBLEMS_TAB: &str = "config.toml";

/// The label on the tab holding the command reference.
pub(super) const COMMANDS_TAB: &str = "commands";

/// Pushes the user's key bindings, returning what had to be dropped.
///
/// Every refusal the workspace reports is mapped onto a [`Refusal`] by *this*
/// face, because only this face knows what its own error type means.
/// [`FaceSetupError::Registry`] cannot name a binding — it is about a command
/// this face registers — so it is reported as such rather than blamed on
/// whichever binding happened to be in the layer at the time.
pub(super) fn install_user_bindings(
    workspace: &mut Workspace<DesktopDocument>,
    bindings: Vec<KeyBinding>,
) -> Vec<Problem> {
    iridium_config::install(bindings, |layer| {
        workspace.push_keymap(layer).map_err(|error| match error {
            FaceSetupError::Keymap(error) => Refusal::Keymap(error),
            FaceSetupError::Registry(error) => Refusal::Face(error.to_string()),
        })
    })
}

/// Replaces the user's key bindings with a freshly-read set.
///
/// ⭐ **Replace, never push.** The obvious implementation of a reload is to
/// call [`install_user_bindings`] again, and it is wrong in a way nothing on
/// screen could explain: pushing leaves the *previous* version of the `user`
/// layer underneath the new one, so every binding deleted from the file goes on
/// firing from below. See
/// [`KeymapStack::replace_named`](iridium_editor::commands::KeymapStack::replace_named).
///
/// Identical to [`install_user_bindings`] in every other respect, including the
/// drop-one-and-retry loop — [`iridium_config::install`] requires only that its
/// closure leave the stack untouched when it refuses, and
/// [`Workspace::replace_keymap`] does, restoring the layer that was already
/// installed. That is what makes a reload safe to attempt: a file with a typo in
/// it costs the typo's binding, not the ones that were working.
pub(super) fn replace_user_bindings(
    workspace: &mut Workspace<DesktopDocument>,
    bindings: Vec<KeyBinding>,
) -> Vec<Problem> {
    iridium_config::install(bindings, |layer| {
        workspace
            .replace_keymap(layer)
            .map_err(|error| match error {
                FaceSetupError::Keymap(error) => Refusal::Keymap(error),
                FaceSetupError::Registry(error) => Refusal::Face(error.to_string()),
            })
    })
}

/// The one line the message strip shows.
///
/// Says how many there are and where the rest of them are, because a count
/// with nowhere to go is just a number.
pub(super) fn summary(problems: &[Problem]) -> String {
    let count = problems.len();
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} problem{plural} in your configuration — see the {PROBLEMS_TAB} tab")
}

/// The text of the report tab.
///
/// Leads with what still works, because that is the question somebody reading
/// this actually has: an editor that opened a tab about their configuration
/// looks, for a moment, like an editor that refused to start.
pub(super) fn report(path: Option<&Path>, problems: &[Problem]) -> String {
    if problems.is_empty() {
        return clean_report(path);
    }
    let mut text = String::from("Some of your configuration could not be used.\n\n");
    if let Some(path) = path {
        // Writing into a `String` cannot fail, so the result is dropped rather
        // than unwrapped: there is no error here to handle or to hide.
        let _ = writeln!(text, "  {}\n", path.display());
    }
    text.push_str(
        "Everything not listed below was applied, and nothing here stops the\n\
         editor. Each line names the section it was found in.\n\n",
    );
    for problem in problems {
        let _ = writeln!(text, "  {problem}");
    }
    text
}

/// The report for a configuration that was applied whole.
///
/// ⚠️ **A reload has to be able to say "nothing is wrong".** Startup only ever
/// opens this tab when there *are* problems, so the question never arose; a
/// reload is different, because the tab from the previous read is already on
/// screen and describes a file that has since been fixed. Leaving that text in
/// place would make a working configuration look broken, and closing the tab
/// under someone reading it is worse. So it says what is true now.
fn clean_report(path: Option<&Path>) -> String {
    let mut text = String::from("Your configuration was applied in full.\n\n");
    if let Some(path) = path {
        // Writing into a `String` cannot fail; see `report`.
        let _ = writeln!(text, "  {}\n", path.display());
    }
    text.push_str(
        "Nothing in it was refused. This tab is here because a reload was\n\
         asked for; it can be closed.\n",
    );
    text
}

/// The one line the strip shows after a reload that refused nothing.
pub(super) fn reloaded() -> String {
    format!("configuration reloaded — see the {PROBLEMS_TAB} tab")
}

/// Every command, by the id a `[keys]` line has to name it by.
///
/// The command palette searches ids but *shows* titles, so without this there
/// is no way to find out what to write — and a binding you cannot name is a
/// feature you cannot use. Rendered from the live registry and the live hint
/// index, so it describes the session it was asked for rather than a list
/// somebody has to remember to update.
///
/// Ordered by id rather than by the palette's ranking: this is a reference to
/// be read down and searched with `⌘F`, and grouping by prefix puts every
/// `cursor.` command together, which is how somebody looking for one thinks.
pub(super) fn command_reference(registry: &CommandRegistry, hints: &KeyHintIndex) -> String {
    let mut rows: Vec<(&str, &str, &str)> = registry
        .commands()
        .map(|meta| {
            let id = meta.id().as_str();
            let key = hints
                .primary_hint(id)
                .map_or("", |hint| hint.label(KeyLabelStyle::MacGlyphsCommandAsMeta));
            (id, meta.title(), key)
        })
        .collect();
    rows.sort_unstable();

    // Each column is padded to its widest entry, which is what makes this a
    // table rather than three runs of text in a monospaced buffer. Widths are
    // in `char`s, not bytes: a title with a non-ASCII character in it would
    // otherwise be padded by however many bytes UTF-8 spent on it and pull its
    // row out of line.
    let ids = rows
        .iter()
        .map(|(id, ..)| id.chars().count())
        .max()
        .unwrap_or(0);
    let titles = rows
        .iter()
        .map(|(_, title, _)| title.chars().count())
        .max()
        .unwrap_or(0);

    let mut text = format!(
        "Every command, by the name you write in the [keys] section of your\n\
         config.toml — {} of them, with the key that runs it today.\n\n",
        rows.len()
    );
    for (id, title, key) in rows {
        // Trimmed rather than conditionally formatted: a command with no key
        // would otherwise leave trailing spaces, which show up the moment
        // somebody selects the line.
        let row = format!("  {id:ids$}   {title:titles$}   {key}");
        let _ = writeln!(text, "{}", row.trim_end());
    }
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use std::path::PathBuf;

    use iridium_config::{Problem, UserConfig};
    use iridium_editor::theme::Theme;
    use iridium_editor::workspace::Workspace;
    use iridium_editor::{EditorConfig, KeyBinding};

    use super::{DesktopDocument, command_reference, install_user_bindings, report, summary};
    use crate::commands;

    /// A workspace built exactly as a session's is: this face's commands
    /// registered, this face's keymap layer pushed, the kernel's default
    /// keymap underneath.
    ///
    /// Built rather than described. The whole value of these tests over the
    /// ones in `iridium-config` is that the refusals here are the *real*
    /// ones — produced by the kernel's own validator, worded by the kernel's
    /// own error type — so a hand-written stand-in would defeat the point.
    fn session() -> Workspace<DesktopDocument> {
        let mut workspace =
            Workspace::<DesktopDocument>::new(EditorConfig::default(), Theme::default());
        for meta in commands::command_metas() {
            workspace
                .register_command(meta)
                .expect("this face's commands register");
        }
        workspace
            .push_keymap(commands::keymap())
            .expect("this face's keymap pushes");
        // A session always has a tab, and `active_editor` is how these tests
        // ask what the *live* keymap ended up being — which is the only
        // question worth asking, since a binding that was accepted and never
        // installed would look identical to one that worked.
        workspace.open("", "untitled", None);
        workspace
    }

    /// The bindings a `[keys]` table would produce.
    fn bindings(text: &str) -> Vec<KeyBinding> {
        let config = UserConfig::parse(text);
        assert!(config.problems.is_empty(), "{:?}", config.problems);
        config.bindings
    }

    #[test]
    fn bindings_the_kernel_accepts_produce_no_problems() {
        let mut workspace = session();
        let problems = install_user_bindings(
            &mut workspace,
            bindings("[keys]\n\"ctrl+alt+j\" = \"cursor.lineDown\"\n"),
        );
        assert!(problems.is_empty(), "{problems:?}");
    }

    /// **The rule, against the real validator.** A command id that does not
    /// exist costs its own binding and nothing else.
    #[test]
    fn a_command_that_does_not_exist_costs_only_its_own_binding() {
        let mut workspace = session();
        let problems = install_user_bindings(
            &mut workspace,
            bindings(
                "[keys]\n\
                 \"ctrl+alt+j\" = \"cursor.lineDown\"\n\
                 \"ctrl+alt+k\" = \"nosuch.command\"\n\
                 \"ctrl+alt+l\" = \"cursor.lineUp\"\n",
            ),
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].detail.contains("ctrl+alt+k"),
            "the report names the binding that was dropped: {problems:?}"
        );

        // The survivors are installed, not merely un-reported.
        let editor = workspace.active_editor().expect("a session has an editor");
        let hints = editor.key_hints();
        assert!(
            hints.primary_hint("cursor.lineDown").is_some(),
            "the accepted bindings reached the live keymap"
        );
    }

    /// **The case the unit tests could only imitate.** A user binding whose
    /// first stroke is already a complete sequence in the default keymap can
    /// never fire, and the kernel refuses the whole layer over it.
    ///
    /// `ctrl+f` opens search in the default keymap, so `ctrl+f x` is stranded.
    /// This is the test that proves the refusal is *matched* to the right
    /// binding: `install` identifies it by the display form inside the
    /// kernel's own error, and nothing but a real refusal can show that the
    /// two spellings agree.
    #[test]
    fn a_sequence_stranded_by_a_default_chord_is_dropped_and_the_rest_survive() {
        let mut workspace = session();
        let problems = install_user_bindings(
            &mut workspace,
            bindings(
                "[keys]\n\
                 \"ctrl+alt+j\" = \"cursor.lineDown\"\n\
                 \"ctrl+f x\" = \"cursor.lineUp\"\n",
            ),
        );
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].detail.contains("ctrl+f x"),
            "the stranded binding is the one named: {problems:?}"
        );
        assert!(
            problems[0].detail.contains("unreachable") && problems[0].detail.contains("`default`"),
            "it is the *shadowing* refusal that was matched, not some other \
             failure that happens to leave one problem behind: {problems:?}"
        );
        assert!(
            !problems[0].detail.contains(" = \"\"`"),
            "no unbind is suggested: the stranded binding is the user's own, \
             and suppressing `ctrl+f` would take search away to make room for \
             a chord they can simply write differently: {problems:?}"
        );

        let editor = workspace.active_editor().expect("a session has an editor");
        assert!(
            editor.key_hints().primary_hint("cursor.lineDown").is_some(),
            "the binding written beside it still installed"
        );
    }

    /// A user binding wins over the default one for the same chord — which is
    /// the whole point of the file, and is worth an assertion because it is
    /// the one outcome that looks identical to "nothing happened".
    #[test]
    fn a_user_binding_overrides_the_default_for_the_same_chord() {
        let mut workspace = session();
        let problems = install_user_bindings(
            &mut workspace,
            bindings("[keys]\n\"ctrl+alt+l\" = \"history.undo\"\n"),
        );
        assert!(problems.is_empty(), "{problems:?}");

        let editor = workspace.active_editor().expect("a session has an editor");
        let hint = editor
            .key_hints()
            .primary_hint("history.undo")
            .expect("undo has a binding");
        assert_eq!(hint.binding_text(), "ctrl+alt+l");
    }

    /// The reference exists so a `[keys]` line can be written at all, so what
    /// it must contain is the *id* — the thing the palette does not show.
    #[test]
    fn the_reference_lists_ids_and_not_only_titles() {
        let workspace = session();
        let editor = workspace.active_editor().expect("a session has an editor");
        let text = command_reference(editor.commands(), editor.key_hints());
        assert!(
            text.contains("cursor.lineDown"),
            "a kernel command's id is there"
        );
        assert!(text.contains("file.save"), "this face's own id is there");
        assert!(text.contains("Save"), "the title is there beside it");
    }

    /// A command with no key still gets a row. It is the ones without a chord
    /// that most need naming in a configuration file.
    #[test]
    fn every_registered_command_gets_a_row() {
        let workspace = session();
        let editor = workspace.active_editor().expect("a session has an editor");
        let registry = editor.commands();
        let text = command_reference(registry, editor.key_hints());
        for meta in registry.commands() {
            assert!(
                text.contains(meta.id().as_str()),
                "{} is missing from the reference",
                meta.id()
            );
        }
    }

    #[test]
    fn a_row_without_a_key_carries_no_trailing_whitespace() {
        // Invisible until somebody selects the line, and then obvious.
        let workspace = session();
        let editor = workspace.active_editor().expect("a session has an editor");
        let text = command_reference(editor.commands(), editor.key_hints());
        for line in text.lines() {
            assert_eq!(line, line.trim_end(), "trailing space on {line:?}");
        }
    }

    #[test]
    fn one_problem_is_reported_in_the_singular() {
        let problems = vec![Problem::keys("something")];
        assert!(
            summary(&problems).starts_with("1 problem in"),
            "{}",
            summary(&problems)
        );
    }

    #[test]
    fn the_summary_says_where_the_rest_of_them_are() {
        let problems = vec![Problem::keys("one"), Problem::editor("two")];
        let line = summary(&problems);
        assert!(line.starts_with("2 problems"), "{line}");
        assert!(line.contains("config.toml"), "{line}");
    }

    #[test]
    fn the_report_names_the_file_and_lists_every_problem() {
        let path = PathBuf::from("/Users/someone/.config/iridium/config.toml");
        let problems = vec![
            Problem::editor("no setting `x`"),
            Problem::keys("bad chord"),
        ];
        let text = report(Some(&path), &problems);
        assert!(
            text.contains("/Users/someone/.config/iridium/config.toml"),
            "{text}"
        );
        assert!(text.contains("[editor] — no setting `x`"), "{text}");
        assert!(text.contains("[keys] — bad chord"), "{text}");
    }

    #[test]
    fn the_report_leads_with_what_still_works() {
        // An editor that opens a tab about your configuration looks, for a
        // moment, like an editor that refused to start.
        let text = report(None, &[Problem::keys("bad chord")]);
        assert!(
            text.contains("Everything not listed below was applied"),
            "{text}"
        );
    }
}
