#!/bin/sh
# Archives the macOS app in Release and uploads it to App Store Connect
# for TestFlight. The build number is the commit count on this branch, so
# every upload is newer than the last; the version is MARKETING_VERSION
# in project.yml.
#
#   scripts/release.sh             # archive only (no upload); the archive lands in .build/release
#   scripts/release.sh --upload    # archive and upload
#
# Signing is automatic with the team in project.yml; Xcode needs a way to
# reach the account: either an Apple ID signed in to Xcode, or an App
# Store Connect API key given as ASC_KEY_ID, ASC_ISSUER_ID and ASC_KEY_PATH
# (the .p8). The first export creates the Apple Distribution certificate
# and the Mac App Store profile if the account has none.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
app="$here/../Lumia-macOS"
out="$here/../.build/release"
: "${DEVELOPER_DIR:=/Applications/Xcode-27.0.0.app/Contents/Developer}"
export DEVELOPER_DIR
upload=false
[ "${1:-}" = "--upload" ] && upload=true

build=$(git -C "$here" rev-list --count HEAD)
auth=""
if [ -n "${ASC_KEY_ID:-}" ]; then
  auth="-authenticationKeyPath $ASC_KEY_PATH -authenticationKeyID $ASC_KEY_ID -authenticationKeyIssuerID $ASC_ISSUER_ID"
fi

"$here/core.sh" --release
(cd "$app" && xcodegen generate >/dev/null)
rm -rf "$out" && mkdir -p "$out"
# shellcheck disable=SC2086
xcodebuild -project "$app/Lumia.xcodeproj" -scheme Lumia -configuration Release \
  -archivePath "$out/Lumia.xcarchive" -derivedDataPath "$out/derived" \
  CURRENT_PROJECT_VERSION="$build" -allowProvisioningUpdates $auth archive | grep -E "error|warning: .*Sources|ARCHIVE (SUCCEEDED|FAILED)" || true
[ -d "$out/Lumia.xcarchive" ] || { echo "release: no archive"; exit 1; }
echo "release: archived build $build at $out/Lumia.xcarchive"
$upload || exit 0

# shellcheck disable=SC2086
xcodebuild -exportArchive -archivePath "$out/Lumia.xcarchive" -exportOptionsPlist "$app/ExportOptions.plist" \
  -exportPath "$out/export" -allowProvisioningUpdates $auth | grep -E "error|Upload|EXPORT (SUCCEEDED|FAILED)" || true
echo "release: uploaded build $build; it appears in TestFlight after processing"
