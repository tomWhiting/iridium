//! Tests that a face's own commands and keymap reach *every* tab.
//!
//! [`Workspace`] already synchronises theme and configuration across open
//! editors, and says why: applying a change to only the active editor leaves
//! every background tab stale, while a document opened afterwards inherits
//! the workspace's — so the two disagree in opposite directions depending on
//! which tab you look at.
//!
//! A face's registered commands and pushed keymap layers are the same kind
//! of state and were **not** synchronised. The failure is quieter than the
//! theme one: a face registers its commands once at startup, sees them work,
//! and only discovers on the second tab that its save chord does nothing.

use super::Workspace;
use crate::commands::{CommandCategory, CommandId, CommandMeta, KeyBinding, Keymap};
use crate::commands::{ModifierPattern, ModifierState, StrokePattern};
use crate::editor::EditorConfig;
use crate::input::KeyCode;
use crate::theme::Theme;

/// A command id no kernel table claims.
const FACE_SAVE: CommandId = CommandId::from_static("face.save");

fn workspace() -> Workspace {
    Workspace::new(EditorConfig::default(), Theme::default())
}

fn face_command() -> CommandMeta {
    CommandMeta::described(
        FACE_SAVE,
        "Save",
        "Writes the document to the file it came from.",
        CommandCategory::GENERAL,
    )
}

/// `Ctrl+S` bound to the face's own command.
fn face_keymap() -> Keymap {
    let mut keymap = Keymap::new("face");
    keymap.push(KeyBinding::new(
        StrokePattern::new(
            KeyCode::Char('s'),
            ModifierPattern::NONE.with_ctrl(ModifierState::Required),
        ),
        &[],
        FACE_SAVE,
    ));
    keymap
}

#[test]
fn a_command_registered_before_any_tab_reaches_the_first_one() {
    let mut workspace = workspace();
    workspace
        .register_command(face_command())
        .expect("no kernel table claims `face.save`");

    let tab = workspace.open("a", "a.rs", None).expect("top level");
    assert!(workspace.activate(tab));

    assert!(
        workspace
            .active_editor()
            .expect("a tab is active")
            .commands()
            .get(FACE_SAVE.as_str())
            .is_some()
    );
}

#[test]
fn a_command_registered_before_any_tab_reaches_the_second_one_too() {
    // The bug this is written for: a face registers once at startup, the
    // first tab works, and the second silently has an empty face registry.
    let mut workspace = workspace();
    workspace.register_command(face_command()).expect("free id");

    workspace.open("a", "a.rs", None).expect("top level");
    let second = workspace.open("b", "b.rs", None).expect("top level");
    assert!(workspace.activate(second));

    assert!(
        workspace
            .active_editor()
            .expect("a tab is active")
            .commands()
            .get(FACE_SAVE.as_str())
            .is_some(),
        "the second tab must know the face's commands, not just the first"
    );
}

#[test]
fn a_command_registered_after_a_tab_is_open_reaches_that_tab_as_well() {
    // The other direction, and the same argument `set_theme` makes: a face
    // that registers a command lazily — on first use of a feature — must not
    // find it works only in tabs opened afterwards.
    let mut workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    assert!(workspace.activate(tab));

    workspace.register_command(face_command()).expect("free id");

    assert!(
        workspace
            .active_editor()
            .expect("a tab is active")
            .commands()
            .get(FACE_SAVE.as_str())
            .is_some(),
        "registration must reach tabs already open, not only future ones"
    );
}

#[test]
fn a_duplicate_id_is_refused_once_rather_than_partly_applied() {
    // A registration that failed on tab three after succeeding on tabs one
    // and two would leave the workspace in a state no single answer
    // describes. Refused up front, against the workspace's own record.
    let mut workspace = workspace();
    workspace.register_command(face_command()).expect("free id");
    workspace.open("a", "a.rs", None).expect("top level");

    assert!(
        workspace.register_command(face_command()).is_err(),
        "the same id twice must be refused"
    );
}

#[test]
fn a_keymap_layer_pushed_before_any_tab_reaches_every_tab() {
    let mut workspace = workspace();
    workspace.register_command(face_command()).expect("free id");
    workspace
        .push_keymap(face_keymap())
        .expect("the command it names is registered");

    workspace.open("a", "a.rs", None).expect("top level");
    let second = workspace.open("b", "b.rs", None).expect("top level");
    assert!(workspace.activate(second));

    let editor = workspace.active_editor().expect("a tab is active");
    assert!(
        editor
            .keymap()
            .layers()
            .iter()
            .any(|layer| layer.name() == "face"),
        "the face's keymap layer must be on every tab's handler"
    );
}

#[test]
fn a_keymap_layer_pushed_after_a_tab_is_open_reaches_that_tab() {
    let mut workspace = workspace();
    workspace.register_command(face_command()).expect("free id");
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    assert!(workspace.activate(tab));

    workspace.push_keymap(face_keymap()).expect("valid");

    let editor = workspace.active_editor().expect("a tab is active");
    assert!(
        editor
            .keymap()
            .layers()
            .iter()
            .any(|layer| layer.name() == "face")
    );
}

#[test]
fn a_keymap_naming_an_unregistered_command_is_refused() {
    // Validation is against the registry, so pushing before registering must
    // fail — and must fail *before* any editor has taken the layer, or the
    // tabs would disagree about which layers exist.
    let mut workspace = workspace();
    workspace.open("a", "a.rs", None).expect("top level");

    assert!(
        workspace.push_keymap(face_keymap()).is_err(),
        "a binding naming an unknown command must not be accepted"
    );

    let editor = workspace.active_editor().expect("a tab is active");
    assert!(
        !editor
            .keymap()
            .layers()
            .iter()
            .any(|layer| layer.name() == "face"),
        "a refused push must leave no layer behind on any tab"
    );
}
