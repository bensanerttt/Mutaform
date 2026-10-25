#!/bin/bash
# Builds Mutaform for macOS (Intel + Apple Silicon, universal) as VST3 and CLAP.
#
# Requirements (one-time):  xcode-select --install   and   https://rustup.rs
# Usage:                    ./scripts/build-macos.sh            (build into dist/macos)
#                           ./scripts/build-macos.sh --install  (build and install for your user)
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
export MACOSX_DEPLOYMENT_TARGET=10.13

rustup target add x86_64-apple-darwin aarch64-apple-darwin
cargo build --release --lib --target x86_64-apple-darwin
cargo build --release --lib --target aarch64-apple-darwin

OUT=dist/macos
rm -rf "$OUT" && mkdir -p "$OUT"
lipo -create -output "$OUT/Mutaform.dylib" \
  target/x86_64-apple-darwin/release/libmutaform.dylib \
  target/aarch64-apple-darwin/release/libmutaform.dylib

make_bundle() {
  local B="$OUT/$1"
  mkdir -p "$B/Contents/MacOS"
  cp "$OUT/Mutaform.dylib" "$B/Contents/MacOS/Mutaform"
  sed "s/@VERSION@/$VERSION/g" scripts/Info.plist.in > "$B/Contents/Info.plist"
  printf 'BNDL????' > "$B/Contents/PkgInfo"
  # Ad-hoc signature: required for plugins to load on Apple Silicon.
  codesign --force --deep --sign - "$B"
}
make_bundle Mutaform.vst3
make_bundle Mutaform.clap
rm "$OUT/Mutaform.dylib"
cp scripts/Install-Mac.command "$OUT/"
chmod +x "$OUT/Install-Mac.command"
echo "Built $OUT/Mutaform.vst3 and $OUT/Mutaform.clap (version $VERSION)"

if [[ "${1:-}" == "--install" ]]; then
  bash "$OUT/Install-Mac.command"
fi
