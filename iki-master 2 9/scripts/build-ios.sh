#!/usr/bin/env bash
# Build the Iki kernel for iOS and generate the Xcode project. macOS + Xcode only.
#   scripts/build-ios.sh            -> target/ios/IkiCore.xcframework + ios/Iki.xcodeproj
#   scripts/build-ios.sh --open     -> also opens the project in Xcode
set -euo pipefail
cd "$(dirname "$0")/.."

[[ "$(uname)" == "Darwin" ]] || { echo "This script needs macOS with Xcode installed." >&2; exit 1; }
command -v xcodebuild >/dev/null || { echo "Install Xcode (App Store) first." >&2; exit 1; }
command -v rustup >/dev/null || { echo "Install Rust: https://rustup.rs" >&2; exit 1; }
if ! command -v xcodegen >/dev/null; then
  command -v brew >/dev/null || { echo "Install Homebrew, then: brew install xcodegen" >&2; exit 1; }
  brew install xcodegen
fi

export IPHONEOS_DEPLOYMENT_TARGET=15.0
TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
rustup target add "${TARGETS[@]}"

# Regenerate Swift ids/theme so they always match the Rust source of truth.
cargo run -q -p iki-android --example gen_ids

for t in "${TARGETS[@]}"; do
  echo "== building iki-ios for $t"
  cargo build --release -p iki-ios --target "$t"
done

OUT=target/ios
rm -rf "$OUT" && mkdir -p "$OUT/sim"
lipo -create \
  target/aarch64-apple-ios-sim/release/libiki_ios.a \
  target/x86_64-apple-ios/release/libiki_ios.a \
  -output "$OUT/sim/libiki_ios.a"

# The xcframework is named IkiCore; headers come from crates/iki-ios/include.
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libiki_ios.a -headers crates/iki-ios/include \
  -library "$OUT/sim/libiki_ios.a" -headers crates/iki-ios/include \
  -output "$OUT/IkiCore.xcframework"

(cd ios && xcodegen generate)
echo
echo "Done. Open ios/Iki.xcodeproj, choose your iPhone, set your Team under Signing, and press Run."
[[ "${1:-}" == "--open" ]] && open ios/Iki.xcodeproj
