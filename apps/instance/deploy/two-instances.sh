#!/bin/sh
# The cross-instance run (docs/specs/deployment.md section 2.3, plan step
# 8): two instances from `compose.two.yml`, the flagship and a community,
# and one identity with an account on both, made by identity proof; a
# second identity on the community alone joins the first one's server
# through an invite and posts, and the first reads it; then the CLI's
# scenario suite runs against each instance. Builds the image unless
# LUMIA_IMAGE names one; leaves the instances up with --keep.
#
#   apps/instance/deploy/two-instances.sh            # from the repository root
#   apps/instance/deploy/two-instances.sh --keep
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root="$here/../../.."
compose="docker compose -f $here/compose.two.yml"
cli="$root/apps/core/target/release/lumia-cli"
work="$here/../.build/two-instances"
flagship=http://localhost:4100
community=http://127.0.0.1:4200
password="correct-horse-battery-staple"
keep=false
for arg in "$@"; do [ "$arg" = --keep ] && keep=true; done

[ -x "$cli" ] || (cd "$root/apps/core" && cargo build --release -p lumia-cli)
if [ -z "${LUMIA_IMAGE:-}" ]; then
  docker build -q -f "$root/apps/instance/Dockerfile" -t lumia/instance "$root" >/dev/null
fi
rm -rf "$work" && mkdir -p "$work"
$compose up -d --wait --quiet-pull
cleanup() { $keep || $compose down -v --remove-orphans >/dev/null 2>&1 || true; }
trap cleanup EXIT

for url in "$flagship" "$community"; do
  for _ in $(seq 1 60); do curl -s -m 2 "$url/api/v1/instance" >/dev/null 2>&1 && break; sleep 2; done
  curl -s -m 2 "$url/api/v1/instance" | grep -q '"host"' || { echo "FAIL: no instance at $url"; exit 1; }
done
echo "both instances answer"

# One identity, an account on each instance by proof of it (architecture 6.1:
# the same identity, two hosts, no directory needed between them).
a="$work/a"
stamp=$(date +%s)
"$cli" --data-dir "$a" instance add "$flagship" >/dev/null
"$cli" --data-dir "$a" identity create --label "two-a" >/dev/null
"$cli" --data-dir "$a" identity profile --display-name "Ada on both" >/dev/null
"$cli" --data-dir "$a" account create-by-identity --handle "ada$stamp" >/dev/null
"$cli" --data-dir "$a" instance add "$community" >/dev/null
"$cli" --data-dir "$a" account create-by-identity --handle "ada$stamp" >/dev/null
me=$("$cli" --data-dir "$a" whoami)
echo "one identity, accounts on both instances ($me on the community)"

# The profile travelled with the proof.
"$cli" --data-dir "$a" identity show | grep -q "profile v1: Ada on both" || { echo "FAIL: the profile is not held"; exit 1; }

# A server on the community, and an invite for it.
server=$("$cli" --data-dir "$a" server-create "Two instances" | cut -d' ' -f1)
channel=$("$cli" --data-dir "$a" channel-create "$server" general | cut -d' ' -f1)
invite=$("$cli" --data-dir "$a" invite-create "$server" --max-uses 1 | cut -d' ' -f1)

# A second identity, on the community alone, joins through the invite and posts.
b="$work/b"
"$cli" --data-dir "$b" instance add "$community" >/dev/null
"$cli" --data-dir "$b" identity create --label "two-b" >/dev/null
"$cli" --data-dir "$b" account create-by-identity --handle "bo$stamp" >/dev/null
"$cli" --data-dir "$b" join "$invite" >/dev/null
"$cli" --data-dir "$b" send "$channel" "Hello across instances $stamp" --mention "$me" >/dev/null
# Two events: the member's arrival in the default channel ("joined",
# architecture 6.8), then the message.
"$cli" --data-dir "$a" channels "$server" | grep -q "general  head=2" || { echo "FAIL: the message did not land on the community"; exit 1; }
echo "a member joined through an invite and posted; the community has the message"

# Two devices of one identity each add a device at a different instance:
# forks at one version, merged by the client (identity spec 5), and every
# device and instance converges on the merged roster.
fh=$(printf %s "$flagship" | sed 's#^[a-z]*://##; s#:#_#'); ch=$(printf %s "$community" | sed 's#^[a-z]*://##; s#:#_#')
a2="$work/a2"
"$cli" --data-dir "$a2" instance add "$community" >/dev/null
"$cli" --data-dir "$a2" instance add "$flagship" >/dev/null
request=$("$cli" --data-dir "$a2" identity link-request --label "two-a2")
code=$("$cli" --data-dir "$a" --instance "$ch" identity approve "$request" | awk '{print $2}')
"$cli" --data-dir "$a2" --instance "$ch" identity claim "$code" | grep -q "roster v2" || { echo "FAIL: the second device did not claim v2"; exit 1; }
"$cli" --data-dir "$a2" --instance "$ch" login --identity >/dev/null
"$cli" --data-dir "$a2" --instance "$fh" login --identity >/dev/null
x="$work/x"; y="$work/y"
"$cli" --data-dir "$x" instance add "$flagship" >/dev/null
"$cli" --data-dir "$y" instance add "$community" >/dev/null
xr=$("$cli" --data-dir "$x" identity link-request --label "two-x")
yr=$("$cli" --data-dir "$y" identity link-request --label "two-y")
# The first device adds x at the flagship, the second adds y at the community: two v3s.
xc=$("$cli" --data-dir "$a" --instance "$fh" identity approve "$xr" | awk '{print $2}')
yc=$("$cli" --data-dir "$a2" --instance "$ch" identity approve "$yr" | awk '{print $2}')
"$cli" --data-dir "$x" identity claim "$xc" | grep -q "roster v3" || { echo "FAIL: x did not claim v3"; exit 1; }
"$cli" --data-dir "$y" identity claim "$yc" | grep -q "roster v3" || { echo "FAIL: y did not claim v3"; exit 1; }
# The first device proves itself at the community: CONFLICT with the fork
# there, merged to v4 and published with the retried proof.
"$cli" --data-dir "$a" --instance "$ch" login --identity >/dev/null
roster_is() { "$cli" --data-dir "$1" identity show | grep -q "roster v$2 " && [ "$("$cli" --data-dir "$1" identity show | grep -c '^  device ')" = "$3" ]; }
roster_is "$a" 4 4 || { echo "FAIL: the first device did not merge to v4 with four devices"; "$cli" --data-dir "$a" identity show; exit 1; }
# The flagship takes the merge with the first device's next proof; the
# other devices adopt it with theirs, at either instance.
"$cli" --data-dir "$a" --instance "$fh" login --identity >/dev/null
"$cli" --data-dir "$a2" --instance "$fh" login --identity >/dev/null
"$cli" --data-dir "$x" login --identity >/dev/null
"$cli" --data-dir "$y" login --identity >/dev/null
for d in "$a2" "$x" "$y"; do
  roster_is "$d" 4 4 || { echo "FAIL: $d did not adopt the merged v4"; "$cli" --data-dir "$d" identity show; exit 1; }
done
echo "two devices forked the roster at different instances; the client merged it and every device and instance holds v4"

# The scenario suite against each instance, as CI runs it against one.
for url in "$flagship" "$community"; do
  d="$work/scenario-$(echo "$url" | tr -c 'a-z0-9' '_')"
  "$cli" --data-dir "$d" instance add "$url" >/dev/null
  "$cli" --data-dir "$d" account create --handle "sc$stamp$(date +%N | cut -c1-4)" --password "$password" >/dev/null
  for name in token-rotation reconnect stale-roster device-revoke roster-rules rotate-handoff; do
    "$cli" --data-dir "$d" scenario "$name" >"$work/$name.log" 2>&1 || { echo "FAIL: scenario $name on $url; see $work/$name.log"; exit 1; }
  done
  echo "scenarios pass on $url"
done
echo "two-instances: ok"
