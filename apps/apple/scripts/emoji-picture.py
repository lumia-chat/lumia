"""A PNG with transparency for the emoji smoke's pictures: a disc, a ring
or a wide pill in one colour with a second colour inside it, on a clear
ground, told apart at a glance and at a server emoji's size; or, `spin`, an
animated GIF of a disc whose wedge of the second colour turns, eight frames
a tenth of a second each, looping. No dependencies.

    python3 scripts/emoji-picture.py <file> <disc|ring|wide|tall|spin> <r,g,b> <r,g,b>
"""
import math
import struct
import sys
import zlib

path, shape = sys.argv[1], sys.argv[2]

if shape == "spin":
    # GIF89a with a four-colour table (0 clear) and LZW kept at three-bit
    # codes by a clear code before every pair, which any decoder reads.
    n, frames = 128, 8
    table = bytes(3) + bytes(int(x) for x in sys.argv[3].split(",")) + bytes(int(x) for x in sys.argv[4].split(",")) + b"\xff\xff\xff"

    def index(x, y, k):
        cx, cy = x + 0.5 - n / 2, y + 0.5 - n / 2
        d = math.hypot(cx, cy) / n
        if d > 0.47:
            return 0
        if d < 0.1:
            return 3
        turn = (math.atan2(cy, cx) / (2 * math.pi) - k / frames) % 1
        return 2 if turn < 0.25 else 1

    def lzw(pixels):
        bits, count, out = 0, 0, bytearray()

        def put(code):
            nonlocal bits, count
            bits |= code << count
            count += 3
            while count >= 8:
                out.append(bits & 0xFF)
                bits >>= 8
                count -= 8

        for i in range(0, len(pixels), 2):
            put(4)
            for p in pixels[i:i + 2]:
                put(p)
        put(5)
        if count:
            out.append(bits & 0xFF)
        return b"".join(bytes([len(out[i:i + 255])]) + bytes(out[i:i + 255]) for i in range(0, len(out), 255)) + b"\0"

    with open(path, "wb") as f:
        f.write(b"GIF89a" + struct.pack("<HHBBB", n, n, 0xF1, 0, 0) + table)
        f.write(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00")
        for k in range(frames):
            f.write(b"\x21\xf9\x04\x09" + struct.pack("<H", 10) + b"\x00\x00")
            f.write(b"\x2c" + struct.pack("<HHHHB", 0, 0, n, n, 0) + b"\x02")
            f.write(lzw([index(x, y, k) for y in range(n) for x in range(n)]))
        f.write(b"\x3b")
    sys.exit(0)
outer = bytes(int(x) for x in sys.argv[3].split(",")) + b"\xff"
inner = bytes(int(x) for x in sys.argv[4].split(",")) + b"\xff"
clear = b"\0\0\0\0"
w, h = {"wide": (192, 96), "tall": (64, 128)}.get(shape, (128, 128))


def pixel(x, y):
    cx, cy = (x + 0.5) / w - 0.5, (y + 0.5) / h - 0.5
    if shape in ("wide", "tall"):
        # A pill: a rectangle whose ends are half circles, with a bar across.
        r = min(w, h) / 2
        px, py = (x + 0.5), (y + 0.5)
        if shape == "wide":
            dx = max(r - px, 0, px - (w - r))
            inside = dx * dx + (py - h / 2) ** 2 <= r * r
            bar = abs(py - h / 2) < h * 0.12 and r * 0.6 < px < w - r * 0.6
        else:
            dy = max(r - py, 0, py - (h - r))
            inside = dy * dy + (px - w / 2) ** 2 <= r * r
            bar = abs(px - w / 2) < w * 0.12 and r * 0.6 < py < h - r * 0.6
        return (inner if bar else outer) if inside else clear
    d = (cx * cx + cy * cy) ** 0.5
    if d > 0.47:
        return clear
    if shape == "ring":
        return outer if d > 0.3 else clear
    return inner if d < 0.22 else outer


rows = b"".join(b"\0" + b"".join(pixel(x, y) for x in range(w)) for y in range(h))


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


with open(path, "wb") as f:
    f.write(b"\x89PNG\r\n\x1a\n")
    f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)))
    f.write(chunk(b"IDAT", zlib.compress(rows)))
    f.write(chunk(b"IEND", b""))
