//! Tests for the named-region verbs.
//!
//! Driven through the whole [`Editor`] by command id, and asserting on the
//! *text* a selection covers rather than on node kinds or offsets — the same
//! discipline as `super::super::expand::tests`, for the same two reasons. The
//! text is what a person sees selected, and it survives a grammar renaming its
//! internals; and the claims worth holding are claims about the editor, not
//! about a helper nobody calls directly.
//!
//! The small helpers below are deliberately a second copy of the ones in the
//! expansion tests rather than a shared module. They are four short functions,
//! and hoisting them would couple two independent test surfaces so that a
//! fixture change in one could break the other.

use iridium_syntax::Language;

use crate::commands::builtin::{
    AST_EXPAND_SELECTION, AST_NEXT_CLASS, AST_NEXT_FUNCTION, AST_PREVIOUS_CLASS,
    AST_PREVIOUS_FUNCTION, AST_SELECT_CLASS_AROUND, AST_SELECT_CLASS_INSIDE,
    AST_SELECT_COMMENT_AROUND, AST_SELECT_FUNCTION_AROUND, AST_SELECT_FUNCTION_INSIDE,
};
use crate::commands::{CommandArgs, CommandId};
use crate::editor::{Editor, EditorConfig};
use crate::input::keyboard::AstRequest;

/// Every named-region verb, so the whole-set tests cannot miss one.
const EVERY_VERB: &[&CommandId] = &[
    &AST_SELECT_FUNCTION_INSIDE,
    &AST_SELECT_FUNCTION_AROUND,
    &AST_SELECT_CLASS_INSIDE,
    &AST_SELECT_CLASS_AROUND,
    &AST_SELECT_COMMENT_AROUND,
    &AST_NEXT_FUNCTION,
    &AST_PREVIOUS_FUNCTION,
    &AST_NEXT_CLASS,
    &AST_PREVIOUS_CLASS,
];

/// Two functions, two classes and a comment, none of them nested.
///
/// Two of each so a jump has somewhere to arrive, and so a walk landing on the
/// wrong one is visible in the assertion rather than coincidentally right.
const RUST: &str = "\
fn alpha() {
    let a = 1;
}

/// A doc comment.
struct Holder {
    field: u32,
}

fn beta() {
    let b = 2;
}

enum Shape {
    Dot,
}
";

/// A function inside a method inside a class.
///
/// Python rather than Rust because its `@function.inside` is the whole body,
/// which makes the nesting ladder legible in an assertion.
const PYTHON: &str = "\
class Outer:
    def method(self):
        def nested():
            return 1
        return nested()
";

/// JSON has neither a function nor a class text object — only comments.
const JSON: &str = "{\"a\": [1, 2]}";

/// An editor holding `source`, parsed as `language`, caret at the origin.
fn editor(language: Language, source: &str) -> Editor {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content(source);
    editor.set_language(language);
    editor
}

/// Places a single caret just before the first occurrence of `needle`.
fn caret_before(editor: &mut Editor, source: &str, needle: &str) {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} does not occur in the fixture"));
    let position = editor
        .state()
        .document
        .offset_to_position(offset)
        .expect("the offset is inside the document");
    editor.set_cursor(position);
}

/// The text each cursor currently covers, primary first.
fn selected(editor: &Editor, source: &str) -> Vec<String> {
    editor
        .state()
        .cursor
        .all_selections()
        .map(|selection| {
            let document = &editor.state().document;
            let start = document
                .position_to_offset(selection.start())
                .expect("a live selection resolves");
            let end = document
                .position_to_offset(selection.end())
                .expect("a live selection resolves");
            source[start..end].to_string()
        })
        .collect()
}

/// Where each caret sits, as byte offsets, primary first.
fn carets(editor: &Editor) -> Vec<usize> {
    editor
        .state()
        .cursor
        .all_selections()
        .map(|selection| {
            editor
                .state()
                .document
                .position_to_offset(selection.cursor_position())
                .expect("a live caret resolves")
        })
        .collect()
}

/// Runs a verb by id, failing loudly if the kernel does not have it.
fn ast(editor: &mut Editor, id: &CommandId) {
    editor
        .run_command(id.as_str(), CommandArgs::default())
        .unwrap_or_else(|error| panic!("the kernel implements `{}`: {error}", id.as_str()));
}

/// The single text `id` leaves selected, starting from a caret before `needle`.
fn after(language: Language, source: &str, needle: &str, id: &CommandId) -> String {
    let mut editor = editor(language, source);
    caret_before(&mut editor, source, needle);
    ast(&mut editor, id);
    selected(&editor, source).join("")
}

#[test]
fn selecting_inside_a_function_and_around_it_are_different_selections() {
    let around = after(
        Language::Rust,
        RUST,
        "let a = 1;",
        &AST_SELECT_FUNCTION_AROUND,
    );
    let inside = after(
        Language::Rust,
        RUST,
        "let a = 1;",
        &AST_SELECT_FUNCTION_INSIDE,
    );

    assert!(
        around.starts_with("fn alpha()") && around.ends_with('}'),
        "around must take the signature and the closing brace: {around:?}"
    );
    assert!(
        !inside.contains("fn alpha"),
        "inside kept the signature: {inside:?}"
    );
    assert!(inside.contains("let a = 1;"), "got {inside:?}");
    assert_ne!(
        around, inside,
        "the two variants collapsed onto the same range, so one of them is wired \
         to the wrong capture"
    );
}

#[test]
fn selecting_inside_a_class_and_around_it_are_different_selections() {
    let around = after(Language::Rust, RUST, "field: u32", &AST_SELECT_CLASS_AROUND);
    let inside = after(Language::Rust, RUST, "field: u32", &AST_SELECT_CLASS_INSIDE);

    assert!(
        around.starts_with("struct Holder"),
        "around must take the declaration: {around:?}"
    );
    assert!(
        !inside.contains("struct Holder"),
        "inside kept the declaration: {inside:?}"
    );
    assert!(inside.contains("field"), "got {inside:?}");
}

#[test]
fn a_caret_outside_every_function_leaves_the_selection_alone() {
    // Not an error and not a guess. A struct field is not inside a function, and
    // the honest answer is that the key does nothing.
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "field: u32");
    let before = editor.state().cursor.clone();

    ast(&mut editor, &AST_SELECT_FUNCTION_AROUND);

    assert_eq!(
        editor.state().cursor,
        before,
        "a caret outside every function selected something anyway"
    );
}

#[test]
fn a_nested_function_selects_the_inner_one_first_and_then_walks_out() {
    // Both halves matter. Landing on the method first would make the verb
    // useless inside a closure; refusing to move on the second press would make
    // it read as a broken key rather than as a finished one.
    let mut editor = editor(Language::Python, PYTHON);
    caret_before(&mut editor, PYTHON, "return 1");

    ast(&mut editor, &AST_SELECT_FUNCTION_AROUND);
    let inner = selected(&editor, PYTHON).join("");
    assert!(
        inner.starts_with("def nested"),
        "the first press jumped past the nested function to {inner:?}"
    );

    ast(&mut editor, &AST_SELECT_FUNCTION_AROUND);
    let outer = selected(&editor, PYTHON).join("");
    assert!(
        outer.starts_with("def method"),
        "the second press stayed on {outer:?}"
    );
}

#[test]
fn jumping_forward_and_back_between_functions_returns_to_where_it_started() {
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "fn alpha");
    let start = carets(&editor);

    ast(&mut editor, &AST_NEXT_FUNCTION);
    let beta = RUST.find("fn beta").expect("the fixture has it");
    assert_eq!(
        carets(&editor),
        vec![beta],
        "the forward jump did not land on the start of the next function"
    );

    ast(&mut editor, &AST_PREVIOUS_FUNCTION);
    assert_eq!(
        carets(&editor),
        start,
        "the two directions are not inverses of each other"
    );
}

#[test]
fn jumping_forward_and_back_between_classes_returns_to_where_it_started() {
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "struct Holder");
    let start = carets(&editor);

    ast(&mut editor, &AST_NEXT_CLASS);
    let shape = RUST.find("enum Shape").expect("the fixture has it");
    assert_eq!(
        carets(&editor),
        vec![shape],
        "an enum is a class text object, and the jump must reach it"
    );

    ast(&mut editor, &AST_PREVIOUS_CLASS);
    assert_eq!(carets(&editor), start);
}

#[test]
fn jumping_past_the_last_function_does_nothing() {
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "fn beta");
    let before = editor.state().cursor.clone();

    ast(&mut editor, &AST_NEXT_FUNCTION);

    assert_eq!(
        editor.state().cursor,
        before,
        "past the last function there is nowhere to go, and the caret must stay"
    );
}

#[test]
fn a_verb_with_nowhere_to_go_reports_that_nothing_happened() {
    // Comparing cursor states cannot catch this, and the previous step's breaks
    // are where that was learned: a verb that returns the state it was handed
    // still *applies* it, and the cursors look identical either way. What
    // differs is the claim — and the cost of a false claim is the expansion
    // stack, which every moving verb clears. A jump past the last function would
    // then silently destroy the ability to shrink back, from a key that visibly
    // did nothing.
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "fn beta");
    editor
        .run_command(AST_EXPAND_SELECTION.as_str(), CommandArgs::default())
        .expect("the kernel implements expansion");
    let depth = editor.state().syntax.expansion_depth();
    assert!(
        depth > 0,
        "there must be frames for this test to protect them"
    );
    let before = editor.state().cursor.clone();

    let moved = editor.perform_ast_request(AstRequest::NextFunction);

    assert!(
        !moved,
        "the jump reported a change it did not make; every caller that acts on \
         that answer is now acting on a lie"
    );
    assert_eq!(editor.state().cursor, before);
    assert_eq!(
        editor.state().syntax.expansion_depth(),
        depth,
        "a verb that did nothing threw away the frames a shrink needs"
    );
}

#[test]
fn a_jump_leaves_a_caret_rather_than_a_selection() {
    // The documented half of the caret-versus-selection decision on
    // `AstRequest::NextFunction`. `jump` answers with the whole `around` extent;
    // collapsing it is what keeps `ast.nextFunction` distinguishable from
    // `ast.selectFunctionAround` and gives the set a verb that simply *goes*
    // somewhere.
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "let a = 1;");

    ast(&mut editor, &AST_NEXT_FUNCTION);

    assert_eq!(
        selected(&editor, RUST),
        vec![""],
        "a go-to verb must leave a caret, not a selection"
    );
    assert_eq!(
        carets(&editor),
        vec![RUST.find("fn beta").expect("the fixture has it")],
        "the caret landed somewhere other than the start of the function"
    );
}

#[test]
fn selecting_a_comment_takes_the_whole_comment() {
    // The trailing newline is the grammar's, not this layer's: tree-sitter-rust
    // ends a `line_comment` node after its terminator. Pinned exactly rather
    // than trimmed away, so a grammar bump that changed it is a visible failure
    // instead of a silently different selection.
    let selected = after(
        Language::Rust,
        RUST,
        "A doc comment",
        &AST_SELECT_COMMENT_AROUND,
    );

    assert_eq!(selected, "/// A doc comment.\n");
}

#[test]
fn a_named_region_verb_throws_away_the_expansion_stack() {
    // Moving, in the taxonomy on `AstRequest`. A text object is a jump to
    // somewhere the grammar names, not a rung on the expansion ladder, so a
    // shrink afterwards must narrow rather than retrace an expansion that no
    // longer describes where the person is.
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "let a = 1;");
    editor
        .run_command(AST_EXPAND_SELECTION.as_str(), CommandArgs::default())
        .expect("the kernel implements expansion");
    assert!(
        editor.state().syntax.expansion_depth() > 0,
        "the expansion must have been recorded for this test to mean anything"
    );

    ast(&mut editor, &AST_SELECT_FUNCTION_AROUND);

    assert!(
        selected(&editor, RUST).join("").starts_with("fn alpha"),
        "the verb did not move, so it never reached the code that clears frames"
    );
    assert_eq!(
        editor.state().syntax.expansion_depth(),
        0,
        "a named-region verb left frames behind that a later shrink would retrace"
    );
}

#[test]
fn every_named_region_verb_is_silent_on_a_document_with_no_language() {
    // The whole set, not a sample: a verb that misbehaves without a tree is a
    // verb that breaks the editor on any file whose language has no grammar,
    // which is most of them.
    for id in EVERY_VERB {
        let mut editor = Editor::new(EditorConfig::default());
        editor.set_content(RUST);
        caret_before(&mut editor, RUST, "let a = 1;");
        let before = editor.state().cursor.clone();

        ast(&mut editor, id);

        assert_eq!(
            editor.state().cursor,
            before,
            "`{}` moved the cursor in a document with no parse tree",
            id.as_str()
        );
    }
}

#[test]
fn every_named_region_verb_leaves_the_document_untouched() {
    // None of these is `.mutating()`, which is what keeps them available in a
    // read-only buffer. If one of them ever edited text that claim would be a
    // lie, and the lie would only surface in a read-only file.
    for id in EVERY_VERB {
        let mut editor = editor(Language::Rust, RUST);
        caret_before(&mut editor, RUST, "let a = 1;");

        ast(&mut editor, id);

        assert_eq!(
            editor.state().document.text(),
            RUST,
            "`{}` changed the document",
            id.as_str()
        );
    }
}

#[test]
fn a_language_without_the_captures_answers_with_no_movement() {
    // JSON ships only `@comment.around`, and this fixture has no comment. Every
    // one of the nine must be quiet rather than erroring: the query layer is
    // fallible, and a failure surfaced at a keypress is something the person
    // cannot act on.
    for id in EVERY_VERB {
        let mut editor = editor(Language::Json, JSON);
        caret_before(&mut editor, JSON, "1");
        let before = editor.state().cursor.clone();

        ast(&mut editor, id);

        assert_eq!(
            editor.state().cursor,
            before,
            "`{}` moved the cursor in a language with no such text object",
            id.as_str()
        );
    }
}
