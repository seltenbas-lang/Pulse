#!/usr/bin/env bash
# Builds the Android APK: the Rust app for each ABI (cargo-ndk), then the Java layer and APK (Gradle).
# Needs: Rust, the Android SDK (platform 35, build-tools) and NDK, Java 17, Gradle 8.9+.
#   export ANDROID_HOME=~/Android/Sdk
#   export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/30.0.16248370   # r28 or newer: Skia needs its libc++
#   ./build-android.sh            # debug APK
#   ./build-android.sh release    # release APK (signed with the debug key unless you set up signing)
# The version comes from Cargo.toml (./bump-version.sh moves it on for each release); the APK is
# copied to dist/pulse-<version>.apk.
# Release signing: point PULSE_SIGNING_ENV at the signing.env kept with the Pulse key (never in this
# repository); without it, the APK is signed with the debug key and won't update an installed release.
set -euo pipefail
cd "$(dirname "$0")"
mode=${1:-debug}
if [ -n "${PULSE_SIGNING_ENV:-}" ]; then
  set -a; . "$PULSE_SIGNING_ENV"; set +a
fi
[ "$mode" = release ] && [ -z "${PULSE_KEYSTORE:-}" ] && echo "warning: no PULSE_SIGNING_ENV, signing with the debug key" >&2
version=$(./bump-version.sh --show)
IFS=. read -r v_major v_minor v_patch <<<"$version"
# always higher for a newer version, so each release installs over the last
export PULSE_VERSION_NAME="${PULSE_VERSION_NAME:-$version}"
export PULSE_VERSION_CODE="${PULSE_VERSION_CODE:-$((v_major * 10000 + v_minor * 100 + v_patch))}"
echo "Pulse $PULSE_VERSION_NAME (version code $PULSE_VERSION_CODE)"
: "${ANDROID_HOME:?set ANDROID_HOME to your Android SDK}"
: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME to an Android NDK (r28 or newer)}"
export ANDROID_NDK="$ANDROID_NDK_HOME"
export ANDROID_JAR="${ANDROID_JAR:-$(ls -d "$ANDROID_HOME"/platforms/android-*/android.jar | sort -V | tail -1)}"
export ANDROID_D8_JAR="${ANDROID_D8_JAR:-$(ls -d "$ANDROID_HOME"/build-tools/*/lib/d8.jar | sort -V | tail -1)}"
command -v cargo-ndk >/dev/null || cargo install cargo-ndk
rustup target add aarch64-linux-android x86_64-linux-android
profile=$([ "$mode" = release ] && echo --release || echo)
cargo ndk -t arm64-v8a -t x86_64 --platform 26 -o android/app/src/main/jniLibs build -p pulse-app --lib $profile
cd android
gradle_cmd=$(command -v gradle || true)
[ -x ./gradlew ] && gradle_cmd=./gradlew
"$gradle_cmd" "assemble${mode^}"
apk=$(find app/build/outputs/apk/"$mode" -name '*.apk' | head -1)
mkdir -p ../dist
cp "$apk" "../dist/pulse-$version.apk"
echo "dist/pulse-$version.apk"
