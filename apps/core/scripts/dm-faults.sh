#!/bin/sh
# The e2ee spec section 12 cases that need a second process or a fault,
# and identity spec 4.5's re-wrap, driven with the CLI against a running
# instance: a crash between the MLS persist and the send, the epoch moving
# under an unacked send, two members committing at once, and a root
# rotation re-wrapping the history backup. Each case gets its own
# identities and conversation.
#
#   apps/core/scripts/dm-faults.sh [instance-url]     # default http://localhost:4001
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
cli="$root/apps/core/target/release/lumia-cli"
instance=${1:-http://localhost:4001}
work="$here/../.build/dm-faults"
rm -rf "$work" && mkdir -p "$work"
stamp=$(date +%s)
[ -x "$cli" ] || (cd "$root/apps/core" && cargo build --release -p lumia-cli)
curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }

fail() { echo "FAIL: $1"; exit 1; }

# Runs a command for a few seconds, then stops it; its output goes to $2.
run_for() { secs=$1; log=$2; shift 2; "$@" >"$log" 2>&1 & pid=$!; sleep "$secs"; kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; }

# Two identities with a conversation between them: A accepts B's request
# and creates the group, B joins from the welcome. Sets A, B, aacct, channel.
pair() {
  A="$work/$1-a"; B="$work/$1-b"
  for d in "$A" "$B"; do "$cli" --data-dir "$d" instance add "$instance" >/dev/null; done
  "$cli" --data-dir "$A" identity create --label "$1-a" | tail -1 >"$work/$1-a.phrase"
  "$cli" --data-dir "$A" account create-by-identity --handle "$1a$stamp" >/dev/null
  aacct=$("$cli" --data-dir "$A" dm setup | grep -oE "account [0-9a-f-]{36}" | cut -d' ' -f2)
  "$cli" --data-dir "$B" identity create --label "$1-b" >/dev/null
  "$cli" --data-dir "$B" account create-by-identity --handle "$1b$stamp" >/dev/null
  "$cli" --data-dir "$B" dm setup >/dev/null
  "$cli" --data-dir "$B" dm request "$aacct" >/dev/null
  req=$("$cli" --data-dir "$A" dm requests | grep -oE "[0-9a-f-]{36}" | head -1)
  "$cli" --data-dir "$A" dm respond "$req" accept >/dev/null
  channel=$("$cli" --data-dir "$B" dm open "$aacct" | grep -oE "^[0-9a-f-]{36}" | head -1)
  [ -n "$channel" ] || fail "$1: no conversation"
}

# Links a new device $2 to the identity on $1, signed in with key packages
# published; it joins no group until a member commits it in.
link() {
  "$cli" --data-dir "$2" instance add "$instance" >/dev/null
  req=$("$cli" --data-dir "$2" identity link-request --label "$(basename "$2")")
  code=$("$cli" --data-dir "$1" identity approve "$req" | awk '{print $2}')
  "$cli" --data-dir "$2" identity claim "$code" >/dev/null
  "$cli" --data-dir "$2" login --identity >/dev/null
  "$cli" --data-dir "$2" dm setup >/dev/null
}

messages() { "$cli" --data-dir "$1" dm messages "$channel"; }

# ---- Crash between the MLS state persist and the send --------------------------------
pair crash
set +e
"$cli" --data-dir "$A" dm send "$channel" "persisted, then the crash" --crash-after-persist >"$work/crash-send.log" 2>&1
code=$?
set -e
[ "$code" = 3 ] || fail "crash: expected the injected exit 3, got $code"
run_for 6 "$work/crash-resend.log" "$cli" --data-dir "$A" dm tail "$channel"
[ "$(messages "$A" | grep -c "persisted, then the crash")" = 1 ] || fail "crash: the outbox did not resend the message on restart, or A did not record it; see $work/crash-resend.log"
run_for 5 "$work/crash-b.log" "$cli" --data-dir "$B" dm tail "$channel"
[ "$(messages "$B" | grep -c "persisted, then the crash")" = 1 ] || fail "crash: B does not hold exactly one copy; see $(messages "$B")"
echo "crash between persist and send: the outbox resent the identical bytes once; both sides hold one message"

# ---- The epoch moves before an unacked send is retried ------------------------------
pair epoch
set +e
"$cli" --data-dir "$A" dm send "$channel" "sealed under the old epoch" --crash-after-persist >/dev/null 2>&1
set -e
link "$B" "$work/epoch-b2"
run_for 6 "$work/epoch-commit.log" "$cli" --data-dir "$B" dm tail "$channel"
grep -q "commit acked" "$work/epoch-commit.log" || fail "epoch: B did not commit its new device; see $work/epoch-commit.log"
run_for 6 "$work/epoch-resend.log" "$cli" --data-dir "$A" dm tail "$channel"
[ "$(messages "$A" | grep -c "sealed under the old epoch")" = 1 ] || fail "epoch: the outbox did not resend; see $work/epoch-resend.log"
run_for 5 "$work/epoch-b.log" "$cli" --data-dir "$B" dm tail "$channel"
[ "$(messages "$B" | grep -c "sealed under the old epoch")" = 1 ] || fail "epoch: B does not hold exactly one copy; see $(messages "$B")"
run_for 6 "$work/epoch-b2.log" "$cli" --data-dir "$work/epoch-b2" dm tail "$channel"
messages "$work/epoch-b2" | grep -q "sent before this device joined" || fail "epoch: the device added in between did not mark the message as before its join"
echo "epoch moved under an unacked send: the resend decrypted under the past epoch, once; the new device marked it as before its join"

# ---- Two members commit concurrently -------------------------------------------------
attempt=0
while :; do
  attempt=$((attempt + 1))
  pair "commits$attempt"
  link "$A" "$work/commits$attempt-a2"
  link "$B" "$work/commits$attempt-b2"
  "$cli" --data-dir "$A" dm tail "$channel" >"$work/commits-a.log" 2>&1 & pa=$!
  "$cli" --data-dir "$B" dm tail "$channel" >"$work/commits-b.log" 2>&1 & pb=$!
  sleep 10; kill $pa $pb 2>/dev/null || true; wait $pa $pb 2>/dev/null || true
  refused=$(cat "$work/commits-a.log" "$work/commits-b.log" | grep -c "commit refused (epoch)" || true)
  if [ "$refused" = 1 ]; then break; fi
  [ "$refused" = 0 ] || fail "commits: $refused refusals; see $work/commits-*.log"
  [ "$attempt" -lt 4 ] || fail "commits: the two commits never overlapped in $attempt attempts"
  echo "   (attempt $attempt: the commits did not overlap; again)"
done
grep -q "commit acked" "$work/commits-a.log" "$work/commits-b.log" || fail "commits: no commit was acked; see $work/commits-*.log"
grep -q "retrying after the refusal" "$work/commits-a.log" "$work/commits-b.log" || fail "commits: the loser did not retry; see $work/commits-*.log"
# The group converged with all four devices: a message from A reaches B, A2 and B2.
run_for 6 "$work/commits-a2.log" "$cli" --data-dir "$work/commits$attempt-a2" dm tail "$channel"
run_for 6 "$work/commits-b2.log" "$cli" --data-dir "$work/commits$attempt-b2" dm tail "$channel"
"$cli" --data-dir "$A" dm send "$channel" "after both commits" >/dev/null
for d in "$B" "$work/commits$attempt-a2" "$work/commits$attempt-b2"; do
  run_for 5 "$work/commits-read.log" "$cli" --data-dir "$d" dm tail "$channel"
  [ "$(messages "$d" | grep -c "after both commits")" = 1 ] || fail "commits: $(basename "$d") did not get the message after the commits"
done
echo "concurrent commits: one was refused on the epoch rule and retried on the new epoch without a duplicate; all four devices read the next message"

# ---- Root rotation re-wraps the history backup ---------------------------------------
pair rotate
"$cli" --data-dir "$A" dm backup on >/dev/null
"$cli" --data-dir "$A" dm send "$channel" "kept in the archive" >"$work/rotate-send.log" 2>&1
grep -q "backed up 1 operation" "$work/rotate-send.log" || fail "rotate: the send was not backed up; see $work/rotate-send.log"
old=$(cat "$work/rotate-a.phrase")
"$cli" --data-dir "$A" identity rotate --phrase "$old" >"$work/rotate.log" 2>&1 || fail "rotate: $(cat "$work/rotate.log")"
grep -q "re-wrapped 1 chunk key" "$work/rotate.log" || fail "rotate: the chunk key was not re-wrapped; see $work/rotate.log"
new=$(tail -1 "$work/rotate.log")
# A device recovered from the new phrase restores the archive; the old phrase recovers nothing.
R="$work/rotate-r"
"$cli" --data-dir "$R" instance add "$instance" >/dev/null
"$cli" --data-dir "$R" identity recover --phrase "$new" >"$work/rotate-recover.log" 2>&1 || fail "rotate: recovery with the new phrase failed: $(cat "$work/rotate-recover.log")"
"$cli" --data-dir "$R" dm restore >"$work/rotate-restore.log" 2>&1 || fail "rotate: restore failed: $(cat "$work/rotate-restore.log")"
[ "$(messages "$R" | grep -c "kept in the archive")" = 1 ] || fail "rotate: the recovered device did not restore the message; see $work/rotate-restore.log"
S="$work/rotate-stale"
"$cli" --data-dir "$S" instance add "$instance" >/dev/null
if "$cli" --data-dir "$S" identity recover --phrase "$old" >"$work/rotate-old.log" 2>&1; then fail "rotate: the old phrase still recovers"; fi
echo "root rotation: the chunk key was re-wrapped, a device recovered from the new phrase restored the archive, and the old phrase is dead"
echo "dm-faults: ok"
