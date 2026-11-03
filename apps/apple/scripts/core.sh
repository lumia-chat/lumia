#!/bin/sh
# Builds the Rust core for Apple platforms and generates its Swift bindings:
#   lumia-ffi as a static library per target, the UniFFI Swift file and C
#   header, an XCFramework the LumiaKit package links, and the Swift
#   tokens (tokens.py). Nothing it produces is committed (LumiaKit/.gitignore).
#   Needs DEVELOPER_DIR pointing at Xcode (27 on this machine).
#
#   scripts/core.sh            # macOS (this machine's arch), debug
#   scripts/core.sh --release  # optimized
#   scripts/core.sh --ios      # adds iOS device and simulator slices
set -eu
here=$(cd "$(dirname "$0")" && pwd)
core="$here/../../core"
kit="$here/../LumiaKit"
out="$here/../.build/core"
profile=debug
ios=false
for arg in "$@"; do
  case "$arg" in
    --release) profile=release ;;
    --ios) ios=true ;;
  esac
done
release_flag=""
[ "$profile" = release ] && release_flag="--release"

# macOS is universal (an archive links both slices); iOS is arm64 on device and on the simulator.
targets="aarch64-apple-darwin x86_64-apple-darwin"
$ios && targets="$targets aarch64-apple-ios aarch64-apple-ios-sim"
for t in $targets; do
  # Inside the core: its rust-toolchain.toml picks the toolchain the target goes on.
  (cd "$core" && { rustup target list --installed | grep -q "^$t$" || rustup target add "$t"; })
  (cd "$core" && cargo build $release_flag -p lumia-ffi --target "$t")
done

# Bindings from the library's own metadata (one source of truth: the Rust).
mkdir -p "$out/bindings"
lib="$core/target/aarch64-apple-darwin/$profile/liblumia_ffi.a"
(cd "$core" && cargo run -q -p uniffi-bindgen -- generate --library "$lib" --language swift --no-format --out-dir "$out/bindings")
# The target's directory holds only generated files, so a fresh checkout lacks it.
mkdir -p "$kit/Sources/LumiaFFI"
rm -rf "$kit/Sources/LumiaFFI"/*.swift
cp "$out/bindings/lumia_ffi.swift" "$kit/Sources/LumiaFFI/LumiaFFI.swift"

# The C module the Swift file imports, laid out as headers for the XCFramework.
headers="$out/headers"
rm -rf "$headers" && mkdir -p "$headers"
cp "$out/bindings/lumia_ffiFFI.h" "$headers/"
cp "$out/bindings/lumia_ffiFFI.modulemap" "$headers/module.modulemap"

mkdir -p "$out/macos"
lipo -create "$core/target/aarch64-apple-darwin/$profile/liblumia_ffi.a" "$core/target/x86_64-apple-darwin/$profile/liblumia_ffi.a" -output "$out/macos/liblumia_ffi.a"
args=""
args="$args -library $out/macos/liblumia_ffi.a -headers $headers"
if $ios; then
  args="$args -library $core/target/aarch64-apple-ios/$profile/liblumia_ffi.a -headers $headers"
  args="$args -library $core/target/aarch64-apple-ios-sim/$profile/liblumia_ffi.a -headers $headers"
fi
rm -rf "$kit/LumiaCore.xcframework"
# shellcheck disable=SC2086
xcodebuild -create-xcframework $args -output "$kit/LumiaCore.xcframework" > /dev/null
python3 "$here/tokens.py"
echo "core: $kit/LumiaCore.xcframework ($profile; targets: $targets)"
