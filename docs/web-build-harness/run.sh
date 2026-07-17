#!/bin/sh
set -eu

repo=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
out="$repo/docs/web-build-harness/pkg"

cd "$repo"
CARGO_TARGET_DIR=/Users/tom/Developer/ablative/iridium/target \
  wasm-pack build --target web --dev --out-dir "$out" crates/iridium-bindings \
  --no-default-features --features web

printf '%s\n' 'Open http://127.0.0.1:8000/docs/web-build-harness/'
exec /usr/bin/python3 -m http.server 8000 --bind 127.0.0.1 --directory "$repo"
