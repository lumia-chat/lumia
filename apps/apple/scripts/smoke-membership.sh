#!/bin/sh
# Channel membership and server DMs by hand in a simulator (architecture
# 6.4 and 6.8): seeds a server with the release CLI and the instance's
# HTTP API (an owner, who is the phone, and Ada and Grace with profiles;
# #design and #random the owner left, so Browse lists them; Ada taking
# adds as invitations and inviting the owner into #design; a server DM
# from Ada; notices in #general), screenshots the screens the
# LUMIA_DEV_SCREEN switch opens for it, then runs
# testChannelMembershipByHand, reading the instance at each step it
# announces: the invitation joined, #random joined from Browse's preview,
# Grace added and Ada invited, Grace removed, a message in the server DM
# opened from Ada's profile card, the owner's server DMs off, #design a
# default channel, and #random left.
#
#   scripts/smoke-membership.sh   # the app built for testing (AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://localhost:4032 LUMIA_CLI=/path/to/lumia-cli scripts/smoke-membership.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
project="$here/../Lumia-iOS"
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-membership}
rm -rf "$work" && mkdir -p "$work/steps"
password="correct-horse-battery-staple"
bundle=chat.lumia

curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
owner="phone$stamp"
c() { d=$1; shift; "$cli" --data-dir "$work/$d" "$@"; }
person() { # <dir> <handle> <display name> [--invite link]
  d=$1; h=$2; n=$3; shift 3
  c "$d" instance add "$instance" >/dev/null
  c "$d" account create --handle "$h" --password "$password" "$@" >/dev/null
  c "$d" identity create >/dev/null
  c "$d" identity link-account --password "$password" >/dev/null
  c "$d" identity profile --display-name "$n" >/dev/null
}
token() {
  curl -s -H "content-type: application/json" -H "accept: application/json" \
    -d "{\"handleOrEmail\":\"$1\",\"password\":\"$password\"}" "$instance/api/v1/auth/sign-in/password" |
    python3 -c "import sys, json; print(json.load(sys.stdin)['accessToken'])"
}
api() { # <token> <method> <path> [json]
  if [ $# -ge 4 ]; then
    curl -s -f -X "$2" -H "content-type: application/json" -H "accept: application/json" -H "authorization: Bearer $1" -d "$4" "$instance/api/v1$3"
  else
    curl -s -f -X "$2" -H "accept: application/json" -H "authorization: Bearer $1" "$instance/api/v1$3"
  fi
}
py() { python3 -c "import sys, json; d = json.load(sys.stdin); $1"; }
uuid() { grep -oE "[0-9a-f-]{36}" | head -1; }

person owner "$owner" "Robin Owner"
server=$(c owner server-create "Acme Works" | uuid)
c owner channel-create "$server" general >/dev/null
general=$(c owner channels "$server" | uuid)
invite=$(c owner invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
person ada "ada$stamp" "Ada Lovelace" --invite "$invite"
person grace "grace$stamp" "Grace Hopper" --invite "$invite"
to=$(token "$owner"); ta=$(token "ada$stamp"); tg=$(token "grace$stamp")
owner_id=$(c owner whoami | uuid); ada_id=$(c ada whoami | uuid); grace_id=$(c grace whoami | uuid)

channel_id() { py "d = d.get('channel', d); print(d['channelId'])"; }
design=$(api "$to" POST /channels "{\"serverId\":\"$server\",\"kind\":\"TEXT\",\"name\":\"design\",\"topic\":\"Mockups, reviews and the type scale\"}" | channel_id)
random=$(api "$to" POST /channels "{\"serverId\":\"$server\",\"kind\":\"TEXT\",\"name\":\"random\"}" | channel_id)
api "$ta" POST "/channels/$design/join" >/dev/null
c ada send "$design" "The new type scale is up for review." >/dev/null
api "$to" POST "/channels/$design/leave" >/dev/null
api "$to" POST "/channels/$random/leave" >/dev/null
api "$to" POST /channels "{\"serverId\":\"$server\",\"kind\":\"TEXT\",\"name\":\"leads\",\"private\":true}" >/dev/null
c owner send "$general" "Welcome to Acme Works." >/dev/null
api "$to" DELETE "/channels/$general/members/$grace_id" >/dev/null
api "$to" POST "/channels/$general/members" "{\"accountIds\":[\"$grace_id\"]}" >/dev/null
api "$ta" PATCH "/servers/$server/membership" '{"channelAddsByInvitation":true}' >/dev/null
api "$tg" PATCH "/servers/$server/membership" '{"serverDmsOff":true}' >/dev/null
api "$to" PATCH "/servers/$server/membership" '{"channelAddsByInvitation":true}' >/dev/null
api "$ta" POST "/channels/$design/members" "{\"accountIds\":[\"$owner_id\"]}" >/dev/null
dm=$(api "$ta" POST "/servers/$server/dms" "{\"accountId\":\"$owner_id\"}" | channel_id)
c ada send "$dm" "Have you got a minute for the review?" >/dev/null
echo "seeded $owner owning Acme Works ($server), with Ada and Grace"

members_of() { api "$to" GET "/channels/$1/members" | py "print(' '.join(d.get('accountIds', [])))"; }
has() { case " $1 " in *" $2 "*) return 0 ;; *) return 1 ;; esac; }
fail() { echo "FAIL: $*"; exit 1; }

udid=$(xcrun simctl list devices available -j | python3 -c "
import json,sys
all=[d for r in json.load(sys.stdin)['devices'].values() for d in r]
devs=[d for d in all if d['name']=='$simulator'] or [d for d in all if d['name'].startswith('iPhone')]
devs.sort(key=lambda d: d['state']!='Booted')
print(devs[0]['udid'] if devs else '')")
[ -n "$udid" ] || { echo "no iPhone simulator"; exit 1; }
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
xcrun simctl install "$udid" "$app"

shoot() { # <picture> <screen> [KEY=VALUE...]
  picture=$1; screen=$2; shift 2
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  for kv in "$@"; do export "SIMCTL_CHILD_$kv"; done
  SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="$owner" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" \
  SIMCTL_CHILD_LUMIA_DEV_SCREEN="$screen" SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 xcrun simctl launch "$udid" "$bundle" >/dev/null
  sleep 10
  xcrun simctl io "$udid" screenshot "$work/$picture.png" >/dev/null 2>&1
  for kv in "$@"; do unset "SIMCTL_CHILD_${kv%%=*}"; done
  echo "  $picture: $work/$picture.png"
}
shoot server server
shoot browse browse
shoot preview preview
shoot general channel LUMIA_DEV_CHANNEL=general
shoot members members LUMIA_DEV_CHANNEL=general
shoot addpeople addpeople LUMIA_DEV_CHANNEL=leads
shoot serverdm serverdm
shoot newdm newdm
shoot card-ada profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=ada$stamp"
shoot card-grace profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=grace$stamp"
shoot yoursettings yoursettings
shoot defaults serversettings LUMIA_DEV_SCROLL=defaults
shoot newchannel newchannel
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true

log="$work/testChannelMembershipByHand.log"
TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$owner" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testChannelMembershipByHand > "$log" 2>&1 &
uitest=$!
trap 'kill "$uitest" 2>/dev/null || true' EXIT

reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && break
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  if [ ! -f "$work/steps/$1" ]; then
    xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
    grep -E "error|failed|XCTAssert" "$log" | head -10
    fail "the UI test did not reach \"$1\"; see $log and $work/failed-$1.png"
  fi
  sleep 1
  xcrun simctl io "$udid" screenshot "$work/step-$1.png" >/dev/null 2>&1
}
seen() { touch "$work/steps/$1.seen"; }

reached invitation 300
has "$(members_of "$design")" "$owner_id" || fail "the owner is not in #design after Join on the invitation"
seen invitation; echo "the invitation, joined from the sidebar: the owner is in #design"
reached browsed 120
has "$(members_of "$random")" "$owner_id" || fail "the owner is not in #random after Join on its bar"
seen browsed; echo "#random read from Browse and joined from its bar"
reached added 120
in_random=$(members_of "$random")
has "$in_random" "$grace_id" || fail "Grace is not in #random after Add"
has "$in_random" "$ada_id" && fail "Ada, who takes adds as invitations, was added rather than invited"
api "$ta" GET /account/snapshot | py "sys.exit(0 if any(i['channel']['channelId'] == '$random' for i in d.get('channelInvitations', [])) else 1)" ||
  fail "Ada has no invitation into #random"
seen added; echo "Grace added to #random, Ada invited"
reached removed 120
has "$(members_of "$random")" "$grace_id" && fail "Grace is still in #random after Remove"
seen removed; echo "Grace removed from #random by the Remove at her row's end"
reached dm 120
api "$to" GET "/channels/$dm/messages" | py "sys.exit(0 if any(m.get('body') == 'Sent from the phone\\'s server DM.' for m in d.get('messages', [])) else 1)" ||
  fail "the message sent in the server DM is not on the instance"
seen dm; echo "the server DM opened from Ada's profile card, and a message sent there"
reached switch 120
api "$to" GET /account/snapshot | py "sys.exit(0 if any(m.get('serverDmsOff') for m in d.get('memberships', []) if m['serverId'] == '$server') else 1)" ||
  fail "the owner's server DMs are not off on the instance"
seen switch; echo "Let members of this server message me: off, on the instance"
reached defaults 120
api "$to" GET "/servers/$server/channels" | py "sys.exit(0 if any(c.get('isDefault') for c in d.get('channels', []) if c['channelId'] == '$design') else 1)" ||
  fail "#design is not a default channel on the instance"
seen defaults; echo "#design made a default channel"
reached left 120
has "$(members_of "$random")" "$owner_id" && fail "the owner is still in #random after Leave channel"
seen left; echo "#random left from its members"
wait "$uitest" || { grep -E "error|failed|XCTAssert" "$log" | head -10; fail "the UI test did not pass; see $log"; }
trap - EXIT
echo "smoke-membership: ok (screenshots under $work)"
