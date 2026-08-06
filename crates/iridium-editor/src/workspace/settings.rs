//! The state a workspace holds *on behalf of the window* and pushes to every
//! open editor.
//!
//! Because each open document owns a whole [`Editor`], anything that is
//! conceptually per-*window* — the theme, the configuration, the size of the
//! viewport, the face's own commands and key bindings — exists once per
//! document and has to be kept in step. Every setter here therefore applies
//! to **every open editor and to every future one**, never to the active tab
//! alone.
//!
//! The failure that shape prevents is always the same, and always quiet:
//! applying a change to only the active editor leaves the background tabs
//! stale, while a document opened *afterwards* inherits the workspace's
//! value — so the two disagree in opposite directions depending on which tab
//! you happen to look at, and neither one is wrong often enough to be
//! noticed.

use core::fmt;

use super::model::Workspace;
use crate::commands::{CommandMeta, Keymap, KeymapError, RegistryError};
use crate::editor::{Editor, EditorConfig};
use crate::theme::Theme;

/// Why a face's command or keymap could not be applied to every tab.
///
/// A union of the two failures because
/// [`Workspace::register_command`] and [`Workspace::push_keymap`] both
/// validate by *building an editor*, and building one replays both. A type
/// that named only the half the caller was adding would be claiming the
/// other half cannot fail — which is true today and is not a property of
/// the type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaceSetupError {
    /// A command could not be registered: a duplicate or empty id.
    Registry(RegistryError),
    /// A keymap layer was refused: most often a binding naming a command
    /// that is not registered.
    Keymap(KeymapError),
}

impl fmt::Display for FaceSetupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => write!(formatter, "{error}"),
            Self::Keymap(error) => write!(formatter, "{error}"),
        }
    }
}

impl core::error::Error for FaceSetupError {}

impl From<RegistryError> for FaceSetupError {
    fn from(error: RegistryError) -> Self {
        Self::Registry(error)
    }
}

impl From<KeymapError> for FaceSetupError {
    fn from(error: KeymapError) -> Self {
        Self::Keymap(error)
    }
}

/// The part of a [`Viewport`](crate::render::Viewport) that belongs to the
/// window rather than to the document.
///
/// A whole `Viewport` must **not** be shared across tabs: it also carries
/// `first_line` and the scroll offsets, which are exactly the per-document
/// facts that make two tabs onto different files show different regions.
/// Only the geometry is common, so only the geometry is stored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportGeometry {
    /// Height of one row, in the same pixels as `width` and `height`.
    pub line_height: f32,
    /// Viewport width in pixels.
    pub width: f32,
    /// Viewport height in pixels.
    pub height: f32,
}

impl<T> Workspace<T> {
    /// The theme every open editor is using.
    #[must_use]
    pub const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Sets the theme on the workspace **and on every open editor**.
    pub fn set_theme(&mut self, theme: Theme) {
        for open in self.documents.values_mut() {
            open.editor.set_theme(theme.clone());
        }
        self.theme = theme;
    }

    /// The configuration every open editor is using.
    #[must_use]
    pub const fn config(&self) -> &EditorConfig {
        &self.config
    }

    /// Sets the configuration on the workspace **and on every open editor**.
    pub fn set_config(&mut self, config: EditorConfig) {
        for open in self.documents.values_mut() {
            open.editor.set_config(config.clone());
        }
        self.config = config;
    }

    /// The window geometry every open editor's viewport is sized to, or
    /// `None` while the face has not reported one.
    #[must_use]
    pub const fn viewport(&self) -> Option<ViewportGeometry> {
        self.viewport
    }

    /// Sizes **every open editor's** viewport to the window, and every future
    /// one with it.
    ///
    /// Painting is driven by the face's own scroll offset, so a stale
    /// viewport does not show — it is `cursor.pageUp` / `cursor.pageDown`
    /// that hop by `visible_lines`, and those are the keys that would jump by
    /// the wrong number of rows. With one tab open a face could get away with
    /// sizing the active editor only; the moment there are two, a background
    /// tab would page by whatever the window was when *it* was last in front.
    ///
    /// Each tab keeps its own `first_line` and scroll offsets: only the
    /// geometry is shared, because only the geometry belongs to the window.
    pub fn set_viewport(&mut self, line_height: f32, width: f32, height: f32) {
        let geometry = ViewportGeometry {
            line_height,
            width,
            height,
        };
        for open in self.documents.values_mut() {
            apply_viewport(&mut open.editor, geometry);
        }
        self.viewport = Some(geometry);
    }

    /// Registers a face's command on every open tab, and on every future one.
    ///
    /// A face registers its commands once at startup; without this the first
    /// tab would have them and every later tab would not, and the symptom — a
    /// chord that works until you open a second file — reads as a keyboard
    /// bug rather than a registration one.
    ///
    /// # Errors
    ///
    /// Whatever [`Editor::register_command`] reports: a duplicate id,
    /// including one a kernel table already claims, or an empty id.
    ///
    /// **Validated by building an editor, not by consulting a second
    /// record.** A parallel check would be a proxy for what [`Editor`]
    /// actually accepts, and would diverge the first time the kernel's
    /// tables changed under it. The tentative entry is removed again if the
    /// build refuses it, so a rejected registration leaves no trace.
    ///
    /// The error is a union of both failures rather than
    /// [`RegistryError`] alone, because building an editor replays the
    /// keymaps too and the honest type says so.
    pub fn register_command(&mut self, meta: CommandMeta) -> Result<(), FaceSetupError> {
        self.face_commands.push(meta);
        if let Err(error) = self.build_editor("") {
            self.face_commands.pop();
            return Err(error);
        }

        // Cannot fail: the build above proved this exact sequence is
        // accepted by an editor built the same way, and every open editor
        // was built that way. Dropped rather than unwrapped for that reason.
        if let Some(meta) = self.face_commands.last().cloned() {
            for open in self.documents.values_mut() {
                drop(open.editor.register_command(meta.clone()));
            }
        }
        Ok(())
    }

    /// Pushes a keymap layer onto every open tab, and onto every future one.
    ///
    /// Layers are replayed in push order, because that order is what decides
    /// precedence between two layers binding the same chord.
    ///
    /// # Errors
    ///
    /// Whatever [`Editor::push_keymap`] reports — most often a binding
    /// naming a command that is not registered. A refused push leaves no
    /// layer behind on any tab; see [`register_command`](Self::register_command)
    /// for why validation is a build rather than a second check.
    pub fn push_keymap(&mut self, keymap: Keymap) -> Result<(), FaceSetupError> {
        self.face_keymaps.push(keymap);
        if let Err(error) = self.build_editor("") {
            self.face_keymaps.pop();
            return Err(error);
        }

        if let Some(keymap) = self.face_keymaps.last().cloned() {
            for open in self.documents.values_mut() {
                drop(open.editor.push_keymap(keymap.clone()));
            }
        }
        Ok(())
    }

    /// Builds an editor exactly as every open tab's was built.
    ///
    /// The single construction path, used both to open a document and to
    /// validate a registration. One path is the point: a validator that
    /// built editors differently from the real thing would agree with it
    /// right up until it mattered.
    pub(super) fn build_editor(&self, content: &str) -> Result<Editor, FaceSetupError> {
        let mut editor = Editor::new(self.config.clone());
        editor.set_theme(self.theme.clone());
        for meta in &self.face_commands {
            editor
                .register_command(meta.clone())
                .map_err(FaceSetupError::Registry)?;
        }
        for keymap in &self.face_keymaps {
            editor
                .push_keymap(keymap.clone())
                .map_err(FaceSetupError::Keymap)?;
        }
        if let Some(geometry) = self.viewport {
            apply_viewport(&mut editor, geometry);
        }
        editor.set_content(content);
        Ok(editor)
    }
}

/// Writes the window's geometry into one editor's viewport, leaving that
/// editor's own scroll position alone.
///
/// `line_height` is set before `resize` because `resize` derives
/// `visible_lines` from it; the other order would compute the row count
/// against the previous row height and be wrong for exactly one resize.
fn apply_viewport(editor: &mut Editor, geometry: ViewportGeometry) {
    let state = editor.state_mut();
    state.viewport.line_height = geometry.line_height;
    state.viewport.resize(geometry.width, geometry.height);
}
