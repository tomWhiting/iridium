//! Tests for the incremental brace scanner.
//!
//! Two things have to hold, and they are tested separately because they fail
//! separately. The scan must produce what the whole-document scan it replaces
//! produced — that is [`legacy`], a verbatim copy of that scanner used as an
//! oracle. And the incremental update must produce what a full rescan of the
//! same text produces, after every edit of an arbitrary sequence, *while
//! actually being incremental* — a cache that quietly rebuilt every time would
//! pass any equivalence check by comparing a full scan with itself.

mod incremental;

use super::*;

/// The whole-document brace scanner this module replaced, transcribed.
///
/// Kept as an oracle rather than as production code. The replacement is an
/// optimisation of an existing behaviour, and the only way to show it did not
/// change that behaviour is to have the behaviour still available to compare
/// against. It is character-based and allocates a `Vec<char>` per line, exactly
/// as it did.
mod legacy {
    /// A foldable brace pair as the old scanner reported one.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) struct Region {
        pub(super) start_line: usize,
        pub(super) end_line: usize,
    }

    pub(super) fn detect_brace_folds(source: &str) -> Vec<Region> {
        let mut regions = Vec::new();
        let mut brace_stack: Vec<(usize, usize)> = Vec::new();
        let mut in_string = false;
        let mut string_char = '"';
        let mut in_block_comment = false;
        let mut prev_char = '\0';

        for (line_num, line) in source.lines().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;

            while i < chars.len() {
                let ch = chars[i];
                let next_char = chars.get(i + 1).copied().unwrap_or('\0');

                if !in_string && !in_block_comment && ch == '/' && next_char == '*' {
                    in_block_comment = true;
                    i += 2;
                    continue;
                }

                if in_block_comment && ch == '*' && next_char == '/' {
                    in_block_comment = false;
                    i += 2;
                    continue;
                }

                if in_block_comment {
                    i += 1;
                    continue;
                }

                if !in_string && ch == '/' && next_char == '/' {
                    break;
                }

                if ch == '"' || ch == '\'' {
                    if in_string {
                        if ch == string_char && prev_char != '\\' {
                            in_string = false;
                        }
                    } else {
                        in_string = true;
                        string_char = ch;
                    }
                    prev_char = ch;
                    i += 1;
                    continue;
                }

                if in_string {
                    prev_char = ch;
                    i += 1;
                    continue;
                }

                if ch == '{' {
                    brace_stack.push((line_num, i));
                }

                if ch == '}' {
                    if let Some((start_line, _)) = brace_stack.pop() {
                        if line_num > start_line {
                            regions.push(Region {
                                start_line,
                                end_line: line_num,
                            });
                        }
                    }
                }

                prev_char = ch;
                i += 1;
            }
        }

        regions.sort_by_key(|r| r.start_line);
        regions
    }
}

/// The regions a full scan of `source` produces.
pub(super) fn full_scan(source: &str) -> Vec<BraceRegion> {
    let mut cache = BraceFoldCache::new();
    cache.rebuild(source);
    cache.regions().to_vec()
}

/// The old scanner's answer for `source`, in this module's terms.
pub(super) fn legacy_scan(source: &str) -> Vec<BraceRegion> {
    legacy::detect_brace_folds(source)
        .into_iter()
        .map(|region| BraceRegion {
            start_line: region.start_line,
            end_line: region.end_line,
        })
        .collect()
}

/// Sources chosen to reach every branch of the scanner.
fn corpus() -> Vec<String> {
    vec![
        String::new(),
        "\n".to_owned(),
        "{}".to_owned(),
        "{\n}".to_owned(),
        "{\n  {\n    {\n    }\n  }\n}".to_owned(),
        "fn a() {\n  b();\n}\n\nfn c() {\n  d();\n}\n".to_owned(),
        // Braces the scanner must not see.
        "let s = \"{\";\nlet t = '}';\n{\n  x\n}\n".to_owned(),
        "// { not a brace\n{\n  y\n}\n".to_owned(),
        "/* {\n   } */\n{\n  z\n}\n".to_owned(),
        // An escaped quote keeps the string open across the brace.
        "\"a\\\" { b\"\n{\n  c\n}\n".to_owned(),
        // A backslash at end of line escapes a quote at the start of the next.
        "\"open \\\n\" { \n}\n".to_owned(),
        // Unbalanced in both directions.
        "}\n}\n{\n".to_owned(),
        "{\n{\n{\n".to_owned(),
        // Non-ASCII, so byte scanning and character scanning must agree.
        "let é = \"ü{ü\";\n{\n  ø\n}\n".to_owned(),
        "// ünïcödé {\n{\n}\n".to_owned(),
        // Windows line endings.
        "{\r\n  a\r\n}\r\n".to_owned(),
        // Several pairs closing on one line, which the stable sort must order.
        "{ {\n} }\n".to_owned(),
        // A comment delimiter split so the two-byte skips are exercised.
        "/*/ { */\n{\n}\n".to_owned(),
    ]
}

#[test]
fn a_full_scan_reproduces_the_scanner_it_replaced() {
    for source in corpus() {
        assert_eq!(
            full_scan(&source),
            legacy_scan(&source),
            "the replacement disagrees with the scanner it replaced on {source:?}"
        );
    }
}

#[test]
fn a_full_scan_reproduces_the_scanner_it_replaced_on_generated_sources() {
    for seed in 0..400u64 {
        let source = generated_source(seed);
        assert_eq!(
            full_scan(&source),
            legacy_scan(&source),
            "seed {seed}: the replacement disagrees with the scanner it replaced"
        );
    }
}

/// A deterministic pseudo-random source built from the scanner's own alphabet.
///
/// Random *text* would almost never contain a string that swallows a brace or a
/// comment that hides one, which are the cases that separate the two scanners.
/// This draws from the tokens that matter.
fn generated_source(seed: u64) -> String {
    const TOKENS: [&str; 14] = [
        "{", "}", "\n", " ", "a", "\"", "'", "//", "/*", "*/", "\\", "\r\n", "é", "\t",
    ];

    let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    let mut out = String::new();
    for _ in 0..80 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let index = usize::try_from((state >> 33) % TOKENS.len() as u64).unwrap_or(0);
        out.push_str(TOKENS[index]);
    }
    out
}

#[test]
fn multi_line_pairs_fold_and_single_line_pairs_do_not() {
    assert_eq!(full_scan("{}\n"), Vec::new());
    assert_eq!(
        full_scan("{\n}\n"),
        vec![BraceRegion {
            start_line: 0,
            end_line: 1
        }]
    );
}
