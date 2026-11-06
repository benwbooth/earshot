#!/usr/bin/env bash
# Builds Earshot's web version into a folder the server can serve:
#   scripts/build-web.sh [out-dir] [--release]
# Needs the WebAssembly target, wasm-bindgen (matching Cargo.lock) and
# woff2_compress, which `nix develop` provides; the Nix build runs it too.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-$root/target/web}
profile=dev
dir=debug
if [ "${2:-}" = "--release" ]; then profile=wasm-release; dir=wasm-release; fi

cd "$root"
cargo build ${CARGO_OFFLINE:+--offline} --profile "$profile" --target wasm32-unknown-unknown -p earshot-web
mkdir -p "$out/icons" "$out/fonts"
wasm-bindgen --target web --no-typescript --out-dir "$out" "target/wasm32-unknown-unknown/$dir/earshot_web.wasm"
cp crates/web/static/index.html crates/web/static/styles.css crates/web/static/manifest.webmanifest "$out/"
cat crates/web/static/pages/*.css > "$out/pages.css" 2>/dev/null || : > "$out/pages.css"
cp qml/icons/*.svg "$out/icons/"
cp packaging/earshot-256.png "$out/"
# Inter, the Qt app's font, so phones show the same letters.
if [ -n "${INTER_TTF:-}" ] && [ ! -e "$out/fonts/InterVariable.woff2" ]; then
  work=$(mktemp -d)
  cp "$INTER_TTF" "$work/InterVariable.ttf"
  chmod u+w "$work/InterVariable.ttf"
  if command -v woff2_compress >/dev/null && woff2_compress "$work/InterVariable.ttf" >/dev/null; then
    cp "$work/InterVariable.woff2" "$out/fonts/"
  else
    cp "$work/InterVariable.ttf" "$out/fonts/"
  fi
fi
echo "Web version built in $out"
