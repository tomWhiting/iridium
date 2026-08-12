//! What is shown before anything is written to the disk.
//!
//! Ruling 7: a confirmation lists the **actual operations, by name**. Not a
//! count, and not the edited rows — the operations, in the order they will
//! happen, because the order is where the surprises are. A rename cycle
//! routed through a temporary looks like three renames and two of them name a
//! file nobody typed; someone about to press `y` should see that.
//!
//! # Nothing is hidden without saying so
//!
//! A panel has a fixed number of rows and a plan has no fixed length, so a
//! long plan cannot be shown whole. The failure to avoid is a list that
//! *looks* complete and is not — a confirmation that quietly drops the delete
//! at the bottom is worse than no confirmation, because it was read and
//! trusted. When the list is cut, the last row says how many operations are
//! not on it.
//!
//! # Paths are read against the folder the panel is showing
//!
//! Twenty absolute paths down a narrow panel are unreadable, and every one of
//! them repeats the same prefix. What distinguishes two rows is where they sit
//! *inside* the project, so that is what is drawn.
//!
//! # The three verbs borrow the three colours the editor already has
//!
//! `change_added`, `change_modified` and `change_deleted` are what the gutter
//! uses for the same three ideas. Reusing them is not thrift: a delete that
//! reads the same as a rename is the one distinction in this panel worth
//! spending a colour on, and it is a distinction the theme has already made.

use std::path::Path;

use iridium_editor::theme::{Color, Theme};

use super::plan::{EditedRow, Operation, Plan, Refusal};
use super::rows::truncate;
use crate::line::LineBuilder;
use crate::row::{PanelRow, Span};

/// The column the path starts at, so the verbs form a left rail and the names
/// line up down the panel instead of stepping in and out.
const NAME_COLUMN: usize = 8;

/// Rows spent on something other than the list: the summary, the two rules
/// around the list, and the prompt.
const CHROME_ROWS: usize = 4;

/// The confirmation for `plan`, in at most `height` rows.
///
/// `root` is the folder the panel is showing; paths are drawn relative to it.
#[must_use]
pub fn plan_rows(
    plan: &Plan,
    root: &Path,
    theme: &Theme,
    width: usize,
    height: usize,
) -> Vec<PanelRow> {
    if plan.is_empty() {
        return vec![
            text_row("Nothing to apply", theme.editor.line_number, width),
            PanelRow::separator(),
            text_row("esc to go back", theme.editor.line_number, width),
        ];
    }

    let mut rows = vec![
        text_row(&plan.summary(), theme.editor.foreground, width),
        PanelRow::separator(),
    ];
    let budget = height.saturating_sub(CHROME_ROWS).max(1);
    extend_capped(
        &mut rows,
        plan.operations.len(),
        budget,
        theme,
        width,
        |index| operation_row(&plan.operations[index], root, theme, width),
    );
    rows.push(PanelRow::separator());
    rows.push(text_row(
        "y to apply    n to go back",
        theme.editor.line_number,
        width,
    ));
    rows
}

/// The refusals for a buffer that cannot be applied at all.
///
/// Every one of them, capped the same way: someone who mistyped two names
/// should see both rather than fix one and be told about the other.
#[must_use]
pub fn refusal_rows(
    edited: &[EditedRow],
    refusals: &[Refusal],
    theme: &Theme,
    width: usize,
    height: usize,
) -> Vec<PanelRow> {
    let mut rows = vec![
        text_row(
            &format!(
                "{} row{} cannot be applied",
                refusals.len(),
                if refusals.len() == 1 { "" } else { "s" }
            ),
            theme.editor.change_deleted,
            width,
        ),
        PanelRow::separator(),
    ];
    let budget = height.saturating_sub(CHROME_ROWS).max(1);
    extend_capped(&mut rows, refusals.len(), budget, theme, width, |index| {
        refusal_row(edited, &refusals[index], theme, width)
    });
    rows.push(PanelRow::separator());
    rows.push(text_row("esc to go back", theme.editor.line_number, width));
    rows
}

/// Appends `total` rows built by `row`, or as many as `budget` allows with the
/// last one saying what is missing.
///
/// The cap is the whole point of the function existing: a caller that
/// truncated its own list would have to remember to say so, and the one that
/// forgot would produce a confirmation that reads as complete.
fn extend_capped(
    rows: &mut Vec<PanelRow>,
    total: usize,
    budget: usize,
    theme: &Theme,
    width: usize,
    row: impl Fn(usize) -> PanelRow,
) {
    if total <= budget {
        rows.extend((0..total).map(row));
        return;
    }
    // One row of the budget goes to saying what was left out, so the shown
    // count is one less than the space available.
    let shown = budget.saturating_sub(1);
    rows.extend((0..shown).map(row));
    let hidden = total - shown;
    rows.push(text_row(
        &format!("… and {hidden} more not shown"),
        theme.editor.change_deleted,
        width,
    ));
}

/// One operation, as `verb  name`.
fn operation_row(operation: &Operation, root: &Path, theme: &Theme, width: usize) -> PanelRow {
    let (verb, color, name) = match operation {
        Operation::Create { path, directory } => (
            "create",
            theme.editor.change_added,
            format!(
                "{}{}",
                within(path, root),
                if *directory { "/" } else { "" }
            ),
        ),
        Operation::Rename { from, to } => (
            "rename",
            theme.editor.change_modified,
            format!("{} → {}", within(from, root), within(to, root)),
        ),
        Operation::Delete { path } => ("delete", theme.editor.change_deleted, within(path, root)),
    };

    let mut line = LineBuilder::new(width);
    line.push(verb, color);
    line.pad_to(NAME_COLUMN, theme.editor.line_number);
    let space = width.saturating_sub(line.used());
    line.push(&truncate(&name, space), theme.editor.foreground);
    PanelRow::new(line.finish())
}

/// One refusal, as `name — reason`.
///
/// Named rather than numbered: "row 3" is a number the user has to count to,
/// and the row they need is the one they can read.
fn refusal_row(edited: &[EditedRow], refusal: &Refusal, theme: &Theme, width: usize) -> PanelRow {
    let name = edited
        .get(refusal.row)
        .map_or("(a row that is no longer there)", |row| row.name.as_str());
    let shown = if name.is_empty() {
        "(a row with no name)"
    } else {
        name
    };

    let mut line = LineBuilder::new(width);
    line.push(
        &truncate(shown, width.saturating_sub(NAME_COLUMN)),
        theme.editor.foreground,
    );
    line.push("  ", theme.editor.line_number);
    let space = width.saturating_sub(line.used());
    line.push(
        &truncate(&refusal.reason, space),
        theme.editor.change_deleted,
    );
    PanelRow::new(line.finish())
}

/// A whole row of one colour, cut to the width.
fn text_row(text: &str, color: Color, width: usize) -> PanelRow {
    PanelRow::new(vec![Span::new(truncate(text, width), color)])
}

/// A path as the confirmation shows it, relative to the folder on screen.
///
/// Falls back to the whole path rather than to an empty string: a row that
/// somehow *is* the root would otherwise be listed as nothing at all, which is
/// the one rendering of a destructive operation that must never happen.
fn within(path: &Path, root: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if relative.as_os_str().is_empty() {
        return path.to_string_lossy().into_owned();
    }
    relative.to_string_lossy().into_owned()
}
