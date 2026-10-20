#!/usr/bin/env python3
"""Génère les archives RAR de test (méthode "stockée") selon la spécification
publique RAR 4.x / 5.0 de RARLAB. Le contenu est identique aux autres tests :
pages PNG 8x12 "1.png", "2.png", "10.png", "__MACOSX/._1.png" + ComicInfo.xml.

    python3 make_fixtures.py   # écrit rar4.cbr et solid_rar5.cbr ici
"""
import struct, zlib, os

HERE = os.path.dirname(os.path.abspath(__file__))
INFO = b"<ComicInfo><Series>Thanos</Series><Number>1</Number></ComicInfo>"


def png(n):
    w, h = 8, 12
    raw = b"".join(b"\0" + bytes([n, 255 - n, 128, 255]) * w for _ in range(h))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def files():
    out = []
    for name in ["10.png", "2.png", "1.png", "__MACOSX/._1.png"]:
        n = int(name.rsplit("/", 1)[-1].lstrip("._").split(".")[0])
        out.append((name, png(n)))
    out.append(("ComicInfo.xml", INFO))
    return out


def rar4(path, solid=False):
    b = bytearray(b"Rar!\x1a\x07\x00")
    def block(htype, flags, body):
        hdr = struct.pack("<BHH", htype, flags, 7 + len(body)) + body
        return struct.pack("<H", zlib.crc32(hdr) & 0xFFFF) + hdr
    b += block(0x73, 0x0008 if solid else 0, b"\0" * 6)
    for i, (name, data) in enumerate(files()):
        nm = name.replace("/", "\\").encode()
        flags = 0x8000 | (0x0010 if solid and i else 0)
        body = struct.pack("<IIBIIBBHI", len(data), len(data), 2, zlib.crc32(data),
                           0x5A210000, 20, 0x30, len(nm), 0x20) + nm
        b += block(0x74, flags, body) + data
    b += block(0x7B, 0x4000, b"")
    open(path, "wb").write(b)


def vint(v):
    out = bytearray()
    while True:
        byte = v & 0x7F
        v >>= 7
        out.append(byte | (0x80 if v else 0))
        if not v:
            return bytes(out)


def rar5(path, solid=False):
    b = bytearray(b"Rar!\x1a\x07\x01\x00")
    def header(fields):
        body = b"".join(fields)
        hdr = vint(len(body)) + body
        return struct.pack("<I", zlib.crc32(hdr)) + hdr
    b += header([vint(1), vint(0), vint(0x0004 if solid else 0)])
    for i, (name, data) in enumerate(files()):
        nm = name.encode()
        comp = 0x40 if solid and i else 0          # bit 6 = solid, méthode 0 = stockée
        b += header([vint(2), vint(0x0002), vint(len(data)),
                     vint(0x0004), vint(len(data)), vint(0x20), struct.pack("<I", zlib.crc32(data)),
                     vint(comp), vint(0), vint(len(nm)), nm]) + data
    b += header([vint(5), vint(0), vint(0)])
    open(path, "wb").write(b)


if __name__ == "__main__":
    rar4(os.path.join(HERE, "rar4.cbr"))
    rar5(os.path.join(HERE, "solid_rar5.cbr"), solid=True)
    print("ok")
