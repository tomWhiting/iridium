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
use crate::{KeyPress, Modifiers};

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

// ---------------------------------------------------------------------------
// Replacing a layer rather than stacking another — the configuration reload
// path. See `KeymapStack::replace_named` for why neither push nor pop-and-push
// is the operation.
// ---------------------------------------------------------------------------

/// A command id for the second half of the reload tests.
const FACE_QUIT: CommandId = CommandId::from_static("face.quit");

fn quit_command() -> CommandMeta {
    CommandMeta::described(
        FACE_QUIT,
        "Quit",
        "Closes the window.",
        CommandCategory::GENERAL,
    )
}

/// A layer called `face`, like [`face_keymap`], but binding `Ctrl+Q` to
/// `face.quit` instead — the shape of a configuration file edited between two
/// reads: same layer, different contents.
fn edited_face_keymap() -> Keymap {
    let mut keymap = Keymap::new("face");
    keymap.push(KeyBinding::new(
        StrokePattern::new(
            KeyCode::Char('q'),
            ModifierPattern::NONE.with_ctrl(ModifierState::Required),
        ),
        &[],
        FACE_QUIT,
    ));
    keymap
}

/// The command `presses` resolves to on the active tab, if any.
fn resolves_to(workspace: &Workspace, key: KeyCode) -> Option<String> {
    let press = KeyPress {
        key,
        modifiers: Modifiers::ctrl(),
    };
    workspace
        .active_editor()?
        .keymap()
        .exact_match(&[press], None)
        .and_then(|binding| binding.command().map(|id| id.as_str().to_owned()))
}

/// ⭐ **The whole reason this method exists.**
///
/// A binding *removed* from an edited configuration file has to stop working.
/// Pushing the re-read layer instead would leave the previous version of it
/// underneath — `Ctrl+S` would go on running `face.save` from the layer below,
/// so the editor would disagree with its own configuration and nothing on
/// screen could explain why.
#[test]
fn a_binding_deleted_from_a_reloaded_layer_stops_firing() {
    let mut workspace = workspace();
    workspace.register_command(face_command()).expect("free id");
    workspace.register_command(quit_command()).expect("free id");
    workspace.push_keymap(face_keymap()).expect("valid");
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    assert!(workspace.activate(tab));
    assert_eq!(
        resolves_to(&workspace, KeyCode::Char('s')).as_deref(),
        Some("face.save"),
        "the first read of the file bound Ctrl+S"
    );

    workspace
        .replace_keymap(edited_face_keymap())
        .expect("the edited layer names a registered command");

    assert_eq!(
        resolves_to(&workspace, KeyCode::Char('q')).as_deref(),
        Some("face.quit"),
        "the edited file's binding is in force"
    );
    assert_eq!(
        resolves_to(&workspace, KeyCode::Char('s')),
        None,
        "a binding deleted from the file must not go on firing from a layer beneath"
    );
}

/// And the layer count does not grow, which is the mechanism behind it.
#[test]
fn a_reload_swaps_the_layer_rather_than_stacking_another() {
    let mut workspace = workspace();
    workspace.register_command(face_command()).expect("free id");
    workspace.register_command(quit_command()).expect("free id");
    workspace.push_keymap(face_keymap()).expect("valid");
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    assert!(workspace.activate(tab));
    let before = workspace
        .active_editor()
        .expect("a tab is active")
        .keymap()
        .len();

    workspace
        .replace_keymap(edited_face_keymap())
        .expect("valid");

    assert_eq!(
        workspace
            .active_editor()
            .expect("a tab is active")
            .keymap()
            .len(),
        before,
        "replacing a layer must not add one"
    );
}

/// A background tab is the case a face that only touched the active editor
/// would pass every other test in this file and still get wrong.
#[test]
fn a_reload_reaches_a_tab_that_is_not_in_front() {
    let mut workspace = workspace();
    workspace.register_command(face_command()).expect("free id");
    workspace.register_command(quit_command()).expect("free id");
    workspace.push_keymap(face_keymap()).expect("valid");
    let first = workspace.open("a", "a.rs", None).expect("top level");
    let second = workspace.open("b", "b.rs", None).expect("top level");
    assert!(workspace.activate(second));

    workspace
        .replace_keymap(edited_face_keymap())
        .expect("valid");

    // Reloaded with the *second* tab in front, so the first is the one nothing
    // was looking at when the bindings moved.
    assert!(workspace.activate(first));
    assert_eq!(
        resolves_to(&workspace, KeyCode::Char('s')),
        None,
        "the background tab came forward still holding the old bindings"
    );
    assert_eq!(
        resolves_to(&workspace, KeyCode::Char('q')).as_deref(),
        Some("face.quit")
    );
}

/// A layer with no counterpart yet is installed rather than refused.
///
/// The case is real: a face whose user layer was abandoned whole at startup —
/// every binding in it refused — has no layer of that name to replace, and a
/// reload that fixed the file must still be able to install it.
#[test]
fn replacing_a_layer_that_is_not_there_installs_it() {
    let mut workspace = workspace();
    workspace.register_command(face_command()).expect("free id");
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    assert!(workspace.activate(tab));

    workspace
        .replace_keymap(face_keymap())
        .expect("a layer that is not there yet is pushed");

    assert_eq!(
        resolves_to(&workspace, KeyCode::Char('s')).as_deref(),
        Some("face.save")
    );
}

/// ⚠️ A reload that cannot be honoured must not cost the bindings that work.
///
/// The failure this pins is the one that would make reload dangerous rather
/// than merely useless: a half-applied replacement leaves the old layer gone
/// and the new one refused, so an editor whose keys all worked a moment ago has
/// none of them, over a typo.
#[test]
fn a_refused_reload_leaves_the_working_layer_in_force() {
    let mut workspace = workspace();
    workspace.register_command(face_command()).expect("free id");
    workspace.push_keymap(face_keymap()).expect("valid");
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    assert!(workspace.activate(tab));

    // `face.quit` was never registered, so the edited layer cannot validate.
    assert!(
        workspace.replace_keymap(edited_face_keymap()).is_err(),
        "a binding naming an unknown command must not be accepted"
    );

    assert_eq!(
        resolves_to(&workspace, KeyCode::Char('s')).as_deref(),
        Some("face.save"),
        "the layer that was already working is still working"
    );
    assert_eq!(
        resolves_to(&workspace, KeyCode::Char('q')),
        None,
        "and nothing from the refused layer half-applied"
    );
}
