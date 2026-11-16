# Paints the Raptor and bakes its paint into assets/f22_paint.jpg (plain Python, with Pillow and NumPy):
#
#   python assets/blender/f22_paint.py [<dir of guides>] [<out.jpg>]
#
# The paint is four pictures of the aircraft: from straight above, from
# straight below, from its left side and from its right side. The game
# projects them back onto the skin (plane.wgsl, `hornet_paint`), so they must
# line up with the model: they are painted over the guides that
#
#   blender -b --factory-startup --python assets/blender/f22.py -- <scratch dir> --guides assets/blender/f22_paint
#
# renders (<view>_guide.png, with the silhouette in <view>_mask.png). Unlike
# the Hornet's, which an image model painted, these are painted here, by
# rule: every colour, patch, panel line and marking below is placed in metres
# on the model (Blender frame: +X right, +Y forward, +Z up, origin at the
# centre of gravity), so the four views agree with each other and with the
# airframe. The painted views are written beside the guides (top.png,
# bottom.png, side.png, right.png) and can be touched up in any paint program;
# run with --bake-only to bake them as they stand without repainting.
#
# The bake is the Hornet's (fa18_paint.py): each picture is cut to the model's
# silhouette, the paint at its edge spread outward over the background, and
# the four laid side by side, nose up: top | bottom | left side | right side.
# The right side's picture is mirrored first, so both sides are read the same
# way (across: height; down: station) and lettering reads properly on each.
import os
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
BAKE_ONLY = "--bake-only" in sys.argv
SRC = ARGS[0] if len(ARGS) > 0 else os.path.join(HERE, "f22_paint")
OUT = ARGS[1] if len(ARGS) > 1 else os.path.join(os.path.dirname(HERE), "f22_paint.jpg")

# The rectangles the pictures cover (m), as GUIDE_* in f22.py.
SPAN, LEN, MID_Y, MID_Z, PX = 14.4, 21.6, 0.4, 1.0, 1536
SHORT = round(PX * SPAN / LEN)
PPM = PX / LEN  # pixels to the metre
RIVET_PITCH, RIVET_OFF = 0.085, 0.034  # fasteners along a seam: how far apart, how far from it (m)
SEED = {"top": 1, "bottom": 2, "side": 3, "right": 4}  # each view weathers its own way
SS = 3          # lines and markings are drawn this many times larger, then reduced

# ---------------------------------------------------------------- the scheme (sRGB)
LIGHT = (146, 153, 158)    # the lighter grey all over (about FS 36375, a little warm under its coating)
DARK = (104, 111, 117)     # the darker patches (about FS 36251)
EDGE = (160, 164, 164)     # leading and trailing edge treatments: paler, less blue
RADOME = (122, 127, 131)
UNDER_DARK = 0.55          # how much of the dark the patches underneath take
LINE = (58, 62, 67)        # panel lines
MARK = (82, 88, 94)        # low-visibility markings
LETTER = (60, 65, 71)      # the tail code and serial, which sit on a dark patch
HEAT = (92, 89, 86)        # bare metal round the nozzles
STRAW = (150, 122, 84)
BLUE = (84, 94, 124)
SOOT = (40, 38, 37)
EDGE_BAND = 0.24           # width of the edge treatments (m)
TAIL_CODE, SERIAL = "FB", "AF22 042"  # (made up)

# Where the darker patches sit, right side (mirrored to the left): outlines in
# (x, y) seen from above, in (y, z) from the side. Their edges are softened and
# made a little irregular when painted.
TOP_PATCHES = [
    [(0, 8.4), (0.4, 7.8), (0.72, 6.2), (0.8, 5.0), (0.5, 4.5), (0, 4.4)],                                      # forebody, round the canopy
    [(0, 3.3), (0.6, 3.0), (0.95, 1.9), (1.75, 1.0), (1.55, -0.6), (2.1, -1.6), (1.45, -3.0), (1.85, -4.3), (1.2, -5.1), (0, -5.3)],  # back
    [(2.1, 0.5), (5.5, -2.6), (5.95, -3.25), (5.3, -3.25), (4.4, -2.75), (3.9, -3.7), (2.1, -4.1)],              # mid wing
    [(2.15, -6.4), (3.55, -7.5), (3.65, -7.95), (2.7, -8.25), (2.15, -8.15)],                                    # tailplane
]
BOTTOM_PATCHES = [
    [(2.7, -0.7), (4.7, -2.5), (4.3, -3.3), (2.7, -3.6)],
    [(0, 6.6), (0.5, 6.0), (0.55, 4.4), (0, 4.0)],
]
SIDE_PATCHES = [
    [(8.4, 0.30), (7.6, 0.22), (6.0, 0.24), (5.0, 0.37), (4.5, 0.50), (4.4, 0.9), (8.4, 0.9)],                    # forebody, above the chine
    [(2.4, 0.05), (0.8, 0.12), (-2.6, 0.08), (-4.7, -0.02), (-4.1, -0.55), (-1.2, -0.78), (1.6, -0.62)],          # flank
    [(-3.42, 0.95), (-3.62, 1.7), (-3.95, 1.85), (-3.92, 2.6), (-4.85, 2.6), (-5.0, 1.6), (-5.25, 0.95)],         # fin
]

# The airframe's own lines (f22.py), for the panel lines that follow them.
WING_ROOT, WING_TIP, BODY_HW = 1.2, 6.78, 1.92
FLAP_X = (2.3, 4.3, 6.4)   # flaperon, aileron
NOZZLE_Y = -6.0


def wing_le(x):
    return 3.6 - 0.9004 * x


def wing_te(x):
    return wing_le(x) - (9.85 - 1.2061 * x)


def lef_hinge(x):
    return wing_le(x) - np.interp(x, (2.3, 6.4), (0.60, 0.35))


def te_hinge(x):
    return wing_te(x) + np.interp(x, (2.3, 6.4), (1.25, 0.50))


def fin(u, c):
    """A point of the fin seen from the side: u up it (0..1), c along its chord (0..1)."""
    return (-2.30 - 1.27 * u - (4.30 - 2.54 * u) * c, 0.42 + 2.66 * u)


# The body's sections (f22.py FUSE: y, half width, top, chine height, bottom, top flat, bottom flat, ku, kl).
FUSE = [(9.90, 0.00, -0.10, -0.10, -0.10, 0.00, 0.00, 1.6, 1.5), (9.60, 0.15, 0.02, -0.10, -0.23, 0.02, 0.03, 1.6, 1.5), (9.10, 0.32, 0.16, -0.10, -0.39, 0.04, 0.06, 1.6, 1.5),
        (8.40, 0.50, 0.31, -0.09, -0.55, 0.07, 0.10, 1.6, 1.5), (7.60, 0.68, 0.45, -0.07, -0.69, 0.12, 0.16, 1.6, 1.5), (6.80, 0.82, 0.55, -0.04, -0.79, 0.20, 0.22, 1.6, 1.5),
        (5.90, 0.94, 0.62, 0.00, -0.86, 0.28, 0.30, 1.6, 1.5), (5.00, 1.03, 0.66, 0.05, -0.92, 0.34, 0.40, 1.5, 1.4), (4.20, 1.12, 0.67, 0.09, -0.95, 0.40, 0.55, 1.5, 1.4),
        (3.20, 1.50, 0.66, 0.14, -0.98, 0.55, 0.95, 1.4, 1.3), (2.00, 1.86, 0.64, 0.18, -1.00, 0.75, 1.40, 1.3, 1.2), (0.50, 1.92, 0.62, 0.18, -1.00, 0.88, 1.45, 1.3, 1.2),
        (-1.50, 1.92, 0.58, 0.16, -0.98, 0.92, 1.45, 1.3, 1.2), (-3.50, 1.92, 0.52, 0.12, -0.92, 0.95, 1.42, 1.3, 1.2), (-5.00, 1.94, 0.44, 0.08, -0.80, 1.00, 1.40, 1.3, 1.2),
        (-6.00, 1.96, 0.36, 0.04, -0.60, 1.36, 1.36, 1.3, 1.2)]
FUSE_Y = [r[0] for r in FUSE][::-1]


def fuse(y, k):
    """Column k of the section table at station y (a number or an array)."""
    return np.interp(y, FUSE_Y, [r[k] for r in FUSE][::-1])


def skin_z(x, y):
    """Height of the forebody's upper skin x out from the centreline at station y."""
    hw, top, mid, wt, ku = (fuse(y, k) for k in (1, 2, 3, 5, 7))
    t = min(max((hw - abs(x)) / max(hw - wt, 1e-6), 0.0), 1.0)
    return mid + (top - mid) * (1 - (1 - t) ** ku)


# ---------------------------------------------------------------- views
class View:
    """One of the four pictures: which way it looks and how metres become pixels.
    Its plane's coordinates are (x, y) from above and below, (y, z) from the sides."""

    def __init__(self, name):
        self.name = name
        self.flat = name in ("top", "bottom")
        self.w, self.h = (SHORT, PX) if self.flat else (PX, SHORT)
        # which way the picture's first axis runs: +x to the right from above, -x from below;
        # the nose to the left from the left side, to the right from the right side
        self.sign = {"top": 1, "bottom": -1, "side": -1, "right": 1}[name]
        cols, rows = np.meshgrid(np.arange(self.w) + 0.5, np.arange(self.h) + 0.5)
        if self.flat:
            self.a = (cols / self.w - 0.5) * SPAN * self.sign
            self.b = MID_Y + (0.5 - rows / self.h) * LEN
        else:
            self.a = MID_Y + (cols / self.w - 0.5) * LEN * self.sign
            self.b = MID_Z + (0.5 - rows / self.h) * SPAN
        self.mask = np.asarray(Image.open(os.path.join(SRC, name + "_mask.png")).convert("L")) > 128
        self.guide = np.asarray(Image.open(os.path.join(SRC, name + "_guide.png")).convert("L")).astype(np.float32) / 255
        self.art = Image.new("RGBA", (self.w * SS, self.h * SS), (0, 0, 0, 0))
        self.pen = ImageDraw.Draw(self.art)
        self.riv = np.zeros((self.h, self.w), np.float32)       # fastener heads
        self.tone = np.zeros((self.h, self.w), np.float32)      # each panel's own shade, -1..1
        self.run = Image.new("L", (self.w, self.h), 0)          # streaks
        self.run_pen = ImageDraw.Draw(self.run)
        self.rng = np.random.default_rng(SEED[name])

    def px(self, a, b, ss=SS):
        if self.flat:
            return ((a * self.sign / SPAN + 0.5) * self.w * ss, (0.5 - (b - MID_Y) / LEN) * self.h * ss)
        return (((a - MID_Y) * self.sign / LEN + 0.5) * self.w * ss, (0.5 - (b - MID_Z) / SPAN) * self.h * ss)

    def line(self, pts, col=LINE, alpha=200, width=0.012, rivets=True):
        """A seam through these points of the view's plane, `width` m wide, a row of fasteners along each side of it."""
        self.pen.line([self.px(*p) for p in pts], fill=(*col, alpha), width=max(1, round(width * PPM * SS)), joint="curve")
        if rivets:
            self.rivets(pts)

    def rivets(self, pts, pitch=RIVET_PITCH, off=RIVET_OFF):
        """Fastener heads every `pitch` m along both sides of a line, `off` m from it."""
        p = np.asarray([self.px(*q, ss=1) for q in pts], np.float32)
        for q0, q1 in zip(p, p[1:]):
            d = q1 - q0
            n = float(np.hypot(*d))
            if n < 2.0:
                continue
            t = (np.arange(pitch * PPM * 0.5, n, pitch * PPM) / n)[:, None]
            nrm = np.array([-d[1], d[0]]) / n * off * PPM
            for side, lag in ((1, 0.0), (-1, 0.5)):
                c = q0 + d * np.clip(t + lag * pitch * PPM / n, 0, 1) + nrm * side - 0.5
                x0, y0 = np.floor(c[:, 0]).astype(int), np.floor(c[:, 1]).astype(int)
                fx, fy = c[:, 0] - x0, c[:, 1] - y0
                for dx, dy, wgt in ((0, 0, (1 - fx) * (1 - fy)), (1, 0, fx * (1 - fy)), (0, 1, (1 - fx) * fy), (1, 1, fx * fy)):
                    ok = (x0 + dx >= 0) & (x0 + dx < self.w) & (y0 + dy >= 0) & (y0 + dy < self.h)
                    np.add.at(self.riv, (y0[ok] + dy, x0[ok] + dx), wgt[ok])

    def streak(self, start, length, strength=1.0):
        """A stain running aft from a point of the view's plane."""
        a, b = start
        end = (a, b - length) if self.flat else (a - length, b)
        self.run_pen.line([self.px(*start, ss=1), self.px(*end, ss=1)], fill=int(255 * min(strength, 1.0)), width=2)

    def both(self, pts, **kw):
        """A line on the right side and its mirror image on the left (views from above and below)."""
        self.line(pts, **kw)
        self.line([(-a, b) for a, b in pts], **kw)

    def shape(self, polys, blur=0.10):
        """These outlines filled, their edges softened over `blur` m: 0..1."""
        im = Image.new("L", (self.w, self.h), 0)
        d = ImageDraw.Draw(im)
        for p in polys:
            d.polygon([self.px(*q, ss=1) for q in p], fill=255)
        return np.asarray(im.filter(ImageFilter.GaussianBlur(blur * PPM))).astype(np.float32) / 255

    def inside(self, margin):
        """How far inside the silhouette each pixel is (m), up to `margin`."""
        im = Image.fromarray((self.mask * 255).astype(np.uint8))
        n = int(margin * PPM) + 2
        d = np.zeros(self.mask.shape, np.float32)
        for _ in range(n):
            d += np.asarray(im) > 128
            im = im.filter(ImageFilter.MinFilter(3))
        return d / PPM


def mirrored(polys):
    return polys + [[(-a, b) for a, b in p] for p in polys]


def noise(view, cell, seed, stretch=1.0):
    """Smooth noise, 0..1, with features about `cell` m across (`stretch` times longer fore and aft)."""
    rng = np.random.default_rng(seed)
    fore, across = cell * PPM * stretch, cell * PPM
    cw, ch = (across, fore) if view.flat else (fore, across)
    g = rng.random((int(view.h / ch) + 3, int(view.w / cw) + 3)).astype(np.float32)
    im = Image.fromarray((g * 255).astype(np.uint8)).resize((round(g.shape[1] * cw), round(g.shape[0] * ch)), Image.BICUBIC)
    return np.asarray(im).astype(np.float32)[: view.h, : view.w] / 255


def smooth(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


def mix(c0, c1, t):
    return c0 + (np.asarray(c1, np.float32) - c0) * t[..., None]


def zigzag(p0, p1, teeth, depth):
    """A sawtooth from p0 to p1: the serrated edge of a door or panel."""
    p0, p1 = np.asarray(p0, float), np.asarray(p1, float)
    d = p1 - p0
    n = np.array([-d[1], d[0]]) / max(np.hypot(*d), 1e-9)
    out = []
    for k in range(2 * teeth + 1):
        out.append(tuple(p0 + d * k / (2 * teeth) + n * (depth if k % 2 else 0.0)))
    return out


def door(v, a0, a1, b0, b1, teeth=3, depth=0.09, split=False, **kw):
    """A door or panel between a0..a1 and b0..b1 whose fore and aft edges are serrated."""
    front, back = zigzag((a0, b1), (a1, b1), teeth, depth), zigzag((a1, b0), (a0, b0), teeth, depth)
    v.line(front + back + [front[0]], **kw)
    if split:
        v.line([((a0 + a1) / 2, b0), ((a0 + a1) / 2, b1)], **kw)


# ---------------------------------------------------------------- lettering and insignia
# Stencil capitals and figures as strokes on a 4 x 6 grid.
GLYPHS = {
    "A": [[(0, 0), (0, 4), (1, 6), (3, 6), (4, 4), (4, 0)], [(0, 3), (4, 3)]],
    "B": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)], [(3, 3), (4, 2), (4, 1), (3, 0), (0, 0)]],
    "F": [[(4, 6), (0, 6), (0, 0)], [(0, 3), (3, 3)]],
    "0": [[(1, 0), (0, 1), (0, 5), (1, 6), (3, 6), (4, 5), (4, 1), (3, 0), (1, 0)]],
    "1": [[(1, 5), (2, 6), (2, 0)], [(1, 0), (3, 0)]],
    "2": [[(0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (0, 0), (4, 0)]],
    "3": [[(0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (3, 3), (1, 3)], [(3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "4": [[(3, 0), (3, 6), (0, 2), (4, 2)]],
    "5": [[(4, 6), (0, 6), (0, 3), (3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "6": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0), (3, 0), (4, 1), (4, 2), (3, 3), (0, 3)]],
    "7": [[(0, 6), (4, 6), (1, 0)]],
    "8": [[(1, 3), (0, 4), (0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (3, 3), (1, 3), (0, 2), (0, 1), (1, 0), (3, 0), (4, 1), (4, 2), (3, 3)]],
    "9": [[(0, 1), (1, 0), (3, 0), (4, 1), (4, 5), (3, 6), (1, 6), (0, 5), (0, 4), (1, 3), (4, 3)]],
    " ": [],
}


def text(v, s, centre, height, col=LETTER, alpha=240):
    """Lettering `height` m tall centred on a point of the view's plane, reading left to right in the picture."""
    u = height * PPM * SS / 6
    cx, cy = v.px(*centre)
    x = cx - (len(s) * 5.6 - 1.6) * u / 2
    for ch in s:
        for stroke in GLYPHS[ch]:
            v.pen.line([(x + gx * u, cy + (3 - gy) * u) for gx, gy in stroke], fill=(*col, alpha), width=max(1, round(u * 0.95)), joint="curve")
        x += 5.6 * u


def star_and_bar(v, centre, r, col=MARK, alpha=225):
    """The national insignia in outline, low-visibility: a star in its disc and a bar each side; r m the disc's radius."""
    cx, cy = v.px(*centre)
    R = r * PPM * SS
    w = max(1, round(R * 0.07))
    pts = []
    for k in range(10):
        a = np.pi / 2 + k * np.pi / 5
        rr = R * (0.94 if k % 2 == 0 else 0.36)
        pts.append((cx + rr * np.cos(a), cy - rr * np.sin(a)))
    v.pen.polygon(pts, fill=(*col, alpha))
    v.pen.ellipse([cx - R, cy - R, cx + R, cy + R], outline=(*col, alpha), width=w)
    top, bot = cy - 0.31 * R, cy + 0.19 * R
    for s in (-1, 1):
        x0, x1 = cx + s * R * 1.02, cx + s * R * 2.0
        v.pen.line([(x0, top), (x1, top), (x1, bot), (x0 * 0.985 + cx * 0.015, bot)], fill=(*col, alpha), width=w)
        v.pen.line([(x0, (top + bot) / 2), (x1, (top + bot) / 2)], fill=(*col, alpha // 2), width=w)


# ---------------------------------------------------------------- panel lines and markings, view by view
def lines_flat(v):
    top = v.name == "top"
    k = dict(alpha=190)
    # The wing: its joint with the body, the flap and aileron hinges and cuts, spars and ribs.
    v.both([(BODY_HW, 1.6), (BODY_HW, -5.2)], **k)
    v.both([(FLAP_X[0], lef_hinge(FLAP_X[0])), (FLAP_X[2], lef_hinge(FLAP_X[2]))], **k)
    v.both([(x, te_hinge(x)) for x in FLAP_X], **k)
    for x in FLAP_X:
        v.both([(x, te_hinge(x)), (x, wing_te(x))], **k)
    for x in (FLAP_X[0], FLAP_X[2]):
        v.both([(x, wing_le(x)), (x, lef_hinge(x))], **k)
    for f in (0.34, 0.60):
        v.both([(x, lef_hinge(x) + (te_hinge(x) - lef_hinge(x)) * f) for x in (BODY_HW, 6.4)], alpha=120, width=0.009)
    for x in (3.3, 4.9, 6.4):
        v.both([(x, lef_hinge(x)), (x, te_hinge(x))], alpha=120, width=0.009)
    # The tailplanes: a spar line, the root rib.
    v.both([(1.95, -6.9), (4.1, -7.75)], alpha=120, width=0.009)
    v.both([(2.25, -5.6), (2.25, -8.9)], alpha=120, width=0.009)
    # The radome's joint, frames across the body (the serrated ones are door and panel edges).
    v.line([(-fuse(7.6, 1), 7.6), (fuse(7.6, 1), 7.6)], alpha=230, width=0.016)
    if top:
        for y, half, teeth in ((3.2, 1.45, 6), (1.3, 1.9, 0), (-1.6, 1.9, 8), (-3.6, 1.9, 0), (-5.0, 1.9, 8)):
            v.line(zigzag((-half, y), (half, y), teeth, 0.08) if teeth else [(-half, y), (half, y)], **k)
        v.both([(0.95, 3.2), (1.0, -5.0)], alpha=120, width=0.009)
        # The refuelling receptacle's doors on the spine, the gun's door on the right shoulder.
        door(v, -0.24, 0.24, -0.5, 0.75, teeth=2, depth=0.07, split=True, **k)
        v.line(zigzag((1.25, 1.75), (1.8, 1.25), 2, 0.05) + [(1.8, 0.75), (1.25, 0.95), (1.25, 1.75)], **k)
        # Access panels over the engines.
        for s in (-1, 1):
            door(v, s * 0.35, s * 1.25, -4.6, -2.2, teeth=3, depth=0.07, alpha=150)
        # Walk-on limits each side of the canopy, a rescue arrow, the insignia on the left wing.
        star_and_bar(v, (-4.2, -1.85), 0.44)
        # Ejection seat warning triangles on the forebody's shoulders (painted from the side too).
        for s in (-1, 1):
            v.pen.polygon([v.px(*p) for p in warning_triangle(s)], outline=(*MARK, 235), width=max(1, round(0.02 * PPM * SS)))
    else:
        # Underneath: the main weapon bay's doors between the wheel wells, the engine bay doors, the nose.
        door(v, -0.74, 0.74, -1.25, 2.85, teeth=6, depth=0.09, split=True, alpha=220)
        v.line([(-0.74, 0.8), (0.74, 0.8)], alpha=140)
        for s in (-1, 1):
            door(v, s * 0.30, s * 1.30, -4.7, -1.75, teeth=4, depth=0.08, alpha=180)
            door(v, s * 0.80, s * 1.62, -1.27, 0.87, teeth=3, depth=0.06, alpha=150)
        door(v, -0.22, 0.22, 4.95, 6.80, teeth=2, depth=0.06, split=True, alpha=200)
        for y in (3.2, -5.0):
            v.line(zigzag((-1.4, y), (1.4, y), 6, 0.08), **k)
        star_and_bar(v, (4.2, -1.85), 0.44)


def warning_triangle(s):
    """The ejection seat warning triangle on the forebody's shoulder, side s: its corners (x, y) from above."""
    return [(s * 0.62, 5.62), (s * 0.80, 5.62), (s * 0.71, 5.36)]


def lines_side(v):
    k = dict(alpha=190)
    # The radome's joint, the nose and tail sections' joints, the chine's seam along the forebody.
    v.line([(7.6, -0.72), (7.6, 0.47)], alpha=230, width=0.016)
    for y, z0, z1 in ((3.2, -0.98, 0.14), (-5.0, -0.80, 0.44)):
        v.line(zigzag((y, z0), (y, z1), 4, 0.07), **k)
    for y in (1.3, -1.6, -3.6):
        v.line([(y, -0.95), (y, 0.14)], alpha=120, width=0.009)
    # The side weapon bay's doors on the intake's flank, an access panel behind them.
    door_side(v, -0.3, 2.5, -0.72, -0.14, 3, alpha=220)
    v.line([(-0.3, -0.43), (2.5, -0.43)], alpha=160)
    door_side(v, -3.3, -1.9, -0.62, -0.2, 2, alpha=150)
    # The fin: the rudder's hinge and ends, a spar, the tip cap.
    v.line([fin(0.03, 0.72), fin(0.70, 0.72), fin(0.70, 1.0)], **k)
    v.line([fin(0.03, 0.72), fin(0.03, 1.0)], **k)
    v.line([fin(0.05, 0.38), fin(0.94, 0.38)], alpha=120, width=0.009)
    v.line([fin(0.94, 0.0), fin(0.94, 1.0)], alpha=150)
    v.line([fin(0.05, 0.0), fin(0.05, 0.72)], alpha=120, width=0.009)
    # Markings: tail code and serial on the fin, the insignia on the intake's flank.
    text(v, TAIL_CODE, (-4.02, 1.86), 0.56)
    text(v, SERIAL, (-4.0, 1.30), 0.17)
    star_and_bar(v, (2.95, 0.02 - 0.44), 0.25)
    # The ejection seat warning triangle (the one painted from above, seen from here) and the rescue arrow under it.
    s = 1 if v.name == "right" else -1
    v.pen.polygon([v.px(y, skin_z(x, y)) for x, y in warning_triangle(s)], outline=(*MARK, 235), width=max(1, round(0.02 * PPM * SS)))
    a0, a1 = v.px(5.2, -0.30), v.px(5.75, -0.30)
    v.pen.line([a0, a1], fill=(*MARK, 220), width=max(1, round(0.05 * PPM * SS)))
    head = 0.11 * PPM * SS * (1 if a1[0] > a0[0] else -1)
    v.pen.polygon([(a1[0] + head, a1[1]), (a1[0] - head * 0.2, a1[1] - abs(head) * 0.7), (a1[0] - head * 0.2, a1[1] + abs(head) * 0.7)], fill=(*MARK, 220))


def door_side(v, y0, y1, z0, z1, teeth, depth=0.08, **kw):
    """A door on the flank, its fore and aft edges serrated."""
    front, back = zigzag((y1, z0), (y1, z1), teeth, depth), zigzag((y0, z1), (y0, z0), teeth, depth)
    v.line(front + back + [front[0]], **kw)


# ---------------------------------------------------------------- the skin's panels, fasteners and stains
def stations(lo, hi, pitch, rng, fixed=()):
    """Boundaries from lo to hi about `pitch` apart, a little uneven, with these among them."""
    n = max(1, round((hi - lo) / pitch))
    out = [lo + (k + rng.uniform(-0.22, 0.22)) * (hi - lo) / n for k in range(1, n)]
    out = [x for x in out if all(abs(x - f) > pitch * 0.45 for f in fixed)]
    return sorted([lo, hi, *out, *fixed])


def lattice(v, fwd, inv, S, T, drop=0.26):
    """Skin panels over one part of the airframe. `fwd` takes the view's plane
    to the part's own coordinates (s, t) and says where the part is; `inv`
    takes them back. Seams run along the boundaries S and T; some stretches
    are left out, so panels come in different sizes. Every panel gets its own
    shade (a few stand out: replaced or touched up)."""
    s, t, inside = fwd(v.a, v.b)
    n, m = len(S) - 1, len(T) - 1
    root = list(range(n * m))

    def find(k):
        while root[k] != k:
            root[k] = root[root[k]]
            k = root[k]
        return k

    def seam(p0, p1):
        v.line([inv(p0[0] + (p1[0] - p0[0]) * k / 4, p0[1] + (p1[1] - p0[1]) * k / 4) for k in range(5)], alpha=int(v.rng.uniform(105, 175)), width=0.009)

    for i in range(n):
        for j in range(m):
            if i + 1 < n:
                if v.rng.random() < drop:
                    root[find((i + 1) * m + j)] = find(i * m + j)
                else:
                    seam((S[i + 1], T[j]), (S[i + 1], T[j + 1]))
            if j + 1 < m:
                if v.rng.random() < drop:
                    root[find(i * m + j + 1)] = find(i * m + j)
                else:
                    seam((S[i], T[j + 1]), (S[i + 1], T[j + 1]))
    shade = v.rng.normal(0, 0.5, n * m)
    shade[v.rng.random(n * m) < 0.10] *= 2.4
    cell = np.array([find(k) for k in range(n * m)])
    i = np.clip(np.searchsorted(S, s) - 1, 0, n - 1)
    j = np.clip(np.searchsorted(T, t) - 1, 0, m - 1)
    v.tone = np.where(inside, np.clip(shade, -1.6, 1.6)[cell[i * m + j]], v.tone)


def panels(v):
    """The panel layout: wings, body and tailplanes from above and below; flank and fin from the sides."""
    if v.flat:
        for sg in (-1, 1):
            chord = lambda x: 9.85 - 1.2061 * x

            def fwd(a, b, sg=sg):
                x = sg * a
                t = (wing_le(x) - b) / np.maximum(chord(x), 0.1)
                return x, t, (x >= BODY_HW) & (x <= WING_TIP) & (t >= 0) & (t <= 1)

            lattice(v, fwd, lambda x, t, sg=sg: (sg * x, wing_le(x) - t * chord(x)), stations(BODY_HW, WING_TIP, 0.5, v.rng, FLAP_X), [0, 0.16, 0.27, 0.38, 0.49, 0.60, 0.71, 0.90, 1])
        width = lambda y: np.minimum(fuse(y, 1), BODY_HW)

        def fwd(a, b):
            t = a / np.maximum(width(b), 0.05)
            return -b, t, (b <= 7.6) & (b >= -6.0) & (np.abs(t) <= 1)

        lattice(v, fwd, lambda s, t: (t * width(-s), -s), stations(-7.6, 6.0, 0.55, v.rng, (-3.2, 5.0)), [-1, -0.74, -0.48, -0.22, 0, 0.22, 0.48, 0.74, 1])
        for sg in (-1, 1):
            le, chord = (lambda x: -5.05 - 0.9004 * (x - 1.75)), (lambda x: 3.97 - 1.2061 * (x - 1.75))

            def fwd(a, b, sg=sg):
                x = sg * a
                t = (le(x) - b) / np.maximum(chord(x), 0.1)
                return x, t, (x >= 1.75) & (x <= 4.42) & (t >= 0) & (t <= 1)

            lattice(v, fwd, lambda x, t, sg=sg: (sg * x, le(x) - t * chord(x)), stations(1.75, 4.42, 0.5, v.rng), [0, 0.2, 0.4, 0.6, 0.8, 1])
    else:
        def fwd(a, b):
            top, bot = fuse(a, 2), fuse(a, 4)
            t = (b - bot) / np.maximum(top - bot, 0.05)
            return -a, t, (a <= 7.6) & (a >= -6.0) & (t >= 0) & (t <= 1)

        lattice(v, fwd, lambda s, t: (-s, fuse(-s, 4) + t * (fuse(-s, 2) - fuse(-s, 4))), stations(-7.6, 6.0, 0.55, v.rng, (-3.2, 5.0)), [0, 0.2, 0.4, 0.6, 0.8, 1])

        def fwd(a, b):
            u = (b - 0.42) / 2.66
            c = (-2.30 - 1.27 * u - a) / np.maximum(4.30 - 2.54 * u, 0.1)
            return u, c, (u >= 0) & (u <= 1) & (c >= 0) & (c <= 1)

        lattice(v, fwd, fin, [0, 0.14, 0.28, 0.42, 0.56, 0.70, 0.84, 1], [0, 0.18, 0.38, 0.55, 0.72, 0.86, 1])


def access_panels(v, d, count):
    """Small access panels and doors scattered over the skin, a quarter of them
    with serrated edges, each ringed with fasteners and its own shade."""
    rows, cols = np.nonzero(d > 0.22)
    for k in v.rng.choice(len(rows), count):
        a, b = float(v.a[rows[k], cols[k]]), float(v.b[rows[k], cols[k]])
        w, h = v.rng.uniform(0.14, 0.46) / 2, v.rng.uniform(0.10, 0.30) / 2
        if v.rng.random() < 0.4:
            w, h = h, w
        if v.rng.random() < 0.25:
            (door if v.flat else door_side)(v, a - w, a + w, b - h, b + h, 2, depth=0.03, alpha=175, width=0.009)
        else:
            v.line([(a - w, b - h), (a + w, b - h), (a + w, b + h), (a - w, b + h), (a - w, b - h)], alpha=165, width=0.009)
        (x0, y0), (x1, y1) = v.px(a - w, b - h, ss=1), v.px(a + w, b + h, ss=1)
        v.tone[int(min(y0, y1)):int(max(y0, y1)) + 1, int(min(x0, x1)):int(max(x0, x1)) + 1] = v.rng.normal(0, 0.8)


def stencils(v, d, count):
    """Servicing stencils: too small to read, a few short dark dashes each."""
    rows, cols = np.nonzero(d > 0.12)
    for k in v.rng.choice(len(rows), count):
        x, y = cols[k] * SS, rows[k] * SS
        for row in range(v.rng.integers(1, 4)):
            x1 = x
            for _ in range(v.rng.integers(1, 4)):
                l = v.rng.uniform(0.04, 0.13) * PPM * SS
                v.pen.line([(x1, y + row * 2.6 * SS), (x1 + l, y + row * 2.6 * SS)], fill=(*MARK, 175), width=SS)
                x1 += l + 1.5 * SS


def louvres(v, a, b, n=4, length=0.16, pitch=0.045):
    """A vent: a few dark slots side by side, stained aft of them."""
    for k in range(n):
        v.line([(a - length / 2, b - k * pitch), (a + length / 2, b - k * pitch)] if v.flat else [(a - k * pitch, b - length / 2), (a - k * pitch, b + length / 2)], col=(30, 32, 34), alpha=235, width=0.02, rivets=False)
    v.streak((a, b - n * pitch) if v.flat else (a - n * pitch, b), 0.55, 0.6)


def stains(v, d):
    """Streaks running aft: from the hinges, from joints here and there, from the drains underneath; the vents."""
    rows, cols = np.nonzero(d > 0.10)
    for k in v.rng.choice(len(rows), 110 if v.flat else 60):
        v.streak((float(v.a[rows[k], cols[k]]), float(v.b[rows[k], cols[k]])), v.rng.uniform(0.15, 0.7), v.rng.uniform(0.2, 0.7))
    if v.flat:
        for sg in (-1, 1):
            for x in v.rng.uniform(2.3, 6.4, 24):
                v.streak((sg * x, te_hinge(x)), v.rng.uniform(0.15, 0.6), v.rng.uniform(0.3, 0.9))
            for x in v.rng.uniform(2.3, 6.4, 12):
                v.streak((sg * x, lef_hinge(x)), v.rng.uniform(0.2, 0.5), v.rng.uniform(0.2, 0.6))
            for x, y in ((1.25, 2.3), (0.6, -4.85)) if v.name == "top" else ((1.1, -2.1), (0.5, 3.7), (1.2, -4.9)):
                louvres(v, sg * x, y)
        if v.name == "bottom":
            for _ in range(26):
                a, b = v.rng.uniform(-1.6, 1.6), v.rng.uniform(-5.0, 6.5)
                x, y = v.px(a, b)
                v.pen.ellipse([x - 1.6 * SS, y - 1.6 * SS, x + 1.6 * SS, y + 1.6 * SS], fill=(28, 28, 30, 235))
                v.streak((a, b), v.rng.uniform(0.3, 1.1), v.rng.uniform(0.5, 1.0))
    else:
        for y, z in ((1.0, -0.86), (-4.3, 0.22), (4.55, -0.55)):
            louvres(v, y, z, length=0.12)


# ---------------------------------------------------------------- painting a view
def paint(name):
    v = View(name)
    a, b = v.a, v.b
    seed = SEED[name]
    if name == "top":
        tone = v.shape(mirrored(TOP_PATCHES))
    elif name == "bottom":
        tone = v.shape(mirrored(BOTTOM_PATCHES)) * UNDER_DARK
    else:
        tone = v.shape(SIDE_PATCHES)
    # Soft, slightly irregular edges to the patches; each side's wobble its own.
    tone = smooth(0.30, 0.70, tone + 0.10 * (noise(v, 0.9, seed + 10) - 0.5))
    rgb = mix(np.asarray(LIGHT, np.float32) * np.ones((v.h, v.w, 1), np.float32), DARK, tone)

    # The edge treatments: a paler band round every leading and trailing edge
    # (from the sides: the fins' only), with a fine darker line where it ends.
    d = v.inside(EDGE_BAND * 1.6)
    band = 1 - smooth(EDGE_BAND * 0.85, EDGE_BAND * 1.0, d)
    seam = np.exp(-((d - EDGE_BAND) / 0.010) ** 2) * 0.16
    # (Not round the forebody's chine; from the sides, the fins' only.)
    where = smooth(4.3, 3.9, b) if v.flat else smooth(0.62, 0.80, b) * smooth(-1.9, -2.3, a)
    band, seam = band * where, seam * where
    rgb = mix(rgb, EDGE, band * 0.9)
    rgb *= (1 - seam)[..., None]

    # The radome: its own grey ahead of its joint.
    y = b if v.flat else a
    rgb = mix(rgb, RADOME, (y > 7.6).astype(np.float32))

    # Bare metal between and behind the engines, heat-tinted and sooted toward the nozzles.
    across = np.abs(a) if v.flat else np.zeros_like(a)
    if v.flat:
        hot = smooth(-4.95, -5.6, y + 0.25 * np.cos(across * 4.0)) * smooth(1.40, 1.22, across)
    else:
        hot = smooth(-5.2, -5.8, y) * smooth(0.40, 0.26, b) * smooth(-9.0, -7.4, y)
    streak = noise(v, 0.05, seed + 20, stretch=22.0) * 0.6 + noise(v, 0.16, seed + 22, stretch=8.0) * 0.4
    tint = noise(v, 0.28, seed + 21, stretch=2.5)
    metal = np.asarray(HEAT, np.float32) * (0.90 + 0.20 * streak)[..., None]
    metal = mix(metal, STRAW, smooth(0.5, 0.85, tint) * 0.22)
    metal = mix(metal, BLUE, smooth(0.5, 0.15, tint) * smooth(-5.4, -6.6, y) * 0.35)
    metal = mix(metal, SOOT, smooth(-5.8, -7.4, y) * (0.20 + 0.35 * streak))
    rgb = mix(rgb, metal, np.clip(hot, 0, 1))

    # Kept clean, but never flat: every panel its own shade (the warmer ones
    # read a little glossier in the game), broad soft mottling, a finer one
    # and a grain over it; a few slightly darker touched-up strips.
    panels(v)
    access_panels(v, d, 70 if v.flat else 30)
    stencils(v, d, 110 if v.flat else 50)
    stains(v, d)
    cool = 1 - np.clip(hot, 0, 1)
    rgb *= (1 + 0.055 * v.tone * cool)[..., None] * (1 + np.asarray((0.010, 0.0, -0.012), np.float32) * (v.tone * cool)[..., None])
    rgb *= (0.950 + 0.10 * noise(v, 1.3, seed + 30))[..., None]
    rgb *= (0.968 + 0.064 * noise(v, 0.35, seed + 32))[..., None]
    rgb *= (0.978 + 0.044 * noise(v, 0.07, seed + 33))[..., None]
    rgb *= (0.985 + 0.03 * noise(v, 0.05, seed + 31, stretch=16.0))[..., None]
    rng = np.random.default_rng(seed + 40)
    rgb *= (1 + rng.normal(0, 0.016, rgb.shape[:2]).astype(np.float32))[..., None]
    touch = Image.new("L", (v.w, v.h), 0)
    td = ImageDraw.Draw(touch)
    for _ in range(60):
        cx, cy = rng.random() * v.w, rng.random() * v.h
        l, t = (0.25 + 0.6 * rng.random()) * PPM, (0.05 + 0.08 * rng.random()) * PPM
        w2, h2 = ((t, l) if rng.random() < 0.5 else (l, t))
        td.rectangle([cx - w2 / 2, cy - h2 / 2, cx + w2 / 2, cy + h2 / 2], fill=int(120 + 135 * rng.random()))
    rgb *= (1 - 0.07 * np.asarray(touch.filter(ImageFilter.GaussianBlur(1.6))).astype(np.float32) / 255 * cool)[..., None]
    # Streaks: darker and a little browner.
    run = np.asarray(v.run.filter(ImageFilter.GaussianBlur(1.3))).astype(np.float32) / 255
    rgb *= 1 - run[..., None] * np.asarray((0.13, 0.145, 0.17), np.float32)

    # The model's own creases and joints from the guide: only its fine detail
    # (the guide's lighting is left out), a little darker.
    # and the soft shading of its hollows and creases.
    g = Image.fromarray((np.where(v.mask, v.guide, v.guide[v.mask].mean()) * 255).astype(np.uint8))
    blur = lambda r: np.maximum(np.asarray(g.filter(ImageFilter.GaussianBlur(r))).astype(np.float32) / 255, 0.05)
    rgb *= np.clip(v.guide / blur(3), 0.86, 1.02)[..., None]
    rgb *= np.clip(blur(5) / blur(36), 0.90, 1.05)[..., None]

    # Panel lines and markings, drawn large and reduced.
    (lines_flat if v.flat else lines_side)(v)
    art = np.asarray(v.art.resize((v.w, v.h), Image.LANCZOS)).astype(np.float32)
    al = art[..., 3:] / 255
    rgb = rgb * (1 - al) + art[..., :3] * al
    # (Grime gathered along the lines: a faint soft shadow of them.)
    soft = np.asarray(v.art.getchannel("A").resize((v.w, v.h), Image.LANCZOS).filter(ImageFilter.GaussianBlur(2.2))).astype(np.float32) / 255
    rgb *= (1 - 0.16 * soft)[..., None]
    # Fastener heads: small dark dots.
    rgb *= (1 - 0.30 * np.clip(v.riv * 1.5, 0, 1))[..., None]

    out = np.where(v.mask[..., None], np.clip(rgb, 0, 255), 255).astype(np.uint8)
    Image.fromarray(out).save(os.path.join(SRC, name + ".png"))
    print(f"painted {name}: {v.w}x{v.h}")


# ---------------------------------------------------------------- the bake (as fa18_paint.py)
def spread(rgb, ok, rounds=128):
    """Grow the painted pixels outward, a pixel a round, over the ones that are not."""
    rgb = rgb.astype(np.float32) * ok[..., None]
    ok = ok.astype(np.float32)
    for _ in range(rounds):
        s = np.zeros_like(rgb)
        n = np.zeros_like(ok)
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                if dy or dx:
                    s += np.roll(rgb, (dy, dx), (0, 1))
                    n += np.roll(ok, (dy, dx), (0, 1))
        grow = (ok == 0) & (n > 0)
        if not grow.any():
            break
        rgb[grow] = s[grow] / n[grow][:, None]
        ok[grow] = 1
    rgb[ok == 0] = rgb[ok > 0].mean(0)
    return rgb


def layer(view):
    mask = Image.open(os.path.join(SRC, view + "_mask.png")).convert("L")
    pic = Image.open(os.path.join(SRC, view + ".png")).convert("RGB")
    if pic.size != mask.size:
        pic = pic.resize(mask.size, Image.LANCZOS)
    rgb = np.asarray(pic)
    # Painted: inside the silhouette (less a few pixels, where the picture's
    # own edge may fall short of the model's) and not the white of the background.
    inside = np.asarray(mask.filter(ImageFilter.MinFilter(7))) > 128
    ok = inside & (rgb.min(axis=2) < 236)
    out = Image.fromarray(np.clip(spread(rgb, ok), 0, 255).astype(np.uint8))
    print(f"{view}: {pic.size[0]}x{pic.size[1]}, {ok.mean() * 100:.0f}% painted")
    return out


VIEWS = ("top", "bottom", "side", "right")
if not BAKE_ONLY:
    for name in VIEWS:
        paint(name)
top, bottom, side, right = (layer(n) for n in VIEWS)
# Nose up, like the other two: the left side turned, the right side mirrored and turned the same way.
side = side.transpose(Image.Transpose.ROTATE_270)
right = right.transpose(Image.Transpose.FLIP_LEFT_RIGHT).transpose(Image.Transpose.ROTATE_270)
assert top.size == bottom.size == side.size == right.size, (top.size, bottom.size, side.size, right.size)
w, h = top.size
atlas = Image.new("RGB", (w * 4, h))
for k, im in enumerate((top, bottom, side, right)):
    atlas.paste(im, (w * k, 0))
atlas.save(OUT, quality=92, subsampling=0)
print("wrote", OUT, atlas.size, os.path.getsize(OUT), "bytes")
