#!/bin/sh
# Archives the iOS app in Release and uploads it to App Store Connect for
# TestFlight and the App Store (architecture section 13, "Distribution":
# the store is the only channel a phone has). The Mac app is the desktop
# shell, a direct download (apps/desktop/scripts/dist-macos.sh).
#
#   scripts/release.sh                   # archive only; the archive lands in .build/release-ios
#   scripts/release.sh 0.2.0 --upload    # the version is the tag without its v; archive and upload
#
# The build number is the commit count on this branch, so every upload is
# newer than the last; without a version, MARKETING_VERSION in project.yml
# is used. Signing is automatic with the team in project.yml; Xcode needs a
# way to reach the account: an Apple ID signed in to Xcode, or an App Store
# Connect API key given as ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH (the
# .p8), which is what the release workflow uses. The first export creates
# the Apple Distribution certificate and the App Store profile if the
# account has none.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
app="$here/../Lumia-iOS"
out="$here/../.build/release-ios"
: "${DEVELOPER_DIR:=/Applications/Xcode-27.0.0.app/Contents/Developer}"
export DEVELOPER_DIR
version=""
upload=false
for arg in "$@"; do
  case "$arg" in
    --upload) upload=true ;;
    *) version=$arg ;;
  esac
done

build=$(git -C "$here" rev-list --count HEAD)
auth=""
if [ -n "${ASC_KEY_ID:-}" ]; then
  auth="-authenticationKeyPath $ASC_KEY_PATH -authenticationKeyID $ASC_KEY_ID -authenticationKeyIssuerID $ASC_ISSUER_ID"
fi
versioning=""
[ -n "$version" ] && versioning="MARKETING_VERSION=$version"

# The core for the device only: the archive has no simulator slice, and the
# XCFramework needs a macOS slice to assemble, this machine's.
"$here/core.sh" --release --host --ios-device
(cd "$app" && xcodegen generate >/dev/null)
rm -rf "$out" && mkdir -p "$out"
# shellcheck disable=SC2086
xcodebuild -project "$app/Lumia.xcodeproj" -scheme Lumia -configuration Release -destination 'generic/platform=iOS' \
  -archivePath "$out/Lumia.xcarchive" -derivedDataPath "$out/derived" \
  CURRENT_PROJECT_VERSION="$build" $versioning -allowProvisioningUpdates $auth archive | grep -E "error|warning: .*Sources|ARCHIVE (SUCCEEDED|FAILED)" || true
[ -d "$out/Lumia.xcarchive" ] || { echo "release: no archive"; exit 1; }
echo "release: archived iOS build $build${version:+ ($version)} at $out/Lumia.xcarchive"
$upload || exit 0

# The upload destination writes no summary plist, so the log is the record.
# shellcheck disable=SC2086
xcodebuild -exportArchive -archivePath "$out/Lumia.xcarchive" -exportOptionsPlist "$app/ExportOptions.plist" \
  -exportPath "$out/export" -allowProvisioningUpdates $auth > "$out/export.log" 2>&1 || true
grep -E "error|Upload succeeded|EXPORT (SUCCEEDED|FAILED)" "$out/export.log" || true
grep -q "Upload succeeded" "$out/export.log" || { echo "release: the upload did not finish; see $out/export.log"; exit 1; }
echo "release: uploaded iOS build $build; it appears in TestFlight after processing"
