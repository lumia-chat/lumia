"""Files for the media smoke, with no dependencies: a landscape picture as a
PNG (a sky going pale toward a horizon over green hills, so its shape and
its way up are plain at a glance; sips makes the JPEG of it), and a
one-page PDF.

    python3 scripts/media.py picture <file.png> [<width> <height>]
    python3 scripts/media.py pdf <file.pdf> <words>
"""
import math
import struct
import sys
import zlib


def png(path, width, height):
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        t = y / height
        for x in range(width):
            hill = 0.62 + 0.08 * math.sin(x / width * math.pi * 3) + 0.04 * math.sin(x / width * math.pi * 11)
            if t < hill:
                k = t / hill
                rows += bytes((int(70 + 150 * k), int(130 + 100 * k), int(220 + 30 * k)))
            else:
                k = (t - hill) / (1 - hill)
                rows += bytes((int(60 - 30 * k), int(150 - 60 * k), int(70 - 30 * k)))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(bytes(rows))))
        f.write(chunk(b"IEND", b""))


def pdf(path, words):
    text = words.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
    stream = f"BT /F1 18 Tf 72 720 Td ({text}) Tj ET".encode()
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        b"<< /Length " + str(len(stream)).encode() + b" >>\nstream\n" + stream + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for n, body in enumerate(objects, 1):
        offsets.append(len(out))
        out += f"{n} 0 obj\n".encode() + body + b"\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
    for o in offsets:
        out += f"{o:010d} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    with open(path, "wb") as f:
        f.write(out)


if sys.argv[1] == "picture":
    w, h = (int(sys.argv[3]), int(sys.argv[4])) if len(sys.argv) > 4 else (1200, 800)
    png(sys.argv[2], w, h)
elif sys.argv[1] == "pdf":
    pdf(sys.argv[2], sys.argv[3])
