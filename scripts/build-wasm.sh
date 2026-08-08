#!/usr/bin/env bash
# Builds the wasm bundle iridium-bindings publishes, and stamps where it came
# from.
#
# ⚠️ `pkg/` is gitignored (.gitignore:34), so nothing in git records which
# commit produced the bytes that go to npm. That is the whole reason this
# script exists: it writes `pkg/PROVENANCE.txt` naming the SHA, the toolchain
# and whether the tree was dirty, and that file ships in the tarball. Without
# it "which version of the editor is in this wasm?" has no answer once the
# working tree moves on.
#
# Refuses to build from a dirty tree unless IRIDIUM_ALLOW_DIRTY=1, because a
# published artefact built from uncommitted work cannot be reproduced by
# anyone, including the person who built it.
set -euo pipefail

cd "$(dirname "$0")/.."
crate="crates/iridium-bindings"

sha="$(git rev-parse HEAD)"
dirty="clean"
if ! git diff --no-ext-diff --quiet HEAD -- "$crate" crates/iridium-editor crates/iridium-syntax; then
    dirty="DIRTY"
    if [[ "${IRIDIUM_ALLOW_DIRTY:-0}" != "1" ]]; then
        echo "!!! the sources this wasm is built from have uncommitted changes." >&2
        echo "    Commit them, or set IRIDIUM_ALLOW_DIRTY=1 if you know why." >&2
        exit 1
    fi
fi

echo "=== wasm-pack build (target web, release) from $sha [$dirty]"
wasm-pack build "$crate" --target web --release --no-default-features --features web

cat > "$crate/pkg/PROVENANCE.txt" <<EOF
iridium-bindings wasm bundle
commit:    $sha
tree:      $dirty
toolchain: $(rustc --version)
wasm-pack: $(wasm-pack --version)
EOF

echo "=== provenance"
cat "$crate/pkg/PROVENANCE.txt"
