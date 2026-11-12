#!/bin/sh
# Makes Lumia.app of the Rune shell (architecture section 13): the binary, an Info.plist
# with the bundle identifier, the version, the lumia: URL scheme and the update feed, the
# icon made from the brand symbol, Sparkle.framework (the updater) and a code signature. A
# bare `cargo run` binary has none of these, so it gets no links from the system, posts no
# notifications, has no icon and does not update itself.
#
#   scripts/bundle-macos.sh                 # cargo build --release, then out/Lumia.app
#   scripts/bundle-macos.sh --debug         # the debug build (it honours LUMIA_DEV_*)
#   scripts/bundle-macos.sh --bin PATH      # a binary built elsewhere (a universal one from lipo)
#   scripts/bundle-macos.sh --out DIR       # DIR/Lumia.app instead of out/Lumia.app
#   scripts/bundle-macos.sh --sparkle DIR   # only unpack the pinned Sparkle release into DIR
#                                           # (its bin/ has generate_appcast and sign_update)
#
# Environment:
#   LUMIA_SIGN_IDENTITY   a codesign identity ("Developer ID Application: Name (TEAM)"): signs
#                         with it, the hardened runtime and a secure timestamp, which is what
#                         notarization asks for. Unset, the bundle is signed ad hoc: it runs on
#                         this machine, and macOS takes each build for a new app (it asks about
#                         notifications again).
#   LUMIA_VERSION         the version to stamp instead of Cargo.toml's (a release's tag)
#   LUMIA_BUILD           the build number instead of the commit count. It is what Sparkle
#                         compares: an update is a higher build number, whatever the version says.
#   LUMIA_BUNDLE_ID       the bundle identifier instead of chat.lumia (a scratch
#                         bundle whose preferences and scheme registration stay apart)
#   LUMIA_UPDATE_FEED     the appcast the app updates from (Info.plist's SUFeedURL) instead of
#                         this repository's releases: a fork points its builds at its own.
#                         Set and empty, the bundle has no updater and no Sparkle in it.
#   LUMIA_UPDATE_KEY      the Ed25519 public key, base64, that the feed and every download
#                         must be signed with (SUPublicEDKey). The private half signs the
#                         release (scripts/dist-macos.sh) and is never in the repository.
#   LUMIA_SPARKLE_CACHE   where the Sparkle release is kept between runs (target/sparkle)
#
# The result is checked before it replaces the last one: the plist lints, the signature
# verifies all the way down, the binary does not link Sparkle (it loads it at run time, so
# the same binary runs bare), and the bundled binary draws a frame headlessly
# (out/bundle-check.png), which needs no window and no screen-recording permission.
#
# Not here: what a release adds to the bundle (scripts/dist-macos.sh): notarization, the
# disk image and the appcast.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
crate=$(cd "$here/.." && pwd)
root=$(cd "$crate/../.." && pwd)

# Sparkle, pinned: the newest stable release of Sparkle 2, and the first line with signed
# feeds (2.9). The archive is fetched once, kept outside the repository's tracked files and
# checked against this digest on every run, cached or not; to move the pin, change both
# lines together and read the release's notes.
sparkle_version=2.10.1
sparkle_sha256=1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27
sparkle_url="https://github.com/sparkle-project/Sparkle/releases/download/$sparkle_version/Sparkle-$sparkle_version.tar.xz"

# Unpacks the pinned Sparkle release into the directory $1, which must be new or empty.
sparkle_into() {
  cache=${LUMIA_SPARKLE_CACHE:-$crate/target/sparkle}
  archive="$cache/Sparkle-$sparkle_version.tar.xz"
  mkdir -p "$cache" "$1"
  if [ ! -f "$archive" ]; then
    echo "bundle: fetching Sparkle $sparkle_version"
    curl -fsSL --retry 2 -o "$archive.part" "$sparkle_url" || { rm -f "$archive.part"; echo "bundle: could not fetch $sparkle_url"; exit 1; }
    mv "$archive.part" "$archive"
  fi
  found=$(shasum -a 256 "$archive" | cut -d' ' -f1)
  if [ "$found" != "$sparkle_sha256" ]; then
    echo "bundle: $archive is not Sparkle $sparkle_version (SHA-256 $found, expected $sparkle_sha256); remove it to fetch it again"
    exit 1
  fi
  tar -xJf "$archive" -C "$1"
  [ -d "$1/Sparkle.framework" ] || { echo "bundle: no Sparkle.framework in $archive"; exit 1; }
}

profile=release
bin=""
out="$crate/out"
while [ $# -gt 0 ]; do
  case "$1" in
    --debug) profile=debug ;;
    --bin) bin=${2:?--bin needs a path}; shift ;;
    --out) out=${2:?--out needs a directory}; shift ;;
    --sparkle) sparkle_into "${2:?--sparkle needs a directory}"; exit 0 ;;
    *) echo "bundle: unknown argument $1 (see the head of $0)"; exit 2 ;;
  esac
  shift
done
[ "$(uname)" = Darwin ] || { echo "bundle: this makes a macOS bundle and needs macOS's iconutil, sips and codesign"; exit 1; }

id=${LUMIA_BUNDLE_ID:-chat.lumia}
version=${LUMIA_VERSION:-$(sed -n 's/^version = "\(.*\)"$/\1/p' "$crate/Cargo.toml" | head -1)}
build=${LUMIA_BUILD:-$(git -C "$root" rev-list --count HEAD 2>/dev/null || echo 1)}
# The feed is the appcast of the latest GitHub release ("latest" resolves to the newest
# published one), and the key is the repository's: the public half of the one every
# release's feed has been signed with.
feed=${LUMIA_UPDATE_FEED-https://github.com/lumia-chat/lumia/releases/latest/download/appcast.xml}
key=${LUMIA_UPDATE_KEY:-GRG890/GstHa+rDtxagnkw6PqSKwmK6D2CwImxkD4yw=}
# They go into a plist through sed: nothing but what each of them can hold.
case "$id" in *[!A-Za-z0-9.-]*|"") echo "bundle: not a bundle identifier: $id"; exit 2 ;; esac
case "$version" in *[!A-Za-z0-9.+-]*|"") echo "bundle: not a version: $version"; exit 2 ;; esac
case "$build" in *[!0-9.]*|"") echo "bundle: not a build number: $build"; exit 2 ;; esac
case "$feed" in
  "") ;;
  *[!A-Za-z0-9:/._~%?=+-]*) echo "bundle: not a feed URL this script will write: $feed"; exit 2 ;;
  https://*) ;;
  # Plain HTTP only to this machine, which is what scripts/update-check.sh serves a feed
  # on (a loopback address needs no App Transport Security exception).
  http://127.0.0.1:*) ;;
  *) echo "bundle: the update feed must be https: $feed"; exit 2 ;;
esac
case "$key" in *[!A-Za-z0-9+/=]*) echo "bundle: not a base64 key: $key"; exit 2 ;; esac
if [ -n "$feed" ] && [ "$(printf %s "$key" | base64 -D 2>/dev/null | wc -c | tr -d ' ')" != 32 ]; then
  echo "bundle: LUMIA_UPDATE_KEY is not an Ed25519 public key (32 bytes, base64)"; exit 2
fi

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

# The icon: the brand symbol at every size an .icns holds.
symbol="$root/assets/brand/lumia-original-icon.png"
[ -f "$symbol" ] || { echo "bundle: no brand icon at $symbol"; exit 1; }
iconset="$work/Lumia.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$symbol" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z "$((size * 2))" "$((size * 2))" "$symbol" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns -o "$stage/Contents/Resources/Lumia.icns" "$iconset"

plist="$stage/Contents/Info.plist"
sed -e "s/@BUNDLE_ID@/$id/g" -e "s/@VERSION@/$version/g" -e "s/@BUILD@/$build/g" \
  -e "s|@UPDATE_FEED@|$feed|g" -e "s|@UPDATE_KEY@|$key|g" \
  "$crate/packaging/macos/Info.plist" >"$plist"
if [ -z "$feed" ]; then
  # No feed, no updater: the shell starts Sparkle only in a bundle that names one.
  for name in SUFeedURL SUPublicEDKey SURequireSignedFeed SUVerifyUpdateBeforeExtraction SUEnableAutomaticChecks SUScheduledCheckInterval; do
    plutil -remove "$name" "$plist"
  done
fi
plutil -lint "$plist" >/dev/null || { echo "bundle: the Info.plist does not lint"; exit 1; }

# Signing, inside out: a signature seals what is inside it, so each nested piece of code is
# signed before the thing that holds it. `--deep` is not used: it would sign every piece
# alike and drop the Downloader service's entitlements.
identity=${LUMIA_SIGN_IDENTITY:-}
sign() {
  if [ -n "$identity" ]; then
    codesign --force --timestamp --sign "$identity" "$@"
  else
    codesign --force --sign - "$@" 2>/dev/null
  fi || { echo "bundle: could not sign (codesign $*)"; exit 1; }
}

if [ -n "$feed" ]; then
  sparkle_into "$work/sparkle"
  framework="$stage/Contents/Frameworks/Sparkle.framework"
  mkdir -p "$stage/Contents/Frameworks"
  # ditto keeps the framework's symbolic links (Versions/Current and the ones beside it).
  ditto "$work/sparkle/Sparkle.framework" "$framework"
  # Headers and the module map are for compiling against it; nothing here does.
  inner="$framework/Versions/B"
  [ -d "$inner" ] || { echo "bundle: Sparkle.framework is not laid out as Versions/B"; exit 1; }
  for name in Headers PrivateHeaders Modules; do
    rm -rf "${inner:?}/$name" "${framework:?}/$name"
  done
  # Sparkle's own order (its sandboxing guide): the two XPC services, the installer, the
  # progress app, then the framework. Its helpers keep the hardened runtime they ship with.
  sign --options runtime "$inner/XPCServices/Installer.xpc"
  sign --options runtime --preserve-metadata=entitlements "$inner/XPCServices/Downloader.xpc"
  sign --options runtime "$inner/Autoupdate"
  sign --options runtime "$inner/Updater.app"
  sign --options runtime "$framework"
fi
if [ -n "$identity" ]; then
  sign --options runtime "$stage"
  signed="$identity"
else
  sign "$stage"
  signed="ad hoc"
fi
codesign --verify --deep --strict "$stage" || { echo "bundle: the signature does not verify"; exit 1; }

# Sparkle is loaded at run time from Contents/Frameworks (crates/sparkle): the binary names
# no such library, which is what lets the same binary run with none. And the framework has
# every architecture the binary has.
if otool -L "$stage/Contents/MacOS/lumia" | grep -q Sparkle; then
  echo "bundle: the binary links Sparkle; it must load it at run time"; exit 1
fi
if [ -n "$feed" ]; then
  for arch in $(lipo -archs "$stage/Contents/MacOS/lumia"); do
    lipo -archs "$framework/Versions/B/Sparkle" | grep -qw "$arch" || { echo "bundle: Sparkle.framework has no $arch"; exit 1; }
  done
fi

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
if [ -n "$feed" ]; then
  echo "bundle: $app ($id $version build $build, signed $signed, Sparkle $sparkle_version, updates from $feed)"
else
  echo "bundle: $app ($id $version build $build, signed $signed, no updater)"
fi
