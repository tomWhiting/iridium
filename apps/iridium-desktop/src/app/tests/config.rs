//! Re-reading the user's configuration without restarting the session.
//!
//! # The failure these were written against
//!
//! Before them there was no reload at all: `UserConfig::read()` ran once, in
//! `DesktopApp::new`, and nothing read the file again. Changing a key binding
//! meant quitting and reopening the editor, which is the point at which people
//! stop customizing anything.
//!
//! # The one that is easy to get wrong
//!
//! The obvious implementation *pushes* the re-read bindings, and it is wrong in
//! a way nothing on screen can explain: the previous version of the `user` layer
//! stays underneath, so a binding deleted from the file goes on firing from
//! below. [`a_binding_removed_from_the_file_stops_working`] is that test, and it
//! is the reason `Workspace::replace_keymap` exists.

use std::path::Path;

use iridium_config::UserConfig;
use iridium_editor::{KeyCode, Modifiers};
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::config::PROBLEMS_TAB;
use crate::app::startup::Options;
use crate::app::state::{DesktopApp, Flow};

/// A session with no file and no configuration, the way a bare invocation
/// starts one.
fn session() -> DesktopApp {
    DesktopApp::new(Options::default()).expect("an empty session opened")
}

/// Applies `text` as though it had just been read from `config.toml`.
fn reload(app: &mut DesktopApp, text: &str) -> Flow {
    app.apply_config(
        &UserConfig::parse(text),
        Some(Path::new("/config/iridium/config.toml")),
    )
}

/// Whether the active tab resolves `Ctrl+<key>` to a command at all.
fn binds_ctrl(app: &DesktopApp, key: KeyCode) -> Option<String> {
    app.workspace
        .active_editor()?
        .keymap()
        .exact_match(
            &[iridium_editor::KeyPress {
                key,
                modifiers: Modifiers::ctrl(),
            }],
            None,
        )
        .and_then(|binding| binding.command().map(|id| id.as_str().to_owned()))
}

#[test]
fn a_reloaded_binding_takes_effect_without_a_restart() {
    let mut app = session();
    assert_eq!(
        binds_ctrl(&app, KeyCode::Char('e')),
        None,
        "nothing binds a bare Ctrl+E before the reload"
    );

    assert_eq!(
        reload(&mut app, "[keys]\n\"ctrl+e\" = \"palette.open\"\n"),
        Flow::Running
    );

    assert_eq!(
        binds_ctrl(&app, KeyCode::Char('e')).as_deref(),
        Some("palette.open"),
        "the binding read from the file is in force on this session"
    );
}

/// ⭐ **The failure `Workspace::replace_keymap` exists to prevent.**
///
/// A reload that pushed instead of replacing would leave the first version of
/// the `user` layer underneath the second, so this binding would still fire —
/// the editor disagreeing with its own configuration file, with nothing on
/// screen able to explain why.
#[test]
fn a_binding_removed_from_the_file_stops_working() {
    let mut app = session();
    reload(&mut app, "[keys]\n\"ctrl+e\" = \"palette.open\"\n");
    assert_eq!(
        binds_ctrl(&app, KeyCode::Char('e')).as_deref(),
        Some("palette.open")
    );

    // The same file with that line taken out.
    reload(&mut app, "[keys]\n\"ctrl+j\" = \"palette.open\"\n");

    assert_eq!(
        binds_ctrl(&app, KeyCode::Char('j')).as_deref(),
        Some("palette.open"),
        "the line that is still there still works"
    );
    assert_eq!(
        binds_ctrl(&app, KeyCode::Char('e')),
        None,
        "the line that was deleted must not go on firing from a layer beneath"
    );
}

#[test]
fn a_reloaded_setting_reaches_every_open_tab() {
    let directory = TempDir::new("desktop-reload-settings");
    let (mut app, _path) = open(&directory, "a.txt", "first");
    let second = fixture(&directory, "b.txt", "second");
    app.dropped(&second);
    assert_eq!(app.workspace.tab_count(), 2);

    reload(&mut app, "[editor]\ntab_width = 7\n");

    assert_eq!(app.workspace.config().tab_width, 7);
    for tab in app.workspace.tabs() {
        let Some(document) = app
            .workspace
            .node(tab)
            .and_then(iridium_editor::workspace::Node::document)
        else {
            continue;
        };
        assert_eq!(
            app.workspace
                .editor(document)
                .expect("the tab has an editor")
                .get_config()
                .tab_width,
            7,
            "a tab that was not in front kept the old tab width"
        );
    }
}

/// ⚠️ A reload of a file with a mistake in it must not cost the bindings that
/// were working.
///
/// This is what makes the command safe to have on a key at all. A half-applied
/// replacement would leave the old layer gone and the new one refused, so an
/// editor whose keys all worked a moment ago would have none of them, over a
/// typo — and the only way back would be the restart the command exists to
/// avoid.
#[test]
fn a_reload_with_a_bad_line_keeps_the_good_ones() {
    let mut app = session();
    reload(&mut app, "[keys]\n\"ctrl+e\" = \"palette.open\"\n");

    reload(
        &mut app,
        "[keys]\n\
         \"ctrl+e\" = \"palette.open\"\n\
         \"ctrl+g\" = \"no.such.command\"\n",
    );

    assert_eq!(
        binds_ctrl(&app, KeyCode::Char('e')).as_deref(),
        Some("palette.open"),
        "the binding that names a real command survived the one that does not"
    );
    assert_eq!(binds_ctrl(&app, KeyCode::Char('g')), None);
}

#[test]
fn a_reload_that_refused_something_says_so_and_opens_one_tab() {
    let mut app = session();
    reload(&mut app, "[keys]\n\"ctrl+g\" = \"no.such.command\"\n");

    let message = app
        .message
        .as_ref()
        .expect("a refusal is reported")
        .text()
        .to_owned();
    assert!(message.contains(PROBLEMS_TAB), "{message}");
    assert_eq!(
        config_tabs(&app),
        1,
        "the report goes in one tab, however many times it is written"
    );
}

/// ⭐ **A second reload writes the same tab, and tells the truth in it.**
///
/// The tab from the previous read is already on screen describing a file that
/// has since been fixed. Opening another would leave two tabs with the same
/// name disagreeing about the same file and no way to tell which is current;
/// leaving the old text would make a working configuration look broken.
#[test]
fn a_second_reload_rewrites_the_tab_rather_than_opening_another() {
    let mut app = session();
    reload(&mut app, "[keys]\n\"ctrl+g\" = \"no.such.command\"\n");
    assert_eq!(config_tabs(&app), 1);

    // The file, fixed.
    reload(&mut app, "[keys]\n\"ctrl+g\" = \"palette.open\"\n");

    assert_eq!(config_tabs(&app), 1, "still exactly one tab about the file");
    let text = config_tab_text(&app).expect("the tab is there");
    assert!(
        text.contains("applied in full"),
        "the tab describes the file as it is now, not as it was: {text}"
    );
    assert!(
        !text.contains("no.such.command"),
        "the refusal from the previous read is gone: {text}"
    );
}

/// A session with nowhere to look still reloads rather than refusing.
#[test]
fn a_reload_with_no_file_to_read_applies_the_defaults() {
    let mut app = session();
    reload(&mut app, "[keys]\n\"ctrl+e\" = \"palette.open\"\n");

    assert_eq!(
        app.apply_config(&UserConfig::default(), None),
        Flow::Running
    );

    assert_eq!(
        binds_ctrl(&app, KeyCode::Char('e')),
        None,
        "a configuration that says nothing binds nothing"
    );
}

/// How many tabs are the configuration report.
fn config_tabs(app: &DesktopApp) -> usize {
    app.workspace
        .tabs()
        .into_iter()
        .filter(|id| {
            app.workspace
                .node(*id)
                .is_some_and(|node| node.label() == PROBLEMS_TAB)
        })
        .count()
}

/// The text of the configuration report tab, if one is open.
fn config_tab_text(app: &DesktopApp) -> Option<String> {
    let tab = app.workspace.tabs().into_iter().find(|id| {
        app.workspace
            .node(*id)
            .is_some_and(|node| node.label() == PROBLEMS_TAB)
    })?;
    let document = app
        .workspace
        .node(tab)
        .and_then(iridium_editor::workspace::Node::document)?;
    Some(app.workspace.editor(document)?.content())
}
