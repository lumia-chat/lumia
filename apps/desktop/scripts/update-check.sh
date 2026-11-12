#!/bin/sh
# Proves the macOS shell's self-update end to end on this machine, with no hands on a
# window: an installed build finds a newer one on its feed, downloads it, verifies it and
# installs it over itself, and refuses the releases it should refuse. Nothing here is a
# mock: the bundles are scripts/bundle-macos.sh's, the releases are scripts/dist-macos.sh's,
# the feed is served over HTTP by a server on this machine, and the updating is Sparkle's
# inside the launched app.
#
#   scripts/update-check.sh                 # cargo build --release, then the checks (a few minutes)
#   scripts/update-check.sh --debug         # the debug build: larger downloads, and its
#                                           # trace also shows the menu's update items made
#   scripts/update-check.sh --bin PATH      # a binary built elsewhere
#
# What it does:
#   1. Makes two throwaway Ed25519 key pairs in its own temporary directory (openssl, or
#      CryptoKit where openssl is LibreSSL). The repository's key is never involved, and
#      nothing reads or writes the Keychain: no generate_keys, no signing identity.
#   2. Makes three releases under a scratch bundle identifier, with the feed on
#      http://127.0.0.1:PORT and the first throwaway public key: an older build, a newer
#      one, and the newer one again with a delta from the older.
#   3. For each case below, copies an "installed" app out of a release's disk image, serves
#      a feed, and opens the app behind the front app with LUMIA_DATA_DIR in the temporary
#      directory. The bundle checks at launch (SUEnableAutomaticChecks, and no check on
#      record), and Sparkle's own setting for automatic download and install, written to the
#      scratch identifier's preferences, lets it install with no click: the installer waits
#      for the app to quit, then swaps the bundle. The script reads Sparkle's lines in the
#      unified log to know where it is, quits the app, and reads the version at that path.
#
# The cases:
#   update        the newer release as published                    -> installed
#   delta         the same with a delta from the older              -> installed, from the delta
#   other key     a release signed throughout with another key      -> the feed is refused
#   image key     our feed, the image signed with another key       -> the image is refused
#   altered       our release, one byte of the image changed        -> the image is refused
#   relabelled    the older release's feed edited to say "newer"    -> the feed is refused
#   downgrade     a validly signed feed offering the older image    -> the install is refused
#                 as a newer version, to a newer installed app
#
# It leaves nothing running and nothing registered: the app, Sparkle's installer, the server
# and the log reader are stopped, the scratch identifier's preferences are deleted and its
# bundles unregistered from LaunchServices, and the temporary directory (the only directory
# this script ever deletes) is removed; it says so at the end, or says what it could not
# undo and fails. macOS and Sparkle keep small cache directories under the scratch
# identifier in ~/Library; they are named at the end, and are not this script's to delete.
#
# It opens a window seven times for a few seconds each, behind whatever is in front. It
# needs to read the unified log (`log stream`), which is how it follows Sparkle, and
# python3 for the server. LUMIA_UPDATE_SETTLE (seconds, 3) is how long Sparkle's installer
# must sit idle before the app is quit; a slow machine may want more.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
crate=$(cd "$here/.." && pwd)
[ "$(uname)" = Darwin ] || { echo "update-check: Sparkle updates the macOS app; this needs macOS"; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "update-check: python3 serves the feed; there is none"; exit 1; }

# Scratch, and only ever this: its preferences are deleted and its bundles unregistered.
id=chat.lumia.desktop.updatetest
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
# How long the installer must sit idle before the app is quit (see `staged`).
settle=${LUMIA_UPDATE_SETTLE:-3}
bin=""
profile=release
while [ $# -gt 0 ]; do
  case "$1" in
    --debug) profile=debug ;;
    --bin) bin=${2:?--bin needs a path}; shift ;;
    *) echo "update-check: unknown argument $1 (see the head of $0)"; exit 2 ;;
  esac
  shift
done

say() { printf 'update-check: %s\n' "$*"; }

work=$(mktemp -d "${TMPDIR:-/tmp}/lumia-update-check.XXXXXX")
# As processes and the log name it (/var is /private/var).
work=$(cd "$work" && pwd -P)
tag=$(basename "$work")
mkdir -p "$work/keys" "$work/logs" "$work/run" "$work/data" "$work/serve"
: >"$work/opened"
server=""
reader=""
app=""
exe=""

# Everything of the installed apps that is still running: the app, and Sparkle's installer
# and progress agent, which run from inside its bundle.
ours() { pgrep -f "$work/run/" || true; }

# Where disk images under the scratch identifier's caches are mounted: the update Sparkle's
# installer is copying out of, which it unmounts itself unless it was stopped half way.
sparkle_volumes() {
  hdiutil info 2>/dev/null | awk -v id="/Caches/$id/" '
    /^image-path/ { ours = index($0, id) > 0 }
    ours && /\/Volumes\// { print $NF }'
}

stop_server() {
  if [ -n "$server" ]; then kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true; server=""; fi
}
stop_reader() {
  if [ -n "$reader" ]; then kill "$reader" 2>/dev/null || true; wait "$reader" 2>/dev/null || true; reader=""; fi
}

cleanup() {
  status=$?
  trap - EXIT INT TERM
  set +e
  for pid in $(ours); do kill "$pid" 2>/dev/null; done
  n=0
  while [ -n "$(ours)" ] && [ "$n" -lt 40 ]; do sleep 0.25; n=$((n + 1)); done
  for pid in $(ours); do kill -9 "$pid" 2>/dev/null; done
  stop_server
  stop_reader
  # A release's image this script had open, and one Sparkle's installer had open if it was
  # stopped while it was copying out of it.
  [ -d "$work/mount/Lumia.app" ] && hdiutil detach "$work/mount" -force >/dev/null 2>&1
  for volume in $(sparkle_volumes); do hdiutil detach "$volume" -force >/dev/null 2>&1; done
  # Sparkle submits its installer and progress agent to launchd under the identifier; they
  # go when the processes do. Anything still listed is removed by its label.
  for label in "$id-sparkle-updater" "$id-sparkle-progress"; do
    launchctl list "$label" >/dev/null 2>&1 && launchctl remove "$label"
  done
  # The scratch identifier's preferences, and the empty file the preferences daemon can
  # leave where they were.
  defaults delete "$id" >/dev/null 2>&1
  rm -f "$HOME/Library/Preferences/$id.plist"
  while read -r bundle; do
    [ -n "$bundle" ] || continue
    "$lsregister" -u "$bundle/Contents/Frameworks/Sparkle.framework/Versions/B/Updater.app" >/dev/null 2>&1
    "$lsregister" -u "$bundle" >/dev/null 2>&1
  done <"$work/opened"
  left=""
  [ -z "$(ours)" ] || left="$left processes ($(ours | tr '\n' ' '))"
  launchctl list 2>/dev/null | grep -q "$id" && left="$left launchd jobs"
  if [ -e "$HOME/Library/Preferences/$id.plist" ] || defaults read "$id" 2>/dev/null | grep -q "="; then
    left="$left preferences"
  fi
  "$lsregister" -dump 2>/dev/null | grep -q "$tag" && left="$left LaunchServices registrations"
  [ -z "$(sparkle_volumes)" ] || left="$left mounted images ($(sparkle_volumes | tr '\n' ' '))"
  [ -d "$work/mount/Lumia.app" ] && left="$left a mounted image at $work/mount"
  rm -rf "$work"
  [ -d "$work" ] && left="$left $work"
  if [ -n "$left" ]; then
    say "LEFT BEHIND:$left"
    [ "$status" -ne 0 ] || status=1
  else
    say "cleaned up: no process, launchd job, preference or LaunchServices registration of $id; $work removed"
  fi
  kept=""
  for dir in "$HOME/Library/Caches/$id" "$HOME/Library/HTTPStorages/$id" "$HOME/Library/Saved Application State/$id.savedState" "$HOME/Library/WebKit/$id"; do
    [ -e "$dir" ] && kept="$kept
  $dir"
  done
  [ -z "$kept" ] || say "kept by macOS and Sparkle for the scratch identifier, not this script's to delete:$kept"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

fail() {
  say "FAIL: $*"
  for log in "$work/logs/$case.access" "$work/logs/$case.sparkle"; do
    [ -f "$log" ] && { echo "--- $(basename "$log")"; cut -c1-400 "$log"; }
  done
  exit 1
}
case=setup

# Waits up to $3 seconds for a line matching the extended expression $2 in the file $1.
await() {
  n=0
  until grep -Eq "$2" "$1" 2>/dev/null; do
    n=$((n + 1))
    [ "$n" -le $(($3 * 4)) ] || return 1
    sleep 0.25
  done
}

# --- keys ---------------------------------------------------------------------------
# A key pair as Sparkle's tools read it: NAME.private is the base64 of the 32-byte seed,
# NAME.public the base64 of the public key (SUPublicEDKey).
umask 077
openssl=""
for candidate in openssl /opt/homebrew/bin/openssl /usr/local/bin/openssl; do
  if command -v "$candidate" >/dev/null 2>&1 && "$candidate" genpkey -algorithm ed25519 -out "$work/keys/probe.pem" 2>/dev/null; then
    openssl=$candidate
    break
  fi
done
rm -f "$work/keys/probe.pem"
keypair() {
  if [ -n "$openssl" ]; then
    "$openssl" genpkey -algorithm ed25519 -out "$work/keys/$1.pem"
    # The key is the last 32 bytes of each DER form.
    "$openssl" pkey -in "$work/keys/$1.pem" -outform DER | tail -c 32 | base64 >"$work/keys/$1.private"
    "$openssl" pkey -in "$work/keys/$1.pem" -pubout -outform DER | tail -c 32 | base64 >"$work/keys/$1.public"
  else
    # macOS's own openssl is LibreSSL, which has no Ed25519 here; CryptoKit does, in memory.
    cat >"$work/keys/pair.swift" <<'SWIFT'
import CryptoKit
import Foundation
let key = Curve25519.Signing.PrivateKey()
print(key.rawRepresentation.base64EncodedString())
print(key.publicKey.rawRepresentation.base64EncodedString())
SWIFT
    swift "$work/keys/pair.swift" >"$work/keys/$1.pair" || { say "no openssl with Ed25519 and no swift: cannot make a key pair"; exit 1; }
    sed -n 1p "$work/keys/$1.pair" >"$work/keys/$1.private"
    sed -n 2p "$work/keys/$1.pair" >"$work/keys/$1.public"
  fi
  [ "$(base64 -D <"$work/keys/$1.public" | wc -c | tr -d ' ')" = 32 ] || { say "could not make an Ed25519 key pair"; exit 1; }
}
keypair ours
keypair theirs
umask 022
public=$(cat "$work/keys/ours.public")
say "two throwaway key pairs in $work/keys ($([ -n "$openssl" ] && echo openssl || echo CryptoKit)); the app is built to trust $public"

# --- releases -----------------------------------------------------------------------
if [ -z "$bin" ]; then
  if [ "$profile" = release ]; then
    (cd "$crate" && cargo build --release)
  else
    (cd "$crate" && cargo build)
  fi
  bin="$crate/target/$profile/lumia"
fi
[ -x "$bin" ] || { say "no binary at $bin"; exit 1; }
port=$(python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
origin="http://127.0.0.1:$port"

# A release of version $2, build $3, into the directory $1; $4 is a previous release's
# directory, for a delta. Signed ad hoc whatever the caller's environment says: a signing
# identity would be read from the Keychain.
release() {
  env -u LUMIA_SIGN_IDENTITY LUMIA_BUNDLE_ID="$id" LUMIA_BUILD="$3" \
    LUMIA_UPDATE_FEED="$origin/appcast.xml" LUMIA_UPDATE_KEY="$public" \
    SPARKLE_KEY_PATH="$work/keys/ours.private" LUMIA_DOWNLOAD_PREFIX="$origin/" LUMIA_PREVIOUS_DIR="${4:-}" \
    "$here/dist-macos.sh" "$2" --bin "$bin" --out "$1" >"$work/logs/release-$(basename "$1").log" 2>&1 \
    || { cat "$work/logs/release-$(basename "$1").log"; say "FAIL: scripts/dist-macos.sh $2"; exit 1; }
}
old_version=1.0.0
old_build=100
new_version=1.0.1
new_build=200
release "$work/release/old" "$old_version" "$old_build"
release "$work/release/new" "$new_version" "$new_build"
release "$work/release/delta" "$new_version" "$new_build" "$work/release/old"
old_dmg="Lumia-$old_version.dmg"
new_dmg="Lumia-$new_version.dmg"
delta="Lumia$new_build-$old_build.delta"
[ -f "$work/release/delta/$delta" ] || { cat "$work/logs/release-delta.log"; say "FAIL: no delta in the third release"; exit 1; }
"$here/bundle-macos.sh" --sparkle "$work/sparkle" >/dev/null
sign_update="$work/sparkle/bin/sign_update"
say "three releases by scripts/dist-macos.sh: $old_version ($old_build), $new_version ($new_build), and $new_version with $delta"

# --- the feeds the cases serve --------------------------------------------------------
# A feed of one item: build $2, version $3, the image $4 (served from this machine) with
# the signature $5; then signed as a whole with the private key $6.
write_feed() {
  length=$(wc -c <"$(dirname "$1")/$4" | tr -d ' ')
  cat >"$1" <<FEED
<?xml version="1.0" standalone="yes"?>
<rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle" version="2.0">
    <channel>
        <title>Lumia</title>
        <item>
            <title>$3</title>
            <sparkle:version>$2</sparkle:version>
            <sparkle:shortVersionString>$3</sparkle:shortVersionString>
            <sparkle:minimumSystemVersion>14.0</sparkle:minimumSystemVersion>
            <enclosure url="$origin/$4" length="$length" type="application/octet-stream" sparkle:edSignature="$5"></enclosure>
        </item>
    </channel>
</rss>
FEED
  "$sign_update" --ed-key-file "$6" "$1" >/dev/null
}
# The signature of the file $1 by the private key $2.
signature() { "$sign_update" --ed-key-file "$2" -p "$1"; }

# A release signed throughout by someone else's key: what a taken-over host can make.
mkdir -p "$work/serve/other-key"
cp "$work/release/new/$new_dmg" "$work/serve/other-key/"
write_feed "$work/serve/other-key/appcast.xml" "$new_build" "$new_version" "$new_dmg" \
  "$(signature "$work/serve/other-key/$new_dmg" "$work/keys/theirs.private")" "$work/keys/theirs.private"

# Our feed, naming an image signed by the other key.
mkdir -p "$work/serve/image-key"
cp "$work/release/new/$new_dmg" "$work/serve/image-key/"
write_feed "$work/serve/image-key/appcast.xml" "$new_build" "$new_version" "$new_dmg" \
  "$(signature "$work/serve/image-key/$new_dmg" "$work/keys/theirs.private")" "$work/keys/ours.private"

# Our release as published, with one byte of the image changed afterwards (the length,
# which the feed also states, stays the same).
mkdir -p "$work/serve/altered"
cp "$work/release/new/$new_dmg" "$work/release/new/appcast.xml" "$work/serve/altered/"
at=$(($(wc -c <"$work/serve/altered/$new_dmg") / 2))
was=$(od -An -tu1 -j "$at" -N 1 "$work/serve/altered/$new_dmg" | tr -d ' ')
printf "\\$(printf '%03o' $(((was + 1) % 256)))" | dd of="$work/serve/altered/$new_dmg" bs=1 seek="$at" conv=notrunc 2>/dev/null
cmp -s "$work/release/new/$new_dmg" "$work/serve/altered/$new_dmg" && { say "FAIL: the image was not altered"; exit 1; }

# The older release's own feed and image, with the feed edited to call it a newer
# version than the one installed. The signature it carries is the one it was published
# with: whoever edits a feed cannot sign it again.
newer_build=300
newer_version=1.0.2
mkdir -p "$work/serve/relabelled"
cp "$work/release/old/$old_dmg" "$work/serve/relabelled/"
sed -e "s|<sparkle:version>$old_build<|<sparkle:version>$newer_build<|" \
  -e "s|<sparkle:shortVersionString>$old_version<|<sparkle:shortVersionString>$newer_version<|" \
  "$work/release/old/appcast.xml" >"$work/serve/relabelled/appcast.xml"
grep -q "<sparkle:version>$newer_build<" "$work/serve/relabelled/appcast.xml" || { say "FAIL: the older feed was not relabelled"; exit 1; }

# The same lie in a feed that does verify: signed with our key, offering the older image,
# with the signature it was published with, as a newer version. No host can make this
# without the key; it is what shows the installer's own refusal to go backwards.
mkdir -p "$work/serve/downgrade"
cp "$work/release/old/$old_dmg" "$work/serve/downgrade/"
write_feed "$work/serve/downgrade/appcast.xml" "$newer_build" "$newer_version" "$old_dmg" \
  "$(signature "$work/serve/downgrade/$old_dmg" "$work/keys/ours.private")" "$work/keys/ours.private"

# --- one launch -----------------------------------------------------------------------
version_of() {
  printf '%s (%s)' "$(plutil -extract CFBundleShortVersionString raw "$1/Contents/Info.plist")" \
    "$(plutil -extract CFBundleVersion raw "$1/Contents/Info.plist")"
}

# Copies the app out of the disk image $2 as the installed app of the case $1, as dragging
# it to a folder does, serves the directory $3 as the feed's origin, and opens the app.
launch() {
  case=$1
  dir="$work/run/$case"
  mkdir -p "$dir" "$work/mount"
  # hdiutil says on every call that newer systems have another spelling for this; it is
  # the one every system has, and the one Sparkle itself uses.
  hdiutil attach -nobrowse -readonly -noautoopen -mountpoint "$work/mount" "$2" >/dev/null 2>&1 || fail "could not open $2"
  ditto "$work/mount/Lumia.app" "$dir/Lumia.app"
  hdiutil detach "$work/mount" >/dev/null 2>&1 || fail "could not close $2"
  app="$dir/Lumia.app"
  exe="$app/Contents/MacOS/lumia"
  [ -x "$exe" ] || fail "no app in $2"
  access="$work/logs/$case.access"
  sparkle="$work/logs/$case.sparkle"
  # No check on record, so the bundle checks as it starts; and Sparkle's own switch for
  # downloading and installing without asking.
  defaults delete "$id" >/dev/null 2>&1 || true
  defaults write "$id" SUAutomaticallyUpdate -bool YES
  python3 -I -m http.server "$port" --bind 127.0.0.1 --directory "$3" >"$access" 2>&1 &
  server=$!
  n=0
  until curl -fs -o /dev/null "$origin/appcast.xml"; do
    n=$((n + 1))
    [ "$n" -le 40 ] || fail "the server on $origin did not start"
    sleep 0.25
  done
  # Sparkle's own account of what it does, from the processes under this directory only.
  /usr/bin/log stream --style compact \
    --predicate "subsystem == \"org.sparkle-project.Sparkle\" AND processImagePath CONTAINS \"$tag\"" >"$sparkle" 2>&1 &
  reader=$!
  await "$sparkle" "Filtering the log data" 10 || fail "the unified log could not be read (log stream), which is how this follows Sparkle"
  echo "$app" >>"$work/opened"
  # The trace is a debug build's; a release build ignores the variable.
  open -n -g --env "LUMIA_DATA_DIR=$work/data/$case" --env "LUMIA_DEV_TRACE=$work/logs/$case.trace" "$app" \
    || fail "could not open $app"
  n=0
  until [ -n "$(pgrep -f "$exe" || true)" ]; do
    n=$((n + 1))
    [ "$n" -le 80 ] || fail "the app did not start"
    sleep 0.25
  done
}

# Whether the file $1 was asked of the server (its access log).
fetched() { grep -q "GET /$1 " "$access"; }

installer() { pgrep -f "Autoupdate $id" || true; }

# Waits until Sparkle's installer has the update copied out, validated and staged: it is
# then idle, waiting for the app to quit. Quitting earlier makes it give the update up
# (which would fail this check, never pass it). Idle is $settle seconds in a row with no
# child process and no processor time used.
staged() {
  quiet=0
  n=0
  last=""
  while [ "$quiet" -lt "$settle" ]; do
    n=$((n + 1))
    [ "$n" -le 120 ] || return 1
    pid=$(installer | head -1)
    [ -n "$pid" ] || return 1
    now="$(ps -o cputime= -p "$pid" | tr -d ' ')$(pgrep -P "$pid" || true)"
    if [ "$now" = "$last" ]; then quiet=$((quiet + 1)); else quiet=0; fi
    last=$now
    sleep 1
  done
}

# Quits the app as `kill` does (which is how tests/live_app.rs quits it) and waits for
# everything of this case to be gone: the installer does its work once the app has.
quit() {
  for pid in $(pgrep -f "$exe" || true); do kill "$pid" 2>/dev/null || true; done
  n=0
  while [ -n "$(ours)" ]; do
    n=$((n + 1))
    [ "$n" -le 240 ] || fail "still running a minute after the app was quit: $(ours | tr '\n' ' ')"
    sleep 0.25
  done
  stop_server
  stop_reader
}

# The app at the case's path is now $1.
expect() {
  is=$(version_of "$app")
  [ "$is" = "$1" ] || fail "the app at $app is $is, expected $1"
}

# A case passed: says so, and shows what it rests on: what the app asked the server for
# (after this script's own probe), and what Sparkle wrote to the log about it, the app's
# lines and the installer's. Each is shown once and cut short (Sparkle reports an error at
# every level it passes through); what says nothing about the verdict is left out: the
# note that a feed should be HTTPS, a stale bookmark, and the installer and its agent
# reporting that they stopped.
ok() {
  say "ok  $case: $*"
  printf '      asked for:%s\n' "$(sed -n 's|.*"GET \(/[^ ]*\) .*|\1|p' "$access" | sed 1d | tr '\n' ' ' | sed 's/^/ /')"
  sed -n 's/^[^ ]* [^ ]* [A-Za-z]* *\([A-Za-z]*\)\[[^]]*\] \[org\.sparkle-project\.Sparkle:Sparkle\] \(.*\)$/\1: \2/p' "$sparkle" \
    | grep -Ev "may need to change to use HTTPS|bookmark data|Agent |Invalidation on remote port|error occurred while running the updater|Failed to unarchive file|Failed to start installer" \
    | sed -e 's/: Error: /: /' -e 's/: Error: /: /' -e 's/ (null) (URL (null))$//' -e "s|$HOME|~|g" \
    | awk '{ line = $0; sub(/^[A-Za-z]*: /, "", line); if (!seen[line]++) print "      " $0 }' | cut -c1-190
}

# --- the cases ------------------------------------------------------------------------
old="$old_version ($old_build)"
new="$new_version ($new_build)"

launch update "$work/release/old/$old_dmg" "$work/release/new"
expect "$old"
await "$sparkle" "OK: EdDSA signature is correct for appcast" 30 || fail "the app did not accept its feed"
await "$sparkle" "OK: EdDSA signature is correct for update" 60 || fail "the installer did not verify the image"
fetched "$new_dmg" || fail "the image was never asked for"
staged || fail "the installer did not stage the update"
quit
expect "$new"
codesign --verify --deep --strict "$app" || fail "the installed update's signature does not verify"
LUMIA_DATA_DIR="$work/data/update-after" "$exe" --snap "$work/logs/update-after.png" >/dev/null 2>&1 && [ -s "$work/logs/update-after.png" ] \
  || fail "the installed update does not draw a frame"
# What only a debug build says: that the shell started the updater itself, and made the
# menu with the two update items. Nothing can look at the menu bar from here.
trace="$work/logs/update.trace"
if [ -f "$trace" ]; then
  grep -q "^updater started" "$trace" || fail "the shell's trace does not say the updater started"
  grep -q "^menu .*updates=Some" "$trace" || fail "the shell's trace does not show the menu made with the update items"
  menu="the debug build's trace: $(grep -c "^updater started" "$trace") updater started, the menu made with $(grep "^menu .*updates=Some" "$trace" | tail -1 | sed 's/.*updates=//')"
else
  menu="a release build keeps no trace, so the menu's update items are not shown here (--debug shows them)"
fi
ok "$old checked its feed at launch, downloaded $new_dmg, verified it and installed it when the app quit; the app at that path is now $new, its signature verifies and it draws; $menu"

launch delta "$work/release/old/$old_dmg" "$work/release/delta"
await "$sparkle" "OK: EdDSA signature is correct for appcast" 30 || fail "the app did not accept its feed"
await "$sparkle" "OK: EdDSA signature is correct for update" 60 || fail "the installer did not verify the delta"
fetched "$delta" || fail "the delta was never asked for"
fetched "$new_dmg" && fail "the full image was downloaded although a delta applied"
staged || fail "the installer did not stage the update"
quit
expect "$new"
codesign --verify --deep --strict "$app" || fail "the app patched by the delta does not verify"
ok "$old took $delta ($(wc -c <"$work/release/delta/$delta" | tr -d ' ') bytes) instead of the image and is now $new, signature intact"

launch other-key "$work/release/old/$old_dmg" "$work/serve/other-key"
await "$sparkle" "EdDSA signature does not match.*appcast" 30 || fail "the app did not refuse a feed signed with another key"
sleep "$settle"
fetched "$new_dmg" && fail "the image was downloaded from a feed that did not verify"
quit
expect "$old"
ok "a release signed throughout with another key is refused at the feed; nothing was downloaded, the app is still $old"

launch image-key "$work/release/old/$old_dmg" "$work/serve/image-key"
await "$sparkle" "OK: EdDSA signature is correct for appcast" 30 || fail "the app did not accept its feed"
await "$sparkle" "Autoupdate.*EdDSA signature does not match" 60 || fail "the installer did not refuse an image signed with another key"
quit
expect "$old"
ok "our feed, an image signed with another key: downloaded, refused by the installer before it was opened; still $old"

launch altered "$work/release/old/$old_dmg" "$work/serve/altered"
await "$sparkle" "OK: EdDSA signature is correct for appcast" 30 || fail "the app did not accept its feed"
await "$sparkle" "Autoupdate.*EdDSA signature does not match" 60 || fail "the installer did not refuse an altered image"
quit
expect "$old"
ok "our release with one byte of the image changed after signing: downloaded, refused by the installer; still $old"

launch relabelled "$work/release/new/$new_dmg" "$work/serve/relabelled"
expect "$new"
await "$sparkle" "EdDSA signature does not match.*appcast" 30 || fail "the app did not refuse an edited feed"
sleep "$settle"
fetched "$old_dmg" && fail "the older image was downloaded from a feed that did not verify"
quit
expect "$new"
ok "the older release's feed edited to offer $old as $newer_version ($newer_build) no longer verifies and is refused; nothing was downloaded, the app is still $new"

launch downgrade "$work/release/new/$new_dmg" "$work/serve/downgrade"
await "$sparkle" "OK: EdDSA signature is correct for appcast" 30 || fail "the app did not accept its feed"
await "$sparkle" "OK: EdDSA signature is correct for update" 60 || fail "the installer did not verify the older image"
await "$sparkle" "Refusing to downgrade" 60 || fail "the installer did not refuse to downgrade"
quit
expect "$new"
ok "a feed signed with our key offering $old as $newer_version ($newer_build): the image verifies, and the installer refuses to replace $new with it"

case=finished
say "all seven cases passed"
