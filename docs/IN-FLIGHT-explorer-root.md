# IN FLIGHT — the explorer root bug (Tom, 6 Aug 2026, 09:14)

**Status: `project.rs` written with tests; NOT yet wired, NOT yet compiled,
NOT yet committed.** Everything before this point is landed and pushed
(`c4b125f`).

## The bug, in Tom's words

> "when you open it up just from the applications menu, it opens up to an
> editor and… what it's trying to index is like the whole system"

**Confirmed as mine.** `app/host_commands.rs::explorer_root()` fell back to
`std::env::current_dir()` when no file was open. A macOS app launched from
Finder or Spotlight inherits **`/`** from LaunchServices — so the explorer
rooted at the filesystem root, and the crawl that landed in `3c269fa` then set
off across the whole disk. Harmless before the crawl; loud after it.

## The fix, and the reasoning

**Two questions, answered separately** — conflating them is what made it bad:
*where to start* and *whether a search may read past what is open*.

A crawl is worth running only when something real bounds it. Inside a
repository, `.gitignore` does. Pointed at a home directory nothing does: there
are no ignore rules, the first twenty thousand directories are application
support files, and it finds nothing anyone wanted. **A crawl that cannot work
is not worth starting.**

Rule, one line: *search past what is open when the root is a project, or when
it is the folder of a file the user actually has open — not when it is a
directory this code guessed.*

## What is written (`apps/iridium-desktop/src/project.rs`, ~230 lines)

```rust
pub struct ExplorerRoot { pub path: PathBuf, pub crawl: bool }

pub fn explorer_root(
    active_file: Option<&Path>,
    working_directory: Option<PathBuf>,
    home: Option<PathBuf>,
) -> ExplorerRoot
```

Pure — the environment is injected, because the failing case **cannot be
reproduced from a terminal at all**. Nine tests, including the reported bug as
`a_launcher_working_directory_never_becomes_the_root`.

- active file → `project_root(dir)` (nearest ancestor holding `.git`, walking
  up, so a submodule roots at the submodule) else the file's own folder;
  `crawl: true`.
- working directory, **rejected when `path.parent().is_none()`** (that is the
  `/` test, and it is right on every platform) → its project with
  `crawl: true`, else itself with `crawl: false`.
- otherwise home, `crawl: false`; then `"."`.

## WHAT REMAINS — every step, none optional

1. **`mod project;`** in `apps/iridium-desktop/src/lib.rs`.
2. **`FileExplorer::open(root: PathBuf, crawl: bool)`** — store the flag in
   `panel.rs` as `pub(super) crawl: bool`.
3. **`panel.rs::poll`** — gate the crawl on it:
   `let crawling = self.crawl && self.is_filtering() && self.files.crawl(CRAWL_PER_FRAME) > 0;`
4. **`panel.rs::is_waiting`** — must not claim to be waiting when the crawl is
   off, or the window repaints forever:
   `self.files.is_waiting() || (self.crawl && self.is_filtering() && !self.files.is_fully_crawled())`
5. **`compose.rs::nothing_found_yet`** — a fourth answer for crawl-off. The
   three that exist are `Searching…` / limit notice / `No matching files`;
   add `"No matches in what is open"` (or similar) when `!self.crawl`, and
   make sure `Searching…` is unreachable with the crawl off.
6. **`app/host_commands.rs::explorer_root`** — replace the body with a call to
   `project::explorer_root(active_file_path, std::env::current_dir().ok(), home)`.
   Home: `std::env::home_dir()` is undeprecated as of Rust 1.85+; **verify
   that before using it**, else read `$HOME` directly.
   Note it needs the **file path**, not its parent — the new function takes
   the file and does the `parent()` itself.
7. **A test in `file_tree/tests/filter.rs`** that a panel opened with
   `crawl: false` posts no reads under a query — the mirror of
   `nothing_is_crawled_until_something_is_typed`.
8. **Six gates**, each unpiped with its own `echo exit=$?`.
9. **Reinstall** (`pgrep -x iridium-desktop` first — never swap under a live
   session) and **tell Tom through the Meridian `send` tool**, or it did not
   happen. Tom: `dm:c9255b2a-5731-4d17-8124-e3bfa2224186`.

## Say this to Tom when it lands

He found it in the first two minutes of using it. The honest framing: the
crawl was right, the root was wrong, and `/` is what a launcher hands you
rather than anything a person chose. Also tell him what the new rule *costs* —
open the app cold with no file and searching will only cover what he has
expanded, until he opens a file or a project.

## Still unanswered by Tom (asked twice)

- Does `Enter` on a folder in the **unfiltered** tree descend and re-root the
  panel, instead of toggling?
- Does the explorer replace `⌘O`, or sit beside it?
