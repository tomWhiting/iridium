//! Recency tests: ordering, deduplication, eviction and the bonus curve.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{CommandMru, MRU_CAPACITY};
use crate::commands::CommandId;

fn id(text: &str) -> CommandId {
    CommandId::new(text.to_owned())
}

fn recorded(mru: &CommandMru) -> Vec<&str> {
    mru.iter().map(CommandId::as_str).collect()
}

#[test]
fn an_empty_history_gives_nothing_a_bonus() {
    let mru = CommandMru::new();
    assert!(mru.is_empty());
    assert_eq!(mru.len(), 0);
    assert_eq!(mru.rank("lines.join"), None);
    assert_eq!(mru.bonus("lines.join"), 0);
}

#[test]
fn the_most_recently_recorded_command_leads() {
    let mut mru = CommandMru::new();
    mru.record(&id("lines.join"));
    mru.record(&id("clipboard.copy"));

    assert_eq!(recorded(&mru), vec!["clipboard.copy", "lines.join"]);
    assert_eq!(mru.rank("clipboard.copy"), Some(0));
    assert_eq!(mru.rank("lines.join"), Some(1));
    assert!(mru.bonus("clipboard.copy") > mru.bonus("lines.join"));
}

#[test]
fn re_recording_moves_a_command_up_without_duplicating_it() {
    let mut mru = CommandMru::new();
    for command in ["a.one", "b.two", "c.three"] {
        mru.record(&id(command));
    }
    mru.record(&id("a.one"));

    assert_eq!(recorded(&mru), vec!["a.one", "c.three", "b.two"]);
    assert_eq!(mru.len(), 3, "re-recording must not add a second copy");
}

#[test]
fn running_one_command_repeatedly_does_not_evict_the_others() {
    // The failure this guards: a plain push would fill all sixteen slots with the
    // same id, and every other command would lose its recency.
    let mut mru = CommandMru::new();
    mru.record(&id("b.other"));
    for _ in 0..MRU_CAPACITY * 2 {
        mru.record(&id("a.repeated"));
    }

    assert_eq!(recorded(&mru), vec!["a.repeated", "b.other"]);
    assert_eq!(mru.rank("b.other"), Some(1));
}

#[test]
fn the_oldest_entry_is_evicted_at_capacity() {
    let mut mru = CommandMru::new();
    let ids: Vec<CommandId> = (0..MRU_CAPACITY + 4)
        .map(|index| id(&format!("test.command{index}")))
        .collect();
    for command in &ids {
        mru.record(command);
    }

    assert_eq!(mru.len(), MRU_CAPACITY);
    assert_eq!(mru.rank("test.command0"), None, "the oldest must be gone");
    assert_eq!(mru.rank(ids[ids.len() - 1].as_str()), Some(0));
    assert_eq!(mru.rank(ids[4].as_str()), Some(MRU_CAPACITY - 1));
}

#[test]
fn the_bonus_decays_strictly_and_stays_positive() {
    let mut mru = CommandMru::new();
    let ids: Vec<CommandId> = (0..MRU_CAPACITY)
        .map(|index| id(&format!("test.command{index}")))
        .collect();
    for command in &ids {
        mru.record(command);
    }

    let bonuses: Vec<i32> = ids.iter().rev().map(|c| mru.bonus(c.as_str())).collect();
    assert_eq!(bonuses.len(), MRU_CAPACITY);
    assert!(
        bonuses.windows(2).all(|pair| pair[0] > pair[1]),
        "every rank must be distinguishable: {bonuses:?}"
    );
    assert!(
        bonuses.iter().all(|&bonus| bonus > 0),
        "even the oldest remembered command outranks one never used: {bonuses:?}"
    );
    assert_eq!(mru.bonus("test.neverUsed"), 0);
}

#[test]
fn clearing_forgets_everything() {
    let mut mru = CommandMru::new();
    mru.record(&id("lines.join"));
    mru.clear();

    assert!(mru.is_empty());
    assert_eq!(mru.bonus("lines.join"), 0);
    assert_eq!(recorded(&mru), Vec::<&str>::new());
}

#[test]
fn owned_and_static_ids_are_the_same_entry() {
    // A palette invocation arrives with an owned id from the host; the registry
    // holds a `'static` one. Recency keyed on the pointer rather than the text
    // would quietly record two entries for one command.
    let mut mru = CommandMru::new();
    mru.record(&CommandId::from_static("lines.join"));
    mru.record(&id("lines.join"));

    assert_eq!(mru.len(), 1);
    assert_eq!(mru.rank("lines.join"), Some(0));
}
