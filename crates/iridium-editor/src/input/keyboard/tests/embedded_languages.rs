//! Language-aware typing through the reversible keyboard command path.

use super::*;

fn type_at(language: &str, source: &str, at: Position, character: char) -> (Document, CursorState) {
    let mut document = Document::new(source);
    document.set_language(Some(language.to_owned()));
    let mut cursor = CursorState::at(at);
    let mut handler = KeyboardHandler::new();
    let command = press(
        &mut handler,
        &KeyEvent::simple(KeyCode::Char(character)),
        &mut document,
        &mut cursor,
    );
    assert!(
        command.is_some(),
        "typing must produce a reversible command"
    );
    (document, cursor)
}

#[test]
fn xml_opening_tag_inserts_its_named_closer() {
    let (document, cursor) = type_at("xml", "<message", Position::new(0, 8), '>');
    assert_eq!(document.text(), "<message></message>");
    assert_eq!(cursor.primary.head, Position::new(0, 9));
}

#[test]
fn xml_attributes_and_namespaces_keep_the_tag_name() {
    let source = "<ns:message value=\"a>b\"";
    let (document, cursor) = type_at("xml", source, Position::new(0, source.chars().count()), '>');
    assert_eq!(document.text(), "<ns:message value=\"a>b\"></ns:message>");
    assert_eq!(cursor.primary.head.column, source.chars().count() + 1);
}

#[test]
fn xml_self_closing_and_closing_tags_stay_single() {
    for source in [
        "<message /",
        "</message",
        "<?xml version=\"1.0\"?",
        "<!-- note --",
    ] {
        let (document, _) = type_at("xml", source, Position::new(0, source.chars().count()), '>');
        assert_eq!(document.text(), format!("{source}>"));
    }
}

#[test]
fn html_void_elements_do_not_receive_end_tags() {
    let (document, _) = type_at("html", "<input", Position::new(0, 6), '>');
    assert_eq!(document.text(), "<input>");
    let (document, _) = type_at("xml", "<input", Position::new(0, 6), '>');
    assert_eq!(document.text(), "<input></input>");
}

#[test]
fn markdown_fences_use_their_language_quote_rules() {
    let source = "```json\n\n```";
    let (document, cursor) = type_at("markdown", source, Position::new(1, 0), '"');
    assert_eq!(document.text(), "```json\n\"\"\n```");
    assert_eq!(cursor.primary.head, Position::new(1, 1));
}

#[test]
fn markdown_xml_fences_close_tags() {
    let (document, cursor) = type_at("markdown", "~~~xml\n<note\n~~~", Position::new(1, 5), '>');
    assert_eq!(document.text(), "~~~xml\n<note></note>\n~~~");
    assert_eq!(cursor.primary.head, Position::new(1, 6));
}

#[test]
fn markdown_prose_does_not_pair_quotes() {
    let (document, _) = type_at("markdown", "plain ", Position::new(0, 6), '"');
    assert_eq!(document.text(), "plain \"");
}
