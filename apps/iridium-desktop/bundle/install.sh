#!/usr/bin/env bash
# Installs BOTH faces — the "machines that build it" distribution path.
#
# iridium.app into /Applications, the terminal face onto the PATH, and the
# `iridium` command that reaches either. Runs bundle.sh for the desktop
# release build and the bundle assembly, builds the terminal face beside
# it, then replaces the installed copies. Nothing here signs differently
# from bundle.sh: the ad-hoc signature it applies is CORRECT for this path
# and is not a placeholder for a Developer ID.
#
# WHY AD-HOC IS ENOUGH HERE, since it looks like a shortcut and is not:
# Gatekeeper refuses an app because of the com.apple.quarantine extended
# attribute, which the *downloader* sets — Safari, curl, an unzip of a
# downloaded archive. A bundle produced by a local `cargo build` never
# acquires it, so it launches with no prompt and no Developer ID. The $99
# Apple Developer Program buys distribution to people who did NOT build
# the app; it buys nothing for a build-from-source install. If prebuilt
# artifacts are ever published, THAT is when hardened runtime, notarytool
# and stapling become necessary — and this comment should be revisited
# rather than trusted.
#
# Destination override: DEST=/somewhere ./install.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
dest_dir="${DEST:-/Applications}"
dest="$dest_dir/iridium.app"

# ---------------------------------------------------------------------------
# Refuse while any iridium-desktop is running.
#
# Two independent reasons, and the guard covers both because it cannot
# tell them apart cheaply:
#   1. bundle.sh's first act is `rm -rf` on target/release/bundle/iridium.app.
#      Deleting a bundle out from under the process executing it is exactly
#      the swap this project forbids.
#   2. Replacing $dest while it runs does the same to the installed copy.
# `pgrep -x` matches the process NAME exactly — never `-f`, which would
# also match this script's own command line and refuse against itself.
# ---------------------------------------------------------------------------
running=""
while IFS= read -r pid; do
  [[ -n "$pid" ]] || continue
  path="$(ps -o comm= -p "$pid" 2>/dev/null || true)"
  running+="  pid $pid  $path"$'\n'
done < <(pgrep -x iridium-desktop 2>/dev/null || true)

if [[ -n "$running" ]]; then
  echo "install: iridium-desktop is running; quit it first." >&2
  echo "$running" >&2
  echo "install: rebuilding or replacing a bundle while its process is live" >&2
  echo "         is the swap this project forbids — refusing." >&2
  exit 1
fi

# The build and the bundle. bundle.sh already validates its own tools,
# lints the plist and verifies the signature, so none of that is repeated
# here — a second copy would be a second thing to keep true.
"$here/bundle.sh"

root="$(cd "$here/../../.." && pwd)"
staged="$root/target/release/bundle/iridium.app"
if [[ ! -d "$staged" ]]; then
  echo "install: $staged was not produced by bundle.sh" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# The terminal face, from the same checkout and the same profile.
#
# Not in bundle.sh, because bundle.sh assembles a macOS application and
# this is not one. In the same run as it, because "install iridium" means
# both faces — and the face nobody installs is the face nobody notices is
# stale.
#
# Built BEFORE anything is replaced, so a terminal face that fails to
# compile leaves the installed desktop face untouched rather than half
# swapped.
# ---------------------------------------------------------------------------
cargo build --release -p iridium --manifest-path "$root/Cargo.toml"

tui_binary="$root/target/release/iridium"
if [[ ! -x "$tui_binary" ]]; then
  echo "install: $tui_binary was not produced by the build" >&2
  exit 1
fi

if [[ ! -d "$dest_dir" ]]; then
  echo "install: $dest_dir does not exist" >&2
  exit 1
fi
if [[ ! -w "$dest_dir" ]]; then
  echo "install: $dest_dir is not writable by $(id -un)." >&2
  echo "         Either run with an admin account, or install elsewhere:" >&2
  echo "           DEST=\"\$HOME/Applications\" $0" >&2
  exit 1
fi

# Copy rather than move: the staged bundle stays put so a later run of
# bundle.sh alone still leaves something runnable in the tree, and so a
# failed install does not consume the artifact. `ditto` is used instead of
# `cp -R` because it preserves the extended attributes and resource forks
# that carry a code signature — `cp -R` can strip them and leave an
# installed app whose signature no longer verifies.
tmp="$dest_dir/.iridium.app.incoming.$$"
rm -rf "$tmp"
ditto "$staged" "$tmp"

# Swap last, and only once the new copy is fully written, so an
# interrupted install cannot leave a half-written app at $dest.
rm -rf "$dest"
mv "$tmp" "$dest"

# Verify what was INSTALLED, not what was staged. bundle.sh verified the
# staged copy; this checks the bytes that actually landed, because the
# copy is exactly the step that can damage a signature.
codesign --verify --strict "$dest"

echo "installed $dest (the desktop face)"

# ---------------------------------------------------------------------------
# The two commands: the terminal face, and the `iridium` shim over both.
#
# ⚠️ THE INSTALL IS NOT FINISHED UNTIL BOTH EXIST. An app in /Applications
# and nothing on the PATH is how a walkthrough came to tell Tom to run
# `iridium-desktop ~/project` on 9 Aug 2026 — a command that had never
# existed anywhere a shell would look for it.
#
# ⭐ AND THE TERMINAL FACE IS INSTALLED HERE TOO, as of 13 Aug 2026. Until
# then this script installed the shim, printed `installed …/iridium`, and
# left `iridium --tui` running whatever binary happened to sit beside it —
# on the box where this was found, one built ten days earlier, carrying
# none of the terminal face's work since. Every one of those runs reported
# success. AN INSTALLER THAT INSTALLS ONE OF TWO FACES REPORTS SUCCESS FOR
# BOTH, so the lines printed below name the ARTEFACTS that landed and not
# the commands that were run.
# ---------------------------------------------------------------------------
bin_dir="${BINDIR:-$HOME/.local/bin}"
shim="$here/iridium"

if [[ ! -f "$shim" ]]; then
  echo "install: $shim is missing; the app is installed but no command was" >&2
  exit 1
fi

mkdir -p "$bin_dir"
target="$bin_dir/iridium"
tui="$bin_dir/iridium-tui"

# Replaces a command by RENAME, never by writing over it in place.
#
# A running program's binary cannot be written to (ETXTBSY) and a running
# shell script is read from disk as it executes, so overwriting either is
# the swap this project forbids. `mv` within one directory is a rename: it
# swaps the directory entry and leaves any running process on the inode it
# already opened. The guard at the top of this script covers the desktop
# face, which must not be rebuilt underneath itself; a terminal editor open
# in another pane needs no such refusal, because a rename cannot disturb
# it — it keeps the binary it started with until it exits.
place() {
  local source="$1" destination="$2" incoming="$2.incoming.$$"
  install -m 755 "$source" "$incoming"
  mv -f "$incoming" "$destination"
}

# The terminal face FIRST, and its landing is what licenses the step after
# it: only once a current `iridium-tui` is on disk is it TRUE to say that an
# older binary at $target has been superseded rather than lost.
place "$tui_binary" "$tui"
echo "installed $tui (the terminal face — reachable as: iridium --tui)"

# `cmp -s` rather than a marker string: the question is whether the file
# already IS this shim, and comparing the bytes answers it without asking
# the file to describe itself.
if [[ -e "$target" ]] && ! cmp -s "$shim" "$target"; then
  # A non-shim `iridium` is the terminal face's own binary, put here by
  # hand before this script existed. It used to be MOVED aside to
  # $tui, because nothing else produced that file. Something does now —
  # the line above, from this checkout — so moving a stale copy of the same
  # program on top of a fresh one would lose the fresh one. It is
  # superseded, and reporting that is the honest answer.
  echo "superseded $target (an older terminal face) — the current one is $tui"
fi

place "$shim" "$target"
echo "installed $target (the command)"

case ":${PATH}:" in
  *":$bin_dir:"*) ;;
  *)
    echo "install: ⚠️  $bin_dir is not on your PATH, so \`iridium\` will not be found." >&2
    echo "         Add it: export PATH=\"$bin_dir:\$PATH\"" >&2
    ;;
esac

echo
echo "  iridium ~/some/project     open a project"
echo "  iridium notes.md           open a file"
echo "  iridium --tui notes.md     the same file, in this terminal"
echo "  iridium --help             everything else"
