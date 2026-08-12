//! What the menu bar is allowed to offer, and when.
//!
//! ⚠️ **This is the only half of the menu bar `cargo test` can reach.**
//! Hanging menus off `NSApp` and pushing `setEnabled` into them needs a
//! running `NSApplication`; deciding *what* to push does not, and that is what
//! these prove. The split exists so the decision is tested even though the
//! delivery cannot be — see `docs/IN-FLIGHT-108-menu-bar.md` for the boundary.

use iridium_editor::commands::builtin::PALETTE_OPEN;

use crate::app::{DesktopApp, Options};
use crate::menubar;
use crate::verbs::Availability;

/// Raises or lowers one modal panel on a session.
type PanelSetter = Box<dyn Fn(&mut DesktopApp, bool)>;

/// An ordinary session on an untitled buffer.
fn session() -> DesktopApp {
    DesktopApp::new(Options::default()).expect("an empty session opened")
}

/// An ordinary editable session offers everything.
#[test]
fn a_plain_session_is_open() {
    assert_eq!(session().menu_availability(), Availability::OPEN);
}

/// ⭐ The four panels that swallow a key outright are the four that grey the
/// bar, and this walks **all four** rather than sampling — the list is read
/// off `press`'s own branch order.
///
/// Each is set and then cleared, so a panel that greys the bar but never
/// un-greys it fails here rather than in somebody's session.
#[test]
fn every_modal_panel_makes_the_bar_inert_and_then_lets_it_go() {
    let mut app = session();

    // All four branches of `press`, not a sample: the two flags directly, and
    // the two panels by building the real thing rather than a stand-in.
    let menu = {
        let editor = app
            .workspace
            .active_editor()
            .expect("a session always has one");
        crate::context_menu::ContextMenu::open(editor, 0.0, 0.0)
    };
    let setters: Vec<(&str, PanelSetter)> = vec![
        (
            "palette",
            Box::new(|app: &mut DesktopApp, up| app.palette_open = up),
        ),
        (
            "history",
            Box::new(|app: &mut DesktopApp, up| app.history_open = up),
        ),
        (
            "prompt",
            Box::new(|app: &mut DesktopApp, up| {
                app.prompt = up.then(crate::prompt::Prompt::save_as);
            }),
        ),
        (
            "context menu",
            Box::new(move |app: &mut DesktopApp, up| {
                app.menu = up.then(|| menu.clone());
            }),
        ),
    ];
    for (name, set) in setters {
        set(&mut app, true);
        assert!(
            app.menu_availability().inert,
            "the {name} panel is up and the bar is still live"
        );
        set(&mut app, false);
        assert!(
            !app.menu_availability().inert,
            "the {name} panel closed and the bar stayed grey"
        );
    }
}

/// ⚠️ Focus is not modality. The explorer takes keys while it has them, but
/// the active editor is still well defined and a menu verb aimed at it still
/// means what it says — greying the whole bar for a side panel holding the
/// caret would be greying on a technicality.
#[test]
fn explorer_focus_does_not_grey_the_bar() {
    let mut app = session();
    app.explorer_focus = super::super::state::ExplorerFocus::Panel;
    assert!(
        !app.menu_availability().inert,
        "a focused side panel is not a modal one"
    );
}

/// A read-only document greys the writers without making the bar inert: the
/// two reasons are different reasons, which is why [`Availability`] has two
/// fields rather than one flag.
#[test]
fn a_read_only_document_is_not_the_same_as_an_inert_session() {
    let mut app = session();
    if let Some(editor) = app.workspace.active_editor_mut() {
        editor.state_mut().read_only = true;
    }
    let availability = app.menu_availability();
    assert!(availability.read_only, "the document refuses writes");
    assert!(
        !availability.inert,
        "a read-only document still lets you copy, search and navigate"
    );
}

/// ⭐ The two states must not resolve the same bar, or the greying push is
/// doing nothing and the guard that skips it would be skipping everything.
///
/// Asserted against the resolved menus rather than against `Availability`,
/// because that is what actually reaches the items.
#[test]
fn an_inert_session_and_an_open_one_resolve_different_bars() {
    let app = session();
    let editor = app
        .workspace
        .active_editor()
        .expect("a session always has one");

    let open = menubar::menus(editor, Availability::OPEN);
    let inert = menubar::menus(
        editor,
        Availability {
            read_only: false,
            inert: true,
        },
    );

    let open_enabled = open
        .iter()
        .flat_map(|menu| menu.commands().map(|row| row.enabled))
        .filter(|enabled| *enabled)
        .count();
    let inert_enabled = inert
        .iter()
        .flat_map(|menu| menu.commands().map(|row| row.enabled))
        .filter(|enabled| *enabled)
        .count();

    assert!(
        open_enabled > 0,
        "an open session offers nothing, so the bar is useless"
    );
    assert_eq!(
        inert_enabled, 0,
        "a modal panel owns the session and the bar still offers {inert_enabled} verbs"
    );
}

/// The palette command is on the bar and is never greyed by a read-only
/// document: opening a panel is not writing to a file.
#[test]
fn opening_the_palette_is_not_a_write() {
    let app = session();
    let editor = app
        .workspace
        .active_editor()
        .expect("a session always has one");
    let menus = menubar::menus(
        editor,
        Availability {
            read_only: true,
            inert: false,
        },
    );
    let palette = menus
        .iter()
        .flat_map(menubar::Menu::commands)
        .find(|row| row.command == PALETTE_OPEN)
        .expect("the palette is on the Go menu");
    assert!(
        palette.enabled,
        "a read-only document can still be searched"
    );
}
