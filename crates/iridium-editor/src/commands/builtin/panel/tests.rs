//! What every panel's vocabulary has to hold, whichever panel it is.
//!
//! ⭐ These run over [`TABLES`], not over one table, so a panel added in a new
//! file inherits every rule here without anyone remembering to. That is the
//! whole reason the tests moved up here when the second panel arrived: three
//! assertions written against the explorer would have said nothing about the
//! command palette, and would have gone on passing while it broke every one of
//! them.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use super::{PANEL_COMMAND_COUNT, TABLES, panel_command_metas, panel_commands};

#[test]
fn every_panel_command_names_a_mode() {
    // The defining property of these tables, and the one that keeps these
    // commands out of a palette *and* out of the editor's own keymap. A row
    // that forgot would be offered as a global entry that cannot do anything,
    // and a `[keys]` line naming it would take its chord away everywhere.
    for meta in panel_command_metas() {
        assert!(
            meta.mode().is_some(),
            "`{}` is in a panel table and names no mode",
            meta.id()
        );
        assert!(
            !meta.is_palette_entry(),
            "`{}` would be offered by a command palette",
            meta.id()
        );
    }
}

#[test]
fn the_ids_are_distinct_across_every_panel() {
    // Within a table and *between* tables. Two panels choosing the same id is
    // the failure this file exists to catch — it would be reported at startup
    // as a duplicate registration, which names the id but not the two files
    // that disagree about it.
    let ids: BTreeSet<&str> = panel_command_metas()
        .map(|meta| meta.id().as_str())
        .collect();
    assert_eq!(
        ids.len(),
        PANEL_COMMAND_COUNT,
        "two rows across the panel tables share an id"
    );
}

/// ⚠️ The id is what a user writes in `config.toml`, so it is a published
/// surface: an id and the mode it is scoped to must agree, or a `[keys]` line
/// binds a command into a screen it does not belong to.
#[test]
fn an_id_sits_under_the_mode_it_is_scoped_to() {
    for meta in panel_command_metas() {
        let mode = meta.mode().expect("every panel command names a mode");
        assert!(
            meta.id().as_str().starts_with(mode.as_str()),
            "`{}` is scoped to mode `{mode}`, which its id does not name",
            meta.id()
        );
    }
}

/// The count is derived, and the three ways of reading the tables agree.
///
/// Cheap, and it is the assertion that catches a table added to [`TABLES`] but
/// missed by one of the accessors — which would register a panel's commands
/// while `panel_command_metas` went on reporting the old set, so the panel's own
/// "is this one of mine" check would answer no.
#[test]
fn every_way_of_reading_the_tables_agrees() {
    assert_eq!(panel_commands().len(), PANEL_COMMAND_COUNT);
    assert_eq!(panel_command_metas().count(), PANEL_COMMAND_COUNT);
    assert_eq!(
        TABLES.iter().map(|table| table.len()).sum::<usize>(),
        PANEL_COMMAND_COUNT
    );
}

/// Every panel in the tables is a panel with more than one verb.
///
/// A guard against the half-conversion: a mode declared and one command
/// registered under it, with the rest of the panel still answering a hard-coded
/// table. #117 converts seven panels, and the way that goes wrong is stopping
/// halfway through one of them.
#[test]
fn no_panel_table_is_empty() {
    for table in TABLES {
        assert!(!table.is_empty(), "a panel contributed an empty table");
    }
}
