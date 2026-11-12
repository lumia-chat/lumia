#!/bin/sh
# Makes Lumia.app of the Rune shell (architecture section 13): the binary, an Info.plist
# with the bundle identifier, the version and the lumia: URL scheme, the icon made from the
# brand symbol, and a code signature. A bare `cargo run` binary has none of these, so it
# gets no links from the system, posts no notifications and has no icon.
#
#   scripts/bundle-macos.sh                 # cargo build --release, then out/Lumia.app
#   scripts/bundle-macos.sh --debug         # the debug build (it honours LUMIA_DEV_*)
#   scripts/bundle-macos.sh --bin PATH      # a binary built elsewhere (a universal one from lipo)
#   scripts/bundle-macos.sh --out DIR       # DIR/Lumia.app instead of out/Lumia.app
#
# Environment:
#   LUMIA_SIGN_IDENTITY   a codesign identity ("Developer ID Application: Name (TEAM)"): signs
#                         with it, the hardened runtime and a secure timestamp, which is what
#                         notarization asks for. Unset, the bundle is signed ad hoc: it runs on
#                         this machine, and macOS takes each build for a new app (it asks about
#                         notifications again).
#   LUMIA_VERSION         the version to stamp instead of Cargo.toml's (a release's tag)
#   LUMIA_BUNDLE_ID       the bundle identifier instead of chat.lumia.desktop (a scratch
#                         bundle whose preferences and scheme registration stay apart)
#
# The result is checked before it replaces the last one: the plist lints, the signature
# verifies, and the bundled binary draws a frame headlessly (out/bundle-check.png), which
# needs no window and no screen-recording permission.
#
# Not here: notarization, the DMG and the update feed. Those are the release's (the
# SwiftUI app's are in apps/apple/scripts/dist.sh) and wait for the shell's updater.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
crate=$(cd "$here/.." && pwd)
root=$(cd "$crate/../.." && pwd)
profile=release
bin=""
out="$crate/out"
while [ $# -gt 0 ]; do
  case "$1" in
    --debug) profile=debug ;;
    --bin) bin=${2:?--bin needs a path}; shift ;;
    --out) out=${2:?--out needs a directory}; shift ;;
    *) echo "bundle: unknown argument $1 (see the head of $0)"; exit 2 ;;
  esac
  shift
done
[ "$(uname)" = Darwin ] || { echo "bundle: this makes a macOS bundle and needs macOS's iconutil, sips and codesign"; exit 1; }

id=${LUMIA_BUNDLE_ID:-chat.lumia.desktop}
version=${LUMIA_VERSION:-$(sed -n 's/^version = "\(.*\)"$/\1/p' "$crate/Cargo.toml" | head -1)}
build=$(git -C "$root" rev-list --count HEAD 2>/dev/null || echo 1)
# They go into a plist through sed: nothing but what an identifier and a version hold.
case "$id" in *[!A-Za-z0-9.-]*|"") echo "bundle: not a bundle identifier: $id"; exit 2 ;; esac
case "$version" in *[!A-Za-z0-9.+-]*|"") echo "bundle: not a version: $version"; exit 2 ;; esac

if [ -z "$bin" ]; then
  if [ "$profile" = release ]; then
    (cd "$crate" && cargo build --release)
  else
    (cd "$crate" && cargo build)
  fi
  bin="$crate/target/$profile/lumia"
fi
[ -x "$bin" ] || { echo "bundle: no binary at $bin"; exit 1; }

mkdir -p "$out"
out=$(cd "$out" && pwd)
app="$out/Lumia.app"
work=$(mktemp -d "$out/.bundle.XXXXXX")
trap 'rm -rf "$work"' EXIT
stage="$work/Lumia.app"
mkdir -p "$stage/Contents/MacOS" "$stage/Contents/Resources"
cp "$bin" "$stage/Contents/MacOS/lumia"
printf 'APPL????' >"$stage/Contents/PkgInfo"

# The icon: the brand symbol at every size an .icns holds, as the SwiftUI app's asset
# catalog and the Tauri shell's `tauri icon` make theirs from the same PNG.
symbol="$root/assets/brand/lumia-original-icon.png"
[ -f "$symbol" ] || { echo "bundle: no brand icon at $symbol"; exit 1; }
iconset="$work/Lumia.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$symbol" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z "$((size * 2))" "$((size * 2))" "$symbol" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns -o "$stage/Contents/Resources/Lumia.icns" "$iconset"

sed -e "s/@BUNDLE_ID@/$id/g" -e "s/@VERSION@/$version/g" -e "s/@BUILD@/$build/g" \
  "$crate/packaging/macos/Info.plist" >"$stage/Contents/Info.plist"
plutil -lint "$stage/Contents/Info.plist" >/dev/null || { echo "bundle: the Info.plist does not lint"; exit 1; }

if [ -n "${LUMIA_SIGN_IDENTITY:-}" ]; then
  codesign --force --options runtime --timestamp --sign "$LUMIA_SIGN_IDENTITY" "$stage"
  signed="$LUMIA_SIGN_IDENTITY"
else
  codesign --force --sign - "$stage" 2>/dev/null
  signed="ad hoc"
fi
codesign --verify --deep --strict "$stage" || { echo "bundle: the signature does not verify"; exit 1; }

# The bundled binary draws: a frame of the first screen, over a data directory of its own.
LUMIA_DATA_DIR="$work/data" "$stage/Contents/MacOS/lumia" --snap "$work/check.png" >/dev/null 2>"$work/check.log" \
  || { cat "$work/check.log"; echo "bundle: the bundled binary did not draw a frame"; exit 1; }
[ -s "$work/check.png" ] || { echo "bundle: the bundled binary drew nothing"; exit 1; }
mv "$work/check.png" "$out/bundle-check.png"
# Running a bundle's binary can make LaunchServices note the bundle, and with it a claim on
# lumia: links at a path about to be gone. The finished bundle is registered when it is
# first opened (or by `lsregister -f`), not by being made.
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
[ -x "$lsregister" ] && "$lsregister" -u "$stage" >/dev/null 2>&1 || true

rm -rf "$app"
mv "$stage" "$app"
echo "bundle: $app ($id $version build $build, signed $signed)"
