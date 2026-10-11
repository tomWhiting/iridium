# Generated parser sources

The Just, Typst and WGSL C parsers are copied byte for byte from Chiron commit
`2026e6b106ba0defbbfba87ed053f52ed61b1f3b` in `crates/syntax/grammars`.
Their generated C sources are pinned in this tree rather than regenerated during a build.
The parser ABI is checked by the native syntax tests. Each directory includes its upstream license.

Upstream projects:

- Just: https://github.com/IndianBoy42/tree-sitter-just
- Typst: https://github.com/uben0/tree-sitter-typst
- WGSL: https://github.com/szebniok/tree-sitter-wgsl

The corresponding language query files come from the same Chiron commit.
XML and DTD highlight queries come from tree-sitter-xml tag `v0.7.0`.
Dependency revisions and published crate versions are pinned by `Cargo.lock`.
