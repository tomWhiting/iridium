//! The parse state the browser's fold regions are derived from.
//!
//! # Why the browser has its own
//!
//! The kernel keeps a tree per `Editor` and refreshes its own folds from it.
//! The web surface keeps a *second* fold state, because the folds a reader has
//! collapsed live in it and are not the kernel's business. That second fold
//! state still needs something to derive regions from, and this is it.
//!
//! # Why it is not just a tree
//!
//! It was: a bare `SyntaxTree` that was re-parsed from the whole document on
//! every content change, and handed to the fold detector with
//! `SyntaxDelta::Full` — "assume nothing carries over". In the browser, where
//! there is no tree-sitter (`docs/WASM-SYNTAX-SPIKE.md`), the detector is a
//! brace scan of the document text, so `Full` meant reading every byte of the
//! document for every keystroke.
//!
//! [`SyntaxState`] is the kernel's answer to exactly that problem and is reused
//! here rather than reimplemented. It takes an edit *span* before the edit lands,
//! reports what moved, and — the part that matters most — compares the document
//! revision it was last told about with the one it is given, so a mutation that
//! never reported itself degrades to a full rescan instead of to a wrong answer.
//! Nothing in this module has to be right for the folds to be right; it only has
//! to be right for them to be cheap.
//!
//! It also settles a mismatch that was live before: the tree was parsed as Rust
//! while the fold state was built for C. With one state there is one language.

use std::borrow::Cow;

use iridium_editor::Language;
use iridium_editor::document::{Document, EditSpan};
use iridium_editor::editor::{FoldState, SyntaxState};

/// The retained parse state one `WebEditor`'s fold regions come from.
#[derive(Debug)]
pub struct WebFoldSyntax {
    /// The kernel's syntax state, driven by this surface rather than by the
    /// kernel's own command path.
    syntax: SyntaxState,
}

impl WebFoldSyntax {
    /// Creates the fold parse state for `language`.
    pub fn new(language: Language) -> Self {
        let mut syntax = SyntaxState::new();
        syntax.set_language(language);
        Self { syntax }
    }

    /// Reports an edit that has just been applied to `document`.
    ///
    /// `span` must have been measured against the document as it was **before**
    /// the edit, which is what `compute_edit_span` returns when called before
    /// the command is applied. Leaving an edit unreported is safe and merely
    /// slow: the revision comparison inside the state catches it and rescans.
    pub fn note_edit(&mut self, document: &Document, span: &EditSpan) {
        self.syntax.note_edit(document, span);
    }

    /// Declares that the document was replaced rather than edited.
    ///
    /// Needed because a revision comparison cannot catch a replacement on its
    /// own — a fresh document starts counting again — and used for the history
    /// traversals too, which move the document by an amount this surface never
    /// measured.
    pub fn invalidate(&mut self) {
        self.syntax.invalidate();
    }

    /// Brings `folds` up to date with `document`, reporting whether the regions
    /// changed.
    ///
    /// The text is passed as a closure because the tree-sitter detector never
    /// reads it and building it from a rope copies the whole document; the brace
    /// scanner does read it, and takes the copy only when it is actually the
    /// detector in play.
    pub fn refresh(&mut self, document: &Document, folds: &mut FoldState) -> bool {
        if self.syntax.sync(document).is_none() {
            return false;
        }
        // Taken before the tree is borrowed again, and taken rather than read so
        // that two consumers cannot both believe they are seeing the change for
        // the first time.
        let delta = self.syntax.take_delta();
        let Some(tree) = self.syntax.tree() else {
            return false;
        };
        folds.update_regions(tree, &delta, || Cow::Owned(document.text()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use iridium_editor::document::compute_edit_span;
    use iridium_editor::history::Command;
    use iridium_editor::{Editor, EditorConfig, Position};

    /// The language the web surface folds with, matching `WebEditor`'s.
    const LANGUAGE: Language = Language::C;

    /// One `WebEditor`'s fold machinery, without the rest of `WebEditor`.
    ///
    /// The wasm surface only compiles for `wasm32`, so it cannot be tested here.
    /// This reproduces the three steps it takes around a content change —
    /// measure the span, apply, report — so that the sequencing those steps
    /// depend on is checked somewhere a test can run.
    struct Surface {
        editor: Editor,
        folds: FoldState,
        syntax: WebFoldSyntax,
    }

    impl Surface {
        fn new(content: &str) -> Self {
            let mut editor = Editor::new(EditorConfig::default());
            editor.set_undo_group_timeout_ms(0);
            editor.set_content(content);

            let mut surface = Self {
                editor,
                folds: FoldState::for_language(LANGUAGE),
                syntax: WebFoldSyntax::new(LANGUAGE),
            };
            surface.refresh();
            surface
        }

        /// `WebEditor::track_and_apply`, reduced to what folds depend on.
        fn apply(&mut self, command: Command) {
            let span = compute_edit_span(&self.editor.state().document, &command)
                .ok()
                .flatten();
            self.editor.apply_command(command);
            if let Some(span) = span {
                self.syntax.note_edit(&self.editor.state().document, &span);
            }
            self.refresh();
        }

        fn refresh(&mut self) {
            self.syntax
                .refresh(&self.editor.state().document, &mut self.folds);
        }

        /// The regions a surface freshly loaded with this text would hold.
        fn oracle(&self) -> Vec<<FoldState as RegionSource>::Region> {
            Self::new(&self.editor.content()).folds.regions().to_vec()
        }
    }

    /// Names the region type without repeating either configuration's path.
    trait RegionSource {
        type Region: Clone + PartialEq + std::fmt::Debug;
    }

    impl RegionSource for FoldState {
        #[cfg(feature = "syntax")]
        type Region = iridium_syntax::FoldRegion;
        #[cfg(not(feature = "syntax"))]
        type Region = iridium_editor::syntax_stubs::FoldRegion;
    }

    /// A fixture with several brace blocks, so folds exist to get wrong.
    fn source(blocks: usize) -> String {
        use std::fmt::Write as _;

        let mut out = String::new();
        for index in 0..blocks {
            // Writing into a `String` is infallible.
            let _ = write!(out, "void f{index}(void) {{\n    body();\n}}\n");
        }
        out
    }

    /// The browser's folds must track the document through real typing.
    ///
    /// Reporting an edit is an optimisation, and an optimisation that changes
    /// the answer is a defect. Newlines and braces are in the burst because both
    /// move every fold after them.
    #[test]
    fn folds_track_the_document_across_a_burst_of_typing() {
        let mut surface = Surface::new(&source(6));

        for (step, text) in ["x", "\n", "    more();", "\n", "{", "}"]
            .into_iter()
            .cycle()
            .take(24)
            .enumerate()
        {
            surface.apply(Command::Insert {
                position: Position::new(1, 4),
                text: text.to_owned(),
            });

            assert_eq!(
                surface.folds.regions(),
                surface.oracle().as_slice(),
                "step {step}: the browser's folds drifted from a fresh load"
            );
        }
    }

    /// A replacement is not an edit, and saying so must keep the folds right.
    #[test]
    fn a_replaced_document_is_refolded_from_scratch() {
        let mut surface = Surface::new(&source(4));
        surface.editor.set_content(&source(2));
        surface.syntax.invalidate();
        surface.refresh();

        assert_eq!(
            surface.folds.regions(),
            surface.oracle().as_slice(),
            "a replaced document must be refolded, not shifted"
        );
    }

    /// A mutation nobody reported must still leave the folds right.
    ///
    /// This is the safety net the whole scheme rests on: the state compares the
    /// document revision it was told about against the one it is given, so a
    /// path that forgets to report costs a rescan rather than correctness.
    #[test]
    fn an_unreported_mutation_costs_speed_rather_than_correctness() {
        let mut surface = Surface::new(&source(4));

        // Straight past `apply`, so no span is ever reported.
        surface.editor.apply_command(Command::Insert {
            position: Position::new(0, 0),
            text: "void extra(void) {\n    x();\n}\n".to_owned(),
        });
        surface.refresh();

        assert_eq!(
            surface.folds.regions(),
            surface.oracle().as_slice(),
            "an unreported mutation left the folds describing a document that \
             no longer exists"
        );
    }

    /// The bug, stated as a test: the browser rescanned everything, every time.
    ///
    /// Only meaningful without a grammar, which is every browser build — with
    /// one, the cost is measured in tree nodes and the tree-sitter cache's own
    /// tests already pin it.
    #[cfg(not(feature = "syntax"))]
    #[test]
    fn typing_does_not_rescan_the_whole_document() {
        fn lines_for_one_keystroke(blocks: usize) -> u64 {
            let mut surface = Surface::new(&source(blocks));

            // Type once first, so the measured keystroke is a steady-state one.
            surface.apply(Command::Insert {
                position: Position::new(1, 4),
                text: "a".to_owned(),
            });
            let before = surface.folds.fold_lines_scanned();

            surface.apply(Command::Insert {
                position: Position::new(1, 4),
                text: "b".to_owned(),
            });
            surface.folds.fold_lines_scanned() - before
        }

        let small = lines_for_one_keystroke(20);
        let large = lines_for_one_keystroke(2_000);

        assert_eq!(
            small, large,
            "one keystroke read {small} lines of a 60-line document and {large} \
             of a 6,000-line one; the browser is still rescanning everything"
        );
    }
}
