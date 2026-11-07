#!/bin/bash
# Build the Iridium WASM module for browser testing

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$SCRIPT_DIR/pkg"

echo "Building Iridium WASM module..."
echo "Project root: $PROJECT_ROOT"
echo "Output dir: $OUTPUT_DIR"

# Build with wasm-pack
cd "$PROJECT_ROOT"
wasm-pack build crates/iridium-bindings \
    --target web \
    --out-dir "$OUTPUT_DIR" \
    --features web \
    --no-default-features

echo ""
echo "Build complete! Output in: $OUTPUT_DIR"
echo ""
# Every port this repository serves on is named once, in ports.json.
PORT="$(sed -n 's/^ *"web-test": *\([0-9][0-9]*\).*/\1/p' "$PROJECT_ROOT/ports.json")"
if [ -z "$PORT" ]; then
    echo "build.sh: ports.json names no web-test port" >&2
    exit 1
fi
echo "To test:"
echo "  cd $SCRIPT_DIR"
echo "  npx serve -p $PORT"
echo "  # Or: deno run -A npm:serve -p $PORT"
echo "  # Open http://localhost:$PORT in a WebGPU-capable browser"
