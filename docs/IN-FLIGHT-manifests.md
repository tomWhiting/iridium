# Languages step 1 — reading the vendored manifests

Ground verified 7 Aug 2026 for **#66**, the first step of #63. No code written
yet: the survey turned up one structural question that has to be answered
before the first line, and it is written down here rather than guessed at.

---

## 1. The schema is richer than the map assumed

21 `config.toml` files under
`crates/iridium-syntax/src/languages/queries/<lang>/`, read by nothing. Every
top-level key, counted across all of them:

| key | in n manifests |
| --- | --- |
| `name`, `grammar` | 21 |
| `brackets` | 20 |
| `path_suffixes`, `autoclose_before` | 18 |
| `line_comments` | 17 |
| `debuggers` | 9 |
| `tab_size`, `prettier_parser_name` | 8 |
| `documentation_comment`, `completion_query_characters`, `block_comment` | 7 |
| …23 more, each in 6 or fewer | |

**The map said Zed's schema carries no block-comment pair. That was wrong** —
`block_comment = { start, prefix, end, tab_size }` is there, in 7 of them.

Any reader must **ignore unknown keys**: most of those 23 describe Zed features
Iridium does not have.

## 2. The comment table derives exactly — including the four that look missing

`comments.rs` hard-codes a line token and a block pair for 13 languages. The
manifests reproduce **all of it**, under one rule:

> **block pair = `block_comment` if present, else `documentation_comment`.**

That fallback is load-bearing. `rust`, `go`, `c` and `cpp` carry no
`block_comment` — but all four carry
`documentation_comment = { start = "/*", prefix = "* ", end = "*/" }`, which is
exactly the `("/*", "*/")` the hard-coded table has. Without the fallback those
four would silently lose block-comment toggling.

The precedence also matters in the other direction: `typescript`, `javascript`
and `tsx` carry **both**, and their `documentation_comment` starts `/**` while
their `block_comment` starts `/*`. The hard-coded table says `/*`, so
`block_comment` must win.

Checked language by language against the existing table:

| language | manifest source | pair | matches today? |
| --- | --- | --- | --- |
| Rust, Go, C, Cpp | `documentation_comment` | `/*` `*/` | ✅ |
| TypeScript, JavaScript, Tsx | `block_comment` | `/*` `*/` | ✅ |
| CSS | `block_comment` | `/*` `*/` | ✅ |
| Markdown | `block_comment` | `<!--` `-->` | ✅ |
| Python, Yaml, Bash | neither | none | ✅ |
| JSON | neither | none | ✅ |

Line tokens match too, after `trim_end` — the manifests write `"// "` and
`"# "` with a trailing space, and `comments.rs` documents its token as being
*without* one. Two manifests (`gitcommit`, `gomod`) write theirs without a
space, so trimming is the rule, not a special case.

**One intentional difference, and only one.** Zed's `json/config.toml` says
`line_comments = ["// "]` where Iridium's table says JSON has no comment at
all. Reading the manifest therefore retires **#62** by data rather than by
ruling: `Ctrl+/` starts working in `.jsonc` *and* in `.json`. It becomes Zed's
call rather than a decision talked into Tom, and it becomes overridable in a
file rather than baked into Rust.

**The oracle this refactor needs** follows directly: assert the manifest-derived
table equals the hard-coded one for the twelve unchanged languages, and assert
JSON's single difference explicitly with its reason. Then delete the hard-coded
table — not before.

## 3. `path_suffixes` carries whole file names, not just extensions

`json` claims `flake.lock`. `jsonc` claims `bun.lock`, `devcontainer.json`,
`pyrightconfig.json`, `tsconfig.json`. `markdown` claims `MD` — upper case,
which `from_extension` already handles by lower-casing.

`Language::from_extension` takes an extension and nothing else, so those
entries have nowhere to go today. Matching on the **whole file name as well**
is strictly more capable and is plainly what they are for; it is also a
behaviour change (`bun.lock` starts highlighting) and should be its own commit
with its own test.

## 4. Three manifests are marked `hidden = true`

`jsdoc`, `regex` and `markdown-inline` — the ones that exist to be *injected*
into another language rather than to own a file. A loader must not offer them
as languages a document can be. That is a field, not a guess, which is one more
argument for reading the manifests rather than maintaining a list.

Note `diff`, `gitcommit`, `gomod` and `gowork` are **not** hidden — they are
real languages Iridium simply has no grammar for yet.

## 5. ⚠️ The structural question — answer before writing code

**The manifests are in the wrong crate for the thing that needs them most.**

`comments.rs` lives in `iridium-editor` and must work **with the `syntax`
feature off** — that is precisely what #60 fixed, after the comment table had
been gated on the parser and answered "no comments" for every language in the
parser-free build. So its data source has to be available in the parser-free
build too.

The manifests currently live inside `iridium-syntax`, which is the optional
crate. Reading them from there would re-create the exact defect #60 removed.

Three ways out, none free:

**(a) Move the vendored tree to `iridium-lang`.** That crate is always
compiled, already owns `Language`, its ids and its extensions, and is already a
dependency of `iridium-syntax` — so nothing has to reach across a boundary
backwards. Cost: `iridium-syntax/src/query/embedded.rs` and its 78
`include_str!` pairings would then be including from another crate's directory,
which breaks `cargo package` and is a smell even unpublished. Fixable by having
`iridium-lang` expose the query source text as well — but `QueryKind` lives in
`iridium-syntax`, so that inverts a dependency.

**(b) A new crate that owns the vendored tree**, with both `iridium-lang` and
`iridium-syntax` depending on it. Clean layering, no backwards reach, one owner
for one directory. Cost: a fifth crate, and it raises the question of what is
left in `iridium-lang` — possibly the answer is that they should be one crate.

**(c) Split the tree**: `config.toml` to `iridium-lang`, `.scm` files stay.
Cheapest edit, worst outcome — one vendored upstream directory becomes two
places, and the next re-vendor has to know that.

**Leaning (b)**, because it is the only one where the directory has exactly one
owner and nothing reaches backwards, and because "what is `iridium-lang` for"
is a question the registry step (#63) has to answer anyway. But it is a
workspace-shape decision and it should be made deliberately rather than fallen
into halfway through an afternoon's refactor.

---

## Order of work, once (a)/(b)/(c) is settled

1. Land the crate move on its own, no behaviour change, gates green.
2. A manifest reader — `include_str!`, parsed at first use, unknown keys
   ignored, `hidden` respected.
3. The equality oracle in §2, against the still-present hard-coded table.
4. Delete `language_tokens()`; `comments.rs` reads the manifest.
5. Delete `Language::extensions()`; `from_extension` reads `path_suffixes`.
6. Whole-file-name matching (§3), separately, with its own test.

Fold node kinds (`folding/language.rs`) are **not** in the schema and are not
part of this step. Neither is the `Language` enum, the grammar table, or the
fixed-size query cache — those are the registry step.
