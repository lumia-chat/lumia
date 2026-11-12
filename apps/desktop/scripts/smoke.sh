#!/bin/sh
# Smoke-runs the desktop shell against a dev instance (mix phx.server in apps/instance):
# seeds two accounts, a server with two channels and a few messages with the release CLI,
# then draws the debug shell signed in as the first account (the LUMIA_DEV_* environment a
# debug build honours) and checks that it loaded the server, connected to the gateway,
# shows the messages and members, and sends. Headless: Rune's --snap renders the window's first frame to a PNG
# with the same renderer, so nothing here needs a screen-recording permission.
#
#   scripts/smoke.sh            # expects `cargo build` here and `cargo build --release -p lumia-cli` in apps/core
#   LUMIA_INSTANCE=http://host:4000 scripts/smoke.sh
#   LUMIA_CLI=/path/to/lumia-cli scripts/smoke.sh     # a CLI built elsewhere (another worktree)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
cli=${LUMIA_CLI:-$root/apps/core/target/release/lumia-cli}
shell="$here/../target/debug/lumia"
work=${LUMIA_SMOKE_WORK:-$here/../out/smoke}
rm -rf "$work" && mkdir -p "$work"
password="correct-horse-battery-staple"

curl -s -m 5 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }

stamp=$(date +%s)
one="desk$stamp"
two="mate$stamp"
a="$cli --data-dir $work/a"
b="$cli --data-dir $work/b"
$a instance add "$instance" >/dev/null
$a account create --handle "$one" --password "$password" >/dev/null
server=$($a server-create "Desktop smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
$a channel-create "$server" general >/dev/null
$a channel-create "$server" design >/dev/null
channel=$($a channels "$server" | grep "#general" | grep -oE "[0-9a-f-]{36}" | head -1)
invite=$($a invite-create "$server" | grep -oE "https?://[^ ]+" | head -1)
$b instance add "$instance" >/dev/null
$b account create --handle "$two" --password "$password" --invite "$invite" >/dev/null
$a send "$channel" "Hello from the CLI" >/dev/null
$b send "$channel" "The desktop shell reads this through the same core, with **Markdown** and \`code\`." >/dev/null
$a send "$channel" "了解しました。明日の朝に確認します 🙏" >/dev/null
echo "seeded $one and $two, server $server, channel $channel"

run() {
  LUMIA_DATA_DIR="$work/shell" LUMIA_DEV_INSTANCE="$instance" LUMIA_DEV_HANDLE="$one" LUMIA_DEV_PASSWORD="$password" "$shell" "$@"
}
run --snap "$work/shell.png" --scene live --theme light 2>"$work/light.log"
# The second run finds the first's session in the stores and its messages in SQLite.
run --snap "$work/shell-dark.png" --scene live --theme dark 2>"$work/dark.log"
run --tree "$work/tree.json" --scene live 2>"$work/tree.log"

fail() { echo "FAIL: $1"; exit 1; }
grep -q "gave up waiting" "$work"/*.log && fail "the shell never finished loading ($(grep -h 'gave up' "$work"/*.log | head -1))"
grep -q "Hello from the CLI" "$work/tree.json" || fail "the first message is not on screen"
grep -q "same core" "$work/tree.json" || fail "the other account's message is not on screen"
grep -q '"Messages in general"' "$work/tree.json" || fail "the message list is not named for its channel"
grep -q "$two" "$work/tree.json" || fail "the other member is not listed"
# The composer through the window's own handlers: a scripted press, typing and Return.
run --scene live --script "$here/send.json" --tree "$work/sent.json" --at 4.0 2>"$work/sent.log"
grep -q "Typed into the" "$work/sent.json" || fail "the message typed into the composer is not on screen"
$b channels "$server" | grep "#general" | grep -q "head=4" || fail "the typed message did not reach the instance"

# Every test, the live ones included (tests/live*.rs): sending and reading back, history
# paged from the store and the instance and scrolled to in the real list, linking this
# computer to an identity, personal conversations with CLI devices, the account screens,
# servers and invites. Each live test skips itself without this environment, which here
# would be a failure.
(cd "$here/.." && LUMIA_DEV_INSTANCE="$instance" LUMIA_DEV_HANDLE="$one" LUMIA_DEV_PASSWORD="$password" LUMIA_CLI="$cli" cargo test >"$work/live.log" 2>&1) \
  || { grep -E "panicked|FAILED|^error" -A12 "$work/live.log" | head -80; fail "the tests"; }
# One live test is left to skip itself: tests/live_app.rs launches the bundled app for real,
# which registers the bundle with LaunchServices and so takes the `lumia:` scheme on this
# Mac from whatever held it. Run it on purpose, with LUMIA_DEV_APP (see the README).
grep "skipped" "$work/live.log" | grep -qv "LUMIA_DEV_APP" && fail "a live test skipped itself"
echo "  window: $work/shell.png, $work/shell-dark.png"
echo "  tests: $(grep -c '^test .* ok$' "$work/live.log") passed"
echo "smoke: ok (signed in, snapshot loaded, gateway connected, messages and members on screen, a send stored, every test passed)"
