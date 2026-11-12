#!/bin/sh
# Builds the Android app for a release: the core optimized and stripped
# for arm64 (the only ABI a phone has), the release APK signed with the
# release key, named by its version under .build/dist (architecture
# section 13, "Distribution": the APK is a GitHub Release's asset, as the
# desktop builds are; a store listing is a later channel, not a build).
#
#   scripts/release.sh            # the version in app/build.gradle.kts; unsigned without a key
#   scripts/release.sh 0.2.0      # the version is the tag without its v
#
# The version code is the commit count on this branch, so every build
# installs over the last. The key: ANDROID_KEYSTORE_PATH (a keystore from
# keytool), ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS and
# ANDROID_KEY_PASSWORD, which the release workflow fills from its secrets.
# Android installs an update only over a build signed with the same key,
# so the keystore is kept for the app's life. Without one the APK is built
# unsigned, which installs nowhere; the name says so.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/.."
out="$root/.build/dist"
version=${1:-}
build=$(git -C "$here" rev-list --count HEAD)

"$here/core.sh" --release --arm64
rm -rf "$out" && mkdir -p "$out"
# shellcheck disable=SC2086
(cd "$root" && ./gradlew -q assembleRelease --no-daemon -PlumiaBuild="$build" ${version:+-PlumiaVersion="$version"})

apk=$(ls "$root"/app/build/outputs/apk/release/*.apk | head -1)
[ -f "$apk" ] || { echo "release: no APK"; exit 1; }
label=${version:-$(sed -n 's/^val lumiaVersion = .*?: "\([^"]*\)".*/\1/p' "$root/app/build.gradle.kts")}
[ -n "$label" ] || { echo "release: no version given and none read from app/build.gradle.kts"; exit 1; }
case "$apk" in
  *unsigned*) name="Lumia-$label-unsigned.apk" ;;
  *) name="Lumia-$label.apk" ;;
esac
cp "$apk" "$out/$name"

# The signature checked with the SDK's apksigner when it is on hand. Its own verdict is
# the gate, and everything it said is shown when the verdict is no: the first release that
# ran this failed here with nothing to say why, because only one line of the output was
# looked for and the rest was dropped.
apksigner=$(ls "${ANDROID_HOME:-/nonexistent}"/build-tools/*/apksigner 2>/dev/null | sort -V | tail -1)
if [ -n "$apksigner" ] && [ "$name" = "Lumia-$label.apk" ]; then
  echo "release: checking the signature with $apksigner"
  if ! "$apksigner" verify --print-certs "$out/$name" >"$out/signature.txt" 2>&1; then
    cat "$out/signature.txt"
    echo "release: the APK's signature does not verify"; exit 1
  fi
  grep "certificate SHA-256" "$out/signature.txt" || { cat "$out/signature.txt"; echo "release: apksigner verified the APK and named no certificate"; exit 1; }
  rm -f "$out/signature.txt"
fi
echo "release: Android build $build${version:+ ($version)} at $out/$name ($(du -h "$out/$name" | cut -f1))"
