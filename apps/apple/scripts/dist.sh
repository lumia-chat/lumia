#!/bin/sh
# Builds the direct-download macOS app for a release (architecture
# section 13, "Distribution"): a universal Release archive, exported with
# the team's Developer ID, notarized and stapled, on a DMG, with the
# Sparkle appcast the installed apps read. The release workflow runs it on
# a tag; it also runs here.
#
#   scripts/dist.sh 0.2.0            # the version is the tag without its v; build number = commit count
#   scripts/dist.sh 0.2.0 --sign     # also notarize and staple, and sign the appcast
#
# Without --sign the app is exported and the DMG built, a check that
# everything assembles; only --sign produces something to publish.
#
# Environment:
#   ASC_KEY_ID, ASC_ISSUER_ID, ASC_KEY_PATH   App Store Connect API key (Admin): cloud signing on a
#                                             machine without the certificate, and notarization (--sign)
#   SPARKLE_KEY_PATH                          the EdDSA private key file (the base64 seed; the public
#                                             half is LUMIA_UPDATE_KEY in project.yml); the appcast is
#                                             written when it is set, signed with it
#   LUMIA_PREVIOUS_DIR                        a directory holding the previous release's DMG; Sparkle
#                                             then publishes a delta update alongside the full one
#
# Output in .build/dist: Lumia-<version>.dmg, appcast.xml, and *.delta when there was a previous release.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
app="$here/../Lumia-macOS"
out="$here/../.build/release"
dist="$here/../.build/dist"
packages="$here/../.build/packages"
: "${DEVELOPER_DIR:=/Applications/Xcode-27.0.0.app/Contents/Developer}"
export DEVELOPER_DIR
version=${1:?version, e.g. 0.2.0}
sign=false
[ "${2:-}" = "--sign" ] && sign=true
build=$(git -C "$here" rev-list --count HEAD)

auth=""
notary=""
if [ -n "${ASC_KEY_ID:-}" ]; then
  auth="-authenticationKeyPath $ASC_KEY_PATH -authenticationKeyID $ASC_KEY_ID -authenticationKeyIssuerID $ASC_ISSUER_ID"
  notary="--key $ASC_KEY_PATH --key-id $ASC_KEY_ID --issuer $ASC_ISSUER_ID"
fi
if $sign; then
  [ -n "$notary" ] || { echo "dist: --sign needs ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH"; exit 1; }
  [ -f "${SPARKLE_KEY_PATH:-}" ] || { echo "dist: --sign needs SPARKLE_KEY_PATH"; exit 1; }
fi

"$here/core.sh" --release
(cd "$app" && xcodegen generate >/dev/null)
rm -rf "$out" "$dist" && mkdir -p "$out" "$dist"

# The archive is signed ad hoc so it carries the entitlements and needs no
# certificate on the machine; the export re-signs it with Developer ID.
# shellcheck disable=SC2086
xcodebuild -project "$app/Lumia.xcodeproj" -scheme Lumia -configuration Release \
  -archivePath "$out/Lumia.xcarchive" -derivedDataPath "$out/derived" -clonedSourcePackagesDirPath "$packages" \
  MARKETING_VERSION="$version" CURRENT_PROJECT_VERSION="$build" \
  CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY=- \
  archive | grep -E "error|warning: .*Sources|ARCHIVE (SUCCEEDED|FAILED)" || true
[ -d "$out/Lumia.xcarchive" ] || { echo "dist: no archive"; exit 1; }

# shellcheck disable=SC2086
xcodebuild -exportArchive -archivePath "$out/Lumia.xcarchive" -exportOptionsPlist "$app/ExportOptionsDeveloperID.plist" \
  -exportPath "$out/export" -allowProvisioningUpdates $auth | grep -E "error|EXPORT (SUCCEEDED|FAILED)" || true
bundle="$out/export/Lumia.app"
[ -d "$bundle" ] || { echo "dist: no export"; exit 1; }
codesign --verify --deep --strict "$bundle"
echo "dist: exported $version ($build), signed as: $(codesign -dv "$bundle" 2>&1 | grep '^Authority' | head -1 | cut -d= -f2)"

if $sign; then
  ditto -c -k --keepParent "$bundle" "$out/Lumia.zip"
  # shellcheck disable=SC2086
  xcrun notarytool submit "$out/Lumia.zip" $notary --wait
  xcrun stapler staple "$bundle"
fi

# The DMG: the app and a link to /Applications, read-only, compressed.
staging="$out/dmg"
rm -rf "$staging" && mkdir -p "$staging"
cp -R "$bundle" "$staging/"
ln -s /Applications "$staging/Applications"
dmg="$dist/Lumia-$version.dmg"
hdiutil create -volname "Lumia" -srcfolder "$staging" -ov -format UDZO -quiet "$dmg"
if $sign; then
  # shellcheck disable=SC2086
  xcrun notarytool submit "$dmg" $notary --wait
  xcrun stapler staple "$dmg"
fi

# The appcast. Sparkle's tools come with the package the archive resolved.
if [ -f "${SPARKLE_KEY_PATH:-}" ]; then
  tools="$packages/artifacts/sparkle/Sparkle/bin"
  [ -x "$tools/generate_appcast" ] || { echo "dist: no generate_appcast under $tools"; exit 1; }
  if [ -d "${LUMIA_PREVIOUS_DIR:-}" ]; then cp "$LUMIA_PREVIOUS_DIR"/*.dmg "$dist/" 2>/dev/null || true; fi
  "$tools/generate_appcast" --ed-key-file "$SPARKLE_KEY_PATH" \
    --download-url-prefix "https://github.com/lumia-chat/lumia/releases/download/v$version/" \
    --link "https://lumia.chat" --maximum-versions 1 --maximum-deltas 1 \
    -o "$dist/appcast.xml" "$dist"
  # Only this release's files are published; a previous DMG served the deltas.
  find "$dist" -name '*.dmg' ! -name "Lumia-$version.dmg" -delete
  rm -f "$dist"/*.html
else
  echo "dist: no SPARKLE_KEY_PATH, so no appcast"
fi
echo "dist: $(ls "$dist" | tr '\n' ' ')"
