#!/bin/sh
# The nine gates, in the battery's order — what CI would run.
#
# ⛔ THIS IS A SCRIPT BECAUSE A CARGO ALIAS CANNOT DO IT, AND THE ALIAS THAT
# CLAIMED TO WAS MALFORMED. `ci = ["fmt-check", "lint", "test --workspace"]`
# reads like a chain and is not one: cargo's list form is ONE command with
# arguments, so that expanded to
#
#     cargo fmt --all -- --check lint test --workspace
#
# — a single rustfmt invocation with three stray arguments. It never ran the
# lint. It never ran the tests. ⭐ The task that recorded this defect said the
# alias "runs three of the nine gates"; measured, it ran NONE. A claim about a
# gate's coverage is worth nothing until someone watches the gate run.
#
# ⚠️ EACH GATE IS RUN UNPIPED AND ITS EXIT STATUS CHECKED ON ITS OWN. Piping a
# gate into anything reports the pipe's status, not the gate's, which is how a
# red run comes back green. The individual `cargo ci-*` aliases in
# `.cargo/config.toml` are the same commands for running one gate by hand.
#
# ⚠️ `fmt --check`, never `fmt`. A gate must not mutate the tree it judges: one
# that reformats and then reports success has verified the tree it made, not the
# tree it was handed.
set -u

cd "$(dirname "$0")/.." || exit 2

failed=0
run() {
    name=$1
    shift
    printf '=== %s\n' "$name"
    if "$@"; then
        # ⛔ NOT a leading `---`. MEASURED: `printf '--- %s OK\n'` fails with
        # `printf: --: invalid option`, rc 2, because printf takes a leading
        # dash as an option. The first version of this script did exactly that,
        # so every gate passed and NOT ONE printed its verdict — while the
        # summary and exit status stayed correct, because `failed` is a counter
        # and the failure branch starts with `!`. ⭐ A reporting line that
        # silently prints nothing on the SUCCESS path is invisible precisely
        # when everything is fine, which is when nobody is looking.
        printf '>>> %s OK\n\n' "$name"
    else
        status=$?
        printf '!!! %s FAILED (exit %d)\n\n' "$name" "$status"
        failed=$((failed + 1))
    fi
}

# Tests first: a compile error surfaces here with the clearest message.
run "test/workspace" cargo test --workspace --all-features --no-fail-fast
run "test/kernel" cargo test -p iridium-editor --no-default-features --no-fail-fast
run "test/syntax" cargo test -p iridium-editor --no-default-features --features syntax --no-fail-fast
run "test/lang" cargo test -p iridium-lang --all-features

# The wasm target is not built by any gate above — wasm.rs is target-gated.
run "check/wasm" cargo check -p iridium-bindings --no-default-features --features web --target wasm32-unknown-unknown

run "clippy/all" cargo clippy --workspace --all-features --all-targets -- -D warnings
run "clippy/kernel" cargo clippy -p iridium-editor --no-default-features --all-targets -- -D warnings
run "clippy/syntax" cargo clippy -p iridium-editor --no-default-features --features syntax --all-targets -- -D warnings
run "clippy/wasm" cargo clippy -p iridium-bindings --no-default-features --features web --target wasm32-unknown-unknown -- -D warnings

run "fmt" cargo fmt --all --check

# ⭐ Every gate runs even after one fails — the point is the whole picture, not
# the first stumble. The count is what decides the exit status.
if [ "$failed" -ne 0 ]; then
    printf '⛔ %d of 10 gates FAILED\n' "$failed"
    exit 1
fi
printf '✅ all 10 gates passed\n'
