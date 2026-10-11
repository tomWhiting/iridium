//! Manifest-backed delimiter rules are separate from edit construction.
use iridium_lang::Language;
use crate::{Document, Position, HighlightType};
use super::multi_char_pairs::MultiCharRules;

// ========== Character classes and pair tables ==========

/// Every delimiter pair this editor knows how to type, in bit order.
///
/// Six because that is the whole single-character set: three brackets and
/// three quotes, which close with themselves. Multi-character openers —
/// Python's `"""`, Rust's `r#"`, the `/* */` seven languages declare — need
/// longest-match against the text before the caret and cannot be spelled as a
/// `(char, char)` at all; they are read straight off the manifest by
/// [`super::multi_char_pairs`], and `docs/design/AUTO-PAIR-MAP.md` §10 is the
/// design.
pub(super) const PAIRS: [(char, char); 6] = [
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('"', '"'),
    ('\'', '\''),
    ('`', '`'),
];

/// What the document's language says about each of [`PAIRS`]: which it
/// auto-closes, and which Enter expands into an indented block.
///
/// ⭐ **The rules are the language's, not this module's.** Every vendored
/// manifest declares a `brackets` table, and they disagree with a fixed set in
/// ways a user feels: Rust declares no `'`, because a lifetime is not a
/// character literal, so a fixed table turns every `<'a>` into `<'a'>`. JSON
/// and YAML declare no backtick; `go.mod` declares only `(`; Markdown declares
/// its three quotes `close = false`, because an apostrophe in prose is an
/// apostrophe; and `gitcommit`, `regex`, `jsdoc` and `diff` set `newline` on
/// nothing at all — a commit message is not a place to grow a three-line block
/// out of a parenthesis.
///
/// ⚠️ **The two answers are independent.** Rust declares `<`→`>` with
/// `close = false, newline = true`: do not type the closer for me, but do
/// expand the block if I typed it myself.
///
/// Two bitmasks over six positions rather than two sets: `Copy`, two bytes,
/// and built once per keystroke rather than consulted per cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PairRules {
    /// Which pairs typing the opener auto-closes.
    closes: u8,
    /// Which pairs Enter expands into an indented block.
    expands: u8,
    /// The characters a closer may be inserted in front of — the manifest's
    /// `autoclose_before`. `None` is a language that has not said, and places
    /// no restriction.
    ///
    /// A borrowed `&'static str` rather than a set: manifests live in a
    /// `LazyLock` for the life of the process, the sets are at most nine
    /// characters, and a linear scan of nine characters on the path that
    /// inserts one is not worth a `HashSet` allocated per keystroke.
    close_before: Option<&'static str>,
    /// Which pairs each of [`SUPPRESSIBLE_SCOPES`] switches off, one mask per
    /// scope in that array's order — the manifest's `not_in`.
    ///
    /// An array indexed by the scope list rather than two named fields, so
    /// teaching the editor a third scope is one entry there and one arm in
    /// [`HighlightType::is_within`] rather than a new field and a new branch in
    /// every reader.
    suppressed: [u8; SUPPRESSIBLE_SCOPES.len()],
    /// The rows whose opener is more than one character — Python's `"""` and
    /// its twelve string prefixes, Rust's three raw-string widths, the `/*`
    /// seven languages share.
    ///
    /// Not a mask, because these are not in [`PAIRS`] and could not be: a
    /// six-slot table of `(char, char)` cannot spell `r#"`→`"#`. A borrowed
    /// manifest instead, which is one nullable pointer and keeps this type
    /// `Copy`. See [`super::multi_char_pairs`].
    multi_char: MultiCharRules,
}

/// The syntax scopes a manifest may switch a pair off inside, as the manifests
/// spell them.
///
/// ⭐ **This is a vocabulary, not a policy**, and it is checked rather than
/// assumed: `not_in_names_only_the_two_scopes_the_editor_can_translate` in
/// `iridium-lang` fails the build if any vendored manifest names something not
/// listed here. Without that ratchet a third value would be read, matched
/// against nothing, and silently ignored — the rule declared and not applied.
///
/// Each name must be one [`HighlightType::is_within`] understands; that
/// function is the translation from the manifests' vocabulary into the capture
/// vocabulary, and the two must grow together.
pub(super) const SUPPRESSIBLE_SCOPES: [&str; 2] = ["string", "comment"];

impl PairRules {
    /// Every pair in [`PAIRS`] closes; the three brackets expand.
    ///
    /// What a document with no language, or a language whose manifest says
    /// nothing about brackets, gets — which is exactly what every document
    /// got before the manifests were read.
    pub(super) const ALL: Self = Self {
        closes: (1 << PAIRS.len()) - 1,
        // Enter expansion has never applied to quotes, and no manifest asks it
        // to: `newline = true` appears on no quote row anywhere in the tree.
        expands: BRACKET_MASK,
        // No restriction on what a closer may be inserted in front of, which is
        // what every document got before `autoclose_before` was read.
        close_before: None,
        // And nothing suppressed anywhere, which is what every document got
        // before `not_in` was read. ⚠️ Absent means "nowhere", not
        // "everywhere": the opposite would switch auto-closing off inside every
        // string in every language that has not said.
        suppressed: [0; SUPPRESSIBLE_SCOPES.len()],
        // And no multi-character rows: those exist only in a manifest, and
        // this is what a document without one gets.
        multi_char: MultiCharRules::NONE,
    };

    /// The rules for `document`'s language.
    ///
    /// [`Self::ALL`] when there is no language, no vendored manifest for it,
    /// or no `brackets` key in that manifest — three distinct ways of *not
    /// having said*, all of which leave behaviour exactly as it was. A
    /// manifest that declares an empty table has said "none", and gets none;
    /// `diff` is the one that does.
    pub(super) fn for_document(document: &Document) -> Self {
        Self::for_language(document.language().and_then(Language::from_id))
    }

    pub(super) fn for_position(document: &Document, position: Position) -> Self {
        Self::for_language(document.language_at(position))
    }

    fn for_language(language: Option<Language>) -> Self {
        let Some(manifest) = language.and_then(Language::manifest) else {
            return Self::ALL;
        };
        let (Some(closing), Some(expanding)) =
            (manifest.auto_close_pairs(), manifest.block_expand_pairs())
        else {
            return Self::ALL;
        };

        let closes = mask_of(closing);

        Self {
            closes,
            // Intersected with the brackets because Enter expansion has never
            // applied to quotes and this slice does not change that. No
            // manifest sets `newline` on a quote, so the intersection removes
            // nothing today; it is here so that one appearing upstream is a
            // deliberate decision rather than a silent behaviour change.
            expands: mask_of(expanding) & BRACKET_MASK,
            // Read independently of `brackets`: `gitcommit` declares six
            // brackets and no `autoclose_before`, so a language can close pairs
            // and place no restriction on where.
            close_before: manifest.autoclose_before(),
            // Intersected with `closes` for the same reason `pairs_suppressed_in`
            // already filters on `close`: a pair this editor never auto-inserts
            // cannot be suppressed from auto-inserting. Belt and braces, and
            // cheap — a bit that could never be read is a bit that could be
            // read wrongly later.
            suppressed: SUPPRESSIBLE_SCOPES
                .map(|scope| manifest.pairs_suppressed_in(scope).map_or(0, mask_of) & closes),
            // Read from the manifest independently of `brackets`' single-
            // character rows, and resolved to `NONE` here when the language
            // declares none — which is most of them, and is what lets every
            // later reader skip the walk on one null test.
            multi_char: MultiCharRules::for_manifest(manifest),
        }
    }

    /// The language's multi-character rows, for [`super::multi_char_pairs`].
    pub(super) const fn multi_char(self) -> MultiCharRules {
        self.multi_char
    }

    /// Whether a closer may be inserted with `next` directly after the caret —
    /// the manifest's `autoclose_before` rule.
    ///
    /// `next` is the character the closer would be pushed in front of, or
    /// `None` at end of line.
    ///
    /// ⚠️ **End of line and whitespace are treated as permitting, and that is
    /// an assumption rather than a reading of a specification.** No vendored
    /// set contains a space, a tab or anything standing for a line ending, so
    /// taken literally every set forbids auto-closing at the end of a line —
    /// which is where brackets are most often typed, and which would make the
    /// feature read as broken. The inference is that these sets constrain what
    /// the closer may be *displaced in front of*, and empty space displaces
    /// nothing.
    ///
    /// If a language ever ships a set where that inference is wrong, this
    /// comment is what should lead someone to it. Ruled by Tom on 8 Aug 2026;
    /// see `docs/design/AUTO-PAIR-MAP.md` S-5 and decision B-5.
    pub(super) fn permits_close_before(self, next: Option<char>) -> bool {
        let Some(allowed) = self.close_before else {
            // The language has not said. Close in front of anything, exactly as
            // this editor did before manifests were read.
            return true;
        };
        // `None` is end of line, and permits.
        next.is_none_or(|c| c.is_whitespace() || allowed.contains(c))
    }

    /// Whether typing `c` could be suppressed by *any* scope this language
    /// names — the cheap question, asked before the expensive one.
    ///
    /// ⚠️ Load-bearing for latency, not just tidiness. The scope lookup behind
    /// [`Self::suppressed_in`] materialises the whole document as a `String`
    /// and runs a tree-sitter query; this is a bitwise test against a value
    /// already in a register. A language whose manifest suppresses nothing —
    /// which is most of them — must never pay the first, and it is this that
    /// stops it.
    pub(super) fn has_any_suppression(self, c: char) -> bool {
        self.suppressed
            .iter()
            .any(|&mask| Self::lookup(mask, c).is_some())
    }

    /// Whether this language switches `c`'s pair off inside `highlight`.
    ///
    /// The manifest names a scope; `highlight` is what the caret actually sits
    /// in. [`HighlightType::is_within`] is the translation between the two, and
    /// it is a containment test rather than an equality one because an escape
    /// sequence is still inside a string and a doc comment is still a comment.
    pub(super) fn suppressed_in(self, c: char, highlight: HighlightType) -> bool {
        SUPPRESSIBLE_SCOPES
            .iter()
            .zip(self.suppressed)
            .any(|(scope, mask)| Self::lookup(mask, c).is_some() && highlight.is_within(scope))
    }

    /// Whether the pair at `index` in [`PAIRS`] is in `mask`.
    const fn has(mask: u8, index: usize) -> bool {
        mask & (1_u8 << index) != 0
    }

    /// The closing character for an auto-pair opener this language closes.
    ///
    /// `None` for every other character, including an opener the language
    /// declines to close.
    pub(super) fn close(self, c: char) -> Option<char> {
        Self::lookup(self.closes, c)
    }

    /// The closing bracket for an opener this language expands on Enter.
    ///
    /// Quotes never reach this — see [`Self::ALL`].
    fn block_close(self, c: char) -> Option<char> {
        Self::lookup(self.expands, c)
    }

    /// The closer paired with `c` when `c`'s pair is in `mask`.
    fn lookup(mask: u8, c: char) -> Option<char> {
        PAIRS
            .iter()
            .enumerate()
            .find_map(|(index, &(open, close))| {
                (open == c && Self::has(mask, index)).then_some(close)
            })
    }

    /// Whether `c` can close a pair this language closes — the
    /// single-character half of the skip-over rule.
    ///
    /// Restricted by the same mask as [`Self::close`], and deliberately: a
    /// closer this language never auto-inserts is a character the user typed,
    /// and stepping over it instead of writing it would drop input.
    ///
    /// This *is* §10.4's rule for a one-character closer, which ends itself
    /// with an empty prefix and so needs no text before the caret examined.
    /// [`multi_char_pairs::steps_over`] asks it first for exactly that reason,
    /// which is what leaves the behaviour that was already here unchanged by
    /// construction.
    pub(super) fn is_closer(self, c: char) -> bool {
        PAIRS
            .iter()
            .enumerate()
            .any(|(index, &(_, close))| close == c && Self::has(self.closes, index))
    }

    /// Whether typing `c` engages auto-pair handling at all.
    ///
    /// The six single-character openers and closers this language declares, plus
    /// the final character of every multi-character opener and of every
    /// **steppable** multi-character closer. Both are *reachability* conditions
    /// rather than decisions: `/*` ends in `*` and `"#` ends in `#`, neither of
    /// which is in any pair, so without them the insertion of §10.3 and the
    /// skip-over of §10.4 could never fire however correct they are.
    ///
    /// ⚠️ Final characters only, and closers only where steppable
    /// (`multi_char_pairs::is_steppable`, private): `" */"` is not, so `/` triggers
    /// nowhere; a whole-delimiter rule would put a **space** in seven languages.
    pub(super) fn is_trigger(self, c: char) -> bool {
        self.close(c).is_some()
            || self.is_closer(c)
            || self.multi_char.completes_opener(c)
            || self.multi_char.completes_closer(c)
    }
}

/// The positions in [`PAIRS`] that are brackets rather than quotes.
pub(super) const BRACKET_MASK: u8 = 0b0000_0111;

/// Folds `(open, close)` pairs into a [`PAIRS`] bitmask, dropping any pair
/// this editor cannot type.
///
/// `<`/`>` is declared by five languages and is dropped here: typing `<` must
/// keep inserting a bare `<` until the auto-pair map's S-2 is ruled on.
fn mask_of(pairs: impl Iterator<Item = (char, char)>) -> u8 {
    let mut mask = 0_u8;
    for (open, close) in pairs {
        if let Some(index) = PAIRS
            .iter()
            .position(|&(known_open, known_close)| known_open == open && known_close == close)
        {
            // The index came from `position` over a six-element array, so the
            // shift is in range by construction.
            mask |= 1_u8 << index;
        }
    }
    mask
}

