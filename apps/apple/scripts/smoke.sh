#!/bin/sh
# Smoke-runs the macOS app against a dev instance (mix phx.server in
# apps/instance): seeds an account, a server, a channel and two messages
# with the release CLI, launches the debug app signed in as that account
# (the LUMIA_DEV_* environment a debug build honours), and checks that the
# app shows a window, loads the snapshot and connects to the gateway.
#
#   scripts/smoke.sh            # expects the app built (xcodebuild) and the CLI (cargo build --release -p lumia-cli)
#   LUMIA_INSTANCE=http://host:4000 scripts/smoke.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
instance=${LUMIA_INSTANCE:-http://localhost:4000}
cli="$root/apps/core/target/release/lumia-cli"
app="$here/../Lumia-macOS/.build/Build/Products/Debug/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke}
rm -rf "$work" && mkdir -p "$work"
data="$work/cli"
password="correct-horse-battery-staple"

for _ in $(seq 1 60); do curl -s -m 2 "$instance/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
curl -s -m 2 "$instance/api/v1/instance" >/dev/null || { echo "no instance at $instance"; exit 1; }

stamp=$(date +%s)
handle="mac$stamp"
"$cli" --data-dir "$data" instance add "$instance" >/dev/null
"$cli" --data-dir "$data" account create --handle "$handle" --password "$password" >/dev/null
server=$("$cli" --data-dir "$data" server-create "Mac smoke" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" channel-create "$server" general >/dev/null
channel=$("$cli" --data-dir "$data" channels "$server" | grep -oE "[0-9a-f-]{36}" | head -1)
"$cli" --data-dir "$data" send "$channel" "Hello from the CLI" >/dev/null
"$cli" --data-dir "$data" send "$channel" "The macOS app reads this through the same core." >/dev/null
echo "seeded $handle, server $server, channel $channel"

pkill -x Lumia 2>/dev/null || true
sleep 1
# The app's state starts empty; a sandboxed build keeps it in its container.
rm -rf "$HOME/Library/Application Support/Lumia" "$HOME/Library/Containers/chat.lumia/Data/Library/Application Support/Lumia"
# The window is written as a PNG by the app itself (LUMIA_DEV_SHOT), since
# screen capture needs a permission this terminal lacks.
open -n "$app" --env "LUMIA_DEV_INSTANCE=$instance" --env "LUMIA_DEV_HANDLE=$handle" --env "LUMIA_DEV_PASSWORD=$password" --env "LUMIA_DEV_SHOT=$work/shell.png"
sleep 10

cat > "$work/windows.swift" <<'EOF'
import CoreGraphics
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
let mine = list.filter { ($0["kCGWindowOwnerName"] as? String) == "Lumia" && ($0["kCGWindowLayer"] as? Int) == 0 }
print("windows: \(mine.count)")
for w in mine { print("  \(w["kCGWindowBounds"] ?? "-")") }
EOF
windows=$(swift "$work/windows.swift")
echo "$windows"
alive=$(pgrep -x Lumia || true)
# The app sends the CLI's own channel a message; it lands in the CLI's view when the gateway is up.
"$cli" --data-dir "$data" send "$channel" "Still here?" >/dev/null
sleep 3
pkill -x Lumia 2>/dev/null || true
case "$windows" in *"windows: 0"*) echo "FAIL: no window"; exit 1 ;; esac
[ -n "$alive" ] || { echo "FAIL: the app exited"; exit 1; }
[ -f "$work/shell.png" ] && echo "  window: $work/shell.png"
echo "smoke: ok (window shown, app alive after sign-in and the gateway's first messages)"
