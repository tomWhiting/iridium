//! Line-block transformations: sort, reverse, dedupe, trim.
//!
//! Each function takes a block of text that has already been expanded to whole
//! lines by the caller, plus the line ending to re-join with, and returns the
//! transformed block. Keeping the line ending an explicit parameter rather than
//! sniffing it per call is what stops these verbs from silently converting a
//! CRLF file to LF one selection at a time.
//!
//! # A note on the trailing line ending
//!
//! A block that ends with a line ending keeps one, and a block that does not
//! does not. That single rule is what makes these verbs safe to run on the last
//! line of a file (which has no trailing ending) and on any interior selection
//! (which does) without special-casing either at the call site.

/// Splits a block into its lines and whether it ended with a line ending.
///
/// Recognises `\r\n`, `\n` and `\r`, so a block is split the same way whatever
/// the file's convention — only the *re-join* uses the caller's line ending.
fn split(block: &str) -> (Vec<&str>, bool) {
    let mut lines: Vec<&str> = Vec::new();
    let mut rest = block;
    let mut trailing = false;

    while !rest.is_empty() {
        if let Some(index) = rest.find(['\n', '\r']) {
            lines.push(&rest[..index]);
            // Consume the ending, treating CRLF as one.
            let after = &rest[index..];
            let skip = if after.starts_with("\r\n") { 2 } else { 1 };
            rest = &after[skip..];
            trailing = true;
        } else {
            lines.push(rest);
            trailing = false;
            break;
        }
    }

    (lines, trailing)
}

/// Re-joins `lines`, restoring a trailing line ending when the block had one.
fn join(lines: &[&str], line_ending: &str, trailing: bool) -> String {
    let mut out = lines.join(line_ending);
    if trailing {
        out.push_str(line_ending);
    }
    out
}

/// Sorts the block's lines.
///
/// The order is Rust's `str` ordering — by Unicode scalar value — which is
/// stable, total, and the same in every face. It is deliberately *not*
/// locale-aware collation: a sort whose result depends on the machine's locale
/// would make "same document, same command, different answer" possible, and
/// this editor's whole architecture exists to prevent that.
///
/// `reverse` sorts descending. An unstable sort is used deliberately: the
/// elements are whole lines, so two "equal" lines are the same text, and no
/// observer can tell which one moved.
#[must_use]
pub fn sort(block: &str, line_ending: &str, reverse: bool) -> String {
    let (mut lines, trailing) = split(block);
    lines.sort_unstable();
    if reverse {
        lines.reverse();
    }
    join(&lines, line_ending, trailing)
}

/// Reverses the block's line order.
#[must_use]
pub fn reverse(block: &str, line_ending: &str) -> String {
    let (mut lines, trailing) = split(block);
    lines.reverse();
    join(&lines, line_ending, trailing)
}

/// Removes duplicate lines, keeping the **first** occurrence of each.
///
/// Order is otherwise preserved — this is dedupe, not sort-and-dedupe, because
/// the two are separate commands and combining them would make it impossible
/// to dedupe a list whose order matters.
#[must_use]
pub fn dedupe(block: &str, line_ending: &str) -> String {
    let (lines, trailing) = split(block);
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let kept: Vec<&str> = lines.into_iter().filter(|line| seen.insert(line)).collect();
    join(&kept, line_ending, trailing)
}

/// Removes trailing whitespace from every line.
///
/// Blank lines become empty rather than being removed: this verb tidies lines,
/// it does not delete them.
#[must_use]
pub fn trim_trailing(block: &str, line_ending: &str) -> String {
    let (lines, trailing) = split(block);
    let trimmed: Vec<&str> = lines.iter().map(|line| line.trim_end()).collect();
    join(&trimmed, line_ending, trailing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorting_is_ascending_by_scalar_value() {
        assert_eq!(
            sort("gamma\nalpha\nbeta", "\n", false),
            "alpha\nbeta\ngamma"
        );
        assert_eq!(sort("gamma\nalpha\nbeta", "\n", true), "gamma\nbeta\nalpha");
        // Uppercase sorts before lowercase, and that is a *decision*: it is
        // scalar order, not locale collation, so every face agrees.
        assert_eq!(sort("beta\nAlpha", "\n", false), "Alpha\nbeta");
    }

    #[test]
    fn a_trailing_line_ending_is_preserved_and_one_absent_is_not_invented() {
        assert_eq!(sort("b\na\n", "\n", false), "a\nb\n");
        assert_eq!(sort("b\na", "\n", false), "a\nb");
        assert_eq!(reverse("a\nb\n", "\n"), "b\na\n");
        assert_eq!(reverse("a\nb", "\n"), "b\na");
    }

    #[test]
    fn the_callers_line_ending_is_the_one_written_back() {
        // The block arrives with LF; the document is CRLF; CRLF is what comes
        // back. Sniffing per call is how a file ends up with both.
        assert_eq!(sort("b\na", "\r\n", false), "a\r\nb");
        // And a CRLF block is split correctly, not left with stray `\r`.
        assert_eq!(sort("b\r\na", "\r\n", false), "a\r\nb");
        assert_eq!(reverse("a\r\nb\r\n", "\r\n"), "b\r\na\r\n");
    }

    #[test]
    fn a_lone_carriage_return_is_a_line_ending_too() {
        assert_eq!(sort("b\ra", "\n", false), "a\nb");
    }

    #[test]
    fn dedupe_keeps_the_first_occurrence_and_the_rest_of_the_order() {
        assert_eq!(dedupe("b\na\nb\nc\na", "\n"), "b\na\nc");
        // Whitespace makes lines distinct — collapsing them would be a
        // different (and lossy) verb.
        assert_eq!(dedupe("a\na \n a", "\n"), "a\na \n a");
    }

    #[test]
    fn trim_empties_blank_lines_rather_than_removing_them() {
        assert_eq!(trim_trailing("a  \n\t\nb\t", "\n"), "a\n\nb");
        assert_eq!(trim_trailing("   ", "\n"), "");
    }

    #[test]
    fn a_single_line_block_survives_every_verb() {
        for output in [
            sort("only", "\n", false),
            sort("only", "\n", true),
            reverse("only", "\n"),
            dedupe("only", "\n"),
            trim_trailing("only", "\n"),
        ] {
            assert_eq!(output, "only");
        }
    }

    #[test]
    fn an_empty_block_stays_empty() {
        for output in [
            sort("", "\n", false),
            reverse("", "\n"),
            dedupe("", "\n"),
            trim_trailing("", "\n"),
        ] {
            assert_eq!(output, "");
        }
    }

    #[test]
    fn blank_lines_are_lines() {
        // A block of nothing but line endings has real (empty) lines in it, and
        // sorting must not silently drop them.
        assert_eq!(sort("b\n\na\n", "\n", false), "\na\nb\n");
        assert_eq!(reverse("a\n\nb", "\n"), "b\n\na");
    }
}
