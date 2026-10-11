//! Open and reload saved configuration without replacing live documents.
use super::{App, Flow, Message};
use std::io::Write as _;
use std::path::PathBuf;

impl App {
    pub(crate) fn prepare_configuration(&mut self) {
        if let Err(error) = self.write_config_reference() {
            self.message = Some(Message::error(error));
        }
    }

    pub(super) fn configuration_path(&self) -> Result<PathBuf, String> {
        self.config_path
            .clone()
            .ok_or_else(|| "Cannot locate the Iridium config directory".into())
    }

    pub(super) fn write_config_reference(&self) -> Result<(), String> {
        let path = self.configuration_path()?;
        let directory = path
            .parent()
            .ok_or("Configuration has no parent directory")?;
        std::fs::create_dir_all(directory).map_err(|e| format!("{}: {e}", directory.display()))?;
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => file.write_all(b"# Save changes, then run :reload-config. See default-keybindings.toml.\n[editor]\n\n[keys]\n")
                .map_err(|e| format!("{}: {e}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
        let text = super::binding_reference::generate()?;
        let reference = directory.join("default-keybindings.toml");
        match std::fs::read_to_string(&reference) {
            Ok(previous) if previous == text => return Ok(()),
            Ok(_) => {},
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(format!("{}: {error}", reference.display())),
        }
        let mut staged = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
        staged
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        staged
            .persist(&reference)
            .map_err(|e| format!("{}: {e}", reference.display()))?;
        Ok(())
    }

    pub(super) fn open_configuration(&mut self, reference: bool) -> Flow {
        let result = self
            .write_config_reference()
            .and_then(|()| self.configuration_path());
        match result {
            Ok(path) => {
                let path = if reference {
                    path.with_file_name("default-keybindings.toml")
                } else {
                    path
                };
                let path = match path.canonicalize() {
                    Ok(path) => path,
                    Err(error) => {
                        self.message = Some(Message::error(format!("{}: {error}", path.display())));
                        return Flow::Running;
                    },
                };
                self.open_file(&path);
                if reference
                    && self.file().is_some_and(|file| file.path() == path)
                    && let Some(editor) = self.workspace.active_editor_mut()
                {
                    editor.state_mut().read_only = true;
                }
            },
            Err(error) => self.message = Some(Message::error(error)),
        }
        Flow::Running
    }

    pub(super) fn reload_configuration(&mut self) -> Result<(), String> {
        let path = self.configuration_path()?;
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let user = iridium_config::UserConfig::parse(&text);
        if !user.problems.is_empty() {
            return Err(format!(
                "Configuration unchanged: {}",
                user.problems
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        let keys = user.keymap();
        self.workspace
            .replace_keymap(keys.clone())
            .map_err(|error| format!("Configuration unchanged: {error}"))?;
        self.workspace.set_config(user.editor);
        self.user_keys = keys;
        self.palette.set_user_keymap(&self.user_keys);
        self.history.set_user_keymap(&self.user_keys);
        if let Some(panel) = self.explorer.as_mut() {
            panel.set_user_keymap(&self.user_keys);
        }
        let ids: Vec<_> = self
            .workspace
            .tabs()
            .iter()
            .filter_map(|tab| self.workspace.node(*tab)?.document())
            .collect();
        for id in ids {
            if let Some(session) = self.workspace.payload_mut(id) {
                session.search.set_user_keymap(&self.user_keys);
            }
        }
        self.sync_viewport();
        self.ensure_caret_visible();
        Ok(())
    }
}
