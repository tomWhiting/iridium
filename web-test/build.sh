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
echo "To test:"
echo "  cd $SCRIPT_DIR"
echo "  npx serve -p 8899"
echo "  # Or: deno run -A npm:serve -p 8899"
echo "  # Open http://localhost:8899 in a WebGPU-capable browser"
