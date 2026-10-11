//! Overlapping literal matching without constructing a large regex automaton.
//!
//! Unicode simple folding comes from the same regex-syntax tables used by the
//! regex engine. Folding identifies scalar equivalence classes; it never rewrites
//! the haystack, expands a scalar into several characters or normalizes accents.
//! KMP carries reusable prefix lengths so repeated prefixes require linear work.
//! The offset ring holds only one needle's worth of original byte positions.

use std::collections::HashMap;
use std::ops::Range;
use std::str::CharIndices;

use regex_syntax::hir::{ClassUnicode, ClassUnicodeRange};

/// A non-regex query compiled to scalar equivalence classes and prefix lengths.
#[derive(Debug)]
pub(super) struct FoldedLiteral {
    needle: Vec<char>,
    prefixes: Vec<usize>,
    folds: HashMap<char, char>,
    ascii: [Option<char>; 128],
}

impl FoldedLiteral {
    pub(super) fn new(query: &str) -> Result<Self, String> {
        let mut folds = HashMap::new();
        let mut needle = Vec::new();
        for character in query.chars() {
            if let Some(&canonical) = folds.get(&character) {
                needle.push(canonical);
                continue;
            }
            let mut class = ClassUnicode::new([ClassUnicodeRange::new(character, character)]);
            class
                .try_case_fold_simple()
                .map_err(|error| format!("Cannot build literal search: {error}"))?;
            folds.extend(class.iter().flat_map(|range| {
                (range.start()..=range.end()).map(|equivalent| (equivalent, character))
            }));
            needle.push(character);
        }
        let prefixes = prefix_lengths(&needle);
        let mut ascii = [None; 128];
        for (&character, &canonical) in &folds {
            if character.is_ascii() {
                ascii[character as usize] = Some(canonical);
            }
        }
        folds.retain(|character, _| !character.is_ascii());
        Ok(Self {
            needle,
            prefixes,
            folds,
            ascii,
        })
    }

    /// Common source-code characters need one table lookup, without hashing.
    fn fold(&self, character: char) -> Option<char> {
        if character.is_ascii() {
            self.ascii[character as usize]
        } else {
            self.folds.get(&character).copied()
        }
    }

    /// Original UTF-8 ranges, including overlaps. An empty query has no matches.
    pub(super) fn matches<'a>(&'a self, text: &'a str) -> Matches<'a> {
        Matches {
            matcher: self,
            text: text.char_indices(),
            starts: vec![0; self.needle.len()],
            write_at: 0,
            matched: 0,
        }
    }
}

/// Longest proper prefix that is also a suffix for each needle prefix.
fn prefix_lengths(needle: &[char]) -> Vec<usize> {
    let mut prefixes = vec![0; needle.len()];
    for index in 1..needle.len() {
        let mut matched = prefixes[index - 1];
        while matched > 0 && needle[index] != needle[matched] {
            matched = prefixes[matched - 1];
        }
        if needle[index] == needle[matched] {
            matched += 1;
        }
        prefixes[index] = matched;
    }
    prefixes
}

/// Streaming state: memory is bounded by the query, independent of document size.
#[derive(Debug)]
pub(super) struct Matches<'a> {
    matcher: &'a FoldedLiteral,
    text: CharIndices<'a>,
    starts: Vec<usize>,
    write_at: usize,
    matched: usize,
}

impl Iterator for Matches<'_> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        let length = self.matcher.needle.len();
        if length == 0 {
            return None;
        }
        for (offset, character) in self.text.by_ref() {
            let Some(folded) = self.matcher.fold(character) else {
                self.matched = 0;
                continue;
            };
            self.starts[self.write_at] = offset;
            self.write_at += 1;
            if self.write_at == length {
                self.write_at = 0;
            }
            while self.matched > 0 && folded != self.matcher.needle[self.matched] {
                self.matched = self.matcher.prefixes[self.matched - 1];
            }
            if folded == self.matcher.needle[self.matched] {
                self.matched += 1;
            }
            if self.matched == length {
                // The ring now holds the last `length` scalar starts. Its next
                // write slot is the oldest, so it identifies this match's start.
                let start = self.starts[self.write_at];
                self.matched = self.matcher.prefixes[length - 1];
                return Some(start..offset + character.len_utf8());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
