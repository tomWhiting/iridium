//! Search diagnostics must survive refresh and reach hosts without losing edits.
use crate::search::{SearchOptions, SearchState};
use crate::{Document, Editor, EditorEvent, KeyCode, KeyEvent};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn failed_query_retains_diagnostic_and_clears_every_old_range() -> Result<(), String> {
    let document = Document::new("foo foo");
    let mut state = SearchState::new();
    state.find_all("foo", &SearchOptions::default(), &document)?;
    assert_eq!(state.match_count(), 2);
    let result = state.find_all("[", &SearchOptions::regex_mode(), &document);
    assert!(result.is_err());
    assert_eq!(state.error(), result.as_ref().err().map(String::as_str));
    let diagnostic = state.error().map(str::to_owned);
    assert!(state.matches.is_empty());
    assert_eq!(state.current_match, None);
    assert!(state.refresh(&document));
    assert_eq!(state.error(), diagnostic.as_deref());
    state.find_all("foo", &SearchOptions::default(), &document)?;
    assert_eq!(state.error(), None);
    assert_eq!(state.match_count(), 2);
    Ok(())
}

#[test]
fn empty_search_and_close_clear_the_diagnostic() {
    let document = Document::new("foo");
    let mut state = SearchState::new();
    assert!(
        state
            .find_all("[", &SearchOptions::regex_mode(), &document)
            .is_err()
    );
    assert!(
        state
            .find_all("", &SearchOptions::regex_mode(), &document)
            .is_ok()
    );
    assert_eq!(state.error(), None);
    assert!(!state.is_active);
    assert!(
        state
            .find_all("[", &SearchOptions::regex_mode(), &document)
            .is_err()
    );
    state.clear();
    assert_eq!(state.error(), None);
    assert!(!state.refresh(&document));
}

fn error_events(editor: &mut Editor) -> Arc<AtomicUsize> {
    let count = Arc::new(AtomicUsize::new(0));
    let events = Arc::clone(&count);
    editor.add_listener(move |event| {
        if let EditorEvent::SearchUpdated {
            error: Some(message),
            match_count,
            current_index,
            scope_ranges: None,
        } = event
        {
            assert!(!message.is_empty());
            assert_eq!(*match_count, 0);
            assert_eq!(*current_index, None);
            events.fetch_add(1, Ordering::SeqCst);
        }
    });
    count
}

#[test]
fn every_failing_search_entry_point_emits_its_diagnostic() -> Result<(), String> {
    let mut editor = Editor::with_defaults();
    editor.set_content("foo [");
    let errors = error_events(&mut editor);
    assert!(editor.find("[", &SearchOptions::regex_mode()).is_err());
    assert_eq!(errors.load(Ordering::SeqCst), 1);
    assert!(editor.update_search("(").is_err());
    assert_eq!(errors.load(Ordering::SeqCst), 2);
    editor.set_search_options(SearchOptions::default())?;
    assert!(editor.search_state().error().is_none());
    assert!(
        editor
            .set_search_options(SearchOptions::regex_mode())
            .is_err()
    );
    assert_eq!(errors.load(Ordering::SeqCst), 3);
    assert!(editor.select_search_matches().is_err());
    assert_eq!(errors.load(Ordering::SeqCst), 4);
    Ok(())
}

#[test]
fn refresh_failure_reports_error_while_edit_and_undo_remain_valid() -> Result<(), String> {
    let mut editor = Editor::with_defaults();
    editor.set_content("[ foo");
    editor.find("[", &SearchOptions::default())?;
    editor.state_mut().search.options.regex = true;
    let errors = error_events(&mut editor);
    editor.handle_key(&KeyEvent::simple(KeyCode::Char('X')));
    assert_eq!(editor.content(), "X[ foo");
    assert!(editor.search_state().error().is_some());
    assert_eq!(errors.load(Ordering::SeqCst), 1);
    assert!(editor.undo());
    assert_eq!(editor.content(), "[ foo");
    assert_eq!(errors.load(Ordering::SeqCst), 2);
    editor.set_content("new");
    assert_eq!(errors.load(Ordering::SeqCst), 3);
    assert!(editor.search_state().matches.is_empty());
    editor.close_search();
    assert!(editor.search_state().error().is_none());
    Ok(())
}

#[test]
fn replacing_current_restarts_at_first_refreshed_match_and_undo_is_exact() -> Result<(), String> {
    let mut editor = Editor::with_defaults();
    editor.set_content("foo foo foo");
    editor.find("foo", &SearchOptions::default())?;
    editor.goto_next_match();
    assert!(editor.replace_current_match("X"));
    assert_eq!(editor.content(), "foo X foo");
    assert_eq!(editor.current_match_index(), Some(0));
    assert_eq!(editor.search_match_count(), 2);
    assert!(editor.undo());
    assert_eq!(editor.content(), "foo foo foo");
    assert_eq!(editor.search_match_count(), 3);
    Ok(())
}
