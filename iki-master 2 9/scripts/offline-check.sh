#!/usr/bin/env bash
# Run the sound-kernel tests with no network and no audio device.
#
# Compiles the real iki-core DSP / engine / looper / Foli / sample sources
# against a tiny `hound` stand-in (tools/offline-check) and runs their unit
# tests plus tests/engine.rs. Use it where crates.io isn't reachable; CI and
# normal development use `cargo test` as usual.
set -euo pipefail
cd "$(dirname "$0")/.."
cp crates/iki-core/tests/engine.rs tools/offline-check/shim/tests/engine.rs
cd tools/offline-check
cargo test --offline "$@"
