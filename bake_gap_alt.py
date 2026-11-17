"""Bakes the alt gap's picture: the same breathing S-shaped gap and black rim as `bake_gap.py`, with
eyes drifting the same way, but its inside is a picture of your own (the back wall) and each eye is
your own sprite.

usage: python3 bake_gap_alt.py <back.png> <eye.png> <out.png>   (also writes <out.png>.mcmeta)

The back is stretched over the whole 128px picture. It may be animated: a strip of square frames
stacked top to bottom (64 wide and 64 x N tall for N frames). Each frame of the gap shows the next
frame of the back, four a second, and the back starts again when it runs out, so for a seamless
loop make N divide 192 (2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96 or 192).

The eye is drawn at each drifting eye's place, its opaque part scaled to EYE_WIDTH of the picture.
"""
import json
import sys
import zlib

import bake_gap as g

RES = g.RES
# How wide each eye is, as a share of the picture: a little over the shader's eyes.
EYE_WIDTH = 2 * g.HALF_W * 1.15


def read_png(path):
    """An 8-bit RGBA (or RGB) non-interlaced PNG: (width, height, rows of RGBA bytes)."""
    data = open(path, 'rb').read()
    assert data[:8] == b'\x89PNG\r\n\x1a\n', path
    pos, idat, w = 8, b'', 0
    while pos < len(data):
        n = int.from_bytes(data[pos:pos + 4], 'big')
        kind, body = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + n]
        if kind == b'IHDR':
            w, h = int.from_bytes(body[:4], 'big'), int.from_bytes(body[4:8], 'big')
            depth, ctype, interlace = body[8], body[9], body[12]
            assert depth == 8 and ctype in (2, 6) and interlace == 0, f'{path}: needs 8-bit RGB(A), not interlaced'
        elif kind == b'IDAT':
            idat += body
        pos += 12 + n
    bpp = 4 if ctype == 6 else 3
    raw, stride = zlib.decompress(idat), w * bpp
    rows, prev = [], bytearray(stride)
    for y in range(h):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        prev = line
        if bpp == 3:
            line = bytearray(b''.join(bytes(line[i:i + 3]) + b'\xff' for i in range(0, stride, 3)))
        rows.append(bytes(line))
    return w, h, rows


def back_frames(path):
    """The back's frames: each a function from uv to RGBA."""
    w, h, rows = read_png(path)
    n = max(1, h // w)

    def frame(k):
        top = k * w

        def at(uv):
            x = min(w - 1, int(uv[0] * w))
            y = min(w - 1, int(uv[1] * w))
            r = rows[top + y]
            return r[4 * x:4 * x + 4]
        return at
    return [frame(k) for k in range(n)]


def eye_sprite(path):
    """The eye: a function from the offset from its centre (in uv) to RGBA, or None outside it."""
    w, h, rows = read_png(path)
    opaque = [(x, y) for y in range(h) for x in range(w) if rows[y][4 * x + 3] >= 16]
    assert opaque, f'{path} is empty'
    x0, x1 = min(p[0] for p in opaque), max(p[0] for p in opaque) + 1
    y0, y1 = min(p[1] for p in opaque), max(p[1] for p in opaque) + 1
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    per_uv = (x1 - x0) / EYE_WIDTH       # sprite pixels in one uv unit
    half = (EYE_WIDTH / 2, (y1 - y0) / per_uv / 2)

    def at(dx, dy):
        if abs(dx) > half[0] or abs(dy) > half[1]:
            return None
        x, y = int(cx + dx * per_uv), int(cy + dy * per_uv)
        if not (x0 <= x < x1 and y0 <= y < y1):
            return None
        px = rows[y][4 * x:4 * x + 4]
        return px if px[3] >= 16 else None
    return at, half


def over(top, under):
    """`top` (RGBA) over the opaque `under`."""
    a = top[3] / 255
    return bytes(round(top[i] * a + under[i] * (1 - a)) for i in range(3)) + b'\xff'


def masks():
    """The gap's shape at each step of its 8-second breath (the same 32 frames over and over):
    per pixel 0 outside, 1 the black rim, 2 inside."""
    cache = {}
    for k in range(g.FRAMES):
        bt = abs(g.fract(k / g.FPS / 8.0) * 2 - 1)
        bt = bt * bt * (3 - 2 * bt)
        key = round(bt, 6)
        if key in cache:
            continue
        d2 = 0.15 + 0.15 * bt
        p0, p3 = (0.125, 0.125), (0.875, 0.875)
        p1, p2 = (0.875, 0.125), (0.125, 0.875)
        m = bytearray(RES * RES)
        for y in range(RES):
            for x in range(RES):
                uv = ((x + .5) / RES, (y + .5) / RES)
                a = g.sdf(uv, p0, p1, (p2[0] + d2, p2[1]), p3)
                b = g.sdf(uv, p0, p1, (p2[0] - d2, p2[1]), p3)
                edge = min(abs(a), abs(b))
                m[y * RES + x] = 1 if edge <= g.OUTLINE else 2 if a * b < 0 else 0
        cache[key] = bytes(m)
        print('shape', len(cache), flush=True)
    return cache


def bake(back_path, eye_path, out):
    backs = back_frames(back_path)
    eye, half = eye_sprite(eye_path)
    shapes = masks()
    rows = []
    for k in range(g.FRAMES):
        sec = k / g.FPS
        bt = abs(g.fract(sec / 8.0) * 2 - 1)
        mask = shapes[round(bt * bt * (3 - 2 * bt), 6)]
        back = backs[k % len(backs)]
        eyes = [g.eye_at(i, sec) for i in range(8)]
        for y in range(RES):
            row = bytearray()
            for x in range(RES):
                m = mask[y * RES + x]
                if m == 0:
                    row += b'\0\0\0\0'
                    continue
                if m == 1:
                    row += b'\0\0\0\xff'
                    continue
                uv = ((x + .5) / RES, (y + .5) / RES)
                px = bytes(back(uv)[:3]) + b'\xff'
                for pos in eyes:
                    e = eye(uv[0] - pos[0], uv[1] - pos[1])
                    if e:
                        px = over(e, px)
                        break
                row += px
            rows.append(bytes(row))
    g.write_png(out, rows, RES)
    with open(out + '.mcmeta', 'w') as f:
        json.dump({'animation': {'frametime': g.TICKS_PER_FRAME}}, f, indent=2)
        f.write('\n')
    print('wrote', out, g.FRAMES, 'frames;', len(backs), 'back frame(s)')


if __name__ == '__main__':
    bake(sys.argv[1], sys.argv[2], sys.argv[3])
