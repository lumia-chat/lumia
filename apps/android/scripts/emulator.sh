#!/bin/sh
# Creates (once) and boots the emulator the smoke uses: an API 35 image
# for this machine's architecture, headless, snapshot kept. Needs the
# emulator and the system image from sdkmanager (see AGENTS.md).
#
#   scripts/emulator.sh         # boots and waits
set -eu
: "${ANDROID_HOME:?set ANDROID_HOME to the SDK}"
name=${LUMIA_AVD:-lumia}
case "$(uname -m)" in arm64|aarch64) arch=arm64-v8a ;; *) arch=x86_64 ;; esac
image="system-images;android-35;google_apis;$arch"
avd="$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager"
if ! "$avd" list avd 2>/dev/null | grep -q "Name: $name$"; then
  echo "no" | "$avd" create avd -n "$name" -k "$image" -d pixel_7 >/dev/null
fi
adb="$ANDROID_HOME/platform-tools/adb"
if "$adb" get-state >/dev/null 2>&1; then echo "emulator: a device is already on adb"; exit 0; fi
nohup "$ANDROID_HOME/emulator/emulator" -avd "$name" -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect > "${TMPDIR:-/tmp}/lumia-emulator.log" 2>&1 &
"$adb" wait-for-device
for _ in $(seq 1 120); do
  [ "$("$adb" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] && { echo "emulator: $name booted"; exit 0; }
  sleep 2
done
echo "emulator: did not boot"; exit 1
