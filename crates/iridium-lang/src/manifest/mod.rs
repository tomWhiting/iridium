//! The vendored language manifests — what each language is called, what files
//! it claims, and how it is commented.
//!
//! Every language directory under `languages/queries/` carries a `config.toml`
//! beside its queries, describing the language itself rather than its syntax
//! tree: display name, grammar, file associations, comment tokens, brackets.
//! Until now nothing read them, and Iridium maintained its own hand-written
//! copies of the same facts in Rust.
//!
//! Two copies of one fact diverge, and these already had: the answer to "what
//! extensions does C++ claim" existed in a `match` arm in this crate and in a
//! vendored file two directories away, with nothing comparing them. The
//! manifests are the ones that come from upstream and get refreshed, so they
//! are the ones that should win.
//!
//! This module is deliberately only the *reader*. Nothing here changes what
//! Iridium believes about a language; the hard-coded tables are replaced one at
//! a time, each against a test asserting the manifest agrees with what it
//! replaces.
//!
//! ⚠️ **The reader is narrower than the file**, and the gap is worth stating so
//! nobody assumes a key is honoured because the manifest declares it. `Fields`
//! deserializes seven keys; `brackets`, `autoclose_before`, `collapsed_placeholder`
//! and the rest are parsed past and dropped. Comment tokens have been replaced
//! that way (#66). **Brackets have not** — the editor's auto-pair table is six
//! hard-coded characters, the same in every language, and it disagrees with
//! seventeen of the twenty manifests. `docs/design/AUTO-PAIR-MAP.md` names each
//! divergence and prices the slices.

mod embedded;
mod schema;

pub use embedded::{all, by_id, parse_failures};
pub use schema::{DelimiterPair, Manifest};

#[cfg(test)]
mod tests;
