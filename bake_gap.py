"""Bakes pipef0x's Yukari Gap Demo shader (fog.fsh, MIT) into an animated 128px sprite strip for
Yukari Gaps: the breathing Bezier gap, its black rim, eight drifting eyes and the stepped fog.
Horizon draws sprites, not custom shaders, so the shader's motion is sampled into frames and made
to loop: LOOP seconds, FPS frames a second. Each eye bounces exactly once across and back on each
axis per loop, and the fog moves round a small circle in noise space instead of straight ahead.

usage: python3 yg_bake_gap.py <out.png>   (also writes <out.png>.mcmeta)
"""
import math, struct, sys, zlib, json

RES = 128
LOOP = 48.0          # seconds; the gap's own 8 s breath divides it
FPS = 4
FRAMES = int(LOOP * FPS)
TICKS_PER_FRAME = 20 // FPS

TWO_PI = 2 * math.pi

def fract(x): return x - math.floor(x)

def solve_cubic(c, b, a):
    """x^3 + a x^2 + b x + c = 0, as fog.fsh's solveCubic: its real roots."""
    a3 = a / 3.0
    a2 = a * a
    p = b - a2 / 3.0
    q = (2.0 * a2 * a - 9.0 * a * b + 27.0 * c) / 27.0
    p3, q2 = p / 3.0, q / 2.0
    d = q2 * q2 + p3 * p3 * p3
    cbrt = lambda v: math.copysign(abs(v) ** (1.0 / 3.0), v)
    if d > 0:
        s = math.sqrt(d)
        return [cbrt(-q2 + s) + cbrt(-q2 - s) - a3]
    if d < 0:
        r = math.sqrt(-p3 * p3 * p3)
        phi = math.acos(max(-1.0, min(1.0, -q2 / r)))
        m = 2.0 * math.sqrt(-p3)
        return [m * math.cos((phi + k * TWO_PI) / 3.0) - a3 for k in range(3)]
    if abs(q2) < 1e-6:
        return [-a3] * 3
    u = cbrt(-q2)
    return [2 * u - a3, -u - a3, -u - a3]

def bez(t, p0, p1, p2, p3):
    u = 1 - t
    return tuple(u*u*u*p0[i] + 3*u*u*t*p1[i] + 3*u*t*t*p2[i] + t*t*t*p3[i] for i in (0, 1))

def bez_d(t, p0, p1, p2, p3):
    u = 1 - t
    return tuple(3*u*u*(p1[i]-p0[i]) + 6*u*t*(p2[i]-p1[i]) + 3*t*t*(p3[i]-p2[i]) for i in (0, 1))

def unsigned_dist(uv, p0, p1, p2, p3):
    best_t, best = 0.0, 1e38
    for i in range(13):
        t = i / 12
        x, y = bez(t, p0, p1, p2, p3)
        d = (x - uv[0]) ** 2 + (y - uv[1]) ** 2
        if d < best:
            best, best_t = d, t
    for _ in range(2):
        x, y = bez(best_t, p0, p1, p2, p3)
        dx, dy = bez_d(best_t, p0, p1, p2, p3)
        den = dx * dx + dy * dy
        if den > 1e-7:
            best_t = max(0.0, min(1.0, best_t - ((x-uv[0])*dx + (y-uv[1])*dy) / den))
    x, y = bez(best_t, p0, p1, p2, p3)
    return math.hypot(x - uv[0], y - uv[1])

def sdf(uv, p0, p1, p2, p3):
    cu = -p0[1] + 3*p1[1] - 3*p2[1] + p3[1]
    qu = 3*p0[1] - 6*p1[1] + 3*p2[1]
    li = -3*p0[1] + 3*p1[1]
    co = p0[1] - uv[1]
    n = 0
    for r in solve_cubic(co/cu, li/cu, qu/cu):
        if 0.0 <= r <= 1.0:
            x = -p0[0] + 3*p1[0] - 3*p2[0] + p3[0]
            x = x*r + 3*p0[0] - 6*p1[0] + 3*p2[0]
            x = x*r - 3*p0[0] + 3*p1[0]
            x = x*r + p0[0]
            if x < uv[0]:
                n += 1
    t1 = (p0[0]-p1[0], p0[1]-p1[1]); t2 = (p2[0]-p3[0], p2[1]-p3[1])
    n1 = (t1[1], -t1[0]); n2 = (t2[1], -t2[0])
    dot = lambda a, b, nn: (a[0]-b[0])*nn[0] + (a[1]-b[1])*nn[1]
    if p0[1] < p1[1]:
        if uv[1] <= p0[1] and dot(uv, p0, n1) < 0: n += 1
    else:
        if not (uv[1] <= p0[1]) and not (dot(uv, p0, n1) < 0): n += 1
    if p2[1] < p3[1]:
        if not (uv[1] <= p3[1]) and dot(uv, p3, n2) < 0: n += 1
    else:
        if uv[1] <= p3[1] and not (dot(uv, p3, n2) < 0): n += 1
    return (1.0 if n % 2 == 0 else -1.0) * unsigned_dist(uv, p0, p1, p2, p3)

# ---- fog ----
def random3(c):
    j = 4096.0 * math.sin(c[0]*17.0 + c[1]*59.4 + c[2]*15.0)
    return (fract(j*32.0) - .5, fract(j*4.0) - .5, fract(j*512.0) - .5)

def simplex3d(p):
    s0 = sum(p) / 3.0
    s = [math.floor(v + s0) for v in p]
    s6 = sum(s) / 6.0
    x = [p[i] - s[i] + s6 for i in range(3)]
    e = [1.0 if x[i] - x[(i+1) % 3] >= 0 else 0.0 for i in range(3)]   # step(0, x - x.yzx)
    ezxy = [e[2], e[0], e[1]]
    i1 = [e[i] * (1 - ezxy[i]) for i in range(3)]
    i2 = [1 - ezxy[i] * (1 - e[i]) for i in range(3)]
    x1 = [x[i] - i1[i] + 1/6 for i in range(3)]
    x2 = [x[i] - i2[i] + 2/6 for i in range(3)]
    x3 = [x[i] - 1 + 3/6 for i in range(3)]
    total = 0.0
    for corner, xx in ((s, x), ([s[i]+i1[i] for i in range(3)], x1),
                       ([s[i]+i2[i] for i in range(3)], x2), ([s[i]+1 for i in range(3)], x3)):
        w = max(0.6 - sum(v*v for v in xx), 0.0)
        w = w ** 4
        r = random3(corner)
        total += w * (r[0]*xx[0] + r[1]*xx[1] + r[2]*xx[2])
    return total * 52.0

COLOR_A = (92/255, 0.0, 154/255)
COLOR_B = (1.0, 0.0, 66/255)
# The shader moves through the noise at 1/16 of a unit a second (t/512 times 32); here that speed
# goes round a circle in the y-z plane, one turn a loop, so the last frame meets the first.
FOG_R = LOOP / 16.0 / TWO_PI

def fog(uv, sec):
    a = TWO_PI * sec / LOOP
    p = (uv[0] * 8.0, uv[1] * 8.0 + FOG_R * (math.cos(a) - 1), FOG_R * math.sin(a))
    v = math.floor(simplex3d(p) * 4) / 4 + 0.5
    return tuple(COLOR_A[i] + (COLOR_B[i] - COLOR_A[i]) * v for i in range(3))

# ---- eyes ----
ARC_R, ARC_OFF, IRIS = 0.095, 0.058, 0.034
PUPIL = IRIS * 0.5
HALF_W = math.sqrt(ARC_R*ARC_R - ARC_OFF*ARC_OFF)
HALF_H = ARC_R - ARC_OFF
SPAN = (1 - 2*HALF_W, 1 - 2*HALF_H)

def eye_hash(i):
    f = i + 1.0
    return (fract(math.sin(f*127.1)*43758.5453), fract(math.sin(f*311.7)*43758.5453))

def tri(u): return 1.0 - abs((u % 2.0) - 1.0)

EYES = []
for i in range(8):
    hx, hy = eye_hash(i)
    ang = hx * TWO_PI + 0.37
    # Once across and back on each axis a loop (0.035 to 0.039 UV a second, as the shader's
    # 0.035), going the way the shader launched it, from where its long offset left it.
    EYES.append((math.copysign(1, math.cos(ang)), math.copysign(1, math.sin(ang)), hy * 2.0, fract(hy * 7.31) * 2.0))

def eye_at(i, sec):
    sx, sy, ox, oy = EYES[i]
    ph = (0.5 + sx * 2 * sec / LOOP + ox, 0.5 + sy * 2 * sec / LOOP + oy)
    return (HALF_W + SPAN[0] * tri(ph[0]), HALF_H + SPAN[1] * tri(ph[1]))

def eye_color(uv, pos):
    px, py = uv[0] - pos[0], uv[1] - pos[1]
    if abs(px) > HALF_W or abs(py) > HALF_H:
        return None
    if max(math.hypot(px, py + ARC_OFF), math.hypot(px, py - ARC_OFF)) - ARC_R > 0:
        return None
    r = math.hypot(px, py)
    if r <= PUPIL: return (0.0, 0.0, 0.0)
    if r <= IRIS: return (0.92, 0.06, 0.18)
    return (1.0, 1.0, 1.0)

OUTLINE = 0.0075

def frame(sec):
    bt = abs(fract(sec / 8.0) * 2 - 1)
    bt = bt * bt * (3 - 2 * bt)
    d2 = 0.15 + 0.15 * bt
    p0, p3 = (0.125, 0.125), (0.875, 0.875)
    p1, p2 = (0.875, 0.125), (0.125, 0.875)
    eyes = [eye_at(i, sec) for i in range(8)]
    rows = []
    for y in range(RES):
        row = bytearray()
        for x in range(RES):
            uv = ((x + .5) / RES, (y + .5) / RES)
            a = sdf(uv, p0, p1, (p2[0] + d2, p2[1]), p3)
            b = sdf(uv, p0, p1, (p2[0] - d2, p2[1]), p3)
            inside = a * b < 0
            edge = min(abs(a), abs(b))
            if not inside and edge > OUTLINE:
                row += b'\0\0\0\0'; continue
            if edge <= OUTLINE:
                row += b'\0\0\0\xff'; continue
            c = None
            for pos in eyes:
                c = eye_color(uv, pos)
                if c: break
            if c is None:
                c = fog(uv, sec)
            row += bytes(max(0, min(255, round(v * 255))) for v in c) + b'\xff'
        rows.append(bytes(row))
    return rows

def write_png(path, rows, w):
    raw = b''.join(b'\0' + r for r in rows)
    chunk = lambda t, d: struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    with open(path, 'wb') as f:
        f.write(b'\x89PNG\r\n\x1a\n')
        f.write(chunk(b'IHDR', struct.pack('>IIBBBBB', w, len(rows), 8, 6, 0, 0, 0)))
        f.write(chunk(b'IDAT', zlib.compress(raw, 9)))
        f.write(chunk(b'IEND', b''))

if __name__ == '__main__':
    out = sys.argv[1]
    rows = []
    for k in range(FRAMES):
        rows += frame(k / FPS)
        if k % 16 == 0:
            print('frame', k, flush=True)
    write_png(out, rows, RES)
    with open(out + '.mcmeta', 'w') as f:
        json.dump({'animation': {'frametime': TICKS_PER_FRAME}}, f, indent=2)
        f.write('\n')
    print('wrote', out, FRAMES, 'frames')
