#!/bin/sh
# Smoke-runs the iOS app in a simulator against a dev instance (mix
# phx.server in apps/instance): seeds an account, a server, a channel and
# two messages with the release CLI, and someone else in that channel who
# writes a long message, replies to the account's and reacts to it (so a
# channel has two names, a message that wraps, a reference and chips),
# gives both a picture (scripts/picture.py, sent over HTTP) and has the
# other open a server DM with the account, so the avatars are pictures,
# installs the debug app, launches it
# signed in as that account (the LUMIA_DEV_* environment a debug build
# honours, passed through simctl's SIMCTL_CHILD_ prefix), and screenshots
# each screen the LUMIA_DEV_SCREEN switch can open (Server settings among
# them, since the account owns its server). Four are drawn again as
# Preferences can make them (LUMIA_DEV_APPEARANCE, LUMIA_DEV_PRESET and
# LUMIA_DEV_FONT_SIZE, which choose for one launch and store nothing):
# Preferences, Server settings and sign-in dark, and the channel compact,
# minimal light and dark, at font size 20, and minimal at 20.
#
#   scripts/smoke-ios.sh            # expects the app built for the simulator (xcodebuild, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://host:4000 LUMIA_SIMULATOR="iPhone 15 Pro" scripts/smoke-ios.sh
#   LUMIA_CLI=/path/to/lumia-cli scripts/smoke-ios.sh   # a CLI built elsewhere (another worktree)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
app="$here/../Lumia-iOS/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-ios}
rm -rf "$work" && mkdir -p "$work"
data="$work/cli"
password="correct-horse-battery-staple"
bundle=chat.lumia

for _ in $(seq 1 60); do curl -s -m 2 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 2 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
handle="ios$stamp"
"$cli" --data-dir "$data" instance add "$instance" >/dev/null
"$cli" --data-dir "$data" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$data" server-create "Phone smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" channel-create "$server" general >/dev/null
channel=$("$cli" --data-dir "$data" channels "$server" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" send "$channel" "Hello from the CLI" >/dev/null
first=$("$cli" --data-dir "$data" send "$channel" "The iOS app reads this through the same core." | sed -n 's/^acked at seq [0-9]*: //p' | tail -1)
# Someone else: their own name in the channel, a message long enough to
# wrap on a phone, a reply to the account's message and two reactions to it.
other="$work/other"
invite=$("$cli" --data-dir "$data" invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
"$cli" --data-dir "$other" instance add "$instance" >/dev/null
"$cli" --data-dir "$other" account create --handle "priya$stamp" --password "$password" --invite "$invite" >/dev/null
"$cli" --data-dir "$other" identity create >/dev/null
"$cli" --data-dir "$other" identity link-account --password "$password" >/dev/null
"$cli" --data-dir "$other" identity profile --display-name "Priya Raman" >/dev/null
"$cli" --data-dir "$other" send "$channel" "Morning. I went through the whole release list last night and everything on it is either done or has a name next to it, so we should be fine for Friday." >/dev/null
"$cli" --data-dir "$other" send "$channel" "Same core, same answers." --reply-to "$first" --quote "The iOS app reads this through the same core." >/dev/null
"$cli" --data-dir "$other" react "$channel" "$first" "👍" >/dev/null
"$cli" --data-dir "$other" react "$channel" "$first" "🎉" >/dev/null
# Pictures for both, over HTTP as an app sends them, and a server DM the
# other opens with the account, so the avatars drawn are pictures.
token() {
  curl -s -f -H "content-type: application/json" -H "accept: application/json" \
    -d "{\"handleOrEmail\":\"$1\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" |
    python3 -c "import sys, json; print(json.load(sys.stdin)['accessToken'])"
}
python3 "$here/picture.py" "$work/mine.png" 20,110,90 240,200,60
python3 "$here/picture.py" "$work/theirs.png" 200,60,120 60,90,220
for who in "$handle:mine" "priya$stamp:theirs"; do
  curl -s -f -X PUT -H "content-type: image/png" -H "accept: application/json" -H "authorization: Bearer $(token "${who%%:*}")" \
    --data-binary "@$work/${who#*:}.png" "$instance/api/v1/account/avatar" >/dev/null || { echo "FAIL: no picture for ${who%%:*}"; exit 1; }
done
me=$("$cli" --data-dir "$data" whoami | grep -oE "[0-9a-f-]{36}" | head -1)
dm=$(curl -s -f -X POST -H "content-type: application/json" -H "accept: application/json" -H "authorization: Bearer $(token "priya$stamp")" \
  -d "{\"accountId\":\"$me\"}" "$instance/api/v1/servers/$server/dms" | python3 -c "import sys, json; d = json.load(sys.stdin); d = d.get('channel', d); print(d['channelId'])")
"$cli" --data-dir "$other" send "$dm" "Got a minute after lunch?" >/dev/null
echo "seeded $handle, server $server, channel $channel, server DM $dm"

# A booted simulator of that name, or boot one; any iPhone when none has
# that name (the CI runner's are whatever its Xcode ships).
udid=$(xcrun simctl list devices available -j | python3 -c "
import json,sys
all=[d for r in json.load(sys.stdin)['devices'].values() for d in r]
devs=[d for d in all if d['name']=='$simulator'] or [d for d in all if d['name'].startswith('iPhone')]
devs.sort(key=lambda d: d['state']!='Booted')
print(devs[0]['udid'] + ' ' + devs[0]['name'] if devs else '')")
[ -n "$udid" ] || { echo "no iPhone simulator"; exit 1; }
echo "simulator: ${udid#* }"
udid=${udid%% *}
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
xcrun simctl install "$udid" "$app"

shoot() { # <screen> <seconds> [<appearance> [<preset> [<font size>]]]: the picture is named for all of them
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  picture="$1${4:+-$4}${5:+-$5}${3:+-$3}"
  # An appearance, a preset or a font size left empty names none: the launch reads what the phone has kept, which is the defaults.
  SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="$handle" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" SIMCTL_CHILD_LUMIA_DEV_SCREEN="$1" SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 \
  SIMCTL_CHILD_LUMIA_DEV_APPEARANCE="${3:-}" SIMCTL_CHILD_LUMIA_DEV_PRESET="${4:-}" SIMCTL_CHILD_LUMIA_DEV_FONT_SIZE="${5:-}" \
    xcrun simctl launch "$udid" "$bundle" >/dev/null
  sleep "$2"
  xcrun simctl io "$udid" screenshot "$work/$picture.png" >/dev/null 2>&1
  echo "  $picture: $work/$picture.png"
}
shoot home 8
alive=$(xcrun simctl spawn "$udid" launchctl list 2>/dev/null | grep -c "$bundle" || true)
# The app sends nothing; the CLI's message lands in its view when the gateway is up.
"$cli" --data-dir "$data" send "$channel" "Still here?" >/dev/null
for screen in server channel members serverdm profilecard requests devices profile preferences new add newchannel invite serversettings signin link; do shoot "$screen" 6; done
# As Preferences can make them: dark, and the channel's messages compact,
# minimal (one line) light and dark, at the largest font size, and both.
shoot preferences 6 dark
shoot serversettings 6 dark
shoot signin 6 dark
shoot channel 6 "" compact
shoot channel 6 dark compact
shoot channel 6 "" minimal
shoot channel 6 dark minimal
shoot channel 6 "" "" 20
shoot channel 6 "" minimal 20
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
[ "$alive" -gt 0 ] || { echo "FAIL: the app was not running after launch"; exit 1; }
echo "smoke-ios: ok (launched signed in, every screen screenshotted under $work)"
