use super::*;
use crate::Language;
#[cfg(feature = "syntax")]
use crate::Position;
fn fixture(text: &str, language: Language, selected: &str) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor.set_language(language);
    let start = text.find(selected).expect("seed");
    editor.set_selection(
        editor
            .state()
            .document
            .offset_to_position(start)
            .expect("start"),
        editor
            .state()
            .document
            .offset_to_position(start + selected.len())
            .expect("end"),
    );
    editor
}
#[test]
#[cfg(feature = "syntax")]
fn kind_and_shape_preserve_seed_and_exclude_overlapping_ancestors() {
    let mut editor = fixture("[[1],[2,3],[4]]", Language::Json, "[1]");
    let original = editor.state().cursor.clone();
    let kind = editor
        .similar_syntax_selections(SyntaxSimilarity::Kind)
        .expect("kind");
    let shape = editor
        .similar_syntax_selections(SyntaxSimilarity::Shape)
        .expect("shape");
    assert_eq!(kind.cursor_count(), 3);
    assert_eq!(shape.cursor_count(), 2);
    assert_eq!(kind.primary, original.primary);
    assert_eq!(editor.state().cursor, original);
    assert_eq!(editor.content(), "[[1],[2,3],[4]]");
}
#[test]
#[cfg(feature = "syntax")]
fn toml_and_rust_use_grammar_shapes_without_language_specific_motion_rules() {
    let mut editor = fixture(
        "[server]\na = 12\nb = 34\nc = \"12\"\n",
        Language::Toml,
        "12",
    );
    assert_eq!(
        editor
            .similar_syntax_selections(SyntaxSimilarity::Kind)
            .expect("TOML")
            .cursor_count(),
        2
    );
    let mut editor = fixture(
        "fn f(){ foo(a); bar(b); foo(a,b); }",
        Language::Rust,
        "foo(a)",
    );
    assert_eq!(
        editor
            .similar_syntax_selections(SyntaxSimilarity::Kind)
            .expect("calls")
            .cursor_count(),
        3
    );
    assert_eq!(
        editor
            .similar_syntax_selections(SyntaxSimilarity::Shape)
            .expect("shape")
            .cursor_count(),
        2
    );
}
#[test]
fn partial_or_missing_grammar_refusal_preserves_selection_and_text() {
    let mut editor = fixture("[123,456]", Language::Json, "12");
    let original = editor.state().cursor.clone();
    assert!(
        editor
            .similar_syntax_selections(SyntaxSimilarity::Kind)
            .is_err()
    );
    assert_eq!(editor.state().cursor, original);
    assert_eq!(editor.content(), "[123,456]");
    editor.set_language(Language::Diff);
    assert!(
        editor
            .similar_syntax_selections(SyntaxSimilarity::Kind)
            .is_err()
    );
}
#[test]
#[cfg(feature = "syntax")]
fn source_edits_are_reparsed_and_match_limit_has_no_partial_publication() {
    let mut editor = fixture("[1,2]", Language::Json, "1");
    assert_eq!(
        editor
            .similar_syntax_selections(SyntaxSimilarity::Kind)
            .expect("before")
            .cursor_count(),
        2
    );
    editor.set_content("[1,2,3]");
    editor.set_selection(Position::new(0, 1), Position::new(0, 2));
    assert_eq!(
        editor
            .similar_syntax_selections(SyntaxSimilarity::Kind)
            .expect("after")
            .cursor_count(),
        3
    );
    editor.set_content(&format!("[{}]", vec!["1"; 10_001].join(",")));
    editor.set_selection(Position::new(0, 1), Position::new(0, 2));
    let original = editor.state().cursor.clone();
    assert!(
        editor
            .similar_syntax_selections(SyntaxSimilarity::Kind)
            .expect_err("limit")
            .contains("10,000")
    );
    assert_eq!(editor.state().cursor, original);
}
