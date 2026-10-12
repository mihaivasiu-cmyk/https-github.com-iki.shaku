#!/usr/bin/env bash
# Linker for cross-building macOS dylibs (the plugin) from Linux with zig.
# rustc passes the exported-symbols list as two `-Wl` arguments, which zig
# 0.16's argument parsing breaks apart (the link then fails); join them. (Its
# Mach-O linker then ignores the list, so all globals stay exported — harmless
# with macOS's two-level namespace.) The target follows rustc's `-arch`,
# and the minimum macOS goes into zig's target triple (zig ignores
# -mmacosx-version-min): $MACOSX_DEPLOYMENT_TARGET, default 11.0.
set -euo pipefail
min="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
arch=x86_64
args=()
prev=""
while [ $# -gt 0 ]; do
    if [ "$1" = "-Wl,-exported_symbols_list" ] && [ $# -ge 2 ]; then
        args+=("-Wl,-exported_symbols_list,${2#-Wl,}")
        shift 2
        continue
    fi
    [ "$prev" = "-arch" ] && [ "$1" = "arm64" ] && arch=aarch64
    prev="$1"
    args+=("$1")
    shift
done
exec cargo-zigbuild zig cc -- -g -fno-sanitize=all -target "$arch-macos.$min-none" "${args[@]}"
