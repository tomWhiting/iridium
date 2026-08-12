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

# How many gates this script is supposed to run.
#
# ⭐ ONE TRANSCRIPTION, NOT TWO. This number used to be typed into both summary
# lines as a literal `10`, which made every run's final sentence a claim nobody
# checked: add a gate and forget one of the strings and the script reports ten
# while eleven ran. It is now a variable, and `ran` below is what it is checked
# against — so the summary is a measurement rather than a copy of an intention.
GATES=10

failed=0
ran=0
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
    # ⚠️ Incremented *after* the gate returns, never before. A gate that takes
    # the shell down with it — a signal, an out-of-memory kill — must not be
    # counted as having run, because the whole point of the count below is to
    # notice that it did not.
    ran=$((ran + 1))
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

# ⛔ THE CENSUS COMES BEFORE THE VERDICT, AND IT HAS TO.
#
# A run that ended early looks exactly like a clean one from here: `failed` is
# still 0 because the gates that would have failed never ran, and the line
# below would print `✅ all 10 gates passed` over a script that got through
# four. **A partial census reads as a green one**, which is the failure this
# whole file exists to prevent — see the alias story at the top, which is the
# same defect wearing a different hat.
#
# So a short count is neither green nor red. It exits **3**, distinct from the
# 1 a real failure uses and the 2 a bad working directory uses, so that a
# caller — a person, or a workflow leg reading the exit status — can tell
# "some gates failed" from "we do not know what the gates would have said".
# The `!!!` lines above still name whatever did fail before the truncation;
# what is refused here is *summing them into a verdict*.
if [ "$ran" -ne "$GATES" ]; then
    printf '⛔ UNMEASURED — %d of %d gates ran; believe neither the passes nor the failures\n' \
        "$ran" "$GATES"
    exit 3
fi

# ⭐ Every gate runs even after one fails — the point is the whole picture, not
# the first stumble. The count is what decides the exit status.
if [ "$failed" -ne 0 ]; then
    printf '⛔ %d of %d gates FAILED\n' "$failed" "$GATES"
    exit 1
fi
printf '✅ all %d gates passed\n' "$GATES"
