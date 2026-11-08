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

The Luau C parser is copied byte for byte from https://github.com/4teapo/tree-sitter-luau at commit
`4f09f79945de3d5575b079103d86a515030c62ab`, the grammar Zed's Luau extension pins. Its queries, and the Lua
and Nix queries, come from the Zed extensions named in each file's first line. Lua and Nix parse with the
published `tree-sitter-lua` 0.5.0 and `tree-sitter-nix` 0.3.0, which are the same commits those queries pin.
