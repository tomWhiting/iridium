# Push and branch cleanup — 8 Aug 2026

Tom asked to push the backlog of commits and to "merge any worktrees into main
and clean them up". The push is done. **The worktrees do not exist**, so the
second half needs a ruling before anything is deleted or merged.

## The push — done

```
fea8c323..907dac18  main -> main
```

110 commits, a plain fast-forward (`origin/main...main` was `0 110` before,
`0 0` after, verified with a fresh `git fetch`). Nothing was rebased, squashed
or force-pushed.

## There are no worktrees

`git worktree list` returns one line — this checkout — and `.git/worktrees`
does not exist, which is where git registers linked checkouts. So there is
nothing to merge from a worktree and nothing to prune.

`libs/iridium-artifacts/` sits beside the repo and looks like it might be one.
It is not: `git rev-parse` in it reports *not a git repository*. It holds
`theme-shots-2026-08-05/` and nothing else.

What actually exists is **29 local branches**, and that is almost certainly
what the 110-commit backlog made visible.

## 24 branches are fully merged into main

Their commits are all reachable from `main`, so deleting the refs loses
nothing — the history stays.

```
001-iridium-editor            tui-cell     vk/0d1c…  vk/757b…  vk/a75b…
002-viewport-syntax-rendering tui-driver   vk/3320…  vk/774b…  vk/c4dd…
backup-before-reset           tui-frame    vk/3e98…  vk/828e…  vk/ef49…
clippy-sweep                  tui-input    vk/44d8…  vk/efda…  vk/f467…
feature/lsp-hover-completion               vk/478b…  vk/f5a3…
feature/terminal-face
```

`git branch -d` (lower-case `-d`, which *refuses* anything unmerged) is the
whole operation. **Not run — awaiting a ruling.**

## 4 branches are not merged, and 3 of them must not be

This is the part worth reading before saying yes. "Merge them into main" is
the wrong instruction for three of the four: they are not work in flight, they
are historical spikes hundreds of commits behind, written against code that no
longer exists in that shape. Merging would resurrect deleted modules.

| branch | commits | behind main | last touched | what it is |
|---|---|---|---|---|
| `spike/web-build` | 1 | 464 | 17 Jul | **docs only, purely additive** |
| `seam-spike` | 3 | 484 | 12 Jul | AWL seam demo, web deltas, underlines |
| `feature/web-tree-sitter-standalone` | 1 | 490 | 2 Feb | 3 lines in `controller/index.ts` |
| `vk/c6cf-phase-6-us4-synt` | 3 | 537 | 11 Jan | superseded syntax architecture |

**`spike/web-build`** — adds `docs/WEB-BUILD-FINDINGS.md` (114 lines),
`docs/web-build-harness/index.html` and `run.sh`. All three are **absent from
main**, and the commit adds only new files, so a `git cherry-pick c74b43ff`
cannot conflict. This is the one that is genuinely worth landing. Recommend:
cherry-pick it, delete the branch (local and `origin/spike/web-build`).

**`seam-spike`** — 1,423 insertions including 400 lines of churn in
`wasm.rs`, a new `crates/iridium-bindings/src/web_delta.rs`, underline
decorations in the theme, and `SEAM-SPIKE-REPORT.md`. `wasm.rs` has been
rewritten repeatedly in the 484 commits since; merging this is a
several-hundred-line conflict against code that has since been split, gated
and clippy-swept. The *report* is the part with lasting value and main has no
equivalent (`docs/IN-FLIGHT-awl*.md` cover the grammar, not the seam).
Recommend: salvage `SEAM-SPIKE-REPORT.md` into `docs/` as its own commit,
then delete the branch. Do **not** merge the code.

**`feature/web-tree-sitter-standalone`** — a 3-line change turning the syntax
worker on by default for standalone deployments, from February, against a
`controller/index.ts` that has been substantially rewritten since (it now
carries the workspace, the palette bridge and `hostCommands`). The intent is
worth re-asking as a question about today's code; the diff is not worth
applying. Recommend: delete, and if standalone-by-default is still wanted,
raise it as its own item.

**`vk/c6cf-phase-6-us4-synt`** — a January review branch carrying
`crates/iridium-editor/src/syntax/` (highlighter, parser, types,
`syntax_colors.rs`, ~1,800 lines). That module **does not exist in main** —
it was replaced wholesale by the `iridium-syntax` crate. Merging it would add
back a dead parallel highlighter. Recommend: delete outright.

Note that its 23 sibling `vk/*` branches *are* merged; this one alone is not,
which reads like it was the one review branch never folded back in.

## Stashes — untouched, and staying that way

There are **eight**, all from the January `vk/*` era plus one on `main` from
`79aba63`. The standing rule in this seat is that stashes are never touched,
so none were inspected and none will be dropped without an explicit
instruction naming them. They are listed here only so the count is on record:

```
stash@{0} main  ·  stash@{1} vk/a75b  ·  stash@{2} vk/774b  ·  stash@{3} vk/828e
stash@{4} 001-iridium-editor  ·  stash@{5} vk/f467
stash@{6} 001-iridium-editor  ·  stash@{7} 001-iridium-editor
```

## Remote branches

`origin` carries four refs: `main`, `001-iridium-editor`,
`feature/lsp-hover-completion`, `spike/web-build`. The first two locals are
merged into main; `spike/web-build` is covered above. Deleting a remote branch
is the one irreversible step in this whole page, so it is listed separately
and not bundled into the local cleanup.

## What is waiting on a ruling

1. Delete the 24 merged locals with `git branch -d`? *(recommend yes)*
2. Cherry-pick `spike/web-build`'s docs commit onto main? *(recommend yes)*
3. Salvage `SEAM-SPIKE-REPORT.md` into `docs/`, then delete `seam-spike`? *(recommend yes)*
4. Delete `feature/web-tree-sitter-standalone` and `vk/c6cf-phase-6-us4-synt`? *(recommend yes)*
5. Delete the three non-`main` remote branches? *(recommend yes, but it is the irreversible one)*

Nothing in 1–5 has been run.
