//! Auto-pair rows whose opener is more than one character: Python's `"""`,
//! `'''` and twelve string prefixes, Rust's three raw-string widths, and the
//! `/* */` seven languages share.
//!
//! The auto-pair map's slice S-3. These cannot live in
//! [`behaviors::PAIRS`](super::behaviors::PAIRS) — a fixed six-slot table of
//! `(char, char)` cannot spell `r#"`→`"#`, and the first `char` of `r#"` would
//! pair a bare `r`. What they need instead is a **longest match against the
//! text before the caret** at the moment the opener's final character is typed,
//! which is what this module is.
//!
//! ⚠️ **The rules are borrowed, never copied.** [`PairRules`] is `Copy` and
//! rebuilt on every keystroke, so it cannot own a `Vec` of pairs. It carries a
//! [`MultiCharRules`] instead — one nullable pointer at a manifest that lives
//! in a `LazyLock` for the life of the process — and `iridium_lang` stays the
//! owner of the rules rather than having them copied into a second shape.
//!
//! Three questions are asked of those rows, one function each:
//! [`insertion_for`] when an opener's final character is typed,
//! [`steps_over`] when a character already present would otherwise be
//! duplicated (`docs/design/AUTO-PAIR-MAP.md` §10.4, as ruling B-10 narrows it
//! in §10.9), and [`written_closer_around`] when Backspace lands on a closer
//! this editor wrote (§10.7 step 5, as ruling B-8 corrects it in §10.8 and
//! ruling B-13 re-founds it in §10.12).
//!
//! ⚠️ Skip-over acts on a pair that **already exists** rather than on one being
//! typed, which is why it consults neither `autoclose_before` nor the
//! apostrophe rule: the question there is whether the delimiter is already
//! present, not what follows it. That is ruling B-7, and it is the same
//! exemption the single-character skip-over and backspace collapse have always
//! had.
//!
//! ⭐⭐ **The delete path asks the manifest nothing at all, which is ruling
//! B-13 (§10.12).** It used to match a row against the buffer and then ask that
//! row's `not_in` — first at the caret, then in front of the opener — trying to
//! reconstruct whether the editor had written the closer in front of the caret
//! or merely found it there. Every such probe is a proxy for history read out of
//! a buffer that does not record history, and each position proposed was refuted
//! by a case at a scope boundary it could not see: five languages regressed on
//! the last one, and Rust could not be fixed by any of them because its nested
//! block comments leave the buffer unparseable. So the question is no longer
//! asked of the buffer. [`super::AutoPairInsertion`] remembers what the
//! insertion wrote, and [`written_closer_around`] is handed that closer or
//! nothing.
//!
//! ⚠️ **The single-character collapse deliberately keeps its buffer-based
//! rule**, in [`super::backspace_pairs::backspace_edits_with_pairs`], and the
//! asymmetry is the point: over-deleting one character is a nuisance and
//! over-deleting four is data loss, so only the second has to be certain.

use core::fmt;

use iridium_lang::manifest::{DelimiterPair, Manifest};

use crate::document::{Document, Position, Range, Selection};
use crate::editor::CaretScopes;

use super::behaviors::{PairRules, SUPPRESSIBLE_SCOPES};
use super::editing::{CaretPlacement, CursorEdit};

/// The multi-character auto-close rows of one language, or nothing.
///
/// `Some` **only** when the manifest declares at least one such row, which is
/// what makes "skip the walk entirely for a language that declares none" a
/// single null test rather than a scan: of the twenty-two vendored manifests,
/// eight declare one and the rest do not.
///
/// A newtype rather than a bare `Option<&'static Manifest>` in [`PairRules`]
/// so that type keeps its derived [`PartialEq`] and a [`fmt::Debug`] that
/// prints something a person can read — a derived one would dump every parsed
/// field of the manifest into the middle of a test failure.
#[derive(Clone, Copy)]
pub(super) struct MultiCharRules(Option<&'static Manifest>);

impl MultiCharRules {
    /// No multi-character rows: no language, no manifest, or a manifest that
    /// declares none.
    pub(super) const NONE: Self = Self(None);

    /// The rows `manifest` declares, or [`Self::NONE`] when it declares none.
    ///
    /// The emptiness is resolved here, once per keystroke, rather than left for
    /// every later reader to rediscover.
    pub(super) fn for_manifest(manifest: &'static Manifest) -> Self {
        let Some(mut rows) = manifest.multi_char_close_pairs() else {
            // No `brackets` key at all — the language has not said.
            return Self::NONE;
        };
        if rows.next().is_some() {
            Self(Some(manifest))
        } else {
            Self::NONE
        }
    }

    /// Whether this language declares no multi-character row at all.
    ///
    /// The test that skips the walk, and the reason [`Self::for_manifest`]
    /// resolves emptiness once: fourteen of the twenty-two vendored manifests
    /// answer true here, and none of them should pay to find out twice.
    pub(super) const fn declares_none(self) -> bool {
        self.0.is_none()
    }

    /// Whether `c` is the final character of some declared opener — the
    /// characters S-3 adds to the auto-pair trigger set.
    ///
    /// ⚠️ This is the slice's one *reachability* change.
    /// [`PairRules::is_trigger`] gates whether typing a character enters
    /// auto-pair handling at all, and `*` is in no pair: without this the `/*`
    /// rows could never fire however correct the matching below is.
    ///
    /// Only the opener's final character, and only the opener's. The `/*` rows
    /// close with `" */"` — a leading space — and a rule reading whole
    /// delimiters would make a space an auto-pair keystroke in seven languages.
    pub(super) fn completes_opener(self, c: char) -> bool {
        let Some(manifest) = self.0 else {
            return false;
        };
        manifest
            .multi_char_close_pairs()
            .is_some_and(|mut rows| rows.any(|row| row.start().ends_with(c)))
    }

    /// Whether `c` is the final character of some **steppable** declared closer
    /// — the other half of the trigger set S-3 adds, and what makes
    /// [`steps_over`] reachable at all.
    ///
    /// `#` is in no pair, so without this `r#"abc"|#` typing `#` never enters
    /// auto-pair handling and §10.4's rule cannot fire however correct it is.
    ///
    /// ⚠️ The final character, and **only** the final character. `"#`
    /// contributes `#`; a rule reading every character of a closer would put a
    /// **space** in the trigger set in seven languages, routing every space
    /// anybody types through the auto-pair machinery.
    ///
    /// ⚠️ And only from a closer [`is_steppable`] admits. `" */"` therefore
    /// contributes **nothing**, so `/` is not a trigger anywhere — ruling B-10
    /// as §10.9 reverses it.
    pub(super) fn completes_closer(self, c: char) -> bool {
        let Some(manifest) = self.0 else {
            return false;
        };
        manifest.multi_char_close_pairs().is_some_and(|mut rows| {
            rows.any(|row| is_steppable(row.end()) && row.end().ends_with(c))
        })
    }

    /// Whether `before` followed by `typed` ends a **steppable** closer
    /// declared on one of these rows.
    ///
    /// The multi-character half of §10.4's rule. The single-character half is
    /// [`PairRules::is_closer`] — the same question asked of the same manifest
    /// through the six-slot mask, so the split is an artefact of how
    /// [`PairRules`] stores the rows rather than two rules.
    ///
    /// A one-character closer strips to an empty prefix and so matches on
    /// `typed` alone, which is §10.4's "a single-character closer satisfies
    /// this with an empty prefix" written out. Every such closer in the
    /// vendored tree — Python's `"` and `'`, reached through the twelve prefix
    /// rows — is already in the single-character set, so this adds nothing to
    /// it today; it is written to the rule rather than to the data because the
    /// next vendor refresh need not be so kind.
    ///
    /// ⚠️ [`is_steppable`] filters the rows here as it does in
    /// [`Self::completes_closer`], and both must apply it: the trigger set is
    /// what makes the branch reachable and this is what decides it, so a row
    /// admitted by one and refused by the other would be a rule that half
    /// exists.
    fn ends_closer(self, before: &str, typed: char) -> bool {
        let Some(manifest) = self.0 else {
            return false;
        };
        manifest.multi_char_close_pairs().is_some_and(|mut rows| {
            rows.any(|row| {
                is_steppable(row.end())
                    && row
                        .end()
                        .strip_suffix(typed)
                        .is_some_and(|prefix| before.ends_with(prefix))
            })
        })
    }

    /// The longest declared opener ending in `typed` whose remaining prefix
    /// `before` ends with.
    ///
    /// ⭐ **Longest wins.** At `rb|` typing `"`, both `b"` (the text ends with
    /// `b`) and `rb"` (it ends with `rb`) match, and only the longer is right.
    ///
    /// ⚠️ In today's data that tie-break is not observable *anywhere*: the one
    /// real contest, `b"` against `rb"`, declares the same closer on both rows,
    /// so shortest and longest write the same text — and under ruling B-8 the
    /// delete path no longer reads an opener's length at all, so it cannot tell
    /// them apart either. It is here because the next vendor refresh need not
    /// be so kind, and because a caller reading the first match would be
    /// relying on manifest order —
    /// [`Manifest::multi_char_close_pairs`] says in as many words that it must
    /// not. What *is* observable is the `ends_with` rule underneath, which
    /// separates Rust's three raw-string widths with no special casing: at
    /// `r##|` typing `"`, `r#"` does not match, because `"r##"` ends with `"##"`
    /// and not with `"r#"`.
    ///
    /// Two rows of equal length can never both match: their prefixes are the
    /// same length and both are a suffix of `before`, so they are the same
    /// prefix and the same opener.
    fn longest_opener(self, before: &str, typed: char) -> Option<DelimiterPair<'static>> {
        let mut best: Option<DelimiterPair<'static>> = None;
        for row in self.0?.multi_char_close_pairs()? {
            // Every opener reported here is at least two characters, so the
            // remainder after stripping `typed` is never empty and
            // `ends_with` can never match vacuously.
            let Some(prefix) = row.start().strip_suffix(typed) else {
                continue;
            };
            if !before.ends_with(prefix) {
                continue;
            }
            if best.is_none_or(|found| found.start().len() < row.start().len()) {
                best = Some(row);
            }
        }
        best
    }
}

impl fmt::Debug for MultiCharRules {
    /// Prints the language the rows came from, not the rows.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("MultiCharRules")
            .field(&self.0.map(Manifest::id))
            .finish()
    }
}

impl PartialEq for MultiCharRules {
    /// By language identifier, which is unique across the vendored tree and so
    /// identifies the manifest exactly as its address would.
    fn eq(&self, other: &Self) -> bool {
        self.0.map(Manifest::id) == other.0.map(Manifest::id)
    }
}

impl Eq for MultiCharRules {}

/// The edit for typing `c` at a collapsed caret when `c` completes a
/// multi-character opener, or `None` when it completes none or a manifest rule
/// refuses the close.
///
/// `None` is "this branch has nothing to say", and the caller falls through to
/// the apostrophe rule and the single-character path exactly as if this module
/// did not exist. That is what keeps a refusal here indistinguishable from a
/// refusal there: at `r#x|` typing `"`, `autoclose_before` turns both down and
/// the user gets one quote either way.
///
/// ⭐ The third element of a `Some` is **the closer this edit writes**, which
/// the caller records against the caret it lands at (ruling B-13, §10.12); it
/// is the only evidence Backspace will have that the editor wrote it. Returning
/// it here rather than re-deriving it later is what keeps "what was written"
/// and "what may be taken back" the same value rather than two readings of the
/// manifest that can disagree.
///
/// Both manifest rules gate this, in the same order and for the same reasons
/// the single-character path applies them — `autoclose_before` first because it
/// is a nine-character scan, `not_in` last because resolving a syntax scope can
/// cost a document-sized allocation and a tree-sitter query.
pub(super) fn insertion_for(
    pairs: PairRules,
    document: &Document,
    sel: &Selection,
    c: char,
    scopes: &CaretScopes<'_>,
) -> Option<(CursorEdit, CaretPlacement, &'static str)> {
    // Before the line is materialised: most languages declare no such row, and
    // most keystrokes in the ones that do complete no opener.
    let rules = pairs.multi_char();
    if rules.declares_none() {
        return None;
    }
    let line = document.line(sel.head.line)?;
    let opener = rules.longest_opener(text_before(&line, sel.head.column), c)?;

    if !pairs.permits_close_before(char_at_column(&line, sel.head.column)) {
        return None;
    }
    if suppressed_at(opener, document, sel.head, scopes) {
        return None;
    }

    // The closer is written exactly as the manifest spells it, leading space
    // and all: `/*` is meant to leave the caret in `/*| */`.
    let closer = opener.end();
    let text = format!("{c}{closer}");
    Some((
        CursorEdit::replace(sel.range(), text),
        CaretPlacement::collapsed(c.len_utf8()),
        closer,
    ))
}

/// Whether typing `c` at `head` steps over the character already there instead
/// of writing a second one.
///
/// §10.4's rule in full: `c` must equal the character at the caret, **and** the
/// text before the caret plus `c` must end a closer this language declares.
///
/// ⭐ **What was already here is unchanged by construction rather than by
/// care.** A one-character closer ends itself with an empty prefix, so
/// [`PairRules::is_closer`] alone decides every case it decided before, and it
/// is asked first. `"""abc|"""` therefore still steps one quote at a time: a
/// rule that skipped a whole closer would jump all three and the user's next
/// two keystrokes would open a new pair.
///
/// ⚠️ **"Declares" means declares *and would insert*.** Markdown declares
/// `<`→`>` with `close = true` and [`mask_of`](super::behaviors) drops it,
/// because typing `<` must keep inserting a bare `<` until the auto-pair map's
/// S-2 is ruled on. Stepping over a `>` this editor never wrote would drop a
/// character the user typed — the exact argument [`PairRules::is_closer`]
/// already makes for the mask. So the second half reads the multi-character
/// rows, every one of which this slice does insert, and not the unfiltered
/// closer list.
///
/// ⚠️ **A closer must also be one this rule could reach.** `" */"` cannot be
/// stepped over from the caret its own insertion leaves — `/*| */` typing `*`
/// finds a **space** at the caret — so it contributes nothing here at all and
/// `/` is not a trigger. That is ruling B-10 as §10.9 reverses it, and the
/// reasoning is in [`is_steppable`]: a generalisation that is unreachable in
/// the case it exists to serve, and reachable only in false-positive cases,
/// buys nothing and costs a swallowed keystroke.
pub(super) fn steps_over(pairs: PairRules, document: &Document, head: Position, c: char) -> bool {
    let single = pairs.is_closer(c);
    let rules = pairs.multi_char();
    // Neither half can match `c` at all: no line to materialise, no rows to
    // walk. Most keystrokes that reach auto-pair handling end here.
    if !single && !rules.completes_closer(c) {
        return false;
    }
    let Some(line) = document.line(head.line) else {
        return false;
    };
    if char_at_column(&line, head.column) != Some(c) {
        return false;
    }
    single || rules.ends_closer(text_before(&line, head.column), c)
}

/// The range one Backspace deletes when the editor wrote `closer` at `head`,
/// or `None` when the buffer no longer shows it there.
///
/// ⭐⭐ **Ruling B-13 (§10.12): the caller has already established that the
/// editor wrote this closer, and this function does not re-derive it.** `closer`
/// comes from [`super::AutoPairInsertion`] — the record the insertion left,
/// matched against the current document identity, content revision and cursor
/// state — so the manifest is not consulted here at all. Every earlier attempt
/// to answer *"did the editor write this"* from the buffer (`not_in` at the
/// caret, then in front of the opener) was a proxy for history in a place that
/// keeps none, and each was refuted by a case at a scope boundary it could not
/// see.
///
/// ⭐ **Ruling B-8 (§10.8) still bounds the left side at one character.** The
/// range is the closer the editor wrote plus the single character the user typed
/// to trigger it — never the prefix that was already in the buffer. Backspace
/// immediately after an auto-pair puts the buffer back exactly where it was,
/// which is the identity `(|)` has always had and the one every user relies on
/// without naming it:
///
/// | before the keystroke | after typing | this deletes, leaving |
/// | --- | --- | --- |
/// | `print(f\|)` | `print(f"\|")` | `print(f\|)` |
/// | `r#\|` | `r#"\|"#` | `r#\|` |
/// | `""\|` | `"""\|"""` | `""\|` |
/// | `print(verb\|)` | `print(verb"\|")` | `print(verb\|)` |
///
/// A whole-pair collapse would leave `print()` for the second of those, taking
/// an `f` the user typed before the editor had done anything, and `print(ve)`
/// for the last, taking two characters out of an identifier. Nothing can know
/// how much of a prefix the user wanted gone; one more Backspace is cheap and a
/// re-typed character the editor ate is not.
///
/// ⚠️ The `starts_with` test cannot fail while the record matches — the record
/// pins the exact revision the insertion produced, so these are the bytes it
/// wrote. It is kept because it is three instructions and it is the difference
/// between a wrong answer and no answer if that ever stops being true.
///
/// ⚠️ Asked **before** the single-character collapse, and it must stay there:
/// at `r#"|"#` the caret has a quote on each side, so the older rule matches
/// too and would win on order alone, leaving a mangled `r##`. Where the two
/// agree — `print(f"|")`, where both take one character each way — the order
/// costs nothing.
///
/// Line-local, for the reason [`char_at`](super::behaviors) and its sibling
/// are: no vendored delimiter contains a line ending, so a closer found on
/// another line could only ever be a false match.
pub(super) fn written_closer_around(
    document: &Document,
    head: Position,
    closer: &str,
) -> Option<Range> {
    let line = document.line(head.line)?;
    let before = text_before(&line, head.column);
    // `before` is a prefix of `line` cut at a character boundary, so this is
    // the rest of the line without scanning it a second time.
    let after = line.get(before.len()..)?;
    if !after.starts_with(closer) {
        return None;
    }
    // One character to the left, always: the trigger character. The record was
    // taken after an insertion that wrote that character at this caret, so it
    // exists; `checked_sub` is here so that a caret at column zero fails safe
    // rather than panics.
    let start = head.column.checked_sub(1)?;
    let end = head.column.checked_add(closer.chars().count())?;
    Some(Range::new(
        Position::new(head.line, start),
        Position::new(head.line, end),
    ))
}

/// Whether the language switches `opener` off in the scope the caret sits in.
///
/// The same three-step shape as
/// [`PairRules::suppressed_in`](super::behaviors::PairRules), and fail-open at
/// every step for the same reason: under ruling B-6 a scope that cannot be
/// resolved is *unknown*, not *nothing*, and a caller must do what it would
/// have done without asking. The cheap test that the row names any scope at all
/// comes first ([`names_a_scope`]), so a row with no `not_in` never reaches the
/// resolver.
fn suppressed_at(
    opener: DelimiterPair<'_>,
    document: &Document,
    head: Position,
    scopes: &CaretScopes<'_>,
) -> bool {
    if !names_a_scope(opener) {
        return false;
    }
    let Some(byte) = document.position_to_offset(head) else {
        return false;
    };
    suppressed_at_byte(opener, byte, scopes)
}

/// Whether `row` names any scope suppression can act on at all.
///
/// The cheap half of both suppression tests, and it must stay first in both: a
/// row with no `not_in` — or one naming a scope this editor's vocabulary does
/// not carry — never reaches [`CaretScopes::scope_at`], which can cost a
/// document-sized allocation and a tree-sitter query.
fn names_a_scope(row: DelimiterPair<'_>) -> bool {
    SUPPRESSIBLE_SCOPES
        .iter()
        .any(|scope| row.is_suppressed_in(scope))
}

/// Whether the scope at `byte` is one `row` names.
///
/// Fail-open on an unresolved scope: under ruling B-6 that is *unknown*, not
/// *nothing*, and the caller must do what it would have done without asking.
fn suppressed_at_byte(row: DelimiterPair<'_>, byte: usize, scopes: &CaretScopes<'_>) -> bool {
    let Some(highlight) = scopes.scope_at(byte) else {
        return false;
    };
    SUPPRESSIBLE_SCOPES
        .iter()
        .any(|scope| row.is_suppressed_in(scope) && highlight.is_within(scope))
}

/// Whether `closer` can be stepped over from the caret its own insertion
/// leaves.
///
/// ⭐ **Ruling B-10 as §10.9 reverses it.** A closer joins the skip-over trigger
/// set only if it is reachable from that caret, and stepping proceeds character
/// by character from the closer's position zero — so a closer whose first
/// character is a space never is. `/*` + caret + `" */"` puts a **space** at the
/// caret, not a `*`, and reaching the `*` would mean typing the space first,
/// which nobody does.
///
/// So `" */"` contributes nothing to the trigger set and `/` is not a trigger,
/// while `"#` still contributes: `r#"` + caret + `"#` has a `"` at the caret,
/// which is that closer's own first character, so it is genuinely reachable.
///
/// ⚠️ **The cost of admitting an unreachable closer is a swallowed keystroke**,
/// and it is not hypothetical: in JavaScript `const s = "a *|/b";` typing `/`
/// left the text untouched, because the text before the caret ends with ` *`
/// and a `/` sits at the caret. Skip-over is exempt from `not_in`, so being
/// inside a string literal does not rescue it. Silently discarding a keystroke
/// is a worse failure than writing an unwanted character — the user's
/// correction for the second is Backspace, and for the first it is to wonder
/// whether the keyboard is broken.
///
/// Whitespace rather than a literal space: a leading tab or line ending is
/// unreachable for exactly the same reason, and neither is a character
/// keystroke this path ever sees. The vendored tree's one case is the space in
/// `" */"`.
fn is_steppable(closer: &str) -> bool {
    !closer.starts_with(char::is_whitespace)
}

/// The text on `line` before `column`.
///
/// Line-local, and deliberately: no vendored opener contains a line ending, so
/// a prefix reaching onto the line above could only ever produce a false match.
/// A `column` past the end yields the whole line, matching what the Enter path
/// does with the same question.
fn text_before(line: &str, column: usize) -> &str {
    let end = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(byte, _)| byte);
    &line[..end]
}

/// The character at `column` — the character a closer would be inserted in
/// front of.
///
/// The same answer `behaviors::char_at` gives, asked of a line already in hand
/// rather than by fetching it a second time.
fn char_at_column(line: &str, column: usize) -> Option<char> {
    line.chars().nth(column)
}
