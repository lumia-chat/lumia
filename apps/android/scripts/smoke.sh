#!/bin/sh
# Smoke-runs the Android app in an emulator against a dev instance (mix
# phx.server in apps/instance): seeds an account, a server, a channel and
# two messages with the release CLI, and a second account in the server
# with two of its own; installs the debug build, launches it signed in as
# the first account (the LUMIA_DEV_* extras a debug build honours, through
# the launch intent) and screenshots each screen the LUMIA_DEV_SCREEN
# switch can open. Then attachments: a picture and a file sent from the
# CLI drawn in a channel, a long press on the picture and on the file's
# Share opening the message's actions, the picture opened in the viewer, and a
# picture attached through the Photo Picker, sent and drawn. Then a video
# the CLI sent plays in its row, and with link cards on, a YouTube card's
# player and an X post load in their cards' place on a press. Then the rail: a second server dragged onto the first
# makes a folder the instance keeps, a tap opens it, Folder settings names
# it and Delete folder takes it away. Then Preferences: dark, Compact, Minimal (light and
# dark) and the font size at 20 as a launch can be told to look
# (LUMIA_DEV_APPEARANCE, LUMIA_DEV_PRESET, LUMIA_DEV_FONT_SIZE), and by taps
# from the account menu, the screen kept as the phone is turned, Minimal
# chosen and a setting changed after it, which is Custom, kept across a
# launch and put back, the screen kept as the phone goes dark.
# Then Server settings, as the server's owner: renamed, a shorter history
# saved after it says what it deletes, Show deleted messages turned on and
# off again, each saved as it is turned, a picture set from elsewhere and
# removed, and the rail drawing it (a plain member, at the very end, sees
# the picture and is not offered the screen). Then roles, titles and tags
# (architecture 7.5): the sections of Server settings, a role made by taps
# and one made elsewhere moved over it from its menu, the tag beside a
# name in the open channel following a role given, a title set and the
# server's choice of the two, each made elsewhere, a role deleted
# through the sheet that asks first, which says of one nobody holds that
# nobody does, a member's row and a role's editor saving what was changed
# in them and nothing a second session changed meanwhile, and this
# account's own title refused on its card once the instance's ten changes
# an hour are spent. Then a server's own emoji (specs/emoji.md), in a
# channel of their own: the composer's : list and its emoji button, a
# custom reaction from the picker, a removed emoji read as its :name:, and
# Server settings' Emoji renaming, removing (asked first) and uploading a
# picture named from its file, each on the instance after. Then the channel is read as assistive technology has it: each account's
# messages in a row are one block, and the second account, typing through
# the CLI, is named under the composer until it falls silent (protocol
# 6.7), in light and dark. Then drafts (issue #23): a reply and words
# typed in a channel and not sent are that channel's alone, there again
# when it is come back to and after a force-stop, untouched by an edit of
# a message, and gone for good once sent. Then push. Then signing out from the account
# menu: the phone's push subscription and its session must be gone from
# the instance, the sign-in screen must say nothing of it, a push that
# still arrives must show nothing, and signing in again must subscribe
# again. Then another account on the same phone: signed out of the first
# and into a third the script made, typed into the sign-in screen, the
# phone must show that account and nothing of the first, hold the phone's
# one session, and be sent the notices for it and none for the first.
# Last, the phone's session is revoked from elsewhere: with the app away,
# the instance must stop sending it notices before the phone has done
# anything, and the app opened finds itself signed out and says so; and
# with the app open, it must go back to sign-in and say so. The emulator
# reaches the host's instance at 10.0.2.2.
#
# Nothing bounds how long this takes but whoever runs it (no workflow
# does); it waits for what it needs and no longer where it can tell.
#
#   scripts/smoke.sh            # expects the app built (gradle assembleDebug) and the CLI, and an emulator or device on adb
#   LUMIA_INSTANCE=http://localhost:4000 scripts/smoke.sh
#   LUMIA_CLI=/path/to/lumia-cli scripts/smoke.sh   # a CLI built elsewhere (a git worktree has none of its own)
#   LUMIA_SMOKE_DB=postgres://postgres:postgres@localhost/lumia_dev scripts/smoke.sh   # the instance's database: the Emoji section at a limit of 0 too
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
emulated=$(printf '%s' "$instance" | sed 's#localhost#10.0.2.2#; s#127\.0\.0\.1#10.0.2.2#')
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
apk="$here/../app/build/outputs/apk/debug/app-debug.apk"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke}
rm -rf "$work" && mkdir -p "$work"
data="$work/cli"
password="correct-horse-battery-staple"
bundle=chat.lumia
: "${ANDROID_HOME:?set ANDROID_HOME to the SDK}"
adb="$ANDROID_HOME/platform-tools/adb"

for _ in $(seq 1 60); do curl -s -m 2 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 2 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -f "$apk" ] || { echo "no apk at $apk"; exit 1; }
"$adb" get-state >/dev/null 2>&1 || { echo "no device on adb; start an emulator (scripts/emulator.sh)"; exit 1; }

stamp=$(date +%s)
handle="android$stamp"
"$cli" --data-dir "$data" instance add "$instance" >/dev/null
"$cli" --data-dir "$data" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$data" server-create "Android smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" channel-create "$server" general >/dev/null
channel=$("$cli" --data-dir "$data" channels "$server" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" send "$channel" "Hello from the CLI" >/dev/null
"$cli" --data-dir "$data" send "$channel" "The Android app reads this through the same core." >/dev/null
# A second account, joined through an invite the phone's account mints:
# the other person in the channel. Two messages in a row from each are a
# block each; it is typing further down, and mentions the phone's account
# for the push after that.
other="$work/other"
other_handle="mention$stamp"
"$cli" --data-dir "$other" instance add "$instance" >/dev/null
"$cli" --data-dir "$other" account create --handle "$other_handle" --password "$password" >/dev/null
invite=$("$cli" --data-dir "$data" invite-create "$server" --max-uses 1 | cut -d' ' -f1)
"$cli" --data-dir "$other" join "$invite" >/dev/null \
  || { echo "FAIL: the second account could not join the server through the invite $invite"; exit 1; }
"$cli" --data-dir "$other" send "$channel" "Hello from someone else" >/dev/null
"$cli" --data-dir "$other" send "$channel" "Two in a row from one person are one block." >/dev/null
# A third account, for the phone to sign in as once it has signed out of
# the first: in no server of the first's, with one of its own that the
# second account joins, so there is someone to mention it there.
next="$work/next"
next_handle="next$stamp"
"$cli" --data-dir "$next" instance add "$instance" >/dev/null
"$cli" --data-dir "$next" account create --handle "$next_handle" --password "$password" >/dev/null
next_server=$("$cli" --data-dir "$next" server-create "Next smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$next" channel-create "$next_server" general >/dev/null
next_channel=$("$cli" --data-dir "$next" channels "$next_server" | grep -oE "[0-9a-f-]{36}" | head -1)
invite=$("$cli" --data-dir "$next" invite-create "$next_server" --max-uses 1 | cut -d' ' -f1)
"$cli" --data-dir "$other" join "$invite" >/dev/null \
  || { echo "FAIL: the second account could not join the third's server through the invite $invite"; exit 1; }
next_me=$("$cli" --data-dir "$next" whoami)
echo "seeded $handle, $other_handle and $next_handle, server $server, channel $channel"

"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" uninstall "$bundle" >/dev/null 2>&1 || true
"$adb" install -r "$apk" >/dev/null
# The phone is turned further down, and left upright and as it was found.
turning=$("$adb" shell settings get system accelerometer_rotation 2>/dev/null | tr -d '\r')
listener=""
hush() {
  [ -z "$listener" ] || { kill "$listener" 2>/dev/null || true; wait "$listener" 2>/dev/null || true; }
  listener=""
}
tidy() {
  hush
  "$adb" shell settings put system user_rotation 0 >/dev/null 2>&1 || true
  "$adb" shell settings put system accelerometer_rotation "${turning:-0}" >/dev/null 2>&1 || true
}
trap tidy EXIT

# Whose session a launch asks for; a launch keeps a session the phone already holds, whoever is named.
who=$handle
launch() { # <screen> <seconds> [more for am start: --es LUMIA_DEV_APPEARANCE dark, --es LUMIA_DEV_PRESET minimal]
  screen=$1 seconds=$2
  shift 2
  "$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
  "$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$who" --es LUMIA_DEV_PASSWORD "$password" --es LUMIA_DEV_SCREEN "$screen" "$@" >/dev/null
  sleep "$seconds"
}
shot() { # <name>
  "$adb" exec-out screencap -p > "$work/$1.png"
  echo "  $1: $work/$1.png"
}
shoot() { # <screen> <seconds>
  launch "$1" "$2"
  shot "$1"
}
# The screen as assistive technology has it: every text and description.
ui() {
  "$adb" shell uiautomator dump /sdcard/lumia-ui.xml >/dev/null 2>&1 || true
  "$adb" shell cat /sdcard/lumia-ui.xml 2>/dev/null || true
}
count() { # <text> <pattern>: how many times
  printf '%s' "$1" | grep -o "$2" | wc -l | tr -d ' '
}
# Whether the screen comes to read <text> within <seconds> (twenty): a wait
# for what is waited for, as long as that takes and no longer.
await() { # <text> [seconds]
  for _ in $(seq 1 "${2:-20}"); do
    case "$(ui)" in *"$1"*) return 0 ;; esac
    sleep 1
  done
  return 1
}
# The phone upright (0) or on its side (1). Android makes the activity again for it.
turn() { # <0|1>
  "$adb" shell settings put system accelerometer_rotation 0
  "$adb" shell settings put system user_rotation "$1"
  sleep 3
}
running() { "$adb" shell pidof "$bundle" 2>/dev/null | tr -d '\r' || true; }
# One fact about the first thing on screen whose text or description is the
# one named: where its middle is, whether it is the choice made (a radio
# button that is checked), whether the switch it labels is on (the row that
# takes the tap is what is checked, around the text; a choice drawn as a
# picture over its name, Preferences' themes and presets, is the same), how
# far down the screen it starts, whether it is wholly in view between the
# screen's top eighth and its last sixteenth (clear of a panel's header and
# the navigation bar), or whether a screen reader has it as a button (Compose
# gives what takes the tap a node of the Button class beside the text,
# inside the one that is clickable).
node() { # <text or description> <middle|chosen|on|top|inview|button>
  fact "$(ui)" "$1" "$2"
}
# The same of a screen already read, for several facts about one screen.
fact() { # <screen> <text or description> <middle|chosen|on|top|inview|button>
  printf '%s' "$1" | python3 -c '
import re, sys
want, fact = sys.argv[1], sys.argv[2]
nodes = re.findall(r"<node [^>]*>", sys.stdin.read())
def box(node):
    return tuple(int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
def within(a, b):
    return a[0] >= b[0] and a[1] >= b[1] and a[2] <= b[2] and a[3] <= b[3]
tall = box(nodes[0])[3] if nodes else 0
for node in nodes:
    if "text=\"%s\"" % want in node or "content-desc=\"%s\"" % want in node:
        x0, y0, x1, y1 = box(node)
        taps = sorted((box(n) for n in nodes if "clickable=\"true\"" in n and within(box(node), box(n))), key=lambda b: (b[2] - b[0]) * (b[3] - b[1]))
        button = bool(taps) and any("class=\"android.widget.Button\"" in n and within(box(n), taps[0]) for n in nodes)
        switch = sorted((n for n in nodes if "checkable=\"true\"" in n and within(box(node), box(n))), key=lambda n: (box(n)[2] - box(n)[0]) * (box(n)[3] - box(n)[1]))
        on = bool(switch) and "checked=\"true\"" in switch[0]
        inview = y0 >= tall // 8 and y1 <= tall - tall // 16
        print({"middle": "%d %d" % ((x0 + x1) // 2, (y0 + y1) // 2), "chosen": "yes" if "checked=\"true\"" in node else "no", "on": "yes" if on else "no", "top": y0, "inview": "yes" if inview else "no", "button": "yes" if button else "no"}[fact])
        break
' "$2" "$3"
}
# Scrolls a long screen (Preferences) to its top and then down a little at
# a time until something reading <text> is wholly in view; fails if it never is.
reveal() { # <text>
  for _ in 1 2 3 4 5; do "$adb" shell input swipe 540 700 540 2000 150; done
  for _ in $(seq 1 12); do
    [ "$(node "$1" inview)" = yes ] && return 0
    "$adb" shell input swipe 540 1700 540 1000 600
    sleep 1
  done
  echo "FAIL: scrolled through the screen and nothing reading \"$1\" came wholly into view"; exit 1
}
# The middle of the first thing reading <text> below the first reading
# <above>: one of two buttons with the same name, told apart by what is over it.
below() { # <text> <above>
  ui | python3 -c '
import re, sys
want, above = sys.argv[1], sys.argv[2]
nodes = re.findall(r"<node [^>]*>", sys.stdin.read())
def box(node):
    return tuple(int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
def reads(node, text):
    return "text=\"%s\"" % text in node or "content-desc=\"%s\"" % text in node
over = next((box(n)[1] for n in nodes if reads(n, above)), None)
for node in nodes:
    if over is not None and reads(node, want) and box(node)[1] > over:
        x0, y0, x1, y1 = box(node)
        print("%d %d" % ((x0 + x1) // 2, (y0 + y1) // 2))
        break
' "$1" "$2"
}
# The colour of the screen at the middle of the first thing reading
# <text>, as red green blue: what is drawn there, a picture or not.
colour() { # <text or description>
  at=$(node "$1" middle)
  [ -n "$at" ] || return 0
  "$adb" exec-out screencap | python3 -c '
import struct, sys
data = sys.stdin.buffer.read()
w, h = struct.unpack("<II", data[:8])
head = len(data) - w * h * 4
x, y = (int(n) for n in sys.argv[1].split())
i = head + (y * w + x) * 4
print("%d %d %d" % tuple(data[i:i + 3]))
' "$at"
}
# Whether a colour is the magenta of the script's picture.
magenta() { # <r g b>
  set -- $1
  [ "${1:-0}" -gt 200 ] && [ "${2:-255}" -lt 60 ] && [ "${3:-0}" -gt 200 ]
}
tap() { # <text or description>
  at=$(node "$1" middle)
  [ -n "$at" ] || { echo "FAIL: nothing on screen reads \"$1\" to tap"; exit 1; }
  # shellcheck disable=SC2086
  "$adb" shell input tap $at
  sleep 1
}
# What the app has stored on the phone beside the saved instance.
stored() {
  "$adb" shell run-as "$bundle" cat shared_prefs/lumia.xml 2>/dev/null || true
}
# What the phone remembers of the push subscriptions it has sent.
marks() {
  "$adb" shell run-as "$bundle" cat shared_prefs/push.xml 2>/dev/null || true
}
field() { python3 -c "import sys, json; d = json.load(sys.stdin); print($1)"; }
# A session of the script's own beside the phone's, over HTTP, as the phone's account or the one named: its token.
sign_in() { # [handle]
  curl -s -H "content-type: application/json" -H "accept: application/json" -d "{\"handleOrEmail\":\"${1:-$handle}\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" | field 'd["accessToken"]'
}
# The account's sessions as the instance lists them for <token>: an id and its device a line.
sessions() { # <token>
  curl -s -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1/sessions" | field '"\n".join(sorted(s["sessionId"] + " " + s.get("deviceId", "") for s in d.get("sessions", [])))'
}
# The lines of the first list that the second does not have.
missing() { # <list> <list>
  printf '%s\n' "$1" | while read -r line; do
    [ -z "$line" ] || printf '%s\n' "$2" | grep -qxF "$line" || echo "$line"
  done
}
lines() { # <list>: how many
  printf '%s' "$1" | grep -c . || true
}
# The room the channel's first messages take: how far under the first the
# one sent after the others starts.
room() {
  seen=$(ui)
  first=$(fact "$seen" "Hello from the CLI" top)
  last=$(fact "$seen" "Still here?" top)
  [ -n "$first" ] && [ -n "$last" ] || { echo "FAIL: the channel does not show the messages the script sent" >&2; exit 1; }
  echo $((last - first))
}
launch home 0
await 'text="connected"' || { shot home; echo "FAIL: the app launched signed in did not connect; see $work/home.png"; exit 1; }
shot home
alive=$(running)
# The app sends nothing; the CLI's message lands in its view when the gateway is up.
"$cli" --data-dir "$data" send "$channel" "Still here?" >/dev/null
for screen in server channel requests devices approve profile preferences new add newchannel invite serversettings serveroverview roles newrole servermembers serveremoji browse members addpeople newdm profilecard yoursettings startover needssetup signin link; do shoot "$screen" 7; done
[ -n "$alive" ] || { echo "FAIL: the app was not running after launch"; exit 1; }

# Attachments (specs/media.md 5), in a channel of their own so the rest
# reads #general as it was: the phone's account sends a picture and a file
# from the CLI, another device of its own, which the channel must draw
# (the picture, with no alt text, as "Photo"; the file as "File, name,
# size"), a long press on either open the message's actions and nothing
# else, and a tap the viewer; then the phone attaches a picture the way a
# person does, through the attach button and the system's Photo Picker,
# and sends it, and the channel must draw it (`channel-media.png`). A CLI
# without `send --attach` cannot seed this, and the script says so.
"$cli" --data-dir "$data" channel-create "$server" media >/dev/null
media=$("$cli" --data-dir "$data" channels "$server" | grep " #media " | grep -oE "[0-9a-f-]{36}" | head -1)
[ -n "$media" ] || { echo "FAIL: the media channel was not made"; exit 1; }
square="$work/square.png"
python3 -c '
import struct, sys, zlib
w, h = 320, 200
rows = b"".join(b"\x00" + b"\x20\xc0\x40" * w for _ in range(h))
def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
' "$square"
printf 'Notes for the smoke.\n' > "$work/notes.txt"
"$cli" --data-dir "$data" send "$media" "A picture and a file" --attach "$square" --attach "$work/notes.txt" >/dev/null \
  || { echo "FAIL: the CLI could not send attachments (LUMIA_CLI must name one with send --attach)"; exit 1; }
launch channel 0 --es LUMIA_DEV_CHANNEL media
await 'content-desc="Photo"' 30 || { shot channel-media; echo "FAIL: the channel does not draw the picture the CLI sent; see $work/channel-media.png"; exit 1; }
await 'content-desc="File, notes.txt, 21 bytes"' 5 || { shot channel-media; echo "FAIL: the channel does not draw the file the CLI sent as a row; see $work/channel-media.png"; exit 1; }
# A long press on what a message carries is the message's, as on its words:
# its actions and nothing of the piece's own tap (the viewer, the share
# sheet). The picture, then the file row's Share.
press() { # <text or description>: held a second where it is and let go
  at=$(node "$1" middle)
  [ -n "$at" ] || { echo "FAIL: nothing on screen reads \"$1\" to press"; exit 1; }
  # shellcheck disable=SC2086
  set -- $at
  "$adb" shell input swipe "$1" "$2" "$1" "$2" 1000
  sleep 1
}
for piece in "Photo" "Share notes.txt"; do
  press "$piece"
  await 'content-desc="Message actions"' 5 || { shot media-held; echo "FAIL: a long press on \"$piece\" did not open the message's actions; see $work/media-held.png"; exit 1; }
  seen=$(ui)
  case "$seen" in *'text="Reply"'*) ;; *) shot media-held; echo "FAIL: the message's actions after a long press on \"$piece\" do not offer Reply; see $work/media-held.png"; exit 1 ;; esac
  case "$seen" in *'content-desc="Share"'*|*'resource-id="android:id/chooser'*|*'resolver_list'*) shot media-held; echo "FAIL: a long press on \"$piece\" also did the piece's own tap; see $work/media-held.png"; exit 1 ;; esac
  [ "$piece" = "Photo" ] && shot media-held
  "$adb" shell input keyevent BACK
  for _ in $(seq 1 10); do case "$(ui)" in *'content-desc="Message actions"'*) sleep 1 ;; *) break ;; esac; done
done
tap "Photo"
await 'content-desc="Share"' 10 || { shot viewer; echo "FAIL: the picture did not open in the viewer; see $work/viewer.png"; exit 1; }
shot viewer
"$adb" shell input keyevent BACK
sleep 1
# The phone's own: the picture put where the Photo Picker finds it, newest first.
"$adb" push "$square" "/sdcard/Pictures/lumia-smoke-$stamp.png" >/dev/null
"$adb" shell am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d "file:///sdcard/Pictures/lumia-smoke-$stamp.png" >/dev/null
sleep 2
tap "Attach"
tap "Photo or video"
await 'Photo taken on' 15 || { shot picker; echo "FAIL: the Photo Picker did not open; see $work/picker.png"; exit 1; }
first=$(ui | python3 -c '
import re, sys
for node in re.findall(r"<node [^>]*>", sys.stdin.read()):
    if "content-desc=\"Photo taken on" in node:
        x0, y0, x1, y1 = (int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
        print("%d %d" % ((x0 + x1) // 2, (y0 + y1) // 2))
        break
')
# shellcheck disable=SC2086
"$adb" shell input tap $first
sleep 1
tap "Add (1)"
# In the strip with its × (the picker names it by its own id, not the file's name).
await 'content-desc="Remove ' 15 || { shot attaching; echo "FAIL: the picture chosen is not in the composer's strip; see $work/attaching.png"; exit 1; }
shot attaching
tap "Send"
for _ in $(seq 1 30); do
  [ "$(count "$(ui)" 'content-desc="Photo"')" -ge 2 ] && break
  sleep 1
done
shot channel-media
[ "$(count "$(ui)" 'content-desc="Photo"')" -ge 2 ] || { echo "FAIL: the picture the phone sent is not drawn in the channel; see $work/channel-media.png"; exit 1; }
"$adb" shell rm -f "/sdcard/Pictures/lumia-smoke-$stamp.png" >/dev/null 2>&1 || true
echo "  attachments: $work/channel-media.png (the CLI's picture and file drawn, a long press on either the message's actions, the viewer opened by a tap, the phone's picture attached through the Photo Picker, sent and drawn)"

# Playing in the conversation (specs/media.md 5 and 6.6, issue #21), in
# the media channel: a video the CLI sends plays in its row on a tap,
# with Media3's controls and its full-screen button (`video-inline.png`).
# Then, where the instance makes link cards (LUMIA_LINK_CARDS=true, and
# the internet), a YouTube link's card has its play button and a press
# puts the site's player in a web view in its place (`embed-youtube.png`),
# and an X post's "Show post" does the same (`embed-post.png`), the
# player going back to its card, one shown at a time; a recording then
# started in its row plays and leaves the post shown (`audio-post.png`),
# the core's rule of what playing stops. The script checks
# the web view is there; what the sites drew is the screenshots'. YouTube
# refuses a page whose referrer is a bare address over http, as a dev
# instance at 10.0.2.2 is ("This video is unavailable"); an instance on
# its https name plays.
"$cli" --data-dir "$data" send "$media" "A video" --attach "$root/apps/desktop/src/scenes/clip.mp4" >/dev/null \
  || { echo "FAIL: the CLI could not send the video"; exit 1; }
launch channel 0 --es LUMIA_DEV_CHANNEL media
await 'content-desc="Video, clip.mp4, 0:03"' 30 || { shot video-inline; echo "FAIL: the channel does not draw the video the CLI sent; see $work/video-inline.png"; exit 1; }
tap "Video, clip.mp4, 0:03"
await 'content-desc="Enter fullscreen"' 20 || { shot video-inline; echo "FAIL: the video did not play in its row with the player's controls; see $work/video-inline.png"; exit 1; }
shot video-inline
# The middle of the first thing whose description starts with <words>.
starting() { # <words>
  ui | python3 -c '
import re, sys
for node in re.findall(r"<node [^>]*>", sys.stdin.read()):
    if "content-desc=\"%s" % sys.argv[1] in node:
        x0, y0, x1, y1 = (int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
        print("%d %d" % ((x0 + x1) // 2, (y0 + y1) // 2))
        break
' "$1"
}
press_card() { # <label>: the card's own press, under its words
  for _ in $(seq 1 60); do at=$(starting "$1"); [ -n "$at" ] && break; sleep 1; done
  [ -n "$at" ] || { shot embeds; echo "FAIL: no card reads \"$1\"; see $work/embeds.png"; exit 1; }
  # shellcheck disable=SC2086
  "$adb" shell input tap $at
}
if curl -s "$instance/api/v1/instance" | grep -q '"linkCardsEnabled":true'; then
  "$cli" --data-dir "$data" send "$media" "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --cards >/dev/null
  press_card "Play video from YouTube"
  await 'class="android.webkit.WebView"' 20 || { shot embed-youtube; echo "FAIL: pressing the YouTube card put no web view in its place; see $work/embed-youtube.png"; exit 1; }
  sleep 8
  shot embed-youtube
  "$cli" --data-dir "$data" send "$media" "https://x.com/jack/status/20" --cards >/dev/null
  press_card "Show post from X"
  # The card gives way to the post's web view (its description the card's title).
  for _ in $(seq 1 20); do [ -z "$(starting "Show post from X")" ] && break; sleep 1; done
  # The player's card, back in its place with its picture, is taller than the player was and
  # can push the post under the composer: the end of the list is brought into view.
  list_end() { "$adb" shell input swipe 540 1600 540 500 300; sleep 1; }
  list_end
  [ -z "$(starting "Show post from X")" ] && await 'class="android.webkit.WebView"' 10 \
    || { shot embed-post; echo "FAIL: Show post put no web view in the X card's place; see $work/embed-post.png"; exit 1; }
  sleep 6
  shot embed-post
  # One at a time: the YouTube player went back to its card (a web view's frames read as web views too, so the card is what is looked for).
  [ -n "$(starting "Play video from YouTube")" ] || { echo "FAIL: the YouTube player did not go back to its card when the post was shown; see $work/embed-post.png"; exit 1; }
  # A post is being read, not played (the core's `playbackStops`): a recording the CLI sends
  # plays in its row on a tap, and the post stays shown over it.
  python3 -c '
import math, struct, sys, wave
w = wave.open(sys.argv[1], "wb"); w.setnchannels(1); w.setsampwidth(2); w.setframerate(8000)
w.writeframes(b"".join(struct.pack("<h", int(3000 * math.sin(2 * math.pi * 330 * i / 8000))) for i in range(8000 * 40)))
w.close()' "$work/tone.wav"
  "$cli" --data-dir "$data" send "$media" "A recording" --attach "$work/tone.wav" >/dev/null \
    || { echo "FAIL: the CLI could not send the recording"; exit 1; }
  sleep 3
  list_end
  await 'content-desc="Audio, tone.wav"' 30 || { shot audio-post; echo "FAIL: the channel does not draw the recording the CLI sent; see $work/audio-post.png"; exit 1; }
  tap "Audio, tone.wav"
  await 'content-desc="Audio, tone.wav, playing"' 20 || { shot audio-post; echo "FAIL: the recording did not play in its row; see $work/audio-post.png"; exit 1; }
  shot audio-post
  [ -z "$(starting "Show post from X")" ] && await 'class="android.webkit.WebView"' 5 \
    || { echo "FAIL: the X post went back to its card when a recording started; see $work/audio-post.png"; exit 1; }
  echo "  playing: $work/video-inline.png, $work/embed-youtube.png, $work/embed-post.png, $work/audio-post.png (look at what the sites drew)"
else
  echo "  playing: $work/video-inline.png; the instance makes no link cards (LUMIA_LINK_CARDS), so no embed was pressed"
fi

# The rail's order and folders (architecture 6.9). A second server of the
# phone's account, long pressed and dragged onto the first's middle, makes
# a folder of the two, which the instance keeps and the phone draws closed;
# a tap opens it, which only the phone keeps (`open_folders`); the folder's
# menu, a long press let go, offers Folder settings, whose name the
# instance keeps, and Delete folder, which leaves its servers where it was,
# the first first, so the rest of the script finds the rail as it was.
# The middle of the first thing whose description starts with <text>
# (a tile's says what is unread after its name), or how far down it starts.
starts() { # <screen> <text> [top]
  printf '%s' "$1" | python3 -c '
import re, sys
want, fact = sys.argv[1], sys.argv[2]
for node in re.findall(r"<node [^>]*>", sys.stdin.read()):
    if "content-desc=\"%s" % want in node:
        x0, y0, x1, y1 = (int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
        print(y0 if fact == "top" else "%d %d" % ((x0 + x1) // 2, (y0 + y1) // 2))
        break
' "$2" "${3:-middle}"
}
# A long press at <x> <y>, moved to the other two in steps, and let go there.
drag() { # <x> <y> <x> <y>
  "$adb" shell input motionevent DOWN "$1" "$2"
  sleep 1
  for i in 1 2 3 4 5 6 7 8; do "$adb" shell input motionevent MOVE "$(($1 + ($3 - $1) * i / 8))" "$(($2 + ($4 - $2) * i / 8))"; done
  sleep 0.5
  "$adb" shell input motionevent UP "$3" "$4"
  sleep 1
}
# A long press let go without moving: a tile's menu.
hold() { # <x> <y>
  "$adb" shell input motionevent DOWN "$1" "$2"
  sleep 1
  "$adb" shell input motionevent UP "$1" "$2"
  sleep 1
}
# The folders on the account's rail as the instance keeps them: a name and its servers a line.
folders() { # <token>
  curl -s -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1/account/snapshot" \
    | field '"\n".join(e["folder"].get("name", "") + " " + ",".join(e["folder"]["serverIds"]) for e in d.get("rail", {}).get("entries", []) if "folder" in e)'
}
rail_server=$("$cli" --data-dir "$data" server-create "Rail smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
launch server 0
await 'content-desc="Rail smoke' || { shot rail; echo "FAIL: the rail does not show a server made after launch; see $work/rail.png"; exit 1; }
seen=$(ui)
# shellcheck disable=SC2046
drag $(starts "$seen" "Rail smoke") $(starts "$seen" "Android smoke")
await 'content-desc="Folder, Android smoke and Rail smoke' || { shot rail-folder; echo "FAIL: a server dragged onto another made no folder of the two; see $work/rail-folder.png"; exit 1; }
shot rail-folder
rail_token=$(sign_in)
[ "$(folders "$rail_token")" = " $server,$rail_server" ] || { echo "FAIL: the instance does not keep the folder the drag made: $(folders "$rail_token")"; exit 1; }
# shellcheck disable=SC2046
"$adb" shell input tap $(starts "$(ui)" "Folder, Android smoke and Rail smoke")
await 'content-desc="Rail smoke' || { shot rail-open; echo "FAIL: a tap did not open the folder; see $work/rail-open.png"; exit 1; }
shot rail-open
stored | grep -qE 'name="open_folders">[0-9a-f-]{36}<' || { echo "FAIL: the phone does not keep the folder open: $(stored)"; exit 1; }
# shellcheck disable=SC2046
hold $(starts "$(ui)" "Folder, Android smoke and Rail smoke")
await "Folder settings…" || { shot rail-menu; echo "FAIL: a long press let go did not open the folder's menu; see $work/rail-menu.png"; exit 1; }
shot rail-menu
tap "Folder settings…"
await "Without a name it reads as its servers." || { echo "FAIL: Folder settings did not open"; exit 1; }
# The empty name field shows the folder's words in its place.
tap "Android smoke and Rail smoke"
"$adb" shell input text "Smokes"
"$adb" shell input keyevent 111
sleep 1
shot rail-settings
tap Save
await 'content-desc="Folder, Smokes' || { shot rail-named; echo "FAIL: the folder does not read as the name saved; see $work/rail-named.png"; exit 1; }
[ "$(folders "$rail_token")" = "Smokes $server,$rail_server" ] || { echo "FAIL: the instance does not have the folder's name: $(folders "$rail_token")"; exit 1; }
# shellcheck disable=SC2046
hold $(starts "$(ui)" "Folder, Smokes")
await "Delete folder" || { echo "FAIL: the folder's menu has no Delete folder"; exit 1; }
tap "Delete folder"
for _ in $(seq 1 10); do [ -z "$(folders "$rail_token")" ] && break; sleep 1; done
[ -z "$(folders "$rail_token")" ] || { echo "FAIL: the instance still has the folder deleted: $(folders "$rail_token")"; exit 1; }
seen=$(ui)
case "$seen" in *'content-desc="Folder,'*) shot rail-deleted; echo "FAIL: the rail still draws the folder deleted; see $work/rail-deleted.png"; exit 1 ;; esac
[ "$(starts "$seen" "Android smoke" top)" -lt "$(starts "$seen" "Rail smoke" top)" ] 2>/dev/null \
  || { shot rail-deleted; echo "FAIL: the deleted folder's servers are not where it was, in its order; see $work/rail-deleted.png"; exit 1; }
shot rail-deleted
echo "  rail: ok (a drag onto a server made a folder, kept by the instance; a tap opened it, kept by the phone; Folder settings named it and the instance has the name; Delete folder left its servers in its place)"

# Preferences. First how a launch can be told to look, for the eye: the
# Preferences screen dark on a light phone, and the channel in Compact and
# in Minimal, light and dark, beside channel.png, which is Comfortable, and
# at the font size 20. Compact must give the same messages less room and
# Minimal less again, every message in it one line that names its author;
# 20 must give them more. None of it may be kept: the extras are that
# launch's, and what is drawn after them is as the phone chose.
launch channel 7
roomy=$(room)
launch preferences 7 --es LUMIA_DEV_APPEARANCE dark
shot preferences-dark
[ "$(node Dark on)" = yes ] || { echo "FAIL: a launch told to be dark does not show Dark as the choice; see $work/preferences-dark.png"; exit 1; }
launch channel 7 --es LUMIA_DEV_PRESET compact
shot channel-compact
tight=$(room)
[ "$tight" -gt 0 ] && [ "$tight" -lt "$roomy" ] \
  || { echo "FAIL: Compact did not give the channel's messages less room ($tight against $roomy); see $work/channel-compact.png"; exit 1; }
launch channel 7 --es LUMIA_DEV_PRESET minimal
shot channel-minimal
least=$(room)
[ "$least" -gt 0 ] && [ "$least" -lt "$tight" ] \
  || { echo "FAIL: Minimal did not give the channel's messages less room than Compact ($least against $tight); see $work/channel-minimal.png"; exit 1; }
# One line: the phone's account sent three of the messages, two in a row,
# and each is a line with its name, on the line of its words.
seen=$(ui)
names=$(count "$seen" "text=\"$handle\"")
[ "$names" = 3 ] || { echo "FAIL: in Minimal every message names its author, and $handle is named $names times for three messages; see $work/channel-minimal.png"; exit 1; }
# The last message's name and words: side by side, their middles level
# (the name's box is its touch target, 48 high around it, so tops differ).
beside=$(printf '%s' "$seen" | python3 -c '
import re, sys
words, name = sys.argv[1], sys.argv[2]
nodes = re.findall(r"<node [^>]*>", sys.stdin.read())
def box(node):
    return tuple(int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
w = [box(n) for n in nodes if "text=\"%s\"" % words in n][-1]
n = [box(n) for n in nodes if "text=\"%s\"" % name in n][-1]
level = abs((w[1] + w[3]) // 2 - (n[1] + n[3]) // 2) < 12
print("yes" if level and w[0] >= n[2] else "no: words at %s, name at %s" % (w, n))
' "Still here?" "$handle")
[ "$beside" = yes ] || { echo "FAIL: in Minimal a message's name is not on the line of its words ($beside); see $work/channel-minimal.png"; exit 1; }
launch channel 7 --es LUMIA_DEV_PRESET minimal --es LUMIA_DEV_APPEARANCE dark
shot channel-minimal-dark
launch channel 7 --es LUMIA_DEV_FONT_SIZE 20
shot channel-font-20
large=$(room)
[ "$large" -gt "$roomy" ] \
  || { echo "FAIL: the font size 20 did not give the channel's messages more room ($large against $roomy); see $work/channel-font-20.png"; exit 1; }
case "$(stored)" in *appearance*|*layout*|*spacing*|*avatars*|*font_size*|*images*|*embeds*) echo "FAIL: a launch's LUMIA_DEV_APPEARANCE, LUMIA_DEV_PRESET or LUMIA_DEV_FONT_SIZE was kept on the phone"; exit 1 ;; esac
# Then as a person gets there and chooses: the rail's own avatar opens the
# account's menu, the menu leads to Preferences, each choice is the one
# shown at the next launch, and then both are put back, so what follows is
# drawn as it was before.
launch home 7
tap "$handle, your account"
shot account
# The menu's last item is the way out, under the rest, and a button named "Sign out" to a screen reader.
seen=$(ui)
[ "$(fact "$seen" "Sign out" button)" = yes ] || { echo "FAIL: the account menu has no button named \"Sign out\"; see $work/account.png"; exit 1; }
for item in "View profile" Preferences Devices; do
  [ "$(fact "$seen" "Sign out" top)" -gt "$(fact "$seen" "$item" top)" ] || { echo "FAIL: Sign out is not under $item in the account menu; see $work/account.png"; exit 1; }
done
tap Preferences
seen=$(ui)
case "$seen" in *"How Lumia looks on this phone"*) ;; *) echo "FAIL: the account menu did not lead to Preferences; see $work/account.png"; exit 1 ;; esac
[ "$(fact "$seen" System on)" = yes ] || { echo "FAIL: a phone that has chosen nothing is not System"; exit 1; }
# The preview, at the head of Messages, is one picture to a screen reader:
# its name, and none of its sample messages' words.
shot preferences
case "$seen" in *'content-desc="Preview of messages with these settings"'*) ;; *) echo "FAIL: Preferences has no preview of messages named for a screen reader; see $work/preferences.png"; exit 1 ;; esac
case "$seen" in *"Priya Nair"*|*"design-review"*) echo "FAIL: the preview's messages are read out one by one, not as one picture; see $work/preferences.png"; exit 1 ;; esac
reveal Comfortable
[ "$(node Comfortable on)" = yes ] || { echo "FAIL: a phone that has chosen nothing is not Comfortable"; exit 1; }
reveal Appearance
# The phone turned with the screen open, and turned back. Android makes
# the activity again for that, and it must be the same app over the same
# session: the process it was, and the screen the person was on, which
# taps led to and nothing of the launch names.
process=$(running)
for way in 1 0; do
  turn "$way"
  [ "$way" = 0 ] || shot preferences-turned
  case "$(ui)" in *"How Lumia looks on this phone"*) ;; *) echo "FAIL: the phone was turned and the app left the screen it was on; see $work/preferences-turned.png"; exit 1 ;; esac
  [ -n "$process" ] && [ "$(running)" = "$process" ] || { echo "FAIL: the app did not survive the phone being turned"; exit 1; }
done
# Minimal writes its values and its card is lit; one line draws no
# avatars, so they are not offered, and its switches are off, each line
# saying what that does. A switch turned on says so, and off again it is
# Minimal again. Then the spacing changed after it: no preset matches,
# none is lit, and the page says Custom. All of it is what the next launch
# has. The page is taller than the screen, so each is scrolled to first.
tap Dark
reveal Minimal
tap Minimal
seen=$(ui)
[ "$(fact "$seen" Minimal on)" = yes ] || { shot preferences-minimal; echo "FAIL: Minimal was chosen and its card is not lit; see $work/preferences-minimal.png"; exit 1; }
case "$seen" in *"match no preset"*) shot preferences-minimal; echo "FAIL: Minimal was chosen and the page says Custom; see $work/preferences-minimal.png"; exit 1 ;; esac
reveal "Font size"
shot preferences-minimal
seen=$(ui)
[ "$(fact "$seen" "One line" chosen)" = yes ] && [ "$(fact "$seen" Tight chosen)" = yes ] \
  || { echo "FAIL: Minimal was chosen and is not shown as one line and tight; see $work/preferences-minimal.png"; exit 1; }
case "$seen" in *'text="Avatars"'*) echo "FAIL: one line offers the avatars it does not draw; see $work/preferences-minimal.png"; exit 1 ;; esac
reveal Reactions
shot preferences-switches
seen=$(ui)
for switch in Images Embeds Reactions; do
  [ "$(fact "$seen" "$switch" on)" = no ] || { echo "FAIL: Minimal was chosen and $switch is still on; see $work/preferences-switches.png"; exit 1; }
done
case "$seen" in *"As links: each one a file row that opens the viewer."*"As links: no cards."*"Hidden. You can still add one from a message"*) ;; *) echo "FAIL: the switches off do not say what that does; see $work/preferences-switches.png"; exit 1 ;; esac
tap Reactions
seen=$(ui)
[ "$(fact "$seen" Reactions on)" = yes ] || { echo "FAIL: Reactions was tapped and is not on"; exit 1; }
case "$seen" in *"Shown under each message."*) ;; *) echo "FAIL: Reactions turned on does not say so"; exit 1 ;; esac
tap Reactions
reveal Roomy
tap Roomy
launch preferences 7
[ "$(node Dark on)" = yes ] || { shot preferences-kept; echo "FAIL: Dark was chosen and the next launch does not have it; see $work/preferences-kept.png"; exit 1; }
reveal Comfortable
shot preferences-kept
seen=$(ui)
for preset in Comfortable Compact Minimal; do
  [ "$(fact "$seen" "$preset" on)" = no ] || { echo "FAIL: one line and roomy match no preset, and $preset is lit; see $work/preferences-kept.png"; exit 1; }
done
case "$seen" in *"Custom: these settings match no preset."*) ;; *) echo "FAIL: settings that match no preset do not say Custom; see $work/preferences-kept.png"; exit 1 ;; esac
reveal "Font size"
seen=$(ui)
[ "$(fact "$seen" "One line" chosen)" = yes ] && [ "$(fact "$seen" Roomy chosen)" = yes ] \
  || { echo "FAIL: one line and roomy were chosen and the next launch does not have them; see $work/preferences-kept.png"; exit 1; }
reveal Appearance
tap System
reveal Comfortable
tap Comfortable
launch preferences 7
[ "$(node System on)" = yes ] || { echo "FAIL: System was chosen again and the next launch does not have it"; exit 1; }
reveal Comfortable
seen=$(ui)
[ "$(fact "$seen" Comfortable on)" = yes ] || { echo "FAIL: Comfortable was chosen again and the next launch does not have it"; exit 1; }
case "$seen" in *"match no preset"*) echo "FAIL: Comfortable was chosen again and the page says Custom"; exit 1 ;; esac
reveal "Font size"
seen=$(ui)
[ "$(fact "$seen" Grouped chosen)" = yes ] && [ "$(fact "$seen" Large chosen)" = yes ] \
  || { echo "FAIL: Comfortable was chosen again and the page is not grouped with large avatars"; exit 1; }
# System is whichever the phone is in, as it changes: the phone goes dark
# with the screen open, the picture is dark for the eye, and the screen is
# still the one the person was on (a phone going dark at sunset does not
# send anyone back to Home).
night=$("$adb" shell cmd uimode night 2>/dev/null | grep -oE "yes|no|auto" | head -1)
"$adb" shell cmd uimode night yes >/dev/null
sleep 3
shot preferences-system-dark
seen=$(ui)
"$adb" shell cmd uimode night "${night:-no}" >/dev/null
case "$seen" in *"How Lumia looks on this phone"*) ;; *) echo "FAIL: the phone went dark and the app left the screen it was on; see $work/preferences-system-dark.png"; exit 1 ;; esac
echo "  preferences: ok (dark, Compact, Minimal and the font size 20 for a launch and not kept, $roomy against $tight, $least and $large; Minimal one line a message; reached from the account menu; the screen kept as the phone was turned and turned back; the preview one picture to a screen reader; Minimal chosen and lit, no Avatars, its switches off and saying so, Reactions turned on and off, then Custom, kept across a launch and put back; the screen kept as the phone went dark)"

# Server settings (architecture 6.7), as the server's owner gets there:
# under its channels, Server settings; the name changed and saved, and the
# instance has it; a shorter history chosen, which says what it would
# delete and changes nothing until Save, and then the instance has it;
# Show deleted messages turned on and off, the instance having each. A
# picture set from elsewhere (the CLI) reaches the open screen through the
# account log, is removed there, and once set again the rail draws it. The
# second account, a plain member, is not offered the screen: last, below.
picture="$work/picture.png"
python3 -c '
import struct, sys, zlib
side = 64
rows = b"".join(b"\x00" + b"\xff\x00\xff" * side for _ in range(side))
def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", side, side, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
' "$picture"
owner=$(sign_in)
# One field of the server as the instance has it, or <default> when the field is at its default and so not sent.
held() { # <field> <default>
  curl -s -H "accept: application/json" -H "authorization: Bearer $owner" "$instance/api/v1/servers/$server" | field "d.get('$1', $2)"
}
until_held() { # <field> <default> <value>: within twenty seconds
  for _ in $(seq 1 20); do
    [ "$(held "$1" "$2")" = "$3" ] && return 0
    sleep 1
  done
  return 1
}
renamed="Android smoke renamed"
launch server 7
[ "$(node "Server settings" button)" = yes ] || { echo "FAIL: the server's owner is not offered Server settings; see $work/server.png"; exit 1; }
tap "Server settings"
# Its sections are a screen of their own (architecture 7.5); the owner has all three, and Overview is today's cards.
await 'text="Members"' || { echo "FAIL: Server settings did not open on its sections"; exit 1; }
seen=$(ui)
for section in Overview Roles Members; do
  case "$seen" in *"text=\"$section\""*) ;; *) shot sections; echo "FAIL: the server's owner is not offered the $section section; see $work/sections.png"; exit 1 ;; esac
done
shot sections
tap Overview
await "How long this server keeps its messages." || { echo "FAIL: Overview did not open"; exit 1; }
seen=$(ui)
[ "$(fact "$seen" Forever chosen)" = yes ] || { echo "FAIL: a server that keeps its messages for ever does not show Forever as the choice"; exit 1; }
case "$seen" in *'content-desc="Android smoke has no picture"'*) ;; *) echo "FAIL: the server's picture is not described as none"; exit 1 ;; esac
case "$seen" in *'text="Remove"'*) echo "FAIL: Remove is offered for a server with no picture"; exit 1 ;; esac
# The field is the one under its label that holds the name.
at=$(below "Android smoke" Name)
[ -n "$at" ] || { echo "FAIL: no Name field holding the server's name"; exit 1; }
# shellcheck disable=SC2086
"$adb" shell input tap $at
"$adb" shell input keyevent KEYCODE_MOVE_END
"$adb" shell input text "%srenamed"
tap Save
await 'text="Saved."' || { shot serversettings-name; echo "FAIL: the name was not saved; see $work/serversettings-name.png"; exit 1; }
"$cli" --data-dir "$data" servers | grep -q "  $renamed\$" || { echo "FAIL: the instance does not have the name saved"; exit 1; }
# Message history is under "Tag beside names" now, further down than the screen is tall.
reveal "Show deleted messages"
tap "30 days"
await "Messages older than 30 days will be deleted for everyone. This cannot be undone." \
  || { shot serversettings-shorter; echo "FAIL: choosing a shorter history did not say what it would delete; see $work/serversettings-shorter.png"; exit 1; }
shot serversettings-shorter
[ "$(held retentionDays 0)" = 0 ] || { echo "FAIL: choosing a shorter history changed the server before Save"; exit 1; }
at=$(below Save "30 days")
[ -n "$at" ] || { echo "FAIL: no Save under the history's choices"; exit 1; }
# shellcheck disable=SC2086
"$adb" shell input tap $at
until_held retentionDays 0 30 || { echo "FAIL: the instance does not keep the server's messages for 30 days after Save"; exit 1; }
# Show deleted messages, off by default, saved as it is turned: on, and the
# instance has it; off again, as the rest of the script expects.
[ "$(node "Show deleted messages" on)" = no ] || { echo "FAIL: Show deleted messages is not off for a new server"; exit 1; }
tap "Show deleted messages"
until_held showDeleted False True || { shot serversettings-deleted; echo "FAIL: the instance does not show deleted messages after the switch was turned on; see $work/serversettings-deleted.png"; exit 1; }
[ "$(node "Show deleted messages" on)" = yes ] || { echo "FAIL: Show deleted messages is not on once turned"; exit 1; }
shot serversettings-deleted
tap "Show deleted messages"
until_held showDeleted False False || { echo "FAIL: the instance still shows deleted messages after the switch was turned off"; exit 1; }
"$cli" --data-dir "$data" server-update "$server" --icon "$picture" >/dev/null
# The picture is in the first card, at the screen's head.
reveal "Choose a picture…"
await 'text="Remove"' || { shot serversettings-picture; echo "FAIL: a picture set from elsewhere did not reach the open screen; see $work/serversettings-picture.png"; exit 1; }
sleep 2
shot serversettings-picture
magenta "$(colour "$renamed's picture")" || { echo "FAIL: the screen does not draw the server's picture; see $work/serversettings-picture.png"; exit 1; }
tap Remove
until_held iconUrl '""' "" || { echo "FAIL: Remove did not take the server's picture away on the instance"; exit 1; }
await "$renamed has no picture" || { echo "FAIL: the screen still shows a picture after Remove"; exit 1; }
"$cli" --data-dir "$data" server-update "$server" --icon "$picture" >/dev/null
launch server 7
drawn=""
for _ in $(seq 1 10); do magenta "$(colour "$renamed")" && { drawn=yes; break; }; sleep 1; done
shot server-picture
[ -n "$drawn" ] || { echo "FAIL: the rail does not draw the server's picture; see $work/server-picture.png"; exit 1; }
launch serversettings 7 --es LUMIA_DEV_APPEARANCE dark
shot serversettings-dark
launch serveroverview 7 --es LUMIA_DEV_APPEARANCE dark
shot serveroverview-dark
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  server settings: ok (offered to the owner, its sections first; renamed and the instance has it; 30 days said what it would delete, changed nothing until Save, and then the instance had it; Show deleted messages on and off, and the instance had each; a picture set elsewhere reached the open screen, Remove took it away, and set again the rail drew it)"

# Roles, titles and tags (architecture 7.5), as the server's owner: a role
# made by taps, which the instance must have in its colour; a second made
# elsewhere, which must reach the open list and go over the first by Move
# up from its menu; then, with the channel open, the second account given
# the role and a title from elsewhere, whose tag must be drawn beside its
# name with nothing touched, the title once the server shows titles and the
# role again once it shows roles; and Delete role, which asks first in a
# sheet saying how many hold it. The CLI has no command for roles, so what
# is made elsewhere is made over HTTP, as another session of the owner's.
owner=$(sign_in)
other_id=$("$cli" --data-dir "$other" whoami | grep -oE "[0-9a-f-]{36}" | head -1)
roles_held() { curl -s -H "accept: application/json" -H "authorization: Bearer $owner" "$instance/api/v1/servers/$server/roles"; }
# One field of the role named, as the instance has it; nothing for no such role.
role_field() { # <name> <field>
  roles_held | field "next((str(r.get('$2', '')) for r in d.get('roles', []) if r.get('name') == '$1'), '')"
}
role_order() { roles_held | field "' '.join(r['name'] for r in d.get('roles', []) if not r.get('isEveryone'))"; }
elsewhere() { # <method> <path> <json>
  curl -s -o /dev/null -X "$1" -H "content-type: application/json" -H "accept: application/json" -H "authorization: Bearer $owner" -d "$3" "$instance/api/v1$2"
}
launch roles 7
await "A role is moved from its menu." || { shot roles; echo "FAIL: Roles did not open; see $work/roles.png"; exit 1; }
tap "New role"
await 'text="New role"' || { echo "FAIL: New role did not open a role's editor"; exit 1; }
at=$(below Name Name)
[ -n "$at" ] || { echo "FAIL: no Name field in a new role's editor"; exit 1; }
# shellcheck disable=SC2086
"$adb" shell input tap $at
"$adb" shell input text "Helpers"
tap Mint
tap Save
made=""
for _ in $(seq 1 20); do [ "$(role_field Helpers color)" = mint ] && { made=yes; break; }; sleep 1; done
[ -n "$made" ] || { shot role; echo "FAIL: the instance does not have the role made, in its colour; see $work/role.png"; exit 1; }
# Saved, the editor is that role's: its header says who holds it, which a new role's does not.
await "· 0 members" || { shot role; echo "FAIL: a new role saved did not open as its own editor; see $work/role.png"; exit 1; }
shot role
elsewhere POST "/servers/$server/roles" '{"role":{"name":"Second","color":"sky"}}'
tap "Back to Roles"
await 'content-desc="More for Second"' || { shot roles-two; echo "FAIL: a role made elsewhere did not reach the open list; see $work/roles-two.png"; exit 1; }
[ "$(role_order)" = "Helpers Second" ] || { echo "FAIL: a new role did not start last, above Everyone: $(role_order)"; exit 1; }
tap "More for Second"
await 'text="Move up"' || { shot roles-menu; echo "FAIL: a role's More did not open its menu; see $work/roles-menu.png"; exit 1; }
shot roles-menu
tap "Move up"
moved=""
for _ in $(seq 1 20); do [ "$(role_order)" = "Second Helpers" ] && { moved=yes; break; }; sleep 1; done
[ -n "$moved" ] || { shot roles-moved; echo "FAIL: Move up did not put the role over the other on the instance: $(role_order); see $work/roles-moved.png"; exit 1; }
shot roles-moved
# The second account's messages are the ones it sent at the start; none is sent here, the blocks further down counting them.
launch channel 7 --es LUMIA_DEV_CHANNEL general
await "Two in a row from one person are one block." || { shot channel-tag; echo "FAIL: the channel did not show the second account's messages; see $work/channel-tag.png"; exit 1; }
case "$(ui)" in *'text="Helpers"'*) echo "FAIL: a tag is drawn beside a name before the role is given"; exit 1 ;; esac
elsewhere PATCH "/servers/$server/members/$other_id" "{\"roles\":{\"roleIds\":[\"$(role_field Helpers roleId)\"]},\"title\":\"On call\"}"
await 'text="Helpers"' || { shot channel-tag; echo "FAIL: a role given elsewhere did not become the tag beside the name in the open channel; see $work/channel-tag.png"; exit 1; }
shot channel-tag
elsewhere PATCH "/servers/$server" '{"memberTag":"MEMBER_TAG_TITLE"}'
await 'text="On call"' || { shot channel-title; echo "FAIL: the server showing titles did not turn the tag into the title in the open channel; see $work/channel-title.png"; exit 1; }
case "$(ui)" in *'text="Helpers"'*) shot channel-title; echo "FAIL: a role's tag is still drawn where the server shows titles; see $work/channel-title.png"; exit 1 ;; esac
shot channel-title
elsewhere PATCH "/servers/$server" '{"memberTag":"MEMBER_TAG_ROLE"}'
await 'text="Helpers"' || { echo "FAIL: the server showing roles again did not bring the role's tag back"; exit 1; }
# The title goes again, so the second account is as the rest of the script expects it.
elsewhere PATCH "/servers/$server/members/$other_id" '{"title":""}'
launch roles 7
tap "More for Helpers"
await 'text="Delete role"' || { echo "FAIL: a role's menu has no Delete role"; exit 1; }
tap "Delete role"
await 'text="Delete Helpers?"' || { shot role-delete; echo "FAIL: Delete role did not ask first; see $work/role-delete.png"; exit 1; }
case "$(ui)" in *"1 member holds this role."*) ;; *) shot role-delete; echo "FAIL: the question does not say how many hold the role; see $work/role-delete.png"; exit 1 ;; esac
shot role-delete
[ -n "$(role_field Helpers roleId)" ] || { echo "FAIL: the role was deleted before the question was answered"; exit 1; }
tap "Delete role"
gone=""
for _ in $(seq 1 20); do [ -z "$(role_field Helpers roleId)" ] && { gone=yes; break; }; sleep 1; done
[ -n "$gone" ] || { echo "FAIL: Delete role, confirmed, left the role on the instance"; exit 1; }
# A role nobody holds says so, the members being read, and is kept. (With
# the members not read the question says nothing of holders: RolesTest.)
await 'content-desc="More for Second"' || { shot role-nobody; echo "FAIL: the role left is not in the list after the other was deleted; see $work/role-nobody.png"; exit 1; }
tap "More for Second"
await 'text="Delete role"' || { echo "FAIL: the second role's menu has no Delete role"; exit 1; }
tap "Delete role"
await 'text="Delete Second?"' || { shot role-nobody; echo "FAIL: Delete role did not ask first about a role nobody holds; see $work/role-nobody.png"; exit 1; }
case "$(ui)" in *"Nobody holds this role. This can"*) ;; *) shot role-nobody; echo "FAIL: the question about a role nobody holds does not say so; see $work/role-nobody.png"; exit 1 ;; esac
tap "Keep it"
[ -n "$(role_field Second roleId)" ] || { echo "FAIL: Keep it deleted the role"; exit 1; }
# A save sends what was changed here and nothing else, so an editor or a
# row left open undoes nobody else's change (the core's `changeRole` and
# `changeMember`). A member's row first: the second account holds Third;
# its row is opened and Third unticked, another session gives it Second,
# and Save must take Third and leave Second, where the whole set the row
# showed would have taken both.
second=$(role_field Second roleId)
role_name() { roles_held | field "next((r.get('name', '') for r in d.get('roles', []) if r.get('roleId') == '$1'), '')"; }
role_perms() { roles_held | field "' '.join(next((r.get('permissions', []) for r in d.get('roles', []) if r.get('roleId') == '$1'), []))"; }
member_roles() { # the second account's, as the instance has them
  curl -s -H "accept: application/json" -H "authorization: Bearer $owner" "$instance/api/v1/servers/$server/members" | field "' '.join(sorted(next((m.get('roleIds', []) for m in d.get('members', []) if m.get('accountId') == '$other_id'), [])))"
}
elsewhere POST "/servers/$server/roles" '{"role":{"name":"Third","color":"peach"}}'
third=$(role_field Third roleId)
[ -n "$second" ] && [ -n "$third" ] || { echo "FAIL: the instance does not have the two roles the member's row is tried with"; exit 1; }
elsewhere PATCH "/servers/$server/members/$other_id" "{\"addRoleIds\":[\"$third\"]}"
[ "$(member_roles)" = "$third" ] || { echo "FAIL: the second account was not given the role over HTTP: $(member_roles)"; exit 1; }
launch servermembers 7
await "text=\"$other_handle\"" || { shot member-row; echo "FAIL: Members does not list the second account; see $work/member-row.png"; exit 1; }
tap "$other_handle"
await 'text="Save"' || { shot member-row; echo "FAIL: a member's row did not open; see $work/member-row.png"; exit 1; }
at=$(below Third Roles)
[ -n "$at" ] || { shot member-row; echo "FAIL: the open row does not offer the role the member holds; see $work/member-row.png"; exit 1; }
# shellcheck disable=SC2086
"$adb" shell input tap $at
sleep 1
elsewhere PATCH "/servers/$server/members/$other_id" "{\"addRoleIds\":[\"$second\"]}"
sleep 2
shot member-row
tap "Save"
left=""
for _ in $(seq 1 20); do [ "$(member_roles)" = "$second" ] && { left=yes; break; }; sleep 1; done
[ -n "$left" ] || { shot member-row; echo "FAIL: a row opened before a role was given elsewhere did not leave the member that role and take only the one unticked: $(member_roles) (given $second, unticked $third); see $work/member-row.png"; exit 1; }
# The second account as the rest of the script expects it, and the role made for this gone.
elsewhere PATCH "/servers/$server/members/$other_id" '{"roles":{"roleIds":[]}}'
elsewhere DELETE "/servers/$server/roles/$third" ''
# Then a role's editor: Second has Create invites switched on elsewhere and
# is opened, which must show it on; it is renamed here, and before Save
# another session switches the permission off. Saved, the instance must
# have the new name and the permission still off: the role as the editor
# showed it, whole, would have switched it on again.
elsewhere PATCH "/servers/$server/roles/$second" '{"grant":["CREATE_INVITES"]}'
case " $(role_perms "$second") " in *" CREATE_INVITES "*) ;; *) echo "FAIL: the role was not given the permission over HTTP: $(role_perms "$second")"; exit 1 ;; esac
launch roles 7
await 'content-desc="More for Second"' || { shot role-open; echo "FAIL: Roles does not list the role; see $work/role-open.png"; exit 1; }
tap "Second"
await 'content-desc="Save"' || { shot role-open; echo "FAIL: the role's row did not open its editor; see $work/role-open.png"; exit 1; }
reveal "Create invites"
[ "$(node "Create invites" on)" = yes ] || { shot role-open; echo "FAIL: the editor does not show a permission switched on elsewhere before it opened; see $work/role-open.png"; exit 1; }
reveal Name
at=$(below Name Name)
[ -n "$at" ] || { echo "FAIL: no Name field in the role's editor"; exit 1; }
# shellcheck disable=SC2086
"$adb" shell input tap $at
"$adb" shell input text "x"
sleep 1
elsewhere PATCH "/servers/$server/roles/$second" '{"revoke":["CREATE_INVITES"]}'
sleep 2
shot role-open
tap "Save"
role_renamed=""
for _ in $(seq 1 20); do now=$(role_name "$second"); [ -n "$now" ] && [ "$now" != Second ] && { role_renamed=$now; break; }; sleep 1; done
[ -n "$role_renamed" ] || { shot role-open; echo "FAIL: the editor's rename did not reach the instance; see $work/role-open.png"; exit 1; }
case " $(role_perms "$second") " in *" CREATE_INVITES "*) shot role-open; echo "FAIL: saving a rename switched back on the permission another session switched off while the editor was open; see $work/role-open.png"; exit 1 ;; esac
# A member's own title is theirs to change ten times an hour in a server;
# the instance refuses the eleventh (RATE_LIMITED), and the card's Edit
# must say so in the sheet's words, with the page still open over what was
# typed. The ten are made elsewhere, as another session of the owner's,
# whose title it then is for the rest of the script (nothing further down
# reads it, and the hour's limit keeps it from being cleared).
for n in 1 2 3 4 5 6 7 8 9 10; do elsewhere PATCH "/servers/$server/membership" "{\"title\":\"Title $n\"}"; done
launch members 7 --es LUMIA_DEV_CHANNEL general
await 'text="You"' || { shot title-limit; echo "FAIL: the channel's members do not list this account; see $work/title-limit.png"; exit 1; }
tap "You"
await 'content-desc="Edit title"' || { shot title-limit; echo "FAIL: this account's own card does not offer to edit its title; see $work/title-limit.png"; exit 1; }
tap "Edit title"
await 'text="Title 10"' || { shot title-limit; echo "FAIL: the card's Edit did not open on the title the instance has; see $work/title-limit.png"; exit 1; }
tap "Title 10"
"$adb" shell input text "x"
tap "Save"
await "changed your title too many times this hour. Try again later." || { shot title-limit; echo "FAIL: an eleventh change of this account's own title was not refused in the sheet's words; see $work/title-limit.png"; exit 1; }
seen=$(ui)
case "$seen" in *'text="Save"'*) ;; *) shot title-limit; echo "FAIL: the title's page closed on a refusal; see $work/title-limit.png"; exit 1 ;; esac
case "$seen" in *'text="Title 10"'*) shot title-limit; echo "FAIL: the title's field lost what was typed when the save was refused; see $work/title-limit.png"; exit 1 ;; esac
shot title-limit
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  roles: ok (the sections; a role made by taps and the instance had it in its colour; one made elsewhere reached the list and Move up put it first; a role and a title given elsewhere were the tag beside the name in the open channel, as the server showed each; Delete role asked first, said who held it, and then the instance had it gone, and said of a role nobody held that nobody did; a row opened on a member took the role unticked in it and left the one given elsewhere meanwhile; an editor's rename was saved and left off the permission switched off elsewhere while it was open; this account's own title, changed ten times elsewhere, was refused the eleventh on its card in the sheet's words, the page still open)"

# A server's own emoji (specs/emoji.md; plan step 12), in a channel of
# their own so #general reads as the rest expects: three uploaded over
# HTTP as the server's owner (an animated GIF and two PNGs from the core's
# test pictures), then on the phone the composer's `:` list completing
# :partyparrot:, sent with its ref (the instance has it), the composer's
# emoji button putting :lgtm: in at the caret from the picker's server
# section, a custom reaction chosen in the picker after a message's Add
# reaction, a removed emoji's message and chip read as its :name:, and
# Server settings' Emoji: its count against the limit, a rename and a
# removal through a row's sheet (asked first), and an upload of a picture
# chosen through the system's document picker, named from its file, each
# on the instance after.
fixtures="$root/apps/core/crates/lumia-client/tests/fixtures"
"$cli" --data-dir "$data" channel-create "$server" emoji >/dev/null
emoji_channel=$("$cli" --data-dir "$data" channels "$server" | grep " #emoji " | grep -oE "[0-9a-f-]{36}" | head -1)
[ -n "$emoji_channel" ] || { echo "FAIL: the emoji channel was not made"; exit 1; }
owner=$(sign_in)
upload_emoji() { # <name> <file> <type>: its id
  curl -s -H "authorization: Bearer $owner" -H "content-type: $3" --data-binary "@$2" "$instance/api/v1/servers/$server/emoji?name=$1" | field 'd["emojiId"]'
}
server_emoji() { # the server's emoji as the instance lists them, a name a line
  curl -s -H "accept: application/json" -H "authorization: Bearer $owner" "$instance/api/v1/servers/$server/emoji" | field '"\n".join(e["name"] for e in d["emoji"])'
}
emoji_messages() { # the emoji channel's messages: body, its refs' count and its reactions, one a line
  curl -s -H "accept: application/json" -H "authorization: Bearer $owner" "$instance/api/v1/channels/$emoji_channel/messages" \
    | field '"\n".join("%s|%d|%s" % (m.get("body", ""), len(m.get("emoji", [])), ",".join("%s=%s" % (r["reaction"], "custom" if r.get("emojiId") else "unicode") for r in m.get("reactions", []))) for m in d["messages"])'
}
parrot_id=$(upload_emoji partyparrot "$fixtures/animated.gif" image/gif)
upload_emoji lgtm "$fixtures/logo.png" image/png >/dev/null
old_id=$(upload_emoji oldlogo "$fixtures/logo.png" image/png)
[ -n "$parrot_id" ] && [ -n "$old_id" ] || { echo "FAIL: the instance did not take the emoji uploaded over HTTP"; exit 1; }
# The composer's field: the last text field on the screen.
emoji_field() {
  ui | python3 -c '
import re, sys
fields = [n for n in re.findall(r"<node [^>]*>", sys.stdin.read()) if "class=\"android.widget.EditText\"" in n]
print(re.search(r"text=\"([^\"]*)\"", fields[-1]).group(1) if fields else "")
'
}
launch channel 0 --es LUMIA_DEV_CHANNEL emoji
await 'content-desc="Emoji"' 30 || { shot emoji-channel; echo "FAIL: the composer has no emoji button; see $work/emoji-channel.png"; exit 1; }
tap "Message #emoji"
"$adb" shell input text "Shipped%sit%s:par"
await 'content-desc=":partyparrot:"' 10 || { shot emoji-list; echo "FAIL: typing :par listed no :partyparrot: over the composer; see $work/emoji-list.png"; exit 1; }
shot emoji-list
tap ":partyparrot:"
[ "$(emoji_field)" = "Shipped it :partyparrot: " ] || { shot emoji-list; echo "FAIL: choosing from the : list left \"$(emoji_field)\" in the composer, not the name and a space"; exit 1; }
tap "Send"
for _ in $(seq 1 20); do emoji_messages | grep -q '^Shipped it :partyparrot:|1|' && break; sleep 1; done
emoji_messages | grep -q '^Shipped it :partyparrot:|1|' || { echo "FAIL: the instance does not have the sent :partyparrot: as one of the server's emoji beside the words"; exit 1; }
# The emoji button: the picker with the server's section, its choice put in at the caret, the keyboard back.
"$adb" shell input text "Looks%sgood%s"
tap "Emoji"
# The server's section, by its emoji: the server was renamed further up.
await 'content-desc=":lgtm:"' 10 || { shot emoji-button; echo "FAIL: the composer's emoji button opened no picker with the server's section; see $work/emoji-button.png"; exit 1; }
shot emoji-button
tap ":lgtm:"
[ "$(emoji_field)" = "Looks good :lgtm: " ] || { echo "FAIL: the picker's :lgtm: left \"$(emoji_field)\" in the composer"; exit 1; }
"$adb" shell dumpsys input_method | grep -q "mInputShown=true" || { echo "FAIL: the keyboard did not come back to the composer after the picker"; exit 1; }
tap "Send"
for _ in $(seq 1 20); do emoji_messages | grep -q '^Looks good :lgtm:|1|' && break; sleep 1; done
emoji_messages | grep -q '^Looks good :lgtm:|1|' || { echo "FAIL: the instance does not have the :lgtm: put in by the emoji button as a ref"; exit 1; }
"$adb" shell input keyevent BACK
sleep 1
# A custom reaction, chosen in the picker after a message's Add reaction.
press "Shipped it :partyparrot:"
tap "Add reaction"
await 'content-desc=":partyparrot:"' 10 || { shot emoji-picker; echo "FAIL: the picker has no :partyparrot: in the server's section; see $work/emoji-picker.png"; exit 1; }
shot emoji-picker
tap ":partyparrot:"
await 'You reacted with :partyparrot:' 15 || { shot emoji-reacted; echo "FAIL: reacting with :partyparrot: drew no chip of it; see $work/emoji-reacted.png"; exit 1; }
emoji_messages | grep -q '^Shipped it :partyparrot:|1|:partyparrot:=custom' || { echo "FAIL: the instance does not have the reaction with :partyparrot: by the emoji's id"; exit 1; }
shot emoji-reacted
# A removed emoji: a reaction that used it reads as the name it was last called, renamed elsewhere
# before it was removed (the core names its reactions anew), the chip still counted.
press "Looks good :lgtm:"
tap "Add reaction"
await 'content-desc=":oldlogo:"' 10 || { echo "FAIL: the picker has no :oldlogo:"; exit 1; }
tap ":oldlogo:"
await 'You reacted with :oldlogo:' 15 || { echo "FAIL: reacting with :oldlogo: drew no chip"; exit 1; }
seen=$(ui)
case "$seen" in *'text=":oldlogo:"'*) shot emoji-removed; echo "FAIL: :oldlogo:'s chip read as its name while the emoji was there"; exit 1 ;; esac
curl -s -o /dev/null -X PATCH -H "authorization: Bearer $owner" -H "content-type: application/json" -d '{"name":"retired_logo"}' "$instance/api/v1/servers/$server/emoji/$old_id"
await 'You reacted with :retired_logo:' 20 || { shot emoji-removed; echo "FAIL: the chip of an emoji renamed elsewhere is not named by its new name"; exit 1; }
curl -s -o /dev/null -X DELETE -H "authorization: Bearer $owner" "$instance/api/v1/servers/$server/emoji/$old_id"
await 'text=":retired_logo:"' 20 || { shot emoji-removed; echo "FAIL: once :retired_logo: was removed its chip did not read as the name it was last called; see $work/emoji-removed.png"; exit 1; }
case "$(ui)" in *'You reacted with :retired_logo:, 1'*) ;; *) shot emoji-removed; echo "FAIL: the removed emoji's reaction is no longer counted"; exit 1 ;; esac
shot emoji-removed
# Server settings' Emoji, as the owner: the count, a rename, a removal asked first, an upload.
launch serveremoji 0
await ' · 2 of 50"' 20 || { shot serveremoji; echo "FAIL: the Emoji section does not say 2 of 50; see $work/serveremoji.png"; exit 1; }
shot serveremoji
tap "More for :lgtm:"
await 'text="Rename"' 10 || { shot emoji-sheet; echo "FAIL: a row's More opened no sheet of its actions"; exit 1; }
shot emoji-sheet
tap "Rename"
"$adb" shell input keyevent KEYCODE_MOVE_END
for _ in $(seq 1 6); do "$adb" shell input keyevent KEYCODE_DEL; done
"$adb" shell input text "looks_good"
"$adb" shell input keyevent BACK
tap "Save"
for _ in $(seq 1 15); do server_emoji | grep -qx looks_good && break; sleep 1; done
server_emoji | grep -qx looks_good || { shot emoji-rename; echo "FAIL: the instance does not have the emoji renamed :looks_good:; see $work/emoji-rename.png"; exit 1; }
await 'text=":looks_good:"' 10 || { echo "FAIL: the list does not show :looks_good:"; exit 1; }
tap "More for :partyparrot:"
tap "Remove"
await 'Remove :partyparrot:?' 10 || { shot emoji-remove; echo "FAIL: Remove did not ask first"; exit 1; }
shot emoji-remove
server_emoji | grep -qx partyparrot || { echo "FAIL: :partyparrot: was removed before the question was answered"; exit 1; }
tap "Remove"
for _ in $(seq 1 15); do server_emoji | grep -qx partyparrot || break; sleep 1; done
! server_emoji | grep -qx partyparrot || { echo "FAIL: the instance still has :partyparrot: after Remove"; exit 1; }
"$adb" push "$fixtures/logo.png" "/sdcard/Download/Lumia-Logo $stamp.png" >/dev/null
"$adb" shell am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d "file:///sdcard/Download/Lumia-Logo%20$stamp.png" >/dev/null
tap "Upload emoji"
await "Lumia-Logo $stamp.png" 20 || { shot emoji-documents; echo "FAIL: Upload emoji's picker does not offer the picture; see $work/emoji-documents.png"; exit 1; }
# This run's picture: an earlier run's may still be listed among the recent ones.
first=$(ui | python3 -c '
import re, sys
for node in re.findall(r"<node [^>]*>", sys.stdin.read()):
    if "content-desc=\"Lumia-Logo %s.png" % sys.argv[1] in node:
        x0, y0, x1, y1 = (int(n) for n in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", node).groups())
        print("%d %d" % ((x0 + x1) // 2, (y0 + y1) // 2))
        break
' "$stamp")
# shellcheck disable=SC2086
"$adb" shell input tap $first
await "lumia_logo_$stamp" 20 || { shot emoji-upload; echo "FAIL: the upload's name was not made from the file's; see $work/emoji-upload.png"; exit 1; }
shot emoji-upload
tap "Save"
for _ in $(seq 1 15); do server_emoji | grep -qx "lumia_logo_$stamp" && break; sleep 1; done
server_emoji | grep -qx "lumia_logo_$stamp" || { echo "FAIL: the instance does not have the emoji uploaded from the phone"; exit 1; }
await "text=\":lumia_logo_$stamp:\"" 10 || { shot serveremoji-after; echo "FAIL: the uploaded emoji is not in the list; see $work/serveremoji-after.png"; exit 1; }
shot serveremoji-after
"$adb" shell rm -f "/sdcard/Download/Lumia-Logo $stamp.png" >/dev/null 2>&1 || true
# A limit of 0, which the operator may set and the instance holds to (every
# upload refused): "2 of 0", Upload off, and the line saying the server has
# as many as it may (0), not that removing one makes room. With LUMIA_SMOKE_DB, the
# instance's database, written as `lumia-admin emoji-limit` writes it.
zero_limit=""
if [ -n "${LUMIA_SMOKE_DB:-}" ]; then
  psql "$LUMIA_SMOKE_DB" -q -c "update servers set emoji_limit = 0 where id = '$server'" >/dev/null
  launch serveremoji 0
  await ' · 2 of 0"' 20 || { shot serveremoji-zero; psql "$LUMIA_SMOKE_DB" -q -c "update servers set emoji_limit = null where id = '$server'" >/dev/null; echo "FAIL: at a limit of 0 the Emoji section does not say 2 of 0; see $work/serveremoji-zero.png"; exit 1; }
  shot serveremoji-zero
  seen=$(ui)
  psql "$LUMIA_SMOKE_DB" -q -c "update servers set emoji_limit = null where id = '$server'" >/dev/null
  case "$seen" in *"has as many emoji as it may (0)."*) ;; *) echo "FAIL: at a limit of 0 the section does not say the server has as many as it may (0); see $work/serveremoji-zero.png"; exit 1 ;; esac
  case "$seen" in *"Remove one to upload another."*) echo "FAIL: at a limit of 0 the section says to remove one, which makes no room; see $work/serveremoji-zero.png"; exit 1 ;; esac
  # Upload emoji's button: what holds its words and takes a tap, off.
  upload_on=$(printf '%s' "$seen" | python3 -c '
import re, sys
nodes = re.findall(r"<node [^>]*>", sys.stdin.read())
def box(n):
    return tuple(int(v) for v in re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", n).groups())
def inside(a, b):
    return a[0] >= b[0] and a[1] >= b[1] and a[2] <= b[2] and a[3] <= b[3]
words = next((box(n) for n in nodes if "text=\"Upload emoji\"" in n), None)
holders = [n for n in nodes if words and inside(words, box(n)) and ("clickable=\"true\"" in n or "class=\"android.widget.Button\"" in n)]
print("missing" if not holders else ("on" if any("enabled=\"true\"" in n and "clickable=\"true\"" in n for n in holders) else "off"))
')
  [ "$upload_on" = off ] || { echo "FAIL: at a limit of 0 Upload emoji is $upload_on, not off; see $work/serveremoji-zero.png"; exit 1; }
  zero_limit="; at a limit of 0, 2 of 0 with Upload off and its line not saying to remove one"
else
  echo "  (no LUMIA_SMOKE_DB: the Emoji section at a limit of 0 not checked)"
fi
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  emoji: ok (the : list completed :partyparrot: and the instance had its ref; the emoji button put :lgtm: in at the caret from the server's section and the keyboard came back; a reaction with :partyparrot: from the picker, kept by the emoji's id; an emoji renamed elsewhere named anew on its chip, and once removed its chip read as :retired_logo:, the name it was last called, and stayed counted; Emoji said 2 of 50, renamed :lgtm: to :looks_good:, asked before removing :partyparrot:, and uploaded a picture named from its file$zero_limit)"

# Blocks, in the channel: the second account's two messages in a row are
# under its name once, and the second says whose it is and when to a
# screen reader, which is all that says it on a phone. The phone's account
# has two blocks, its first two messages and the one sent after the
# others, so it is named twice and described once.
launch channel 7
seen=$(ui)
[ "$(count "$seen" "text=\"$other_handle\"")" = 1 ] && [ "$(count "$seen" "content-desc=\"$other_handle, ")" = 1 ] \
  || { echo "FAIL: two messages in a row from $other_handle are not one block; see $work/channel.png"; exit 1; }
[ "$(count "$seen" "text=\"$handle\"")" = 2 ] && [ "$(count "$seen" "content-desc=\"$handle, ")" = 1 ] \
  || { echo "FAIL: the messages of $handle are not the two blocks they should be; see $work/channel.png"; exit 1; }

# Who is typing (protocol 6.7): the second account says so once through
# the CLI, the line under the composer names it, and nine seconds of
# silence end it (the instance's eight, and the wait for the CLI).
"$cli" --data-dir "$other" typing "$channel" >/dev/null &
typing=""
for _ in $(seq 1 8); do
  case "$(ui)" in *"$other_handle is typing"*) typing=yes; break ;; esac
done
shot typing
wait
[ -n "$typing" ] || { echo "FAIL: $other_handle was typing and the channel did not say so; see $work/typing.png"; exit 1; }
sleep 9
case "$(ui)" in *"is typing"*) echo "FAIL: someone is still shown typing after nine seconds of silence"; exit 1 ;; esac

# The same in the dark theme, for the eye.
night=$("$adb" shell cmd uimode night 2>/dev/null | grep -oE "yes|no|auto" | head -1)
"$adb" shell cmd uimode night yes >/dev/null
launch channel 7
"$cli" --data-dir "$other" typing "$channel" >/dev/null &
sleep 3
shot typing-dark
wait
"$adb" shell cmd uimode night "${night:-no}" >/dev/null
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  blocks and typing: ok ($other_handle was named once over two messages, shown typing, and no longer after its silence)"

# Drafts (issue #23; architecture 13, "Local-first"): what is typed in a
# conversation and not sent is that conversation's, kept on the phone with
# the reply it answers. In the media channel, so #general stays as the
# rest reads it: a reply begun and words typed, the channel left for
# #general, whose composer must be empty, and come back to, which must
# have both again; more typed and a second later the app force-stopped, as
# a crash ends it with nothing told, and the next launch must have it all;
# an edit of a message begun and typed in, which is not a draft, so the
# launch after it must have the draft as it was and the message as it was;
# then Send, which must empty the composer and leave it empty when the
# channel is left and come back to, and at another launch.
# What the composer's field holds: the last field on the screen, one a
# message is edited in being over it.
composer() {
  ui | python3 -c '
import html, re, sys
fields = [n for n in re.findall(r"<node [^>]*>", sys.stdin.read()) if "class=\"android.widget.EditText\"" in n]
if fields:
    print(html.unescape(re.search(r" text=\"([^\"]*)\"", fields[-1]).group(1)))
'
}
# Whether it comes to hold <words> within twenty seconds.
composed() { # <words>
  for _ in $(seq 1 20); do
    [ "$(composer)" = "$1" ] && return 0
    sleep 1
  done
  return 1
}
# Taps the first thing whose description starts with <words>: the panel's
# Back, whatever it goes back to (the Back key would cancel the reply
# first), and a channel's row, which says what is unread after its name.
tap_starting() { # <words>
  at=$(starting "$1")
  [ -n "$at" ] || { echo "FAIL: nothing on screen is described as \"$1…\" to tap"; exit 1; }
  # shellcheck disable=SC2086
  "$adb" shell input tap $at
  sleep 1
}
# From one channel's screen to another's, as a person goes: back to the server's list and into it.
switch() { # <channel>
  tap_starting "Back to "
  await 'text="Channels"' 10 || { echo "FAIL: Back did not leave the channel for the server's list"; exit 1; }
  tap_starting "$1 channel"
  await "text=\"# $1\"" 10 || { echo "FAIL: #$1 did not open from the server's list"; exit 1; }
}
asked="Reply to this $stamp"
words="Kept for later"
"$cli" --data-dir "$data" send "$media" "$asked" >/dev/null
launch channel 0 --es LUMIA_DEV_CHANNEL media
await "text=\"$asked\"" 30 || { shot draft; echo "FAIL: the media channel does not show the message to reply to; see $work/draft.png"; exit 1; }
[ -z "$(composer)" ] || { shot draft; echo "FAIL: the media channel's composer is not empty before anything was typed: $(composer)"; exit 1; }
press "$asked"
await 'content-desc="Message actions"' 5 || { shot draft; echo "FAIL: a long press on the message did not open its actions; see $work/draft.png"; exit 1; }
tap "Reply"
await "text=\"Replying to $handle\"" 5 || { shot draft; echo "FAIL: Reply did not put the reply bar in the composer; see $work/draft.png"; exit 1; }
"$adb" shell input text "$(printf '%s' "$words" | sed 's/ /%s/g')"
composed "$words" || { shot draft; echo "FAIL: the composer does not hold what was typed; see $work/draft.png"; exit 1; }
shot draft
switch general
seen=$(ui)
[ -z "$(composer)" ] || { shot draft-other; echo "FAIL: #general's composer holds what was typed in #media: $(composer)"; exit 1; }
case "$seen" in *'text="Message #general"'*) ;; *) shot draft-other; echo "FAIL: #general's composer does not show its empty words; see $work/draft-other.png"; exit 1 ;; esac
case "$seen" in *"text=\"Replying to"*) shot draft-other; echo "FAIL: #general's composer has the reply begun in #media; see $work/draft-other.png"; exit 1 ;; esac
switch media
composed "$words" || { shot draft-back; echo "FAIL: the media channel, left and come back to, does not have its draft: $(composer); see $work/draft-back.png"; exit 1; }
case "$(ui)" in *"text=\"Replying to $handle\""*) ;; *) shot draft-back; echo "FAIL: the draft come back to has lost the reply it answers; see $work/draft-back.png"; exit 1 ;; esac
shot draft-back
# More, and the app ended a second later with nothing told.
tap "$words"
"$adb" shell input keyevent KEYCODE_MOVE_END
"$adb" shell input text "%sand%smore"
words="$words and more"
composed "$words" || { shot draft-more; echo "FAIL: the composer does not hold what was typed after the draft was put back; see $work/draft-more.png"; exit 1; }
sleep 1
launch channel 0 --es LUMIA_DEV_CHANNEL media
composed "$words" || { shot draft-launched; echo "FAIL: after a force-stop and a launch the composer does not have the draft: $(composer); see $work/draft-launched.png"; exit 1; }
case "$(ui)" in *"text=\"Replying to $handle\""*) ;; *) shot draft-launched; echo "FAIL: after a force-stop and a launch the draft has lost the reply it answers; see $work/draft-launched.png"; exit 1 ;; esac
shot draft-launched
# An edit of a message has a field of its own, and is not a draft.
press "$asked"
await 'content-desc="Message actions"' 5 || { shot draft-edit; echo "FAIL: a long press on the message did not open its actions the second time; see $work/draft-edit.png"; exit 1; }
tap "Edit message"
await 'text="Save"' 5 || { shot draft-edit; echo "FAIL: Edit message did not open the message's field; see $work/draft-edit.png"; exit 1; }
"$adb" shell input text "%sedited"
for _ in $(seq 1 10); do case "$(ui)" in *"text=\"$asked edited\""*) break ;; esac; sleep 1; done
seen=$(ui)
case "$seen" in *"text=\"$asked edited\""*) ;; *) shot draft-edit; echo "FAIL: what was typed did not go into the field the message is edited in; see $work/draft-edit.png"; exit 1 ;; esac
[ "$(composer)" = "$words" ] || { shot draft-edit; echo "FAIL: editing a message changed the composer's draft: $(composer); see $work/draft-edit.png"; exit 1; }
shot draft-edit
sleep 1
launch channel 0 --es LUMIA_DEV_CHANNEL media
composed "$words" || { shot draft-edit; echo "FAIL: an edit left unsaved changed the draft the next launch has: $(composer); see $work/draft-edit.png"; exit 1; }
seen=$(ui)
case "$seen" in *"$asked edited"*) echo "FAIL: an edit left unsaved was kept, as a draft or as the message"; exit 1 ;; esac
case "$seen" in *"text=\"$asked\""*) ;; *) echo "FAIL: the message whose edit was left unsaved is not as it was"; exit 1 ;; esac
# Sent: the composer is empty, and stays empty.
tap "Send"
composed "" || { shot draft-sent; echo "FAIL: Send did not empty the composer: $(composer); see $work/draft-sent.png"; exit 1; }
await "text=\"$words\"" 20 || { shot draft-sent; echo "FAIL: the draft sent is not drawn in the channel; see $work/draft-sent.png"; exit 1; }
case "$(ui)" in *"text=\"Replying to"*) shot draft-sent; echo "FAIL: the reply bar is still in the composer after the send; see $work/draft-sent.png"; exit 1 ;; esac
shot draft-sent
switch general
switch media
await 'text="Message #media"' 10 && [ -z "$(composer)" ] || { shot draft-sent; echo "FAIL: the sent words came back to the composer when the channel was left and come back to: $(composer); see $work/draft-sent.png"; exit 1; }
sleep 1
launch channel 0 --es LUMIA_DEV_CHANNEL media
await 'text="Message #media"' 30 && [ -z "$(composer)" ] || { shot draft-sent; echo "FAIL: the sent words came back to the composer at the next launch: $(composer); see $work/draft-sent.png"; exit 1; }
case "$(ui)" in *"text=\"Replying to"*) shot draft-sent; echo "FAIL: the reply sent came back to the composer at the next launch; see $work/draft-sent.png"; exit 1 ;; esac
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  drafts: $work/draft-back.png, $work/draft-launched.png, $work/draft-sent.png (a reply and words typed in #media were not in #general, were there again when #media was come back to and after a force-stop and a launch; an edit left unsaved changed neither the draft nor the message; sent, the composer was empty and stayed empty, left and come back to and at the next launch)"

# Push (architecture 15.5): the second account mentions the phone's
# account while the app is away. The receivers' stand-in, a launch with
# LUMIA_DEV_SCREEN=notify, makes the same call under the same lock as a
# push would and posts the notification, its content recorded in
# notification.txt; then the notification's intent, as a tap sends it,
# opens the channel.
me=$("$cli" --data-dir "$data" whoami)
body="Hey, this is for you on the phone $stamp"
"$cli" --data-dir "$other" send "$channel" "$body" --mention "$me" >/dev/null
"$adb" shell pm grant "$bundle" android.permission.POST_NOTIFICATIONS >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$handle" --es LUMIA_DEV_PASSWORD "$password" --es LUMIA_DEV_SCREEN notify >/dev/null
content=""
for _ in $(seq 1 40); do
  content=$("$adb" shell run-as "$bundle" cat files/lumia/notification.txt 2>/dev/null || true)
  [ -n "$content" ] && break
  sleep 1
done
echo "notification content:"; printf '%s\n' "$content" | sed 's/^/  /'
sleep 1
"$adb" exec-out screencap -p > "$work/notify.png"
posted=$("$adb" shell dumpsys notification --noredact 2>/dev/null | grep -c "$stamp" || true)
case "$content" in
  *"$body"*) ;;
  *) echo "FAIL: the mention was not found for the notice"; exit 1 ;;
esac
[ "${posted:-0}" -gt 0 ] || { echo "FAIL: no notification was posted"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_INSTANCE "$emulated" --es LUMIA_DEV_HANDLE "$handle" --es LUMIA_DEV_PASSWORD "$password" --es channel_id "$channel" --es kind mention >/dev/null
sleep 8
"$adb" exec-out screencap -p > "$work/opened.png"
ui | grep -q "# general" || { echo "FAIL: the tap did not open the channel; see $work/opened.png"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  push: $work/notify.png (the notification), $work/opened.png (the channel it opened)"

# Signing out, from the account menu, as a person does it. For the script
# to see the phone's push subscription held and then gone, the phone is
# given an endpoint the script listens on (LUMIA_DEV_PUSH, a distributor's
# stand-in): the instance posts its notice there for a mention while the
# phone has no live session, and must stop once the phone has signed out.
: > "$work/pushes.log"
port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 -c '
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
class Push(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length") or 0))
        with open(sys.argv[2], "a") as log:
            log.write(self.path + " " + body.decode("utf-8", "replace").replace("\n", " ") + "\n")
        self.send_response(201)
        self.end_headers()
    def log_message(self, *args):
        pass
HTTPServer(("127.0.0.1", int(sys.argv[1])), Push).serve_forever()
' "$port" "$work/pushes.log" &
listener=$!
endpoint="http://127.0.0.1:$port/phone-$stamp"
pushes() { wc -l < "$work/pushes.log" | tr -d ' '; }
# The second account mentions the phone's first account in its channel, or the account named in the channel named.
mention() { # <text> [account] [channel]
  "$cli" --data-dir "$other" send "${3:-$channel}" "$1" --mention "${2:-$me}" >/dev/null
}
subscribed() { # whether the phone remembers sending the endpoint, within twenty seconds
  for _ in $(seq 1 20); do
    case "$(marks)" in *unified_push*"$endpoint"*) return 0 ;; esac
    sleep 1
  done
  return 1
}
pushed() { # <count>: whether the instance has posted that many notices, within twenty seconds
  for _ in $(seq 1 20); do
    [ "$(pushes)" -ge "$1" ] && return 0
    sleep 1
  done
  return 1
}
# The endpoint stands in for a distributor's: the launch hands it to the
# sign-in's registration, which is what sends it (`PushSetup.standIn`).
launch home 0 --es LUMIA_DEV_PUSH "$endpoint"
subscribed || { echo "FAIL: a launch given an endpoint did not subscribe the phone with it"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
sleep 2
asked=$(date +%s)
mention "Signed in and away, the phone is sent a notice $stamp"
# An instance that cannot reach this machine's loopback (one that is not
# on it) posts nowhere the script can see: then only the phone's side of
# the subscription is checked, and the last line says so.
held=""
pushed 1 && held=yes
# How long the script waits before it says no notice came: three times
# what this one took to come, and three seconds. Nothing, where none can.
quiet=$(( ($(date +%s) - asked) * 3 + 3 ))
[ -n "$held" ] || quiet=0
token=$(sign_in)
before=$(sessions "$token")
# The menu in the dark theme, for the eye; the back button closes it and leaves the shell.
launch home 7 --es LUMIA_DEV_APPEARANCE dark
tap "$handle, your account"
shot account-dark
"$adb" shell input keyevent 4
sleep 1
seen=$(ui)
case "$seen" in *'text="Sign out"'*) echo "FAIL: the back button did not close the account menu"; exit 1 ;; esac
case "$seen" in *"$handle, your account"*) ;; *) echo "FAIL: the back button closed more than the account menu"; exit 1 ;; esac
launch home 0
await 'text="connected"' || { echo "FAIL: the app was not connected before signing out"; exit 1; }
tap "$handle, your account"
tap "Sign out"
out=""
for _ in $(seq 1 20); do
  seen=$(ui)
  case "$seen" in *"Welcome back"*) out=yes; break ;; esac
  sleep 1
done
shot signed-out
[ -n "$out" ] || { echo "FAIL: Sign out did not lead to the sign-in screen; see $work/signed-out.png"; exit 1; }
# The line is for a session the instance ended, not for one the person chose to end.
case "$seen" in *"You were signed out"*) echo "FAIL: the sign-in screen after signing out says the phone was signed out; see $work/signed-out.png"; exit 1 ;; esac
# On the instance: one session fewer, the phone's, and no other change.
after=$(sessions "$token")
gone=$(missing "$before" "$after")
[ "$(lines "$gone")" = 1 ] && [ -z "$(missing "$after" "$before")" ] \
  || { echo "FAIL: signing out did not end the phone's session on the instance, and only that (before: $before; after: $after)"; exit 1; }
# On the phone: the subscription it sent is forgotten; the instance and the preferences are the phone's and stay.
case "$(marks)" in *unified_push*) echo "FAIL: the phone signed out and still remembers the subscription it sent"; exit 1 ;; esac
kept=$(stored)
case "$kept" in *'"instance">'*) ;; *) echo "FAIL: signing out took the saved instance with it"; exit 1 ;; esac
case "$kept" in *'"appearance">system<'*) ;; *) echo "FAIL: signing out took the phone's appearance with it"; exit 1 ;; esac
case "$kept" in *'"layout">grouped<'*) ;; *) echo "FAIL: signing out took the phone's message settings with it"; exit 1 ;; esac
# The launch after it is the sign-in screen, with nothing to say.
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" >/dev/null
await "Welcome back" || { echo "FAIL: the launch after signing out is not the sign-in screen"; exit 1; }
case "$(ui)" in *"You were signed out"*) echo "FAIL: the launch after signing out says the phone was signed out"; exit 1 ;; esac
# The instance sends the signed-out phone nothing: the same mention as
# before, and no notice for it in the time the first took several times over.
sent=$(pushes)
mention "Signed out, the phone is sent nothing $stamp"
sleep "$quiet"
[ "$(pushes)" = "$sent" ] || { echo "FAIL: the instance still sent the signed-out phone a notice; see $work/pushes.log"; exit 1; }
# And a push that reaches it all the same (a subscription the instance
# could not be asked to drop) shows nothing, the generic notice neither:
# the receivers' stand-in again, signed out this time.
"$adb" shell run-as "$bundle" rm -f files/lumia/notification.txt >/dev/null 2>&1 || true
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" --es LUMIA_DEV_SCREEN notify >/dev/null
content=""
for _ in $(seq 1 20); do
  content=$("$adb" shell run-as "$bundle" cat files/lumia/notification.txt 2>/dev/null || true)
  [ -n "$content" ] && break
  sleep 1
done
sleep 2
[ "$content" = none ] || { echo "FAIL: a push for the signed-out phone found something to say: $content"; exit 1; }
posted=$("$adb" shell dumpsys notification --noredact 2>/dev/null | grep -c "NotificationRecord.*pkg=$bundle " || true)
[ "${posted:-0}" = 0 ] || { echo "FAIL: a push for the signed-out phone posted a notification"; exit 1; }
case "$(ui)" in *"Welcome back"*) ;; *) echo "FAIL: a push for the signed-out phone took it off the sign-in screen"; exit 1 ;; esac
# Signing in again subscribes again, with the endpoint the phone had sent
# before and forgot having sent, from the device it was: the identity and
# the device's keys stayed. And the instance sends it notices again.
launch home 0 --es LUMIA_DEV_PUSH "$endpoint"
subscribed || { echo "FAIL: the phone signed in again and did not subscribe again"; exit 1; }
again=$(missing "$(sessions "$token")" "$after")
device=${gone#* }
[ "$(lines "$again")" = 1 ] && [ "${again#* }" = "$device" ] && [ -n "$device" ] \
  || { echo "FAIL: the phone signed in again and is not the device it was (was: $gone; is: $again)"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
sleep 2
[ "$(pushes)" = "$sent" ] || { echo "FAIL: a notice for the signed-out phone arrived late; see $work/pushes.log"; exit 1; }
mention "Signed in again and away, the phone is sent a notice $stamp"
if [ -n "$held" ]; then
  pushed $((sent + 1)) || { echo "FAIL: the phone signed in again and the instance sent it no notice; see $work/pushes.log"; exit 1; }
  subscription="the instance sent the phone a notice before, none while it was signed out, and one after it signed in again"
else
  subscription="the instance did not reach the script's listener at $endpoint, so the subscription's removal was checked on the phone only"
fi
echo "  signed out: $work/account-dark.png (the menu, dark), $work/signed-out.png (the sign-in screen, which said nothing of it; the session was gone from the instance; $subscription; a push that still arrived showed nothing; the next launch was sign-in; the phone's preferences and its device were kept)"

# Another account on the same phone, as a person changes it: signed out of
# the first from the menu and, without the app being closed, into the
# third, typed into the sign-in screen. Nothing of the first may be left:
# the phone shows the third account and its server and none of the
# first's, registers for push again by itself (the registration is once
# per sign-in, and the endpoint given at launch is handed over again), is
# the same device to the instance with one session, the third account's,
# and is sent the notices for that account and none for the first.
launch home 0 --es LUMIA_DEV_PUSH "$endpoint"
await 'text="connected"' || { echo "FAIL: the app was not connected before signing out to change accounts"; exit 1; }
process=$(running)
tap "$handle, your account"
tap "Sign out"
await "Welcome back" || { echo "FAIL: Sign out did not lead to the sign-in screen the second time"; exit 1; }
tap "you@example.com"
"$adb" shell input text "$next_handle"
tap "Your password"
"$adb" shell input text "$password"
tap "Sign in"
await "$next_handle, your account" || { shot next-account; echo "FAIL: signing in as $next_handle did not open the app as that account; see $work/next-account.png"; exit 1; }
await 'text="connected"' || { shot next-account; echo "FAIL: signed in as $next_handle, the app did not connect; see $work/next-account.png"; exit 1; }
shot next-account
seen=$(ui)
[ "$(running)" = "$process" ] || { echo "FAIL: the app did not stay running from one account to the next"; exit 1; }
case "$seen" in *"You were signed out"*) echo "FAIL: the phone says it was signed out after signing in as another account"; exit 1 ;; esac
case "$seen" in *'content-desc="Next smoke"'*) ;; *) echo "FAIL: the phone signed in as $next_handle does not show its server; see $work/next-account.png"; exit 1 ;; esac
case "$seen" in *"Android smoke"*|*"$handle"*) echo "FAIL: the phone signed in as $next_handle still shows something of $handle; see $work/next-account.png"; exit 1 ;; esac
subscribed || { echo "FAIL: the phone signed in as another account did not subscribe again"; exit 1; }
next_token=$(sign_in "$next_handle")
under_next=$(sessions "$next_token" | grep -c " $device\$" || true)
under_first=$(sessions "$token" | grep -c " $device\$" || true)
[ "$under_next" = 1 ] && [ "$under_first" = 0 ] \
  || { echo "FAIL: the phone's device should hold one session, under $next_handle: it holds $under_next there and $under_first under $handle"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
sleep 2
if [ -n "$held" ]; then
  sent=$(pushes)
  mention "For the account the phone signed out of $stamp"
  sleep "$quiet"
  [ "$(pushes)" = "$sent" ] || { echo "FAIL: the phone signed in as $next_handle was sent a notice for $handle; see $work/pushes.log"; exit 1; }
  mention "For the account the phone signed in as $stamp" "$next_me" "$next_channel"
  pushed $((sent + 1)) || { echo "FAIL: the phone signed in as $next_handle was sent no notice for it; see $work/pushes.log"; exit 1; }
  notices="a mention of it brought the phone a notice and a mention of $handle none"
else
  notices="the instance did not reach the script's listener, so the notices were not checked"
fi
echo "  another account: $work/next-account.png (signed out of $handle and in as $next_handle in the app, which showed that account and its server and nothing of the first; one session for the phone's device, under $next_handle; subscribed again by itself; $notices)"

# Signed out from elsewhere while the app is away (protocol 12): the
# script, signed in beside the phone over HTTP, revokes every other session
# of the account, as "sign out everywhere" does. The instance itself
# removes the device's push subscription with its last session, so a
# mention brings no notice before the phone has done anything: the app is
# not running. Opened after that, the app holds tokens the instance
# refuses, goes to the sign-in screen and says so.
revoke_others() { # <token>: every session of the account but the token's own
  others=$(curl -s -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1/sessions" | field '" ".join(s["sessionId"] for s in d.get("sessions", []) if not s.get("current"))')
  [ -n "$others" ] || { echo "FAIL: the account has no session but the script's"; exit 1; }
  for id in $others; do
    answer=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE -H "authorization: Bearer $1" "$instance/api/v1/sessions/$id")
    [ "$answer" = 204 ] || { echo "FAIL: revoking a session answered $answer"; exit 1; }
  done
}
who=$next_handle
revoke_others "$next_token"
if [ -n "$held" ]; then
  sent=$(pushes)
  mention "For a phone signed out from elsewhere $stamp" "$next_me" "$next_channel"
  sleep "$quiet"
  [ "$(pushes)" = "$sent" ] || { echo "FAIL: the instance sent a notice to a phone whose session it had revoked; see $work/pushes.log"; exit 1; }
  away="a mention then brought no notice, the app not having run"
else
  away="the instance did not reach the script's listener, so the notice was not checked"
fi
hush
"$adb" shell am start -W -n "$bundle/.MainActivity" >/dev/null
await "You were signed out of this phone" || { shot revoked-away; echo "FAIL: opened after its session was revoked, the app did not go to sign-in and say so; see $work/revoked-away.png"; exit 1; }
shot revoked-away
case "$(marks)" in *unified_push*) echo "FAIL: the session had ended and the phone still remembers the subscription it sent"; exit 1 ;; esac
echo "  revoked while away: $work/revoked-away.png ($away; opened, the app went to sign-in and said why)"

# The same with the app open: the gateway is told and its loop ends, the
# app asks for the snapshot, is refused, and goes back to the sign-in
# screen, which says what happened. The tokens went with the session, so
# the launch after that is the sign-in screen and says nothing. Last,
# because the phone's account loses every session it had. The phone signs
# in again first, and subscribes; a session ended for it cannot ask the
# instance to drop that, and what the phone remembered sending is
# forgotten all the same, so the next sign-in sends it again.
launch home 0 --es LUMIA_DEV_PUSH "$endpoint"
# The phone's store has held two accounts by now, and the second's account
# log has moved since it signed in (the mentions): the gateway must still
# take this account's events. The line under Home says how it stands.
await 'text="connected"' || {
  shot not-connected
  line=$(ui | grep -oE 'text="(connecting|reconnecting|disconnected|closed|not connected)[^"]*"' | head -1)
  echo "FAIL: signed in again as $next_handle, the second account this phone has held, the app did not connect (${line:-nothing said}); see $work/not-connected.png"
  exit 1
}
subscribed || { echo "FAIL: the phone signed in again does not remember the subscription it sent"; exit 1; }
revoke_others "$(sign_in "$next_handle")"
await "You were signed out of this phone" || { shot revoked; echo "FAIL: the session was revoked and the app did not go back to sign-in and say so; see $work/revoked.png"; exit 1; }
shot revoked
# What Preferences chose is the phone's and not the session's: it is still there.
kept=$(stored)
case "$kept" in *'"appearance">system<'*) ;; *) echo "FAIL: the session's end took the phone's appearance with it"; exit 1 ;; esac
case "$kept" in *'"layout">grouped<'*) ;; *) echo "FAIL: the session's end took the phone's message settings with it"; exit 1 ;; esac
case "$(marks)" in *unified_push*) echo "FAIL: the session ended and the phone still remembers the subscription it sent"; exit 1 ;; esac
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
"$adb" shell am start -W -n "$bundle/.MainActivity" >/dev/null
await "Welcome back" || { echo "FAIL: the launch after a revoked session is not the sign-in screen"; exit 1; }
case "$(ui)" in *"You were signed out"*) echo "FAIL: the launch after a revoked session said so a second time"; exit 1 ;; esac
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  revoked: $work/revoked.png (sent back to sign-in, which said why; the next launch did not; the phone's preferences were kept and the subscription it had sent forgotten)"

# A plain member is not offered Server settings, and sees the server's
# picture: the second account, signed in on the phone, which holds no
# session now.
who=$other_handle
launch server 0
await 'text="connected"' || { shot member-server; echo "FAIL: signed in as $other_handle, the app did not connect; see $work/member-server.png"; exit 1; }
tap "$renamed"
await "Invite people" || { shot member-server; echo "FAIL: $other_handle could not open $renamed; see $work/member-server.png"; exit 1; }
drawn=""
# The rail's tile, which says "unread" after the name while the member has
# messages to read; the header's title reads the name alone, and is no picture.
for _ in $(seq 1 10); do { magenta "$(colour "$renamed, unread")" || magenta "$(colour "$renamed")"; } && { drawn=yes; break; }; sleep 1; done
shot member-server
case "$(ui)" in *"Server settings"*) echo "FAIL: $other_handle, a plain member, is offered Server settings; see $work/member-server.png"; exit 1 ;; esac
[ -n "$drawn" ] || { echo "FAIL: the rail does not draw the server's picture for a member; see $work/member-server.png"; exit 1; }
"$adb" shell am force-stop "$bundle" >/dev/null 2>&1 || true
echo "  a member: $work/member-server.png ($other_handle is not offered Server settings, and the rail draws the server's picture)"
echo "smoke: ok (launched signed in, every screen screenshotted under $work; server settings renamed the server, kept its messages 30 days after saying what that deletes, and a picture set and removed reached the screen and the rail, offered to the owner and not to a member; a role made, moved and deleted, the tag beside a name following a role, a title and the server's choice made elsewhere, a member's row and a role's editor saving their own changes and undoing nobody else's, and an eleventh change of this account's own title in an hour refused on its card in words; preferences reached from the account menu, kept across a launch and as the phone was turned, dark, Compact, Minimal and the font size 20 drawn, a setting changed after a preset shown as Custom; messages in blocks and someone typing in the channel; a draft and its reply kept through another channel and a force-stop, and cleared by its send; a mention became a notification and its tap opened the channel; Sign out in the account menu ended the session and the push subscription and said nothing; another account signed in on the same phone was the only one it showed and was sent notices for; a session revoked from elsewhere took the subscription with it, and the app went back to sign-in and said so, opened afterwards or open at the time)"
