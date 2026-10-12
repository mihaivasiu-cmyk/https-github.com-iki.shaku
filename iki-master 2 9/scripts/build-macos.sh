#!/usr/bin/env bash
# Cross-build Iki for macOS (Apple Silicon + Intel, one universal .app) from Linux.
#
#   scripts/build-macos.sh            → dist/Iki.app and dist/Iki-macos-universal.zip
#
# Needs: rustup, zig (0.13+), cargo-zigbuild, rcodesign (apple-codesign), curl, zip.
#   cargo install cargo-zigbuild --locked
#   cargo install apple-codesign --locked --bin rcodesign
#
# The macOS SDK (framework stubs to link CoreAudio, CoreMIDI, AppKit, OpenGL…)
# is downloaded once into ~/.cache/iki/. It is Apple software under the Xcode
# license; it is only used for linking and is not shipped in the app.
# Override with MACOS_SDK_DIR=/path/to/MacOSX*.sdk.
set -euo pipefail
cd "$(dirname "$0")/.."

SDK_VERSION="${SDK_VERSION:-26.1}"
SDK_DIR="${MACOS_SDK_DIR:-$HOME/.cache/iki/MacOSX${SDK_VERSION}.sdk}"
SDK_URL="https://github.com/joseluisq/macosx-sdks/releases/download/${SDK_VERSION}/MacOSX${SDK_VERSION}.sdk.tar.xz"
TARGET=universal2-apple-darwin
MIN_MACOS=13.0
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
DIST=dist
APP="$DIST/Iki.app"

need() { command -v "$1" >/dev/null || { echo "missing tool: $1 — $2" >&2; exit 1; }; }
need zig "install zig (pacman -S zig / brew install zig / ziglang.org)"
need cargo-zigbuild "cargo install cargo-zigbuild --locked"
need zip "install zip"

rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null

if [ ! -d "$SDK_DIR/System/Library/Frameworks/CoreAudio.framework" ]; then
    echo "==> downloading macOS SDK ${SDK_VERSION} (one time, ~90 MB)"
    mkdir -p "$(dirname "$SDK_DIR")"
    curl -fL --progress-bar "$SDK_URL" | tar -xJ -C "$(dirname "$SDK_DIR")"
fi

echo "==> building $TARGET (macOS ≥ $MIN_MACOS)"
# Header padding leaves room for the code-signature load command (zig only
# reserves it for arm64), so rcodesign can sign both slices.
PAD="-C link-arg=-Wl,-headerpad,0x4000"
SDKROOT="$SDK_DIR" MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS" \
    CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS="$PAD" \
    CARGO_TARGET_X86_64_APPLE_DARWIN_RUSTFLAGS="$PAD" \
    cargo zigbuild --release --target "$TARGET"

echo "==> bundling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/$TARGET/release/iki" "$APP/Contents/MacOS/iki"
cp packaging/macos/Iki.icns "$APP/Contents/Resources/Iki.icns"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

if command -v rcodesign >/dev/null; then
    echo "==> ad-hoc signing (required for Apple Silicon)"
    rcodesign sign "$APP" >/dev/null
    # (`rcodesign verify` rejects every ad-hoc signature, so check the slices directly.)
    n=$(rcodesign print-signature-info "$APP/Contents/MacOS/iki" 2>/dev/null | grep -c "flags: CodeSignatureFlags(ADHOC)")
    [ "$n" = 2 ] || { echo "error: expected 2 ad-hoc signed slices, found $n" >&2; exit 1; }
    echo "    arm64 + x86_64 slices ad-hoc signed"
else
    echo "warning: rcodesign not found — the app will not start on Apple Silicon until signed" >&2
    echo "         (cargo install apple-codesign --locked --bin rcodesign, or on the Mac: codesign -s - --deep Iki.app)" >&2
fi

echo "==> zipping"
rm -f "$DIST/Iki-macos-universal.zip"
(cd "$DIST" && zip -qry Iki-macos-universal.zip Iki.app)
ls -lh "$DIST/Iki-macos-universal.zip"
echo "done: $APP  (see README.md for launching it on macOS)"
