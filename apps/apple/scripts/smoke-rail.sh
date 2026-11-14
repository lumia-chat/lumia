#!/bin/sh
# The rail's order and folders by hand (architecture 6.9), in a simulator
# against an instance built from this tree. It seeds the phone's account
# with six servers and arranges them over HTTP (PUT /account/rail) as the
# "Server folders" board has them: Acme, the folder Work (Northwind Studio,
# Open Source Guild, Design Systems Club, Lumia Beta), Trail Runners; Ada,
# from the CLI by invite, writes in Northwind and mentions the account twice
# in Open Source Guild and once in Lumia Beta. It draws the rail light and
# dark, then runs LumiaUITests/SmokeUITests/testRailOrderAndFolders and reads
# the rail back from the instance's snapshot at each step: Work closed with
# its count, opened, Trail Runners moved up by its menu, Acme in a folder of
# its own by New folder, that folder named Weekend in Sky through Folder
# settings…, and Lumia Beta dragged out of Work; then draws the rail again,
# light and dark. Screenshots in .build/smoke-rail/.
#
#   scripts/smoke-rail.sh   # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://localhost:4047 LUMIA_SIMULATOR="iPhone 15 Pro" LUMIA_CLI=/path/to/lumia-cli scripts/smoke-rail.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-rail}
rm -rf "$work" && mkdir -p "$work/steps"
password="correct-horse-battery-staple"
bundle=chat.lumia

curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
owner="rail$stamp"
c() { d=$1; shift; "$cli" --data-dir "$work/$d" "$@"; }
person() { # <dir> <handle> [--invite link]
  d=$1; h=$2; shift 2
  c "$d" instance add "$instance" >/dev/null
  c "$d" account create --handle "$h" --password "$password" "$@" >/dev/null
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
uuid() { grep -oE "[0-9a-f-]{36}" | head -1; }
uuid7() { python3 -c "
import os, time, uuid
b = bytearray(int(time.time() * 1000).to_bytes(6, 'big') + os.urandom(10))
b[6] = 0x70 | (b[6] & 0x0f); b[8] = 0x80 | (b[8] & 0x3f)
print(uuid.UUID(bytes=bytes(b)))"; }
fail() { echo "FAIL: $*"; exit 1; }

person owner "$owner"
person ada "ada$stamp"
to=$(token "$owner"); ta=$(token "ada$stamp")
owner_id=$(c owner whoami | uuid)
# A server with a channel #general, Ada in it; prints the server's id and the channel's.
server() { # <name>
  s=$(c owner server-create "$1" | uuid)
  c owner channel-create "$s" general >/dev/null
  ch=$(c owner channels "$s" | uuid)
  invite=$(c owner invite-create "$s" | grep -oE "https?://[^ ]+" | head -1)
  c ada join "$invite" >/dev/null
  echo "$s $ch"
}
# Ada mentions the phone's account in <channel>, with the mention's byte range.
mention() { # <channel> <body>
  m=$(python3 -c "
import sys, json
body, word = sys.argv[1], '@' + sys.argv[2]
start = len(body[:body.index(word)].encode())
print(json.dumps({'body': body, 'mentions': [{'accountId': sys.argv[3], 'start': start, 'end': start + len(word.encode())}]}))" "$2" "$owner" "$owner_id")
  api "$ta" POST "/channels/$1/commands" "{\"commandId\":\"$(uuid7)\",\"channelId\":\"$1\",\"sendMessage\":$m}" >/dev/null
}
read -r acme _ <<EOF
$(server "Acme")
EOF
read -r north north_ch <<EOF
$(server "Northwind Studio")
EOF
read -r guild guild_ch <<EOF
$(server "Open Source Guild")
EOF
read -r club _ <<EOF
$(server "Design Systems Club")
EOF
read -r beta beta_ch <<EOF
$(server "Lumia Beta")
EOF
read -r trail _ <<EOF
$(server "Trail Runners")
EOF
c ada send "$north_ch" "The proofs are back." >/dev/null
mention "$guild_ch" "Release notes, @$owner?"
mention "$guild_ch" "And the changelog, @$owner."
mention "$beta_ch" "@$owner the beta is out."
folder=$(uuid7)
api "$to" PUT /account/rail "{\"entries\":[{\"serverId\":\"$acme\"},{\"folder\":{\"folderId\":\"$folder\",\"name\":\"Work\",\"tone\":\"mint\",\"serverIds\":[\"$north\",\"$guild\",\"$club\",\"$beta\"]}},{\"serverId\":\"$trail\"}]}" >/dev/null
echo "seeded $owner: Acme, Work (Northwind Studio, Open Source Guild, Design Systems Club, Lumia Beta), Trail Runners"

# The rail as the instance's snapshot has it, by the servers' names: "Acme | Work[Northwind Studio, ...] | Trail Runners".
names="{\"$acme\": \"Acme\", \"$north\": \"Northwind Studio\", \"$guild\": \"Open Source Guild\", \"$club\": \"Design Systems Club\", \"$beta\": \"Lumia Beta\", \"$trail\": \"Trail Runners\"}"
rail() {
  api "$to" GET /account/snapshot | python3 -c "
import sys, json
names = json.loads(sys.argv[1])
out = []
for e in json.load(sys.stdin).get('rail', {}).get('entries', []):
    if 'serverId' in e: out.append(names[e['serverId']])
    else:
        f = e['folder']
        out.append('%s/%s[%s]' % (f.get('name', ''), f.get('tone', ''), ', '.join(names[s] for s in f.get('serverIds', []))))
print(' | '.join(out))" "$names"
}
expect() { # <step> <rail>
  got=$(rail)
  [ "$got" = "$2" ] || fail "at $1 the instance's rail is \"$got\", not \"$2\""
  echo "$1: $got"
}
expect seeded "Acme | Work/mint[Northwind Studio, Open Source Guild, Design Systems Club, Lumia Beta] | Trail Runners"

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
rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
xcrun simctl install "$udid" "$app"

shoot() { # <picture> <appearance>
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="$owner" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" \
  SIMCTL_CHILD_LUMIA_DEV_SCREEN=server SIMCTL_CHILD_LUMIA_DEV_APPEARANCE="$2" SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 xcrun simctl launch "$udid" "$bundle" >/dev/null
  sleep 10
  xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1
  echo "  $1: $work/$1.png"
}
shoot rail light
shoot rail-dark dark
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true

uitest=""
# shellcheck disable=SC2329
finish() { [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true; }
trap finish EXIT
log="$work/testRailOrderAndFolders.log"
TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$owner" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testRailOrderAndFolders > "$log" 2>&1 &
uitest=$!

reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && break
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  if [ ! -f "$work/steps/$1" ]; then
    xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
    grep -E "error|failed|XCTAssert" "$log" | head -10
    fail "the UI test did not reach \"$1\"; see $log"
  fi
  sleep 1
  xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1
}
seen() { touch "$work/steps/$1.seen"; }
# The instance takes an edit a moment after the phone draws it.
settled() { # <step> <rail>
  for _ in $(seq 1 20); do [ "$(rail)" = "$2" ] && break; sleep 0.5; done
  expect "$1" "$2"
}

reached closed 240
expect closed "Acme | Work/mint[Northwind Studio, Open Source Guild, Design Systems Club, Lumia Beta] | Trail Runners"
seen closed
reached open 60
seen open
reached menu 30
seen menu
reached moved 60
settled moved "Acme | Trail Runners | Work/mint[Northwind Studio, Open Source Guild, Design Systems Club, Lumia Beta]"
seen moved
reached made 60
got=$(rail)
case "$got" in
  "/"*"[Acme] | Trail Runners | Work/mint[Northwind Studio, Open Source Guild, Design Systems Club, Lumia Beta]") echo "made: $got" ;;
  *) fail "at made the instance's rail is \"$got\", not Acme in a folder of its own with no name" ;;
esac
seen made
reached settings 60
seen settings
reached named 60
settled named "Weekend/sky[Acme] | Trail Runners | Work/mint[Northwind Studio, Open Source Guild, Design Systems Club, Lumia Beta]"
seen named
reached dragged 60
settled dragged "Weekend/sky[Acme] | Trail Runners | Lumia Beta | Work/mint[Northwind Studio, Open Source Guild, Design Systems Club]"
seen dragged
wait "$uitest" || { grep -E "error|failed|XCTAssert" "$log" | head -10; fail "the UI test did not pass; see $log"; }
grep -q "Executed 1 test" "$log" || fail "the UI test did not run (is the build older than it?); see $log"
uitest=""

shoot after light
shoot after-dark dark
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "smoke-rail: ok (a folder closed with its count and opened, a server moved by its menu, a folder made by New folder and named in Folder settings, a server dragged out of a folder, each read back from the instance's snapshot; screenshots under $work)"
