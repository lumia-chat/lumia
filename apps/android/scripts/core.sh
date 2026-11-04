#!/bin/sh
# Builds the Rust core for Android and generates its Kotlin bindings:
#   lumia-ffi as a shared library per ABI through cargo-ndk into
#   lumiakit's jniLibs, the UniFFI Kotlin file, and the Kotlin tokens
#   (tokens.py). Nothing it produces is committed (.gitignore). Needs
#   ANDROID_HOME with an NDK (ANDROID_NDK_HOME, or the newest under
#   $ANDROID_HOME/ndk), cargo-ndk, and the Rust targets
#   aarch64-linux-android and x86_64-linux-android.
#
#   scripts/core.sh            # arm64 devices and the x86_64 emulator, debug
#   scripts/core.sh --release  # optimized
#   scripts/core.sh --arm64    # devices only
set -eu
here=$(cd "$(dirname "$0")" && pwd)
core="$here/../../core"
kit="$here/../lumiakit"
profile=debug
abis="arm64-v8a x86_64"
for arg in "$@"; do
  case "$arg" in
    --release) profile=release ;;
    --arm64) abis="arm64-v8a" ;;
  esac
done
release_flag=""
[ "$profile" = release ] && release_flag="--release"
: "${ANDROID_HOME:?set ANDROID_HOME to the SDK}"
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
  ANDROID_NDK_HOME=$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1)
  export ANDROID_NDK_HOME
fi
[ -d "$ANDROID_NDK_HOME" ] || { echo "core: no NDK under $ANDROID_HOME/ndk"; exit 1; }

# The shared library per ABI, with the 16 KB page alignment Android 15 wants.
rm -rf "$kit/src/main/jniLibs"
# shellcheck disable=SC2086
(cd "$core" && targets=""; for abi in $abis; do targets="$targets -t $abi"; done; cargo ndk $targets -o "$kit/src/main/jniLibs" --platform 28 build $release_flag -p lumia-ffi)

# Bindings from the library's own metadata (one source of truth: the Rust).
first=$(echo "$abis" | cut -d' ' -f1)
lib="$kit/src/main/jniLibs/$first/liblumia_ffi.so"
out="$here/../.build/bindings"
rm -rf "$out" && mkdir -p "$out"
(cd "$core" && cargo run -q -p uniffi-bindgen -- generate --library "$lib" --language kotlin --no-format --out-dir "$out")
rm -rf "$kit/src/main/kotlin/uniffi"
mkdir -p "$kit/src/main/kotlin"
cp -R "$out/uniffi" "$kit/src/main/kotlin/uniffi"
python3 "$here/tokens.py"

# A release is stripped once the bindings have read the metadata the
# symbols carry (cargo-ndk no longer strips, and a profile strip would
# take the metadata with it). The NDK's strip keeps what the loader needs.
if [ "$profile" = release ]; then
  strip=$(ls "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-strip | head -1)
  for abi in $abis; do
    "$strip" --strip-unneeded "$kit/src/main/jniLibs/$abi/liblumia_ffi.so"
  done
fi
echo "core: $kit/src/main/jniLibs ($profile; ABIs: $abis)"
