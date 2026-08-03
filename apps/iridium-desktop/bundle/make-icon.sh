#!/usr/bin/env bash
# Regenerates iridium.icns from icon.html.
#
# Pipeline: render-icon.js paints the tile once at 1024×1024 with a
# transparent background; sips (macOS-bundled) downscales it into the ten
# iconset entries; iconutil compiles the iconset into iridium.icns beside
# this script, where bundle.sh picks it up. Downscaling from one 1024 master
# is the standard iconutil workflow — text rendered natively at 16px would be
# mush, a filtered downscale is not.
#
# Needs node and the cached Playwright chromium (see render-icon.js for the
# environment overrides). bundle.sh does NOT run this: the generated icns is
# kept in the repo so bundling works on a checkout without the render stack.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

for tool in node sips iconutil; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "make-icon: $tool is not on PATH" >&2
    exit 1
  fi
done

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

master="$work/icon-1024.png"
node "$here/render-icon.js" "$here/icon.html" "$master"

iconset="$work/iridium.iconset"
mkdir "$iconset"

# name → pixel size, per iconutil's naming contract (@2x is double the name).
scale() {
  local name="$1" size="$2"
  sips -z "$size" "$size" "$master" --out "$iconset/$name" >/dev/null
}
scale icon_16x16.png 16
scale icon_16x16@2x.png 32
scale icon_32x32.png 32
scale icon_32x32@2x.png 64
scale icon_128x128.png 128
scale icon_128x128@2x.png 256
scale icon_256x256.png 256
scale icon_256x256@2x.png 512
scale icon_512x512.png 512
cp "$master" "$iconset/icon_512x512@2x.png"

iconutil -c icns "$iconset" -o "$here/iridium.icns"
echo "wrote $here/iridium.icns"
