//! Hints must describe keys that work in the live panel and its current root.
use super::tests::support::*;
use super::{ExplorerOutcome, FileExplorer};
use crate::PanelFit;
use iridium_editor::commands::builtin::*;
use iridium_editor::theme::Theme;
use iridium_editor::{CommandId, KeyBinding, KeyCode, KeyLabelStyle, Keymap, Modifiers};
use std::fs;

fn configured(explorer: &mut FileExplorer, bindings: &[(&str, CommandId)]) {
    let mut user = Keymap::new("user");
    for (key, command) in bindings {
        user.push(KeyBinding::parse(key, command.clone()).expect("binding"));
    }
    explorer.set_user_keymap(&user);
}

fn screen(explorer: &mut FileExplorer) -> String {
    explorer
        .body(&Theme::dark(), PanelFit::popover(80, 24))
        .rows
        .iter()
        .flat_map(|row| {
            row.spans
                .iter()
                .map(|span| span.text.as_str())
                .chain(std::iter::once("\n"))
        })
        .collect()
}

#[test]
fn portable_and_desktop_hints_use_their_actual_modifier_conventions() {
    let dir = project();
    fs::write(dir.path().join(".secret"), "secret").expect("fixture");
    let mut explorer = opened(&dir);
    assert!(screen(&mut explorer).contains("1 hidden (Ctrl+.)"));
    select_row(&mut explorer, "README.md");
    explorer.handle_key(&press(KeyCode::Tab));
    assert!(screen(&mut explorer).contains("Ctrl+S review · Esc browse"));
    explorer.set_key_label_style(KeyLabelStyle::MacGlyphsCommandAsMeta);
    assert!(screen(&mut explorer).contains("⌘S review · Esc browse"));
    explorer.handle_key(&press(KeyCode::Escape));
    assert!(screen(&mut explorer).contains("1 hidden (⌘.)"));
}

#[test]
fn control_s_reviews_and_confirmation_is_required_before_a_rename() {
    let dir = project();
    let mut explorer = opened(&dir);
    select_row(&mut explorer, "README.md");
    explorer.handle_key(&press(KeyCode::Tab));
    explorer.paste(".new");
    explorer.handle_key(&chord(KeyCode::Char('s'), Modifiers::ctrl()));
    assert!(explorer.mode.plan().is_some());
    assert!(screen(&mut explorer).contains("Y to apply"));
    assert!(dir.path().join("README.md").exists());
    assert!(!dir.path().join("README.md.new").exists());
    explorer.handle_key(&press(KeyCode::Char('y')));
    assert!(!dir.path().join("README.md").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("README.md.new")).expect("renamed"),
        "#"
    );
}

#[test]
fn customized_edit_confirm_cancel_and_discard_hints_match_their_keys() {
    let dir = project();
    let mut explorer = opened(&dir);
    configured(
        &mut explorer,
        &[
            ("f2", EXPLORER_BEGIN_EDIT),
            ("f3", EXPLORER_EDIT_ASK_APPLY),
            ("f4", EXPLORER_EDIT_LEAVE),
            ("f5", EXPLORER_CONFIRM_APPLY),
            ("f6", EXPLORER_CONFIRM_CANCEL),
        ],
    );
    assert!(screen(&mut explorer).contains("F2 to edit these rows"));
    select_row(&mut explorer, "README.md");
    explorer.handle_key(&press(KeyCode::F2));
    assert!(screen(&mut explorer).contains("F3 review · F4 browse"));
    explorer.paste(".new");
    explorer.handle_key(&press(KeyCode::F3));
    assert!(screen(&mut explorer).contains("F5 to apply · F6 to go back"));
    explorer.handle_key(&press(KeyCode::F6));
    assert!(explorer.mode.plan().is_none());
    assert!(explorer.unapplied_edits_message().contains("F4 twice"));
    let refused = explorer.handle_key(&press(KeyCode::F4));
    assert!(matches!(refused, ExplorerOutcome::Failed(message) if message.contains("F4 again")));
    assert!(explorer.has_unapplied_edits());
    explorer.handle_key(&press(KeyCode::F4));
    assert!(!explorer.has_unapplied_edits());
    assert!(dir.path().join("README.md").exists());
}

#[test]
fn suppressed_keys_are_not_advertised_and_hidden_count_remains() {
    let dir = project();
    fs::write(dir.path().join(".secret"), "secret").expect("fixture");
    let mut explorer = opened(&dir);
    let mut user = Keymap::new("user");
    for key in ["tab", "ctrl+.", "meta+.", "ctrl+s", "meta+s", "escape"] {
        let (first, rest) = KeyBinding::parse_sequence(key).expect("suppression");
        user.push(KeyBinding::unbound(first, &rest));
    }
    explorer.set_user_keymap(&user);
    let text = screen(&mut explorer);
    assert!(text.contains("1 hidden"));
    assert!(!text.contains("1 hidden ("));
    assert!(!text.contains("to edit these rows"));
    assert!(explorer.key_hint(&EXPLORER_EDIT_ASK_APPLY).is_none());
    assert!(
        explorer
            .unapplied_edits_message()
            .contains("bind explorer.edit.leave")
    );
    explorer.handle_key(&press(KeyCode::Tab));
    assert!(!explorer.mode.is_editing());
}

#[test]
fn changing_roots_keeps_custom_keys_and_label_style() {
    let dir = project();
    let mut explorer = opened(&dir);
    explorer.set_key_label_style(KeyLabelStyle::MacGlyphsCommandAsMeta);
    configured(
        &mut explorer,
        &[
            ("f2", EXPLORER_BEGIN_EDIT),
            ("f8", EXPLORER_ROOT_AT_SELECTION),
        ],
    );
    select_row(&mut explorer, "engine/");
    explorer.handle_key(&press(KeyCode::F8));
    settle(&mut explorer);
    assert_eq!(explorer.root_path(), dir.path().join("engine"));
    assert!(screen(&mut explorer).contains("F2 to edit these rows"));
    explorer.handle_key(&press(KeyCode::F2));
    assert!(explorer.mode.is_editing());
    assert!(screen(&mut explorer).contains("⌘S review"));
}

#[test]
fn custom_refusal_dismissal_key_is_shown_and_reaches_editing_again() {
    let dir = project();
    let mut explorer = opened(&dir);
    configured(&mut explorer, &[("f8", EXPLORER_REFUSED_DISMISS)]);
    select_row(&mut explorer, "README.md");
    explorer.handle_key(&press(KeyCode::Tab));
    // Removing the entire filename is invalid and must yield a refusal.
    for _ in 0..9 {
        explorer.handle_key(&press(KeyCode::Backspace));
    }
    explorer.handle_key(&chord(KeyCode::Char('s'), Modifiers::ctrl()));
    assert!(!explorer.mode.refusals().is_empty());
    assert!(screen(&mut explorer).contains("F8 to go back"));
    explorer.handle_key(&press(KeyCode::F8));
    assert!(explorer.mode.refusals().is_empty());
    assert!(explorer.mode.is_editing());
    assert!(dir.path().join("README.md").exists());
}

#[test]
fn a_custom_control_binding_wins_over_a_default_mac_binding() {
    let dir = project();
    let mut explorer = opened(&dir);
    explorer.set_key_label_style(KeyLabelStyle::MacGlyphsCommandAsMeta);
    configured(&mut explorer, &[("ctrl+r", EXPLORER_EDIT_ASK_APPLY)]);
    assert_eq!(explorer.key_hint(&EXPLORER_EDIT_ASK_APPLY), Some("⌃R"));
}
