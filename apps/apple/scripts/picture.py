"""A square PNG in two colours split on the diagonal, for the smokes' pictures:
told from initials at a glance. No dependencies.

    python3 scripts/picture.py <file> <r,g,b> <r,g,b>
"""
import struct
import sys
import zlib

path = sys.argv[1]
upper = bytes(int(x) for x in sys.argv[2].split(","))
lower = bytes(int(x) for x in sys.argv[3].split(","))
n = 96
rows = b"".join(b"\0" + b"".join(upper if x > y else lower for x in range(n)) for y in range(n))


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


with open(path, "wb") as f:
    f.write(b"\x89PNG\r\n\x1a\n")
    f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", n, n, 8, 2, 0, 0, 0)))
    f.write(chunk(b"IDAT", zlib.compress(rows)))
    f.write(chunk(b"IEND", b""))
