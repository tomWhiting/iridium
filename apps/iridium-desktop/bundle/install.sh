#!/usr/bin/env bash
# Installs iridium.app into /Applications — the "machines that build it"
# distribution path.
#
# Runs bundle.sh for the one release build and the bundle assembly, then
# replaces the installed copy. Nothing here signs differently from
# bundle.sh: the ad-hoc signature it applies is CORRECT for this path and
# is not a placeholder for a Developer ID.
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

echo "installed $dest"

# ---------------------------------------------------------------------------
# The `iridium` command.
#
# ⚠️ THE INSTALL IS NOT FINISHED UNTIL THIS EXISTS. An app in /Applications
# and nothing on the PATH is how a walkthrough came to tell Tom to run
# `iridium-desktop ~/project` on 9 Aug 2026 — a command that had never
# existed anywhere a shell would look for it.
#
# NOTHING IS DELETED HERE. A pre-existing `iridium` that is not this shim is
# the terminal face's binary, and it is MOVED to `iridium-tui` beside the
# shim — which is where the shim's own `--tui` looks for it — rather than
# overwritten. The two faces then share one command and neither is lost.
# ---------------------------------------------------------------------------
bin_dir="${BINDIR:-$HOME/.local/bin}"
shim="$here/iridium"

if [[ ! -f "$shim" ]]; then
  echo "install: $shim is missing; the app is installed but no command was" >&2
  exit 1
fi

mkdir -p "$bin_dir"
target="$bin_dir/iridium"

# `cmp -s` rather than a marker string: the question is whether the file
# already IS this shim, and comparing the bytes answers it without asking
# the file to describe itself.
if [[ -e "$target" ]] && ! cmp -s "$shim" "$target"; then
  preserved="$bin_dir/iridium-tui"
  if [[ -e "$preserved" ]] && ! cmp -s "$target" "$preserved"; then
    echo "install: $preserved already exists and differs from $target." >&2
    echo "         Refusing to overwrite it — move it aside and run again." >&2
    exit 1
  fi
  mv -f "$target" "$preserved"
  echo "preserved the previous $target as $preserved (reachable as: iridium --tui)"
fi

install -m 755 "$shim" "$target"
echo "installed $target"

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
echo "  iridium --help             everything else"
