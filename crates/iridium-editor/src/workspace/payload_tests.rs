//! Tests for the per-document payload a face supplies.
//!
//! The single claim under test: **the payload lives and dies with its
//! buffer, not with the tab that happened to show it.** That is the rule a
//! face would otherwise have to reimplement from outside, using only
//! `close`'s return value — which reports whether a *node* went away, not
//! whether a *document* did. The two agree in every workspace where no file
//! is open twice, so the mistake is invisible until it isn't.

use std::cell::Cell;
use std::rc::Rc;

use super::{Node, Workspace};
use crate::editor::EditorConfig;
use crate::theme::Theme;

/// A payload that records its own drop, so the lifecycle can be asserted
/// rather than inferred from a count.
///
/// `Rc<Cell<u32>>` rather than a static counter: tests share a process, and
/// a global would make this one's result depend on which others ran.
#[derive(Debug, Clone, Default)]
struct Tracked {
    drops: Rc<Cell<u32>>,
    label: String,
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.drops.set(self.drops.get().saturating_add(1));
    }
}

fn workspace<T>() -> Workspace<T> {
    Workspace::new(EditorConfig::default(), Theme::default())
}

#[test]
fn the_default_payload_is_the_unit_and_costs_nothing() {
    // `Workspace<()>` must stay the zero-effort spelling, or every existing
    // caller pays for a feature only the faces use.
    let mut workspace: Workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    let document = workspace.node(tab).and_then(Node::document).expect("a tab");

    assert_eq!(workspace.payload(document), Some(&()));
}

#[test]
fn a_payload_opened_with_the_document_is_readable_and_writable() {
    let mut workspace: Workspace<String> = workspace();
    let tab = workspace
        .open_with("a", "a.rs", None, "mine".to_owned())
        .expect("top level");
    let document = workspace.node(tab).and_then(Node::document).expect("a tab");

    assert_eq!(
        workspace.payload(document).map(String::as_str),
        Some("mine")
    );

    workspace
        .payload_mut(document)
        .expect("open")
        .push_str(" and edited");
    assert_eq!(
        workspace.payload(document).map(String::as_str),
        Some("mine and edited")
    );
}

#[test]
fn closing_the_last_tab_onto_a_document_drops_its_payload() {
    let drops = Rc::new(Cell::new(0));
    let mut workspace: Workspace<Tracked> = workspace();
    let tab = workspace
        .open_with(
            "a",
            "a.rs",
            None,
            Tracked {
                drops: Rc::clone(&drops),
                label: "a".to_owned(),
            },
        )
        .expect("top level");

    assert_eq!(drops.get(), 0);
    assert!(workspace.close(tab));
    assert_eq!(
        drops.get(),
        1,
        "the face's per-document state must go when the buffer does"
    );
}

#[test]
fn closing_one_of_two_tabs_onto_a_document_keeps_its_payload() {
    // The case that makes a face-owned second map wrong. Here the tab count
    // drops and the document count does not, so anything keyed by document
    // must survive.
    let drops = Rc::new(Cell::new(0));
    let mut workspace: Workspace<Tracked> = workspace();
    let first = workspace
        .open_with(
            "shared",
            "here.rs",
            None,
            Tracked {
                drops: Rc::clone(&drops),
                label: "shared".to_owned(),
            },
        )
        .expect("top level");
    let document = workspace
        .node(first)
        .and_then(Node::document)
        .expect("a tab");
    workspace
        .open_existing(document, "also", None)
        .expect("the document is open");

    assert!(workspace.close(first));

    assert_eq!(
        drops.get(),
        0,
        "a face keying off `close` returning true would have dropped this"
    );
    assert_eq!(
        workspace.payload(document).map(|p| p.label.as_str()),
        Some("shared")
    );

    // And it does go once the second tab closes.
    let second = workspace.tabs()[0];
    assert!(workspace.close(second));
    assert_eq!(drops.get(), 1);
}

#[test]
fn closing_a_group_drops_the_payloads_of_every_document_inside_it() {
    let drops = Rc::new(Cell::new(0));
    let mut workspace: Workspace<Tracked> = workspace();
    let group = workspace.create_group("work", None).expect("top level");
    for label in ["a", "b", "c"] {
        workspace
            .open_with(
                label,
                format!("{label}.rs"),
                Some(group),
                Tracked {
                    drops: Rc::clone(&drops),
                    label: label.to_owned(),
                },
            )
            .expect("in the group");
    }

    assert!(workspace.close(group));

    assert_eq!(drops.get(), 3, "a subtree close must not leak its buffers");
    assert_eq!(workspace.document_count(), 0);
}

#[test]
fn the_editor_and_its_payload_can_be_borrowed_together() {
    // The reason the pair accessor exists: `editor_mut` and `payload_mut`
    // cannot both be held, and a face applying an edit while consulting its
    // own span index needs exactly that.
    let mut workspace: Workspace<Vec<usize>> = workspace();
    let tab = workspace
        .open_with("hello", "a.rs", None, Vec::new())
        .expect("top level");
    let document = workspace.node(tab).and_then(Node::document).expect("a tab");

    let (editor, payload) = workspace
        .editor_and_payload_mut(document)
        .expect("just opened");
    payload.push(editor.content().len());

    assert_eq!(workspace.payload(document), Some(&vec![5]));
}

#[test]
fn the_active_accessors_follow_the_activation() {
    let mut workspace: Workspace<String> = workspace();
    workspace
        .open_with("a", "a.rs", None, "first".to_owned())
        .expect("top level");
    workspace
        .open_with("b", "b.rs", None, "second".to_owned())
        .expect("top level");

    assert_eq!(
        workspace.active_payload().map(String::as_str),
        Some("first")
    );

    assert!(workspace.next_tab());
    assert_eq!(
        workspace.active_payload().map(String::as_str),
        Some("second")
    );

    workspace
        .active_payload_mut()
        .expect("a tab is active")
        .push('!');
    assert_eq!(
        workspace.active_payload().map(String::as_str),
        Some("second!")
    );
}

#[test]
fn an_empty_workspace_has_no_active_payload_rather_than_panicking() {
    let mut workspace: Workspace<String> = workspace();

    assert_eq!(workspace.active_payload(), None);
    assert_eq!(workspace.active_payload_mut(), None);
    assert!(workspace.active_editor_and_payload_mut().is_none());
}

#[test]
fn a_stale_document_id_resolves_to_no_payload() {
    let mut workspace: Workspace<String> = workspace();
    let tab = workspace
        .open_with("a", "a.rs", None, "gone".to_owned())
        .expect("top level");
    let document = workspace.node(tab).and_then(Node::document).expect("a tab");
    assert!(workspace.close(tab));

    assert_eq!(workspace.payload(document), None);
    assert_eq!(workspace.payload_mut(document), None);
}
