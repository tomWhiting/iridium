//! The user's configuration file, for Iridium's native faces.
//!
//! ```toml
//! # ~/.config/iridium/config.toml
//!
//! [editor]
//! tab_width = 2
//! font_size = 15.0
//!
//! [keys]
//! "cmd+shift+p" = "palette.open"
//! "cmd+k cmd+c" = "edit.toggleLineComment"
//! ```
//!
//! # Four rules
//!
//! **1. A bad configuration never stops the editor.** Nothing here returns a
//! fatal error. Every complaint is collected into [`UserConfig::problems`], the
//! thing it names falls back to its default, and the editor starts. The reason
//! is circular and decisive: the usual way to fix a configuration file is to
//! open it in the editor.
//!
//! **2. A mistake costs its own line.** Sections are read separately, and
//! inside `[editor]` every setting is applied separately. A typo'd boolean does
//! not take the keybindings with it, and a misspelled setting does not take the
//! settings written beside it. "I changed one thing and everything reverted" is
//! the failure this shape exists to prevent, and reverted settings are silent,
//! so it would be found long after it was caused.
//!
//! The exception is a whole-file syntax error, which by construction leaves no
//! sections to isolate. That one is reported and everything defaults.
//!
//! **3. A refused binding is reported, never swallowed.** The kernel validates
//! a keymap layer whole and refuses the whole thing, which would mean one
//! mistyped command id costing every binding in the file. [`install`] drops the
//! binding the refusal names, records why, and offers the rest again — see that
//! module for what is deliberately *not* done on the user's behalf.
//!
//! **4. No file is not a problem.** An absent configuration file is how almost
//! everybody runs an editor. Only a file that exists and is wrong says
//! anything.
//!
//! # Why this is a crate, and a native-only one
//!
//! A browser has no configuration file, and the wasm bundle must not carry a
//! TOML parser to prove it. Both native faces — desktop and terminal — want the
//! same file with the same meaning, which is what a shared crate is for.
//!
//! Nothing here decides what a setting *means* or what a chord *is*: those are
//! [`EditorConfig`](iridium_editor::EditorConfig) and
//! [`KeyBinding`](iridium_editor::KeyBinding), in the kernel, shared with the
//! browser. This crate only turns a file into them, which is why a mirror of
//! either type appears nowhere in it.

pub mod fields;
pub mod install;
pub mod keys;
pub mod load;
pub mod location;
pub mod problem;
pub mod settings;
pub mod suggest;

pub use install::install;
pub use keys::KEYMAP_NAME;
pub use load::UserConfig;
pub use location::{CONFIG_DIRECTORY, CONFIG_FILE, config_path, user_config_path};
pub use problem::{Problem, Section};
