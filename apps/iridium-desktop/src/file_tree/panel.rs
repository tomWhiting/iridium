//! The panel itself: its state, its keys, and its rows.

use std::path::{Path, PathBuf};

use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_explorer::{EntryKind, FileTree, NodeId};
use iridium_tree::{Tree, TreeSource as _};

use crate::overlay::{PanelAnchor, PanelContent, PanelFit, PanelRow, Span};

/// The most rows the panel shows at once, before the window's own limit.
///
/// The same number the palette and the undo tree use, so the three panels are
/// the same size on screen; a file list that grew to fill the window would
/// make the popover a sidebar, which is the thing it was chosen instead of.
const PANEL_MAX_VISIBLE_ROWS: usize = 12;

/// One indent level, in characters.
const INDENT: usize = 2;

/// What a key press did to the panel.
///
/// There is no `Ignored`: the panel is modal, and a key it does not bind is
/// swallowed rather than falling through to the document — the same rule the
/// undo tree follows, for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerOutcome {
    /// The panel consumed the key and stays open.
    Handled,
    /// The panel was closed.
    Closed,
    /// This file should be opened in a tab. The host decides what that
    /// means; the panel does not touch the workspace.
    Open(PathBuf),
}

/// The file explorer.
///
/// The host owns one of these, creates it when the kernel reports the
/// `explorer.togglePanel` host command, hands it every key while it is open,
/// polls it once per frame, and paints it over the composed frame.
#[derive(Debug)]
pub struct FileExplorer {
    /// The filesystem, read off the frame thread.
    files: FileTree,
    /// What is open and what is therefore visible.
    tree: Tree<FileTree>,
    /// The first row the window shows.
    scroll: usize,
    /// A node that was asked to open before its listing had landed, and
    /// opens as soon as it does.
    ///
    /// **This is the trap the panel exists around.** Expanding a node whose
    /// children have not arrived makes the tree record it as a leaf —
    /// `TreeSource`'s fourth rule says a node that promised children and
    /// produced none is treated as one from then on — so it would never open
    /// again. Every expansion therefore goes through
    /// [`open_row`](Self::open_row), which either opens a node that is
    /// already listed or defers to here. The root is the first user of it.
    ///
    /// One deep, not a set: a keyboard user opens one node at a time, and
    /// the intent that matters is the last one expressed.
    wanted: Option<NodeId>,
}

impl FileExplorer {
    /// Opens an explorer rooted at `root`.
    ///
    /// The root opens itself on the first [`poll`](Self::poll) that brings
    /// its listing, not here: a panel showing one collapsed row has told the
    /// user nothing, but expanding before the children exist would mark the
    /// root a leaf permanently. See [`root_opened`](Self::root_opened).
    ///
    /// # Errors
    ///
    /// Returns the operating system's message when the directory reader
    /// cannot be started.
    pub fn open(root: PathBuf) -> Result<Self, String> {
        let mut files = FileTree::open(root)?;
        let root_id = files.root();
        let mut tree = Tree::new(&mut files);
        tree.select(0);
        Ok(Self {
            files,
            tree,
            scroll: 0,
            wanted: Some(root_id),
        })
    }

    /// Collects any directory reads that finished, and reports whether the
    /// rows changed.
    ///
    /// Call once per frame while the panel is open, and repaint on `true`.
    /// The common answer is `false` and costs one non-blocking channel poll.
    pub fn poll(&mut self) -> bool {
        if !self.files.drain() {
            return false;
        }
        self.tree.refresh(&mut self.files);
        self.open_what_was_wanted();
        true
    }

    /// Opens the node a keypress asked for, once its listing has arrived.
    ///
    /// A listing that *failed* clears the intent rather than holding it
    /// forever: the node stays closed, its row carries the error, and a
    /// second press is free to ask again.
    fn open_what_was_wanted(&mut self) {
        let Some(id) = self.wanted else {
            return;
        };
        if self.files.is_listed(id) {
            if let Some(index) = self.tree.index_of(&id) {
                self.tree.expand(&mut self.files, index);
            }
            self.wanted = None;
        } else if self.files.info(id).is_some_and(|info| info.error.is_some()) {
            self.wanted = None;
        }
    }

    /// Opens the row at `index`, now or as soon as its listing lands.
    ///
    /// The gate is [`FileTree::is_listed`]: expanding a node whose children
    /// have not arrived would mark it a leaf permanently. Asking the source
    /// for the children is what posts the read.
    fn open_row(&mut self, index: usize) {
        let Some(id) = self.tree.row(index).map(|row| row.id) else {
            return;
        };
        if self.files.is_listed(id) {
            self.tree.expand(&mut self.files, index);
            return;
        }
        // Posts the read as a side effect, which is exactly what
        // `TreeSource::children` is documented to do for a listing it does
        // not have.
        drop(self.files.children(Some(&id)));
        self.wanted = Some(id);
    }

    /// Whether a directory read is still outstanding.
    ///
    /// The host asks for another frame while this is `true`: the reads land
    /// on a worker thread and nothing wakes winit when they do, so a window
    /// that stopped repainting would leave a directory permanently loading.
    #[must_use]
    pub const fn is_waiting(&self) -> bool {
        self.files.is_waiting()
    }

    /// The path of the row under the selection, if there is one.
    #[must_use]
    pub fn selected_path(&self) -> Option<&Path> {
        let id = self.tree.selected_id()?;
        self.files.info(*id).map(|info| info.path)
    }

    /// Handles one key press. Every key is consumed; see [`ExplorerOutcome`].
    pub fn handle_key(&mut self, event: &KeyEvent) -> ExplorerOutcome {
        match (chord(event.modifiers), event.key) {
            (Chord::Plain, KeyCode::Escape)
            | (Chord::CtrlAlt | Chord::MetaAlt, KeyCode::Char('e' | 'E')) => {
                ExplorerOutcome::Closed
            },
            (Chord::Plain, KeyCode::Enter) => self.activate(),
            (Chord::Plain, KeyCode::Up) | (Chord::Ctrl, KeyCode::Char('p' | 'P')) => {
                self.tree.move_up();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Down) | (Chord::Ctrl, KeyCode::Char('n' | 'N')) => {
                self.tree.move_down();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Right) => {
                self.expand_or_descend();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Left) => {
                self.tree.move_left();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Home) => {
                self.tree.move_to_first();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::End) => {
                self.tree.move_to_last();
                ExplorerOutcome::Handled
            },
            // Modal: everything else is swallowed, not passed to the
            // document.
            _ => ExplorerOutcome::Handled,
        }
    }

    /// What `Right` does: open a closed node, or step into an open one.
    ///
    /// Split from [`iridium_tree::Tree::move_right`] only for the closed
    /// case, which has to go through [`open_row`](Self::open_row) rather
    /// than expanding a node whose children may not have arrived.
    fn expand_or_descend(&mut self) {
        let Some(index) = self.tree.selected() else {
            return;
        };
        if self
            .tree
            .row(index)
            .is_some_and(iridium_tree::Row::is_openable)
        {
            self.open_row(index);
        } else {
            self.tree.move_right(&mut self.files);
        }
    }

    /// What `Enter` does to the selected row.
    ///
    /// A directory toggles; a file opens. **This is the placeholder half of
    /// an undecided question** — whether `Enter` on a directory should
    /// instead descend, making the popover re-root itself, is Tom's to rule
    /// on, and toggling is the answer that cannot surprise anyone in the
    /// meantime: it is what `Right` already does, and it never moves the
    /// ground under the selection.
    fn activate(&mut self) -> ExplorerOutcome {
        let Some(index) = self.tree.selected() else {
            return ExplorerOutcome::Handled;
        };
        let Some(id) = self.tree.row(index).map(|row| row.id) else {
            return ExplorerOutcome::Handled;
        };
        let Some(info) = self.files.info(id) else {
            return ExplorerOutcome::Handled;
        };
        if info.kind.is_expandable() {
            if self.tree.row(index).is_some_and(|row| row.expanded) {
                self.tree.collapse(index);
            } else {
                self.open_row(index);
            }
            return ExplorerOutcome::Handled;
        }
        ExplorerOutcome::Open(info.path.to_path_buf())
    }

    /// Composes the panel for painting.
    ///
    /// `&mut self` because composition is where the scroll window follows the
    /// selection and the page size is learned, exactly as in the palette.
    pub fn content(&mut self, theme: &Theme, fit: PanelFit) -> PanelContent {
        let total = self.tree.len();
        // At least one row, so a tree whose root has not listed yet still
        // says "Reading…" rather than composing an empty panel; and never
        // more than the panel's own limit or the window's.
        let ceiling = PANEL_MAX_VISIBLE_ROWS.min(fit.max_interior_rows.max(1));
        let visible = total.clamp(1, ceiling.max(1));
        self.follow_selection(total, visible);

        let mut rows = Vec::with_capacity(visible);
        for offset in 0..visible {
            let index = self.scroll + offset;
            match self.tree.row(index) {
                Some(row) => {
                    let selected = self.tree.selected() == Some(index);
                    rows.push(self.entry_row(
                        row.id,
                        row.depth,
                        row.has_children,
                        row.expanded,
                        selected,
                        theme,
                        fit.content_columns,
                    ));
                },
                // Only reachable for a tree with no rows at all, which is a
                // root whose read has not landed yet.
                None => rows.push(PanelRow::new(vec![Span::new(
                    "Reading…",
                    theme.editor.line_number,
                )])),
            }
        }

        PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: fit.content_columns,
            rows,
            // No field has focus: this panel has no query yet. The filter
            // arrives with the next step and brings a caret with it.
            caret: None,
        }
    }

    /// One row: indent, disclosure, name, and whatever the node has to say
    /// for itself.
    #[expect(
        clippy::too_many_arguments,
        reason = "every argument is one column of the row being composed; \
                  bundling them into a struct would name the same seven \
                  values twice"
    )]
    fn entry_row(
        &self,
        id: NodeId,
        depth: usize,
        has_children: bool,
        expanded: bool,
        selected: bool,
        theme: &Theme,
        width: usize,
    ) -> PanelRow {
        let Some(info) = self.files.info(id) else {
            return PanelRow::new(vec![Span::new("?", theme.editor.line_number)]);
        };

        // Two characters per level, and a disclosure column every row pays
        // for whether or not it has an arrow — so names line up down the
        // panel instead of stepping in and out with the arrows.
        let mut text = " ".repeat(depth.saturating_mul(INDENT));
        text.push_str(match (has_children, expanded) {
            (true, true) => "▾ ",
            (true, false) => "▸ ",
            (false, _) => "  ",
        });
        text.push_str(info.name);
        if info.kind == EntryKind::Directory {
            text.push('/');
        }
        if info.is_loading {
            text.push('…');
        }

        let mut spans = vec![Span::new(
            truncate(&text, width),
            if info.kind.is_expandable() {
                theme.editor.foreground
            } else {
                // Files sit a shade back from directories, so the shape of
                // the tree reads before its contents do.
                theme.editor.line_number
            },
        )];
        if let Some(error) = info.error {
            let room = width.saturating_sub(text.chars().count() + 2);
            if room > 0 {
                spans.push(Span::new(
                    format!("  {}", truncate(error, room)),
                    theme.editor.line_number,
                ));
            }
        }

        if selected {
            PanelRow::selected(spans)
        } else {
            PanelRow::new(spans)
        }
    }

    /// Slides the window so the selection is inside it, and never past the
    /// end of the rows.
    ///
    /// The clamp is applied on the way out as well as the way in: a refresh
    /// that removed rows can leave a scroll pointing past the end, and a
    /// window starting past the last row draws nothing at all.
    fn follow_selection(&mut self, total: usize, visible: usize) {
        let last_start = total.saturating_sub(visible);
        if let Some(selected) = self.tree.selected() {
            if selected < self.scroll {
                self.scroll = selected;
            } else if visible > 0 && selected >= self.scroll + visible {
                self.scroll = selected + 1 - visible;
            }
        }
        self.scroll = self.scroll.min(last_start);
    }
}

/// Cuts `text` to `width` characters, by characters and not by bytes.
///
/// An ellipsis rather than a hard cut, so a truncated name is visibly
/// truncated — a path silently missing its last three characters is how
/// someone opens the wrong file.
pub(super) fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// The modifier shapes this panel distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chord {
    /// No modifier that changes the meaning of the key.
    Plain,
    /// Control alone — the `Ctrl+N` / `Ctrl+P` movement pair.
    Ctrl,
    /// Control and Alt together — the toggle chord that closes the panel.
    CtrlAlt,
    /// Meta and Alt together — the mac spelling of the same toggle.
    MetaAlt,
    /// Anything else, which the modal panel swallows.
    Other,
}

/// The chord one modifier set names.
const fn chord(modifiers: Modifiers) -> Chord {
    match (modifiers.ctrl, modifiers.alt, modifiers.meta) {
        (false, false, false) => Chord::Plain,
        (true, false, false) => Chord::Ctrl,
        (true, true, false) => Chord::CtrlAlt,
        (false, true, true) => Chord::MetaAlt,
        _ => Chord::Other,
    }
}
