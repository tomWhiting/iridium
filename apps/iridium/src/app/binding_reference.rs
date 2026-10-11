//! Generate the complete terminal reference from executable keymaps.
use iridium_editor::{KeyBinding, Keymap};

pub(super) fn generate() -> Result<String, String> {
    let layers = vec![
        (
            "Editor base",
            iridium_editor::commands::default_non_modal_keymap(),
        ),
        ("Terminal host", super::commands::keymap()),
        (
            "Structural modes",
            iridium_editor::commands::structural_keymap::structural_keymap(),
        ),
        ("Terminal modal overrides", super::modal::keymap()),
        (
            "Terminal workspace",
            super::workbench::keymap().map_err(|error| error.to_string())?,
        ),
        (
            "File browser (when focused)",
            iridium_panel::explorer::default_keymap(),
        ),
        ("Search (when focused)", iridium_tui::frame::search_keymap()),
        (
            "Command palette (when focused)",
            iridium_tui::frame::command_palette_keymap(),
        ),
        (
            "History (when focused)",
            iridium_tui::frame::history_panel_keymap(),
        ),
    ];
    let mut text = String::from(concat!(
        "# Generated Iridium TERMINAL keybindings. Regenerated on startup; do not edit.\n",
        "# Copy individual entries into [keys] in config.toml, then :reload-config.\n",
        "# :config opens your config; :keys opens this reference.\n",
        "# Later editor layers override earlier ones. Panel layers apply only in their panels.\n",
        "# --non-modal omits the Terminal modal overrides layer.\n",
        "# All entries are comments: repeated chords show each layer and mode.\n",
        "# A chord may have only one entry in your [keys] table. Keep the mode field.\n",
        "# ~modifier means either state; !modifier means absent; bare modifiers mean required.\n",
        "# Unbound printable keys type into the focused input.\n\n[keys]\n",
    ));
    for (title, map) in &layers {
        write_layer(&mut text, title, map)?;
    }
    text.push_str("\n# All registered command IDs (including commands without a shortcut):\n");
    let mut registry =
        iridium_editor::commands::builtin::default_registry().map_err(|e| e.to_string())?;
    for meta in super::commands::command_metas() {
        registry.register(meta).map_err(|e| e.to_string())?;
    }
    for meta in registry.commands() {
        text.push_str("# ");
        text.push_str(meta.id().as_str());
        text.push_str(" — ");
        text.push_str(meta.title());
        text.push('\n');
    }
    Ok(text)
}

fn write_layer(text: &mut String, title: &str, map: &Keymap) -> Result<(), String> {
    text.push_str("\n# ");
    text.push_str(title);
    text.push('\n');
    for binding in map.bindings() {
        text.push_str("# ");
        text.push_str(&entry(binding)?);
        text.push('\n');
    }
    Ok(())
}

fn quote(value: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

pub(super) fn entry(binding: &KeyBinding) -> Result<String, String> {
    let mut fields = Vec::new();
    if let Some(command) = binding.command() {
        fields.push(format!("command = {}", quote(command.as_str())?));
    } else if binding.is_suppression() {
        fields.push("command = \"\"".into());
    }
    if let Some(mode) = binding.mode() {
        fields.push(format!("mode = {}", quote(mode.as_str())?));
    }
    if let Some(mode) = binding.enters_mode() {
        fields.push(format!("enter_mode = {}", quote(mode.as_str())?));
    }
    if binding.accepts_count() {
        fields.push("count = true".into());
    }
    if binding.records_input() {
        fields.push("record_input = true".into());
    }
    if !binding.following_commands().is_empty() {
        let following = binding
            .following_commands()
            .iter()
            .map(|id| quote(id.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        fields.push(format!("then = [{}]", following.join(", ")));
    }
    Ok(format!(
        "{} = {{ {} }}",
        quote(&binding.display_sequence())?,
        fields.join(", ")
    ))
}
