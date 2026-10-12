#!/usr/bin/env bash
# Build the Iki instrument plugin (crates/iki-plugin) as VST3 + CLAP bundles
# for Windows and macOS (and Linux), cross-compiled from Linux.
#
#   scripts/build-plugin.sh              → dist/Iki-plugin-windows.zip, dist/Iki-plugin-macos.zip,
#                                          dist/Iki-plugin-linux.tar.gz
#   scripts/build-plugin.sh windows      → only some of: windows macos linux
#
# Each archive holds Iki.vst3 (Ableton Live, Cubase, FL Studio, …) and Iki.clap
# (Bitwig, Reaper, …); see README.md for where to put them.
#
# Needs: rustup, zig (0.16+), cargo-zigbuild, zip; for macOS also rcodesign
# (cargo install apple-codesign --locked --bin rcodesign) and the macOS SDK,
# downloaded once into ~/.cache/iki/ like scripts/build-macos.sh does.
set -euo pipefail
cd "$(dirname "$0")/.."

TARGETS=("$@")
[ ${#TARGETS[@]} -eq 0 ] && TARGETS=(windows macos linux)
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
OUT=target/plugin
DIST=dist
mkdir -p "$OUT" "$DIST"

need() { command -v "$1" >/dev/null || { echo "missing tool: $1 — $2" >&2; exit 1; }; }
need zig "install zig 0.16+ (pacman -S zig / ziglang.org)"
need cargo-zigbuild "cargo install cargo-zigbuild --locked"
need zip "install zip"

windows() {
    echo "==> Windows x86_64 (VST3 + CLAP)"
    rustup target add x86_64-pc-windows-gnu >/dev/null
    cargo zigbuild --release -p iki-plugin --target x86_64-pc-windows-gnu
    local dll=target/x86_64-pc-windows-gnu/release/iki_plugin.dll
    local d="$OUT/windows"
    rm -rf "$d"
    mkdir -p "$d/Iki.vst3/Contents/x86_64-win"
    cp "$dll" "$d/Iki.vst3/Contents/x86_64-win/Iki.vst3"
    cp "$dll" "$d/Iki.clap"
    cp README.md "$d/README.md"
    (cd "$d" && rm -f "../Iki-plugin-windows.zip" && zip -qr "../Iki-plugin-windows.zip" Iki.vst3 Iki.clap README.md)
    cp "$OUT/Iki-plugin-windows.zip" "$DIST/"
    ls -lh "$DIST/Iki-plugin-windows.zip"
}

# A macOS plugin bundle around the universal dylib.
mac_bundle() { # <bundle dir> <identifier suffix>
    local b=$1 id=$2
    mkdir -p "$b/Contents/MacOS"
    cp target/universal2-apple-darwin/release/libiki_plugin.dylib "$b/Contents/MacOS/Iki"
    sed -e "s/@VERSION@/$VERSION/g" -e "s/@ID@/$id/g" packaging/macos/Plugin-Info.plist > "$b/Contents/Info.plist"
    printf 'BNDL????' > "$b/Contents/PkgInfo"
}

macos() {
    echo "==> macOS universal (Apple Silicon + Intel; VST3 + CLAP)"
    local sdk_version="${SDK_VERSION:-26.1}"
    local sdk="${MACOS_SDK_DIR:-$HOME/.cache/iki/MacOSX${sdk_version}.sdk}"
    if [ ! -d "$sdk/System/Library/Frameworks/AppKit.framework" ]; then
        echo "    downloading macOS SDK ${sdk_version} (one time, ~90 MB)"
        mkdir -p "$(dirname "$sdk")"
        curl -fL --progress-bar \
            "https://github.com/joseluisq/macosx-sdks/releases/download/${sdk_version}/MacOSX${sdk_version}.sdk.tar.xz" |
            tar -xJ -C "$(dirname "$sdk")"
    fi
    rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
    # Room for the code signature (zig only reserves it for arm64), and a
    # neutral install name instead of the build path.
    local flags="-C link-arg=-Wl,-headerpad,0x4000 -C link-arg=-Wl,-install_name,@loader_path/Iki"
    SDKROOT="$sdk" MACOSX_DEPLOYMENT_TARGET=11.0 \
        CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="$PWD/scripts/zig-apple-cc.sh" \
        CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER="$PWD/scripts/zig-apple-cc.sh" \
        CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS="$flags" \
        CARGO_TARGET_X86_64_APPLE_DARWIN_RUSTFLAGS="$flags" \
        cargo zigbuild --release -p iki-plugin --target universal2-apple-darwin
    local d="$OUT/macos"
    rm -rf "$d"
    mkdir -p "$d"
    mac_bundle "$d/Iki.vst3" vst3
    mac_bundle "$d/Iki.clap" clap
    if command -v rcodesign >/dev/null; then
        echo "    ad-hoc signing (required for Apple Silicon)"
        rcodesign sign "$d/Iki.vst3" >/dev/null
        rcodesign sign "$d/Iki.clap" >/dev/null
    else
        echo "    WARNING: rcodesign not found — unsigned bundles won't load on Apple Silicon" >&2
    fi
    cp README.md "$d/README.md"
    (cd "$d" && rm -f "../Iki-plugin-macos.zip" && zip -qry "../Iki-plugin-macos.zip" Iki.vst3 Iki.clap README.md)
    cp "$OUT/Iki-plugin-macos.zip" "$DIST/"
    ls -lh "$DIST/Iki-plugin-macos.zip"
}

linux() {
    echo "==> Linux x86_64 (VST3 + CLAP)"
    cargo build --release -p iki-plugin
    local d="$OUT/linux"
    rm -rf "$d"
    mkdir -p "$d/Iki.vst3/Contents/x86_64-linux"
    cp target/release/libiki_plugin.so "$d/Iki.vst3/Contents/x86_64-linux/Iki.so"
    cp target/release/libiki_plugin.so "$d/Iki.clap"
    cp README.md "$d/README.md"
    tar -czf "$DIST/Iki-plugin-linux.tar.gz" -C "$d" Iki.vst3 Iki.clap README.md
    ls -lh "$DIST/Iki-plugin-linux.tar.gz"
}

for t in "${TARGETS[@]}"; do
    case "$t" in
        windows | macos | linux) "$t" ;;
        *) echo "unknown target '$t' (windows, macos, linux)" >&2; exit 1 ;;
    esac
done
echo "done — install and use: see README.md"
