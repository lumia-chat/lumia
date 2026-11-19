#!/bin/sh
# Roles, titles and tags by hand (architecture 7.5, plan step 11), in a
# simulator against an instance built from this tree. It seeds the boards'
# Acme with the CLI and the instance's HTTP API, which makes what the CLI
# cannot (roles, titles, a guest's invite): the phone's account owning it,
# Maya, Devon, Sam, Priya and Leo in it and Jordan a guest, the roles Admin,
# Moderator, Design, Engineering and Can invite (not shown as a tag), a
# title for most, and a morning in #general. It draws each screen light and
# dark, as the owner and as Devon, who manages only what is below Moderator,
# into .build/smoke-roles/, to be looked at beside the boards. Then
# LumiaUITests/SmokeUITests/testRolesTitlesAndTagsByHand drives the owner's
# phone and this script reads the instance at each step: a role made, edited
# (Administrator asked about first; a permission switched on from here while
# its editor is open stays on through the editor's save, and its colour
# changed from here afterwards is followed by the editor), moved up from its menu, given to a
# member and another role taken from her by the Remove beside her, a title
# set, the tag beside a name in #general following a change made from here
# while the conversation is on screen, a title and roles changed from a
# profile card, the owner's own card saying You and Owner and refusing a
# title with a hidden character, the server switched to titles, a member kicked, and the role
# deleted after its sheet. Last, testRolesAsOneWhoManagesWhatIsBelow, as Devon.
#
#   scripts/smoke-roles.sh   # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md) and the CLI
#   LUMIA_INSTANCE=http://host:4000 LUMIA_SIMULATOR="iPhone 15 Pro" scripts/smoke-roles.sh
#   LUMIA_CLI=/path/to/lumia-cli scripts/smoke-roles.sh   # a CLI built elsewhere (another worktree)
#   LUMIA_SMOKE_PICTURES=only scripts/smoke-roles.sh      # the pictures, and no test
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-roles}
rm -rf "$work" && mkdir -p "$work/steps"
password="correct-horse-battery-staple"
bundle=chat.lumia

curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }
[ -d "$app" ] || { echo "no app at $app"; exit 1; }

stamp=$(date +%s)
owner="ryan$stamp"
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
fail() { echo "FAIL: $*"; exit 1; }

person owner "$owner" "Ryan Winchester"
server=$(c owner server-create "Acme" | uuid)
c owner channel-create "$server" general >/dev/null
general=$(c owner channels "$server" | uuid)
invite=$(c owner invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
to=$(token "$owner")
guest_invite=$(api "$to" POST /invites "{\"serverId\":\"$server\",\"asGuest\":true,\"guestDays\":30}" | py "d = d.get('invite', d); print(d['code'])")
person maya "maya$stamp" "Maya Chen" --invite "$invite"
person devon "devon$stamp" "Devon Park" --invite "$invite"
person sam "sam$stamp" "Sam Okafor" --invite "$invite"
person priya "priya$stamp" "Priya Natarajan" --invite "$invite"
person leo "leo$stamp" "Leo Martins" --invite "$invite"
person jordan "jordan$stamp" "Jordan Lee" --invite "$guest_invite"
for who in owner maya devon sam priya leo jordan; do eval "${who}_id=\$(c $who whoami | uuid)"; done

# The roles, made in the order they stand in, highest first.
role() { # <name> <colour> <hide tag> <permissions as JSON>
  api "$to" POST "/servers/$server/roles" "{\"role\":{\"name\":\"$1\",\"color\":\"$2\",\"hideTag\":$3,\"permissions\":$4}}" | py "d = d.get('role', d); print(d['roleId'])"
}
admin=$(role Admin indigo false '["MANAGE_SERVER","MANAGE_ROLES","MANAGE_CHANNELS","MANAGE_MEMBERS","CREATE_INVITES","MENTION_EVERYONE","MANAGE_MESSAGES"]')
moderator=$(role Moderator green false '["MANAGE_ROLES","MANAGE_MEMBERS","MENTION_EVERYONE","MANAGE_MESSAGES"]')
design=$(role Design peach false '[]')
engineering=$(role Engineering sky false '[]')
caninvite=$(role "Can invite" "" true '["CREATE_INVITES"]')
member() { # <account id> <title> <role id>...
  account=$1; title=$2; shift 2
  ids=""
  for id in "$@"; do ids="$ids${ids:+,}\"$id\""; done
  api "$to" PATCH "/servers/$server/members/$account" "{\"roles\":{\"roleIds\":[$ids]},\"title\":\"$title\"}" >/dev/null
}
# shellcheck disable=SC2154
{
member "$maya_id" "Head of Design" "$admin" "$design"
member "$devon_id" "Staff Engineer" "$moderator" "$engineering"
member "$sam_id" "Backend Engineer" "$engineering" "$caninvite"
member "$priya_id" "" "$design"
member "$leo_id" "Mobile Lead" "$caninvite"
member "$jordan_id" "Contractor"
}
api "$to" PATCH "/servers/$server/membership" '{"title":"Founder"}' >/dev/null
c maya send "$general" "Morning all. The new onboarding flow is in the design channel if anyone wants a look before standup." >/dev/null
c devon send "$general" "Looks great. I’ll check the copy on the second screen against the tone guide." >/dev/null
c sam send "$general" "The importer keeps its old path for one more release, so nothing blocks it." >/dev/null
c priya send "$general" "Two screens changed since Friday, both in the sign-in card." >/dev/null
c leo send "$general" "Mobile gets the same composer this week." >/dev/null
c owner send "$general" "Thanks, everyone. Thursday still stands." >/dev/null
echo "seeded $owner owning Acme ($server): five roles, six members with them and their titles, Jordan a guest"

# The server as the instance has it, read as the owner from beside the phone.
roles_now() { api "$to" GET "/servers/$server/roles" | py "print(' '.join(r['name'].replace(' ', '_') for r in sorted(d.get('roles', []), key=lambda r: r.get('position', 0)) if not r.get('isEveryone')))"; }
role_field() { api "$to" GET "/servers/$server/roles" | py "r = next((r for r in d.get('roles', []) if r['name'] == '$1'), None); print('gone' if r is None else $2)"; }
member_field() { api "$to" GET "/servers/$server/members" | py "m = next((m for m in d.get('members', []) if m.get('member', m)['accountId'] == '$1'), None); m = m and m.get('member', m); print('gone' if m is None else $2)"; }
server_field() { api "$to" GET "/servers/$server" | py "d = d.get('server', d); print($1)"; }
[ "$(roles_now)" = "Admin Moderator Design Engineering Can_invite" ] || fail "the seeded roles stand as \"$(roles_now)\""

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
# A new phone each time: the app and the stores its group keeps across installs.
fresh() {
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
  xcrun simctl uninstall "$udid" "$bundle" 2>/dev/null || true
  rm -rf "$HOME/Library/Developer/CoreSimulator/Devices/$udid/data/Containers/Shared/AppGroup"/*/Lumia 2>/dev/null || true
}
fresh
xcrun simctl install "$udid" "$app"

signed=$owner
shoot() { # <picture> <screen> [KEY=VALUE...]: once light and once dark
  picture=$1; screen=$2; shift 2
  for kv in "$@"; do export "SIMCTL_CHILD_$kv"; done
  for look in light dark; do
    xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
    SIMCTL_CHILD_LUMIA_DEV_INSTANCE="$instance" SIMCTL_CHILD_LUMIA_DEV_HANDLE="$signed" SIMCTL_CHILD_LUMIA_DEV_PASSWORD="$password" \
    SIMCTL_CHILD_LUMIA_DEV_SCREEN="$screen" SIMCTL_CHILD_LUMIA_DEV_APPEARANCE="$look" SIMCTL_CHILD_LUMIA_DEV_NO_PUSH=1 xcrun simctl launch "$udid" "$bundle" >/dev/null
    sleep "${LUMIA_SMOKE_WAIT:-8}"
    xcrun simctl io "$udid" screenshot "$work/$picture-$look.png" >/dev/null 2>&1
  done
  for kv in "$@"; do unset "SIMCTL_CHILD_${kv%%=*}"; done
  echo "  $picture: $work/$picture-light.png, $work/$picture-dark.png"
}
if [ "${LUMIA_SMOKE_PICTURES:-}" != none ]; then
  shoot settings serversettings
  shoot overview-tag overview LUMIA_DEV_SCROLL=tag
  shoot roles roles
  shoot editor role LUMIA_DEV_ROLE=Moderator
  shoot editor-permissions role LUMIA_DEV_ROLE=Moderator LUMIA_DEV_SCROLL=permissions
  shoot editor-end role LUMIA_DEV_ROLE=Moderator LUMIA_DEV_SCROLL=end
  shoot editor-delete role LUMIA_DEV_ROLE=Moderator LUMIA_DEV_SCROLL=end LUMIA_DEV_ASK=delete
  shoot editor-administrator role LUMIA_DEV_ROLE=Moderator LUMIA_DEV_SCROLL=permissions LUMIA_DEV_ASK=administrator
  shoot editor-untagged role "LUMIA_DEV_ROLE=Can invite"
  shoot editor-everyone role LUMIA_DEV_ROLE=Everyone
  shoot editor-new newrole
  shoot add-members rolemembers LUMIA_DEV_ROLE=Moderator
  shoot members servermembers
  shoot member servermembers "LUMIA_DEV_PERSON=devon$stamp"
  shoot member-guest servermembers "LUMIA_DEV_PERSON=jordan$stamp"
  shoot channel channel LUMIA_DEV_CHANNEL=general
  shoot channel-members members LUMIA_DEV_CHANNEL=general
  shoot card-devon profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=devon$stamp"
  shoot card-devon-roles profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=devon$stamp" LUMIA_DEV_EDIT=roles
  shoot card-devon-title profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=devon$stamp" LUMIA_DEV_EDIT=title
  shoot card-own profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=$owner"
  shoot card-leo profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=leo$stamp"
  # No instance behind this one: every tone, and a tag at its longest beside a name.
  shoot tags tags
  # As Devon, whose highest role is Moderator: Admin and Moderator are not Devon's to manage.
  fresh
  xcrun simctl install "$udid" "$app"
  signed="devon$stamp"
  shoot devon-settings serversettings
  shoot devon-roles roles
  shoot devon-editor role LUMIA_DEV_ROLE=Engineering LUMIA_DEV_SCROLL=permissions
  shoot devon-member-above servermembers "LUMIA_DEV_PERSON=maya$stamp"
  shoot devon-member-below servermembers "LUMIA_DEV_PERSON=sam$stamp"
  shoot devon-card-owner profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=$owner"
  shoot devon-card-own profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=devon$stamp"
  # As Priya, who manages nothing and has no title: "Add a title" on her own card.
  fresh
  xcrun simctl install "$udid" "$app"
  signed="priya$stamp"
  shoot priya-card-own profilecard LUMIA_DEV_CHANNEL=general "LUMIA_DEV_PERSON=priya$stamp"
  xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
fi
[ "${LUMIA_SMOKE_PICTURES:-}" = only ] && { echo "pictures only: $work"; exit 0; }

uitest=""
# shellcheck disable=SC2329
finish() { [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true; }
trap finish EXIT
run() { # <handle> <test>
  fresh
  TEST_RUNNER_LUMIA_DEV_INSTANCE="$instance" TEST_RUNNER_LUMIA_DEV_HANDLE="$1" TEST_RUNNER_LUMIA_DEV_PASSWORD="$password" TEST_RUNNER_LUMIA_DEV_NO_PUSH=1 \
  TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" \
    xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
    -only-testing:"LumiaUITests/SmokeUITests/$2" > "$work/$2.log" 2>&1 &
  uitest=$!
  log="$work/$2.log"
}
# Waits for the test to announce a step and screenshots it; fails with the test's log when it ends first.
reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && break
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  if [ ! -f "$work/steps/$1" ]; then
    echo "FAIL: the UI test did not reach \"$1\"; see $log"
    xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
    grep -E "error|failed|XCTAssert" "$log" | head -10
    exit 1
  fi
  sleep 1
  xcrun simctl io "$udid" screenshot "$work/step-$1.png" >/dev/null 2>&1
}
seen() { touch "$work/steps/$1.seen"; }
passed() {
  wait "$uitest" || { echo "FAIL: the UI test did not pass; see $log"; grep -E "error|failed|XCTAssert" "$log" | head -10; exit 1; }
  uitest=""
}

run "$owner" testRolesTitlesAndTagsByHand
reached sections 240
seen sections
echo "the owner is offered Server settings: Overview, Roles (5 roles) and Members (7)"
reached made 120
[ "$(roles_now)" = "Admin Moderator Design Engineering Can_invite Support" ] || fail "after New role the roles stand as \"$(roles_now)\""
[ "$(role_field Support "r.get('color', '')")" = "sky" ] || fail "Support was not made in sky"
seen made
echo "a role made through the screen: Support, in sky, last above Everyone"
reached renaming 60
# From elsewhere, while the role's editor is open on the phone with its name typed over.
support=$(role_field Support "r['roleId']")
api "$to" PATCH "/servers/$server/roles/$support" '{"grant":["MENTION_EVERYONE"]}' >/dev/null
[ "$(role_field Support "' '.join(sorted(r.get('permissions', [])))")" = "MENTION_EVERYONE" ] || fail "Mention everyone was not switched on from elsewhere"
seen renaming
reached asks 60
seen asks
echo "Administrator turned on asks first, in a sheet"
reached edited 60
[ "$(role_field Helpers "' '.join(sorted(r.get('permissions', [])))")" = "ADMINISTRATOR MANAGE_MESSAGES MENTION_EVERYONE" ] || fail "Helpers grants \"$(role_field Helpers "' '.join(sorted(r.get('permissions', [])))")\" after a save from an editor that was open while Mention everyone was switched on elsewhere"
[ "$(role_field Support "1")" = "gone" ] || fail "the role is still called Support"
[ "$(role_field Helpers "r.get('color', '')")" = "sky" ] || fail "Helpers is in \"$(role_field Helpers "r.get('color', '')")\""
api "$to" PATCH "/servers/$server/roles/$support" '{"color":"mint"}' >/dev/null
seen edited
echo "the role edited: renamed Helpers, granting Manage messages and Administrator, and Mention everyone, switched on from elsewhere while the editor was open, still on; then its colour changed from elsewhere for the editor to follow"
reached menu 60
seen menu
reached moved 60
[ "$(roles_now)" = "Admin Moderator Design Engineering Helpers Can_invite" ] || fail "after Move up the roles stand as \"$(roles_now)\""
seen moved
echo "Move up from the role's menu: Helpers stands above Can invite on the instance"
helpers=$(role_field Helpers "r['roleId']")
reached given 60
# shellcheck disable=SC2154
case " $(member_field "$priya_id" "' '.join(m.get('roleIds', []))") " in *" $helpers "*) ;; *) fail "Priya does not hold Helpers after Add members" ;; esac
seen given
echo "the role given to Priya from its editor's Add members"
reached removed 60
case " $(member_field "$priya_id" "' '.join(m.get('roleIds', []))") " in *" $design "*) fail "Priya still holds Design after Remove" ;; esac
seen removed
echo "Design taken from Priya by the Remove beside her in its editor"
reached titled 60
# shellcheck disable=SC2154
[ "$(member_field "$leo_id" "m.get('title', '')")" = "Release Captain" ] || fail "Leo's title is \"$(member_field "$leo_id" "m.get('title', '')")\""
seen titled
echo "a title set from Leo's row in Members"
reached tags 60
# From elsewhere, while #general is on the phone's screen: Maya loses Admin, so her tag is her next role.
# shellcheck disable=SC2154
api "$to" PATCH "/servers/$server/members/$maya_id" "{\"roles\":{\"roleIds\":[\"$design\"]}}" >/dev/null
seen tags
reached live 60
seen live
echo "a tag in #general followed a change made from elsewhere while the conversation was open: Maya, Admin to Design"
reached card 90
# shellcheck disable=SC2154
[ "$(member_field "$devon_id" "m.get('title', '')")" = "Principal Engineer" ] || fail "Devon's title is \"$(member_field "$devon_id" "m.get('title', '')")\" after the card's Edit"
# shellcheck disable=SC2154
case " $(member_field "$devon_id" "' '.join(m.get('roleIds', []))") " in *" $design "*) ;; *) fail "Devon does not hold Design after the card's Edit" ;; esac
seen card
echo "Devon's title and roles changed from his profile card's two Edits"
reached own 90
# shellcheck disable=SC2154
[ "$(member_field "$owner_id" "m.get('title', '')")" = "Founder" ] || fail "the owner's title is \"$(member_field "$owner_id" "m.get('title', '')")\" after one with a hidden character was typed"
seen own
echo "the owner's own card says You and Owner, and a title with a hidden character was refused in its editor"
reached titles 90
[ "$(server_field "d.get('memberTag', 'MEMBER_TAG_ROLE')")" = "MEMBER_TAG_TITLE" ] || fail "the server's tag is $(server_field "d.get('memberTag', 'MEMBER_TAG_ROLE')") after Titles was chosen"
seen titles
echo "the server switched to titles from Overview: memberTag on the instance, and #general tags by title"
reached kicked 90
# shellcheck disable=SC2154
[ "$(member_field "$sam_id" "1")" = "gone" ] || fail "Sam is still a member after Kick"
seen kicked
echo "Sam kicked from Members, with nothing asked first"
reached deleting 90
seen deleting
reached deleted 60
[ "$(role_field Helpers "1")" = "gone" ] || fail "Helpers is still a role after Delete role"
[ "$(roles_now)" = "Admin Moderator Design Engineering Can_invite" ] || fail "after the delete the roles stand as \"$(roles_now)\""
seen deleted
passed
echo "the role deleted after its sheet said who holds it"

# Devon's turn: the server showing roles again, and Maya Admin again, so above Devon.
api "$to" PATCH "/servers/$server" '{"memberTag":"MEMBER_TAG_ROLE"}' >/dev/null
# shellcheck disable=SC2154
member "$maya_id" "Head of Design" "$admin" "$design"
run "devon$stamp" testRolesAsOneWhoManagesWhatIsBelow
reached below 240
seen below
passed
echo "Devon (Moderator) is offered Roles and Members and no Overview; Admin and Moderator are locked, and Administrator is not Devon's to give"
xcrun simctl terminate "$udid" "$bundle" 2>/dev/null || true
echo "ok: $work"
