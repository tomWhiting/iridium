#!/usr/bin/env bash
# Builds iridium.app — the desktop face as a macOS application bundle.
#
# One release build of iridium-desktop, then a hand-assembled bundle under
# target/release/bundle/: the layout, the Info.plist and the ad-hoc codesign
# are all written here, so packaging a single target carries no bundling
# dependency (docs/DESKTOP-SHELL-PLAN.md, step 2, "Bundle").
#
# The icon (iridium.icns beside this script) is a checked-in artifact of
# icon.html; regenerate it with make-icon.sh when the tile changes. This
# script only copies it, so bundling works on any checkout with only cargo
# and the macOS-bundled plutil/codesign.
#
# Idempotent: the previous bundle is replaced wholesale on every run.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"

for tool in cargo plutil codesign; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "bundle: $tool is not on PATH" >&2
    exit 1
  fi
done

icns="$here/iridium.icns"
if [[ ! -f "$icns" ]]; then
  echo "bundle: $icns is missing — run $here/make-icon.sh first" >&2
  exit 1
fi

# The one release build.
cargo build --release -p iridium-desktop --manifest-path "$root/Cargo.toml"

binary="$root/target/release/iridium-desktop"
if [[ ! -x "$binary" ]]; then
  echo "bundle: $binary was not produced by the build" >&2
  exit 1
fi

# The bundle version is the workspace version — the first `version = "…"` in
# the root manifest is [workspace.package]'s.
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -n 1)"
if [[ -z "$version" ]]; then
  echo "bundle: could not read the workspace version from $root/Cargo.toml" >&2
  exit 1
fi

app="$root/target/release/bundle/iridium.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$binary" "$app/Contents/MacOS/iridium-desktop"
cp "$icns" "$app/Contents/Resources/iridium.icns"

cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>iridium</string>
	<key>CFBundleDisplayName</key>
	<string>iridium</string>
	<key>CFBundleIdentifier</key>
	<string>io.ablative.iridium</string>
	<key>CFBundleExecutable</key>
	<string>iridium-desktop</string>
	<key>CFBundleIconFile</key>
	<string>iridium</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleShortVersionString</key>
	<string>${version}</string>
	<key>CFBundleVersion</key>
	<string>${version}</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSSupportsAutomaticGraphicsSwitching</key>
	<true/>
</dict>
</plist>
PLIST

plutil -lint "$app/Contents/Info.plist"

codesign --force -s - "$app"
codesign --verify --strict "$app"

echo "bundled $app"
