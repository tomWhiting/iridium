#!/usr/bin/env bash
# Builds iridium.app — the desktop face as a macOS application bundle.
#
# One release build of iridium-desktop, then a hand-assembled bundle under
# target/release/bundle/: the layout, the Info.plist and the code signature
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
target_dir="${CARGO_TARGET_DIR:-$root/target}"
# Explicit identities fail closed: a failed certificate signature must never
# silently become an ad-hoc signature. Local builds default to ad-hoc.
signing_identity="${IRIDIUM_SIGNING_IDENTITY:--}"
signing_timestamp="${IRIDIUM_SIGNING_TIMESTAMP:-none}"
case "$signing_timestamp" in
  none|secure) ;;
  *) echo "bundle: IRIDIUM_SIGNING_TIMESTAMP must be none or secure" >&2; exit 1 ;;
esac

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

binary="$target_dir/release/iridium-desktop"
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

app="$target_dir/release/bundle/iridium.app"
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
	<key>CFBundleDocumentTypes</key>
	<array>
		<dict>
			<key>CFBundleTypeName</key>
			<string>Text and Source Code</string>
			<key>CFBundleTypeRole</key>
			<string>Editor</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<key>LSItemContentTypes</key>
			<array>
				<string>public.plain-text</string>
				<string>public.source-code</string>
				<string>public.json</string>
				<string>public.xml</string>
			</array>
		</dict>
	</array>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSSupportsAutomaticGraphicsSwitching</key>
	<true/>
</dict>
</plist>
PLIST

plutil -lint "$app/Contents/Info.plist"

if [[ "$signing_identity" == "-" ]]; then
  codesign --force --sign - "$app"
else
  timestamp_option="--timestamp=none"
  if [[ "$signing_timestamp" == "secure" ]]; then
    timestamp_option="--timestamp"
  fi
  codesign --force --options runtime "$timestamp_option" --sign "$signing_identity" "$app"
fi
codesign --verify --strict "$app"

echo "bundled $app"
