#!/bin/sh
# Makes what a release publishes for macOS (architecture section 13, "Distribution"):
# Lumia.app on a disk image, which is what a person downloads and what Sparkle installs an
# update from, and the appcast the installed apps read, signed with the release's Ed25519
# key. With a Developer ID and the notary's credentials in the environment the app and the
# image are signed, notarized and stapled. This is the only macOS release path; the release
# workflow runs it on a tag, and it also runs here.
#
#   scripts/dist-macos.sh 0.3.0                 # the version is the tag without its v
#   scripts/dist-macos.sh 0.3.0 --release       # refuse to make anything less than a release
#   scripts/dist-macos.sh 0.3.0 --bin PATH      # a binary built elsewhere (a universal one from lipo)
#   scripts/dist-macos.sh 0.3.0 --out DIR       # DIR instead of out/dist
#
# Without --release it makes what the environment allows: with nothing set, an app signed ad
# hoc on an image, a check that everything assembles. --release is for the workflow: it
# stops unless the signing identity, the notary's credentials and the appcast key are all
# there, so a missing secret cannot publish an unsigned build.
#
# Environment:
#   LUMIA_SIGN_IDENTITY     the codesign identity ("Developer ID Application: Name (TEAM)"),
#                           in a keychain on the machine; the workflow imports it from a
#                           secret into a keychain of its own. The app (scripts/bundle-macos.sh)
#                           and the image are signed with it.
#   ASC_KEY_ID, ASC_ISSUER_ID, ASC_KEY_PATH
#                           an App Store Connect API key (any role), for the notary: the app
#                           is notarized and stapled, then the image. Needs the identity.
#   SPARKLE_KEY_PATH        the Ed25519 private key file (the base64 seed, as Sparkle's tools
#                           read it; a secret written to a file). Its public half must be
#                           the bundle's LUMIA_UPDATE_KEY. The appcast is written when it is
#                           set, and signed with it: each download, and the feed as a whole.
#                           The key is read from this file only, never from the Keychain.
#   LUMIA_PREVIOUS_DIR      a directory holding the previous release's Lumia-*.dmg; Sparkle
#                           then publishes a delta beside the image, which an app on that
#                           release downloads instead
#   LUMIA_DOWNLOAD_PREFIX   where the release's files will be served from, instead of this
#                           repository's release for the version
#                           (https://github.com/lumia-chat/lumia/releases/download/vVERSION/);
#                           a fork sets it with LUMIA_UPDATE_FEED
#   LUMIA_BUILD, LUMIA_BUNDLE_ID, LUMIA_UPDATE_FEED, LUMIA_UPDATE_KEY
#                           as scripts/bundle-macos.sh reads them
#
# Output, and all there is in the output directory when it ends (out/dist): Lumia-<version>.dmg,
# appcast.xml when there was a key, and Lumia<build>-<previous build>.delta when there was a
# previous release. Everything else it makes is in a temporary directory inside the output
# directory, removed when it ends.
#
# The signing and notarizing branch has never run: no Developer ID or notary credentials
# were used where this was written. It follows the script that released the previous
# macOS app (the same notarytool and stapler calls, in the same order).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
crate=$(cd "$here/.." && pwd)
version=${1:?version, e.g. 0.3.0}
shift
bin=""
dist="$crate/out/dist"
release=false
while [ $# -gt 0 ]; do
  case "$1" in
    --release) release=true ;;
    --bin) bin=${2:?--bin needs a path}; shift ;;
    --out) dist=${2:?--out needs a directory}; shift ;;
    *) echo "dist: unknown argument $1 (see the head of $0)"; exit 2 ;;
  esac
  shift
done
[ "$(uname)" = Darwin ] || { echo "dist: this makes a macOS release and needs macOS"; exit 1; }
case "$version" in *[!A-Za-z0-9.+-]*|"") echo "dist: not a version: $version"; exit 2 ;; esac
prefix=${LUMIA_DOWNLOAD_PREFIX:-https://github.com/lumia-chat/lumia/releases/download/v$version/}
case "$prefix" in */) ;; *) prefix="$prefix/" ;; esac
identity=${LUMIA_SIGN_IDENTITY:-}
key=${SPARKLE_KEY_PATH:-}
if [ -n "$key" ] && [ ! -f "$key" ]; then echo "dist: no key file at $key"; exit 1; fi

notarize=false
if [ -n "${ASC_KEY_ID:-}" ] || [ -n "${ASC_ISSUER_ID:-}" ] || [ -n "${ASC_KEY_PATH:-}" ]; then
  [ -n "${ASC_KEY_ID:-}" ] && [ -n "${ASC_ISSUER_ID:-}" ] && [ -f "${ASC_KEY_PATH:-}" ] \
    || { echo "dist: notarization needs ASC_KEY_ID, ASC_ISSUER_ID and the key file at ASC_KEY_PATH, all three"; exit 1; }
  [ -n "$identity" ] || { echo "dist: notarization needs LUMIA_SIGN_IDENTITY: the notary takes only what a Developer ID signed"; exit 1; }
  notarize=true
fi
if $release; then
  [ -n "$identity" ] || { echo "dist: --release needs LUMIA_SIGN_IDENTITY"; exit 1; }
  $notarize || { echo "dist: --release needs ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH"; exit 1; }
  [ -n "$key" ] || { echo "dist: --release needs SPARKLE_KEY_PATH"; exit 1; }
fi
# Submits the file $1 to the notary and waits for its answer. The staple that follows is
# what fails when the answer was no: there is then no ticket to staple.
notary() {
  xcrun notarytool submit "$1" --key "$ASC_KEY_PATH" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID" --wait
}

mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
work=$(mktemp -d "$dist/.dist.XXXXXX")
trap 'rm -rf "$work"' EXIT

# The bundle, signed as the environment says (ad hoc, or the identity with the hardened
# runtime and a timestamp on every piece of code in it, Sparkle's included), and checked.
if [ -n "$bin" ]; then
  LUMIA_VERSION="$version" "$here/bundle-macos.sh" --bin "$bin" --out "$work/app"
else
  LUMIA_VERSION="$version" "$here/bundle-macos.sh" --out "$work/app"
fi
app="$work/app/Lumia.app"
build=$(plutil -extract CFBundleVersion raw "$app/Contents/Info.plist")
if ! plutil -extract SUFeedURL raw "$app/Contents/Info.plist" >/dev/null 2>&1; then
  echo "dist: this bundle has no update feed (LUMIA_UPDATE_FEED is empty), so there is no release to make of it"; exit 1
fi

if $notarize; then
  # The notary takes an archive; the ticket it issues is stapled to the app itself, so the
  # app opens on a Mac that is offline. The staple changes the bundle, which is why this
  # comes before the image is made and before anything is signed for Sparkle.
  ditto -c -k --keepParent "$app" "$work/Lumia.zip"
  notary "$work/Lumia.zip"
  xcrun stapler staple "$app"
  codesign --verify --deep --strict "$app" || { echo "dist: the stapled app's signature does not verify"; exit 1; }
fi

# The previous release's image, set aside before the output directory is cleared (it may be
# the same directory).
feeddir="$work/feed"
mkdir -p "$feeddir" "$work/home"
if [ -d "${LUMIA_PREVIOUS_DIR:-}" ]; then
  for previous in "$LUMIA_PREVIOUS_DIR"/Lumia-*.dmg; do
    if [ -f "$previous" ] && [ "$(basename "$previous")" != "Lumia-$version.dmg" ]; then
      cp "$previous" "$feeddir/"
    fi
  done
fi

# The disk image: the app and a link to /Applications, read-only, compressed. ditto keeps
# the framework's symbolic links, the executable bits and the signatures.
rm -f "$dist"/Lumia-*.dmg "$dist"/Lumia*.delta "$dist/appcast.xml"
staging="$work/dmg"
mkdir -p "$staging"
ditto "$app" "$staging/Lumia.app"
ln -s /Applications "$staging/Applications"
dmg="$dist/Lumia-$version.dmg"
hdiutil create -volname "Lumia" -srcfolder "$staging" -ov -format UDZO -quiet "$dmg"
if [ -n "$identity" ]; then
  # The image itself is signed too, so Gatekeeper can assess it as a whole, not only the
  # app inside.
  codesign --sign "$identity" --timestamp "$dmg"
fi
if $notarize; then
  notary "$dmg"
  xcrun stapler staple "$dmg"
fi

if [ -z "$key" ]; then
  echo "dist: no SPARKLE_KEY_PATH, so no appcast"
  echo "dist: $version build $build in $dist: $(ls "$dist" | tr '\n' ' ')"
  exit 0
fi

# Sparkle's own tools, from the release the bundle script pins.
"$here/bundle-macos.sh" --sparkle "$work/sparkle"
tools="$work/sparkle/bin"
[ -x "$tools/generate_appcast" ] && [ -x "$tools/sign_update" ] || { echo "dist: no generate_appcast and sign_update under $tools"; exit 1; }

# generate_appcast works over a directory of images: this release's, and the previous
# one's when there is one, which it makes the delta from. It keeps unpacked images in a
# cache under the home directory; that goes to the work directory instead. The image is
# signed here, after the staple: the signature is over the file as it will be downloaded.
cp "$dmg" "$feeddir/"
CFFIXED_USER_HOME="$work/home" "$tools/generate_appcast" --ed-key-file "$key" \
  --download-url-prefix "$prefix" --link "https://lumia.chat" \
  --maximum-versions 1 --maximum-deltas 1 \
  -o "$feeddir/appcast.xml" "$feeddir" >"$work/appcast.log" 2>&1 \
  || { cat "$work/appcast.log"; echo "dist: generate_appcast failed"; exit 1; }
cat "$work/appcast.log"
feed="$feeddir/appcast.xml"
[ -f "$feed" ] || { echo "dist: no appcast was written"; exit 1; }
# generate_appcast only warns when the key file is not the one the app was built to trust,
# and leaves the image unsigned: an update no installed app would take.
if grep -q "does not match" "$work/appcast.log"; then
  echo "dist: SPARKLE_KEY_PATH is not the private half of the bundle's LUMIA_UPDATE_KEY"; exit 1
fi
grep -q "Lumia-$version.dmg\"[^>]*sparkle:edSignature=" "$feed" \
  || { echo "dist: the appcast does not carry a signature for Lumia-$version.dmg"; exit 1; }
# The feed as published verifies against the key: the apps require it (SURequireSignedFeed).
"$tools/sign_update" --verify --ed-key-file "$key" "$feed" >/dev/null 2>&1 \
  || { echo "dist: the appcast's own signature does not verify"; exit 1; }

# Only this release's files are published; the previous image served the delta.
cp "$feed" "$dist/appcast.xml"
for delta in "$feeddir"/Lumia"$build"-*.delta; do
  if [ -f "$delta" ]; then cp "$delta" "$dist/"; fi
done

echo "dist: $version build $build, downloads from $prefix"
echo "dist: in $dist: $(ls "$dist" | tr '\n' ' ')"
