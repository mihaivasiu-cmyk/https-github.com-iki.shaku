#!/usr/bin/env bash
# Build the Android app (crates/iki-android): the Rust kernel for all ABIs,
# JVM tests with the real kernel (Robolectric screenshots), then the APK.
#
#   scripts/build-android.sh            → dist/iki-android.apk (+ dist/android-screenshots/)
#   SKIP_TESTS=1 scripts/build-android.sh
#
# Everything generated goes to target/android/ (jniLibs, Gradle build and caches).
# Needs: Android SDK (platform 36, build-tools, NDK r30) in $ANDROID_HOME or ~/Android/Sdk,
#        JDK 21, rustup, cargo-ndk (cargo install cargo-ndk --locked).
set -euo pipefail
cd "$(dirname "$0")/.."

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK_VERSION="30.0.16248370"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/$NDK_VERSION}"
if [ -z "${JAVA_HOME:-}" ] && [ -d /usr/lib/jvm/java-21-openjdk ]; then
    export JAVA_HOME=/usr/lib/jvm/java-21-openjdk
fi

need() { command -v "$1" >/dev/null || { echo "missing tool: $1 — $2" >&2; exit 1; }; }
need cargo-ndk "cargo install cargo-ndk --locked"
[ -d "$ANDROID_NDK_HOME" ] || { echo "NDK not found at $ANDROID_NDK_HOME (sdkmanager \"ndk;$NDK_VERSION\")" >&2; exit 1; }

rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android >/dev/null

echo "==> Kotlin ids from crates/iki-android/src/params.rs"
cargo run -q -p iki-android --example gen_ids

echo "==> Rust kernel for arm64-v8a, armeabi-v7a, x86_64 (API 26+)"
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 --platform 26 \
    -o target/android/jniLibs build --release -p iki-android

gradle() { (cd crates/iki-android && ./gradlew --console=plain -q --project-cache-dir ../../target/android/gradle-cache "$@"); }

if [ -z "${SKIP_TESTS:-}" ]; then
    echo "==> host build of the JNI library + Robolectric tests (screenshots → dist/android-screenshots)"
    cargo build --release -p iki-android
    gradle testDebugUnitTest
fi

echo "==> APK"
gradle assembleRelease
mkdir -p dist
cp target/android/gradle/outputs/apk/release/iki-release.apk dist/iki-android.apk
ls -lh dist/iki-android.apk
echo "done: dist/iki-android.apk  (adb install -r dist/iki-android.apk)"
