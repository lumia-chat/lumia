#!/bin/sh
# The picture viewer's zoom on the phone (specs/media.md 5), in a simulator
# with no instance: a debug build draws the viewer's picture itself
# (LUMIA_DEV_SCREEN viewer, 2,560 by 1,920 pixels in squares of 320, and
# viewer-small, 180 by 120 in squares of 30; DevScenes.swift), every square
# a grey of its own, so a screenshot says which part of the picture is
# where and how large it is drawn. testTheViewerZoomsByTheCoresRules taps,
# pinches and drags, and this script measures the screenshot of each step
# against sizes worked out here, apart from the app and the core:
#   fitted    the large picture across the screen, in its middle
#   own       two taps at a point: a pixel a point, the square that was
#             under the finger still under it
#   back      two taps again: fitted
#   small     the small picture at its own size in the middle, not grown
#   twice     two taps off its middle: twice its size, still in the middle
#   furthest  pinched out: four times its own size and no further
#   dragged   dragged across and down: stopped at its leading edge, and
#             no lower, since its height fits the screen
# and on a screen that turns, which an iPad's does and a phone's does not:
#   turned        the large picture at its own size, then the screen
#                 turned: as many times its fitted size as before
#   refitted      two taps then: fitted into the turned screen
#   small-turned  the small picture at four times its own size, turned:
#                 the same size, in the turned screen's middle
# Screenshots land in .build/smoke-viewer/.
#
#   scripts/smoke-viewer.sh     # expects the app and its tests built for the simulator (xcodebuild build-for-testing, see AGENTS.md)
#   LUMIA_SIMULATOR="iPad (10th generation)" scripts/smoke-viewer.sh
set -eu
here=$(cd "$(dirname "$0")" && pwd)
simulator=${LUMIA_SIMULATOR:-iPhone 15 Pro}
project="$here/../Lumia-iOS"
app="$project/.build/Build/Products/Debug-iphonesimulator/Lumia.app"
work=${LUMIA_SMOKE_WORK:-$here/../.build/smoke-viewer}
rm -rf "$work" && mkdir -p "$work/steps"
bundle=chat.lumia

[ -d "$app" ] || { echo "no app at $app"; exit 1; }

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

uitest=""
log="$work/uitest.log"
# shellcheck disable=SC2329
finish() { [ -z "$uitest" ] || kill "$uitest" 2>/dev/null || true; }
trap finish EXIT
TEST_RUNNER_LUMIA_UITEST_STEPS="$work/steps" \
  xcodebuild test-without-building -project "$project/Lumia.xcodeproj" -scheme Lumia -destination "id=$udid" -derivedDataPath "$project/.build" \
  -only-testing:LumiaUITests/SmokeUITests/testTheViewerZoomsByTheCoresRules > "$log" 2>&1 &
uitest=$!

# Waits for the test to announce a step; fails with the test's log when it ends first.
reached() { # <step> <seconds>
  for _ in $(seq 1 "$2"); do
    [ -f "$work/steps/$1" ] && return 0
    kill -0 "$uitest" 2>/dev/null || break
    sleep 1
  done
  [ -f "$work/steps/$1" ] && return 0
  xcrun simctl io "$udid" screenshot "$work/failed-$1.png" >/dev/null 2>&1 || true
  echo "FAIL: the UI test did not reach \"$1\"; see $log"
  grep -E "error|failed|XCTAssert" "$log" | head -10
  exit 1
}

# Screenshots a step, lets the test go on, and measures the picture in it
# (as a bitmap, whose pixels are there to read: rows from the bottom, blue
# first). The screen's size and the point tapped, in points, are what the
# test said at its first step; a turned step says the turned screen's size.
measure() { # <step> [<width> <height> of a turned screen]
  xcrun simctl io "$udid" screenshot "$work/$1.png" >/dev/null 2>&1
  echo > "$work/steps/$1.seen"
  sips -s format bmp "$work/$1.png" --out "$work/$1.bmp" >/dev/null 2>&1
  # shellcheck disable=SC2086
  python3 - "$1" "$work/$1.bmp" ${2:-$said} <<'PY' || { echo "FAIL: $1 is not drawn as the rules say; see $work/$1.png"; exit 1; }
import struct, sys

step, path = sys.argv[1], sys.argv[2]
W, H = float(sys.argv[3]), float(sys.argv[4])
tx, ty = (float(v) for v in sys.argv[5:7]) if len(sys.argv) > 6 else (0.0, 0.0)
data = open(path, "rb").read()
start, = struct.unpack("<I", data[10:14])
width, height, _, bits = struct.unpack("<iiHH", data[18:30])
size = bits // 8
stride = (width * size + 3) // 4 * 4
# A screenshot is the display as the device holds it, so a turned screen's
# is on its side. Which side is not told, so what is measured of a turned
# screen is the same either way round: how long, and how far from the middle.
sideways = (W > H) != (width > abs(height))
px = (abs(height) if sideways else width) / W  # device pixels to a point
across, down = int(round(W * px)), int(round(H * px))
# What the system draws over the picture, the clock at one end and the
# home indicator at the other, is kept out of what is measured down the screen.
top, bottom = 64.0, H - 64.0

def grey(x, y):  # in the screen's device pixels, from its top leading corner
    if sideways:
        x, y = y, across - 1 - x
    row = y if height < 0 else abs(height) - 1 - y
    at = start + row * stride + x * size
    return (data[at] + data[at + 1] + data[at + 2]) / 3

def at(x, y):  # in points
    return grey(min(across - 1, max(0, int(x * px))), min(down - 1, max(0, int(y * px))))

def line(along, where):
    """The pixels to read along one way, at `where` points the other way."""
    if along == "x":
        return 0, across, lambda v: grey(v, int(where * px))
    return int(top * px), int(bottom * px), lambda v: grey(int(where * px), v)

def extent(along, where):
    """Where the picture starts and ends, in points."""
    low, high, read = line(along, where)
    lit = [v for v in range(low, high) if read(v) >= 12]
    return (lit[0] / px, (lit[-1] + 1) / px) if lit else (0.0, 0.0)

def square(x, y):
    """Which of the picture's squares is at a point, counted across and
    then down: the nth is the grey 20 + 5 * (n * 29 % 48)."""
    g = at(x, y)
    k = round((g - 20) / 5)
    if not 0 <= k < 48 or abs(g - (20 + 5 * k)) > 2:
        return -1
    return k * 5 % 48

def edges(x, y, along):
    """The edges of the square at a point, in points, along one way: each
    the middle of the blur between this grey and the next, or the end of
    what is read."""
    low, high, read = line(along, y if along == "x" else x)
    here = at(x, y)
    found = []
    for way in (-1, 1):
        v = int((x if along == "x" else y) * px)
        while low <= v + way < high and abs(read(v + way) - here) <= 2:
            v += way
        last = v
        if not low <= v + way < high:
            found.append((v if way < 0 else v + 1) / px)
            continue
        # Through the blur, to the first pixel of the next grey.
        v += way
        while low <= v + way < high and abs(read(v + way) - read(v)) > 0.5:
            v += way
        found.append((last + v + 1) / 2 / px)
    return tuple(found)

def whole(where):
    """How long each square is that is whole across the screen at a height."""
    lengths, x = [], 1.0
    while x < W - 1:
        left, right = edges(x, where, "x")
        if left > 0.5 and right < W - 0.5:
            lengths.append(right - left)
        x = right + 3
    return lengths

failed = []
def same(what, got, want, within=1.0):
    ok = len(got) == len(want) and all(abs(g - w) <= within for g, w in zip(got, want))
    print("  %s %s: %s (%s)" % ("ok  " if ok else "FAIL", what, ", ".join("%.1f" % g for g in got), ", ".join("%.1f" % w for w in want)))
    if not ok:
        failed.append(what)

def middle(w, h):
    """Where a picture of a size in points starts and ends across and down
    the screen when it is in its middle, as far as can be measured."""
    return (max(0.0, (W - w) / 2), min(W, (W + w) / 2)), (max(top, (H - h) / 2), min(bottom, (H + h) / 2))

def fitted(pw, ph):
    """A picture fitted into the screen, never grown: by how much, and where."""
    k = min(W / pw, H / ph, 1.0)
    return (k,) + middle(pw * k, ph * k)

if step in ("fitted", "own", "back"):
    pw, ph, side = 2560.0, 1920.0, 320.0
    k, wide, tall = fitted(pw, ph)
    # The picture's pixel under the point tapped, and its square.
    sx, sy = (tx - (W - pw * k) / 2) / k, (ty - (H - ph * k) / 2) / k
    column, row = int(sx // side), int(sy // side)
    n = row * int(pw // side) + column
    if step == "own":
        # A pixel a point about the point tapped: its square's edges are
        # as far from it in points as they are from the pixel in pixels.
        same("the square under the finger", (square(tx, ty),), (n,), 0)
        same("that square across the screen", edges(tx, ty, "x"), (max(0.0, tx - (sx - column * side)), min(W, tx + ((column + 1) * side - sx))))
        same("that square down the screen", edges(tx, ty, "y"), (max(top, ty - (sy - row * side)), min(bottom, ty + ((row + 1) * side - sy))))
    else:
        same("the picture across the screen", extent("x", H / 2), wide)
        same("the picture down the screen", extent("y", W / 2), tall)
        same("the square under the finger", (square(tx, ty),), (n,), 0)
elif step == "turned":
    # At its own size before the screen turned, which was so many times
    # its fitted size there; as many times its fitted size here.
    before = min(H / 2560.0, W / 1920.0, 1.0)
    k, _, _ = fitted(2560.0, 1920.0)
    lengths = whole(H / 2)
    same("each whole square across the screen", lengths, [320.0 * k / before] * max(1, len(lengths)), 1.5)
elif step == "refitted":
    _, wide, tall = fitted(2560.0, 1920.0)
    same("the picture across the screen", extent("x", H / 2), wide)
    same("the picture down the screen", extent("y", W / 2), tall)
else:
    pw, ph, side = 180.0, 120.0, 30.0
    scale = {"small": 1, "twice": 2}.get(step, 4)
    wide, tall = middle(pw * scale, ph * scale)
    same("the picture across the screen", extent("x", H / 2 + 5), wide)
    same("the picture down the screen", extent("y", W / 2 + 5), tall)
    if scale == 4:
        # A square is 120 points at four times its own size; the one under
        # the screen's middle starts there down the screen, where the
        # picture fits and so stays in the middle.
        same("a square down the screen", edges(W / 2 + 5, H / 2 + 60, "y"), (H / 2, H / 2 + 120))
    if step == "dragged" and pw * scale > W:
        # The third row's first square, against the screen's leading edge.
        same("the square at the leading edge", (square(1, H / 2 + 60),), (2 * int(pw // side),), 0)
        same("that square across the screen", edges(1, H / 2 + 60, "x"), (0.0, 120.0))
sys.exit(1 if failed else 0)
PY
  rm -f "$work/$1.bmp"
}

reached fitted 300
said=$(cat "$work/steps/fitted")
turns=false
case "$said" in *turns) turns=true ;; esac
said=${said% *}
echo "the screen is ${said%% *} points across; two taps go at point ${said#* * }"
measure fitted
echo "the large picture opens fitted across the screen, in its middle"
reached own 60
measure own
echo "two taps zoomed it to its own size, a pixel a point, about the point tapped"
reached back 60
measure back
echo "two taps again put it back to fitted"
if $turns; then
  reached turned 60
  measure turned "$(cat "$work/steps/turned")"
  echo "turned at its own size, it is as many times its fitted size on the turned screen"
  reached refitted 60
  measure refitted "$(cat "$work/steps/refitted")"
  echo "two taps then fitted it into the turned screen"
fi
reached small 120
measure small
echo "the small picture opens at its own size in the middle, not grown"
reached twice 60
measure twice
echo "two taps zoomed it to twice its size, still in the middle"
reached furthest 60
measure furthest
echo "a pinch stopped at four times its own size"
reached dragged 60
measure dragged
echo "a drag moved it no further than its leading edge, and no way down"
if $turns; then
  reached small-turned 60
  measure small-turned "$(cat "$work/steps/small-turned")"
  echo "turned at four times its own size, it is the same size in the turned screen's middle"
else
  echo "note: this screen does not turn, so a zoom kept across a turn was not looked at (LUMIA_SIMULATOR names an iPad for that)"
fi
wait "$uitest" || { echo "FAIL: the UI test did not pass; see $log"; grep -E "error|failed|XCTAssert" "$log" | head -10; exit 1; }
uitest=""
echo "smoke-viewer: ok (fitted and never grown, two taps to its own size about the point and back, twice for a small one, a pinch to four times its own size, a drag to the edge; screenshots under $work)"
