#!/usr/bin/env python3
"""Generate the application icon source (1024x1024 PNG) without external deps.

A dark disc of sky, a warm solar disc, and a magnetospheric arc: the app's own
mark, drawn here so the repository carries no third-party icon asset.
"""
import math, struct, zlib, sys, pathlib

N = 1024
CX = CY = N / 2


def srgb(c):
    return max(0, min(255, int(round(c))))


def blend(dst, src, a):
    return tuple(srgb(d * (1 - a) + s * a) for d, s in zip(dst, src))


def build():
    px = [[(11, 14, 22, 0) for _ in range(N)] for _ in range(N)]
    for y in range(N):
        for x in range(N):
            dx, dy = x + 0.5 - CX, y + 0.5 - CY
            r = math.hypot(dx, dy)
            # Rounded-square app plate.
            k = 8.0
            plate = (abs(dx) / (N * 0.47)) ** k + (abs(dy) / (N * 0.47)) ** k
            if plate > 1.0:
                continue
            t = min(1.0, r / (N * 0.66))
            col = (
                11 + 18 * (1 - t),
                16 + 26 * (1 - t),
                30 + 40 * (1 - t),
            )
            a = 1.0
            # Corona glow.
            g = math.exp(-((r - N * 0.20) ** 2) / (2 * (N * 0.10) ** 2))
            col = blend(col, (255, 176, 80), 0.42 * g)
            # Solar disc.
            disc = N * 0.185
            if r < disc + 2:
                edge = min(1.0, max(0.0, (disc - r) / 2.0))
                shade = 1.0 - 0.25 * (r / disc) ** 2
                col = blend(col, (255, 214, 138), edge * shade)
            # Magnetospheric arc, offset to the right.
            ax = dx - N * 0.02
            arc = abs(math.hypot(ax / 0.72, dy) - N * 0.345)
            if arc < N * 0.016 and ax > -N * 0.05:
                col = blend(col, (120, 196, 255), 1.0 - arc / (N * 0.016))
            px[y][x] = (srgb(col[0]), srgb(col[1]), srgb(col[2]), srgb(255 * a))
    return px


def write_png(path, px):
    raw = b"".join(
        b"\x00" + b"".join(struct.pack("BBBB", *p) for p in row) for row in px
    )
    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", N, N, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    pathlib.Path(path).write_bytes(png)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "src-tauri/icons/source.png"
    write_png(out, build())
    print(f"wrote {out}")
