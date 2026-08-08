#!/bin/sh
# Build the `web` feature for wasm32 and serve this directory's smoke page.
#
# Two things were corrected when this script was cherry-picked out of
# `spike/web-build` on 8 Aug 2026 — both would have failed on any box:
#
#   * It pinned `CARGO_TARGET_DIR` to an absolute path that no longer exists
#     (the repository moved under `libs/`). Removed rather than repointed:
#     the default target directory is correct, and a hard-coded absolute path
#     is wrong again the next time anything moves.
#   * It served on port 8000, which is one of the four ports this repository
#     bans in CLAUDE.md because they are always occupied on Tom's box.
#
# The port is overridable so two of these can run at once:
#   PORT=14572 docs/web-build-harness/run.sh
set -eu

port="${PORT:-14571}"

repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
out="$repo/docs/web-build-harness/pkg"

cd "$repo"
wasm-pack build --target web --dev --out-dir "$out" crates/iridium-bindings \
  --no-default-features --features web

printf '%s\n' "Open http://127.0.0.1:${port}/docs/web-build-harness/"
exec /usr/bin/python3 -m http.server "$port" --bind 127.0.0.1 --directory "$repo"
