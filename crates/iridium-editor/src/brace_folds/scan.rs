//! The brace scanner itself: one line at a time, with its state made explicit.
//!
//! The rules are not new. They are the ones the fold detector used when there
//! was no parser — match `{` against `}`, ignore anything inside a string or a
//! comment, and fold only what spans more than one line — transcribed so that
//! the state carried *between* lines is a value rather than a set of locals in
//! one long loop. That is the whole trick behind making the scan incremental:
//! a scan can be resumed from any line whose entry state is known, and two scans
//! that reach the same state over the same text stay identical from there on.
//!
//! # Bytes, not characters
//!
//! Every byte the scanner reacts to — `{`, `}`, `"`, `'`, `/`, `*`, `\` — is
//! ASCII, and every byte of a multi-byte UTF-8 sequence is `>= 0x80`, so no
//! continuation byte can be mistaken for one of them. Scanning bytes therefore
//! produces exactly what scanning characters did, without materialising a
//! `Vec<char>` per line.

/// The state a line scan starts from.
///
/// Deliberately free of line numbers: the whole point is that the state of
/// every line after an edit can be kept as-is when only the line *indices*
/// moved. The open-brace lines that regions are built from are carried
/// separately, in the scanning stack, and reconstructed at the few points that
/// need them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct LineState {
    /// How many `{` are open.
    pub(super) depth: usize,
    /// The quote that opened the string being scanned, or `0` outside a string.
    ///
    /// One byte rather than a `bool` plus a delimiter, because "not in a string"
    /// and "in a string opened by nothing" must not be separately expressible.
    pub(super) string_delimiter: u8,
    /// Whether a `/* ... */` comment is open.
    pub(super) in_block_comment: bool,
    /// The byte before the scan position, which decides whether a quote closing
    /// a string was escaped.
    ///
    /// Carried across line boundaries because the original scanner carried it:
    /// it is not reset per line, so a line ending in a backslash escapes a quote
    /// at the start of the next one.
    pub(super) previous: u8,
}

/// A foldable brace pair, in the terms the detector publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BraceRegion {
    /// Line holding the opening `{`, zero-based.
    pub start_line: usize,
    /// Line holding the matching `}`, zero-based and inclusive.
    pub end_line: usize,
}

/// Scans one line, advancing `state` and `stack` and appending closed regions.
///
/// `stack` holds the line of every currently open `{`, innermost last;
/// `state.depth` is kept equal to its length. A `}` that closes a brace opened
/// on an earlier line appends a region — a pair opened and closed on one line
/// folds nothing, so it is popped without being recorded.
///
/// `line` must not contain a line terminator: it is one item of the same
/// splitting the caller uses to number lines.
pub(super) fn scan_line(
    line: &str,
    line_number: usize,
    state: &mut LineState,
    stack: &mut Vec<usize>,
    out: &mut Vec<super::TrackedBrace>,
) {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        let in_string = state.string_delimiter != 0;

        // A block comment opens. Note `previous` is deliberately not advanced
        // over the delimiter, matching the scanner this replaces.
        if !in_string && !state.in_block_comment && byte == b'/' && next == b'*' {
            state.in_block_comment = true;
            i += 2;
            continue;
        }

        if state.in_block_comment && byte == b'*' && next == b'/' {
            state.in_block_comment = false;
            i += 2;
            continue;
        }

        if state.in_block_comment {
            i += 1;
            continue;
        }

        // A line comment runs to the end of its line, so the rest of the line
        // is not examined at all — and `previous` keeps whatever it held.
        if !in_string && byte == b'/' && next == b'/' {
            break;
        }

        if byte == b'"' || byte == b'\'' {
            if in_string {
                if byte == state.string_delimiter && state.previous != b'\\' {
                    state.string_delimiter = 0;
                }
            } else {
                state.string_delimiter = byte;
            }
            state.previous = byte;
            i += 1;
            continue;
        }

        if in_string {
            state.previous = byte;
            i += 1;
            continue;
        }

        if byte == b'{' {
            stack.push(line_number);
        }

        if byte == b'}' {
            if let Some(start_line) = stack.pop() {
                if line_number > start_line {
                    out.push(super::TrackedBrace {
                        open_line: start_line,
                        open_depth: stack.len(),
                        end_line: line_number,
                    });
                }
            }
        }

        state.previous = byte;
        i += 1;
    }

    state.depth = stack.len();
}

/// Splits `source` the way line numbering is defined, yielding `(byte offset,
/// line)`.
///
/// This is [`str::lines`] with the byte offset of each line kept, which
/// [`str::lines`] discards and a resumable scan cannot do without. The splitting
/// rule is identical — break after `\n`, drop one `\r` immediately before it,
/// and produce no trailing empty line for a source ending in `\n` — because the
/// line numbers this hands out are the ones the published fold regions carry.
pub(super) fn lines_from(source: &str, offset: usize) -> impl Iterator<Item = (usize, &str)> {
    let tail = &source[offset..];
    let mut consumed = 0usize;
    let mut done = tail.is_empty();

    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let rest = &tail[consumed..];
        let start = offset + consumed;
        let Some(index) = rest.find('\n') else {
            done = true;
            return Some((start, rest));
        };

        consumed += index + 1;
        done = consumed >= tail.len();
        let mut line = &rest[..index];
        if line.ends_with('\r') {
            line = &line[..line.len() - 1];
        }
        Some((start, line))
    })
}

/// Whether any byte of `bytes` in `range` is a `\r` not followed by a `\n`.
///
/// The follower is looked up in the whole of `bytes`, not in the range, so a
/// window can be asked about without its answer depending on where the window
/// was cut.
///
/// This is checked because the scanner numbers lines by [`str::lines`], which
/// breaks on `\n` alone, while the document that describes an edit numbers them
/// by ropey's `LF_CR`, which also breaks on a lone `\r`. The two agree exactly
/// when no lone `\r` exists, and an edit described in rows that mean something
/// different from the rows being scanned would move the wrong lines. A source
/// that has one is therefore only ever scanned whole.
pub(super) fn has_lone_carriage_return(bytes: &[u8], range: std::ops::Range<usize>) -> bool {
    let end = range.end.min(bytes.len());
    let start = range.start.min(end);
    (start..end).any(|index| bytes[index] == b'\r' && bytes.get(index + 1) != Some(&b'\n'))
}
