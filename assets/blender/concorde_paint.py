# Paints Concorde and bakes its paint into assets/concorde_paint.jpg (plain Python, with Pillow and NumPy):
#
#   python assets/blender/concorde_paint.py [--bake-only] [<dir of pictures>] [<out.jpg>]
#
# The paint is four pictures of the aircraft: from straight above, from
# straight below, from its left side and from its right side
# (assets/blender/concorde_paint/top.png, bottom.png, side.png, right.png). The
# game projects them back onto the skin (plane.wgsl, `hornet_paint`), so they
# must line up with the model: they are painted over the guides that
#
#   blender -b --factory-startup --python assets/blender/concorde.py -- <scratch dir> --guides assets/blender/concorde_paint
#
# renders (<view>_guide.png, the silhouette in <view>_mask.png, and
# <view>_geo.npz: how deep into the picture the model is at each pixel, its
# normal there, and which piece and material it is). As the Phantom's
# (f4_paint.py), the pictures are painted here, by rule, from that geometry: a
# pixel's colour comes from where on the aircraft it is, so the livery and the
# panel lines meet across the views.
#
# Concorde was white all over, to stay cool at Mach 2. The livery here is made
# up (no airline's that flew it): CALLISTO, a deep blue cheatline under the
# cabin windows with a gold pinstripe, sweeping up into a blue fin that
# carries a gold ring and a white dart; QX-CLO is no country's registration.
# What is lettered on the fuselage's sides and the fin (windows, doors, names,
# the emblem) is drawn once for each side on a sheet of its own, laid out by
# station and height, and every view looks its pixels up there: so the
# lettering reads the right way on both sides, and where the fuselage curves
# over into the view from above it is the same paint.
#
# A picture's pixel is 43 mm of aircraft (66 m in 1536 pixels: the Phantom's is
# 13 mm), so everything is drawn for that: cabin windows four pixels by three,
# seams and fastener rows wider apart than the real ones and faint, as they
# are on a well-kept airliner seen from across the apron. The game takes the
# darker lines to lie below the skin round them (plane.wgsl), so they are
# relief there, not only ink.
#
# The pictures can be touched up or replaced in any paint program:
# --bake-only then bakes them as they stand, without painting them again.
# (The aircraft being white, the pictures' background is magenta: keep it so,
# the bake leaves out what is that colour or outside the masks.)
#
# The bake is the Hornet's (fa18_paint.py): each picture is cut to the model's
# silhouette, the paint at its edge spread outward over the background, and
# the four laid side by side, nose up: top | bottom | left | right. The side
# views are turned so that both are laid out alike (height across, the nose at
# the top), which is how the game looks them up.
import os
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
BAKE_ONLY = "--bake-only" in sys.argv
SRC = ARGS[0] if ARGS else os.path.join(HERE, "concorde_paint")
OUT = ARGS[1] if len(ARGS) > 1 else os.path.join(os.path.dirname(HERE), "concorde_paint.jpg")

# The rectangles the pictures cover (GUIDE_* in concorde.py): metres across and
# fore and aft, the station and height at their middle, pixels per metre.
SPAN, LEN, MID_Y, MID_Z = 44.0, 66.0, 2.17, 1.0
PX = 1536 / LEN

# ---------------------------------------------------------------- the livery (sRGB; metres, Blender frame: x right, y forward, z up)
WHITE = (247, 247, 244)
BLUE = (22, 38, 92)           # the cheatline, the fin, the lettering
GOLD = (196, 158, 62)         # the pinstripe, the fin's ring
METAL = (158, 158, 156)       # leading edges, the nacelles' aft ends
STEEL = (104, 100, 96)        # round the jet pipes
STRAW, BLUED, SOOT = (150, 126, 88), (92, 94, 116), (46, 44, 42)
GLASS, FRAME = (34, 38, 42), (30, 31, 33)
WINDOW = (40, 46, 56)
DOOR_LINE = (112, 116, 124)
EXIT_RED = (176, 40, 36)
GRIME = (92, 86, 76)

AIRLINE = "CALLISTO"                     # (made up)
REGISTRATION = "QX-CLO"                  # (Q is no country's prefix)
WORDMARK = (12.6, 1.00, 0.62)            # station of its middle, height of its middle, letter height
TYPE_NAME = (17.55, -0.42, 0.27)         # "CONCORDE", behind the forward door
FUSELAGE_REG = (-12.4, -0.30, 0.44)      # the registration on the rear fuselage
WING_REG = (9.0, -15.2, 0.9)             # |x|, station, letter height: over the right wing, under the left
CHEAT_Z, CHEAT_HALF = 0.11, 0.09         # the cheatline: height of its middle along the cabin, half its width
CHEAT_RISE = (-9.5, -17.6, 1.55, 0.50)   # it sweeps up to the fin between these stations: by this much, growing this much wider
PIN_GAP, PIN_WIDTH = 0.06, 0.07          # the gold pinstripe under it
FIN_BLUE = 0.32                          # the fin is blue behind this fraction of its chord
FIN_EMBLEM = (0.45, 0.63, 1.05)          # where up the fin, how far back along its chord, the ring's radius
WINDOW_Z, WINDOW_SIZE, WINDOW_PITCH = 0.45, (0.17, 0.13), 0.50
CABINS = ((18.30, 5.65), (3.65, -8.90))  # the window rows: station from, to
DOORS = ((19.60, 18.85, -0.55, 1.05), (5.05, 4.30, -0.55, 1.05), (-9.60, -10.25, -0.40, 0.95))  # station from, to; height from, to

# ---------------------------------------------------------------- the structure under the paint
# (as concorde.py) The fuselage's centre line by station; the wing's leading edge by span station.
MID_LINE = ((-28.66, 1.05), (-27.0, 1.00), (-23.0, 0.76), (-19.0, 0.44), (-15.0, 0.14), (-11.0, 0.0), (18.0, 0.0), (20.0, -0.07), (22.0, -0.25), (25.0, -0.52), (28.0, -0.70), (33.0, -0.80))
WING_LE = ((0.9, 19.8), (1.44, 15.0), (2.0, 10.0), (3.0, 3.5), (4.0, -0.5), (5.5, -4.3), (7.0, -7.2), (9.0, -10.5), (11.0, -13.6), (12.2, -15.6), (12.8, -16.8))
FRAMES = tuple(np.round(np.arange(31.0, -28.5, -1.1), 2)) + (25.3, 20.0, -13.0)   # the fuselage's main frames, and where its pieces join
SPARS = tuple(np.round(np.arange(17.2, -19.0, -1.55), 2))                        # the wing's spars lie across the aircraft
FIN_SPARS = (0.12, 0.30, 0.52, 0.78)                                             # (the rudders hinge at 0.78)
PANEL_SEED = 8
PANEL_PITCH = (1.25, 1.5, 1.1)             # how far apart the crossing seams are: round the fuselage (m of girth), along the wing, up the fin
SEAM, RIVET_PITCH, RIVET_INSET = 0.034, 0.172, 0.085   # a seam's width; fasteners 4 pixels apart, this far in from the seam
FIN_Z, FIN_H = 1.45, 5.15                  # the fin's root and how tall it is above that
# How dark the structure shows through the white: a seam, a fastener, the dirt gathered along a seam, and how
# much one panel's white differs from the next. Faint on top and on the sides (a clean airliner seen from across
# the apron: no tiles); the underside and the bare metal, which are not washed as often, get the second figure more.
SEAM_DARK, RIVET_DARK, SEAM_DIRT, PANEL_TONE = (0.13, 0.16), (0.05, 0.08), (0.03, 0.05), (0.014, 0.03)
NACELLE = (4.08, 6.78, -7.3, -17.5)        # each engine pair's box: |x| from, to; station from, to
ENGINES = (4.755, 6.105)                   # the engines' centrelines, and the station their jet pipes end at
JET_Y = -18.9
ELEVON_FAIRINGS = ((2.2, 3.0), (3.4, 3.0), (7.4, 2.5), (8.9, 2.15), (10.3, 1.8), (11.6, 1.1))  # |x|, the elevon's chord there: the jacks' fairings underneath
TANK_PANELS = ((2.6, 10.6, 14.0, -13.0), (4.4, 7.2, 1.5, -5.0), (7.6, 11.0, -9.0, -14.5))      # rows of oval tank doors under the wing: |x| from, to; station from, to
GEAR_BAY = (0.04, 1.37, -1.06, -4.14)      # the main wheel bays' doors; the legs' own doors beside them
LEG_DOOR = (1.37, 4.02, -2.28, -2.92)
NOSE_BAY = (0.0, 0.40, 19.80, 14.95)
WALKWAY = (1.6, 3.4, 1.5, -3.2)            # the escape path over each wing, aft of the middle door: |x| from, to; station from, to


# ---------------------------------------------------------------- a picture's geometry
class View:
    """One guide: for each pixel, the point of the model it shows (x, y, z), the
    surface's normal there, and what it is a picture of."""

    def __init__(self, name):
        self.name = name
        g = np.load(os.path.join(SRC, name + "_geo.npz"))
        self.ok = g["piece"] < 255
        h, w = self.ok.shape
        i, j = np.meshgrid((np.arange(w, dtype=np.float32) + 0.5 - w / 2) / PX, (h / 2 - np.arange(h, dtype=np.float32) - 0.5) / PX)
        depth = np.nan_to_num(g["depth"].astype(np.float32))
        if name in ("top", "bottom"):
            pos = np.stack([i if name == "top" else -i, MID_Y + j, depth], -1)
        else:
            pos = np.stack([depth, MID_Y + (-i if name == "side" else i), MID_Z + j], -1)
        pos *= self.ok[..., None]
        self.x, self.y, self.z = pos[..., 0], pos[..., 1], pos[..., 2]
        nrm = g["nrm"].astype(np.float32) / 127.0
        self.nx, self.ny, self.nz = nrm[..., 0], nrm[..., 1], nrm[..., 2]
        self.pos = pos
        pieces, mats = [str(s) for s in g["pieces"]], [str(s) for s in g["mats"]]
        self.piece_id = g["piece"]
        self._piece = lambda test: np.isin(g["piece"], [k for k, n in enumerate(pieces) if test(n)])
        self._mat = lambda *names: np.isin(g["mat"], [mats.index(n) for n in names if n in mats])
        self.h, self.w = self.ok.shape
        guide = np.asarray(Image.open(os.path.join(SRC, name + "_guide.png")).convert("L"), np.float32) / 255.0
        self.shade = guide / np.median(guide[self.ok])

    def piece(self, *words):
        return self._piece(lambda n: any(w in n for w in words))

    def mat(self, *names):
        return self._mat(*names)

    def px(self, a, b):
        """The pixel of a point of the picture's plane: (x, station) seen from above
        or below, (station, height) seen from a side."""
        if self.name == "top":
            return self.w / 2 + a * PX, self.h / 2 - (b - MID_Y) * PX
        if self.name == "bottom":
            return self.w / 2 - a * PX, self.h / 2 - (b - MID_Y) * PX
        s = -1.0 if self.name == "side" else 1.0  # the left side is seen nose to the left
        return self.w / 2 + s * (a - MID_Y) * PX, self.h / 2 - (b - MID_Z) * PX


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def noise(pos, size, seed, n=18):
    """Smooth noise over the model, about -2..2 (unit variance): waves of random
    direction, `size` metres long along each axis."""
    rng = np.random.default_rng(seed)
    k = rng.normal(size=(n, 3))
    k = k / np.linalg.norm(k, axis=1)[:, None] * rng.uniform(0.6, 1.5, (n, 1)) * 2 * np.pi / np.asarray(size, np.float32)
    phase = rng.uniform(0, 2 * np.pi, n)
    out = np.zeros(pos.shape[:2], np.float32)
    for ki, p in zip(k.astype(np.float32), phase):
        out += np.cos(pos @ ki + p)
    return out * np.sqrt(2.0 / n)


def mix(a, b, t):
    return a + (np.asarray(b, np.float32) - a) * t[..., None]


def line(d, w=SEAM):
    """1 on a line, falling to 0 `w` metres from it; d the distance from it."""
    return 1.0 - smoothstep(w * 0.45, w, np.abs(d))


def lines_at(v, stations, w=SEAM):
    out = np.zeros(v.shape, np.float32)
    for s in stations:
        out = np.maximum(out, line(v - s, w))
    return out


def blur(a, r):
    return np.asarray(Image.fromarray(np.clip(a * 255, 0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(r)), np.float32) / 255.0


def interp(x, table):
    t = np.asarray(table, np.float32)
    return np.interp(x, t[:, 0], t[:, 1]).astype(np.float32)


# ---------------------------------------------------------------- lettering and what is drawn on a sheet
# Capitals: strokes on a grid 4 wide and 6 high.
GLYPHS = {
    "A": [[(0, 0), (2, 6), (4, 0)], [(0.8, 2.1), (3.2, 2.1)]],
    "C": [[(4, 4.9), (3, 6), (1.2, 6), (0, 4.8), (0, 1.2), (1.2, 0), (3, 0), (4, 1.1)]],
    "D": [[(0, 0), (0, 6), (2.6, 6), (4, 4.6), (4, 1.4), (2.6, 0), (0, 0)]],
    "E": [[(4, 6), (0, 6), (0, 0), (4, 0)], [(0, 3.1), (3, 3.1)]],
    "I": [[(2, 0), (2, 6)]],
    "L": [[(0, 6), (0, 0), (4, 0)]],
    "N": [[(0, 0), (0, 6), (4, 0), (4, 6)]],
    "O": [[(1.2, 0), (3, 0), (4, 1.2), (4, 4.8), (3, 6), (1.2, 6), (0, 4.8), (0, 1.2), (1.2, 0)]],
    "Q": [[(1.2, 0), (3, 0), (4, 1.2), (4, 4.8), (3, 6), (1.2, 6), (0, 4.8), (0, 1.2), (1.2, 0)], [(2.4, 1.6), (4.2, -0.4)]],
    "R": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)], [(2.2, 3), (4, 0)]],
    "S": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 4), (1, 3), (3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "T": [[(0, 6), (4, 6)], [(2, 6), (2, 0)]],
    "X": [[(0, 0), (4, 6)], [(0, 6), (4, 0)]],
    "-": [[(0.8, 3), (3.2, 3)]],
}
SS = 6  # sheets are drawn this many times too big and scaled down, for clean edges


class Sheet:
    """Something painted over the white: drawn `SS` times too big on a picture
    `w` by `h` pixels; `at(a, b)` is the pixel of a point given in metres."""

    def __init__(self, w, h, at):
        self.w, self.h, self.at = w, h, at
        self.im = Image.new("RGBA", (w * SS, h * SS), (0, 0, 0, 0))
        self.d = ImageDraw.Draw(self.im)

    def p(self, a, b):
        x, y = self.at(a, b)
        return x * SS, y * SS

    def text(self, s, a, b, height, colour, lean=0.0, weight=1.0, pitch=5.4):
        """Letters `height` m tall centred on (a, b), reading left to right in the
        picture; `lean`: how far their tops lean to the right (italics)."""
        cx, cy = self.p(a, b)
        u = height * PX * SS / 6.0
        # Each letter takes its own width (an I is narrow), with `pitch` - 4 between them.
        ext = [(min(g[0] for st in GLYPHS[ch] for g in st), max(g[0] for st in GLYPHS[ch] for g in st)) for ch in s]
        starts = np.cumsum([0.0] + [hi - lo + pitch - 4 for lo, hi in ext])
        x0 = cx - (starts[-1] - (pitch - 4)) * u / 2
        wd = max(int(round(weight * u)), 1)
        for k, ch in enumerate(s):
            for stroke in GLYPHS[ch]:
                pts = [(x0 + (starts[k] - ext[k][0] + gx + lean * (gy - 3)) * u, cy + (3 - gy) * u) for gx, gy in stroke]
                self.d.line(pts, fill=colour + (255,), width=wd, joint="curve")
                for q in pts:  # round the ends and the corners
                    self.d.ellipse([q[0] - wd / 2, q[1] - wd / 2, q[0] + wd / 2, q[1] + wd / 2], fill=colour + (255,))

    def rrect(self, a0, a1, b0, b1, r, fill=None, outline=None, width=0.03):
        p0, p1 = self.p(a0, b0), self.p(a1, b1)
        box = [min(p0[0], p1[0]), min(p0[1], p1[1]), max(p0[0], p1[0]), max(p0[1], p1[1])]
        self.d.rounded_rectangle(box, radius=r * PX * SS, fill=fill and fill + (255,), outline=outline and outline + (255,), width=max(int(round(width * PX * SS)), 1))

    def polygon(self, pts, colour):
        self.d.polygon([self.p(*q) for q in pts], fill=colour + (255,))

    def ring(self, a, b, r, width, colour):
        cx, cy = self.p(a, b)
        R = r * PX * SS
        self.d.ellipse([cx - R, cy - R, cx + R, cy + R], outline=colour + (255,), width=max(int(round(width * PX * SS)), 1))

    def path(self, pts, width, colour, alpha=255):
        self.d.line([self.p(*q) for q in pts], fill=colour + (alpha,), width=max(int(round(width * PX * SS)), 1), joint="curve")

    def streak(self, a, b, length, width, colour, alpha):
        """A stain run back along the airflow from (a, b), fading out."""
        n = 12
        for k in range(n):
            t0, t1 = k / n, (k + 1) / n
            self.path([(a, b - length * t0), (a, b - length * t1)], width * (1 + 1.5 * t0), colour, int(alpha * (1 - t0) ** 1.5))

    def array(self, soft=0.0):
        im = self.im.filter(ImageFilter.GaussianBlur(soft * SS)) if soft else self.im
        return np.asarray(im.resize((self.w, self.h), Image.LANCZOS), np.float32)


def view_sheet(v):
    return Sheet(v.w, v.h, v.px)


def over(rgb, a, where):
    return mix(rgb, a[..., :3], a[..., 3] / 255.0 * where)


# The fuselage's sides and the fin, one sheet for each side: laid out by
# station (the nose to the left on the left side's, to the right on the right
# side's) and height, at the pictures' own scale.
SIDE_TOP, SIDE_BOTTOM = 8.6, -3.0
SIDE_W, SIDE_H = 1536, int(round((SIDE_TOP - SIDE_BOTTOM) * PX))


def fin_at(u, f):
    """The point of the fin (station, height) a fraction u up it and f back along its chord."""
    return -12.5 - 9.9 * u - f * (10.5 - 8.3 * u), FIN_Z + FIN_H * u


def side_sheet(s):
    """What is lettered on one side (s = -1 the left, 1 the right)."""
    k = Sheet(SIDE_W, SIDE_H, lambda y, z: (SIDE_W / 2 + s * (y - MID_Y) * PX, (SIDE_TOP - z) * PX))
    # The cabin windows, in two rows with the middle door between them.
    hw, hh = WINDOW_SIZE[0] / 2, WINDOW_SIZE[1] / 2
    for y0, y1 in CABINS:
        for y in np.arange(y0, y1 - 1e-3, -WINDOW_PITCH):
            k.rrect(y - hw, y + hw, WINDOW_Z - hh, WINDOW_Z + hh, 0.04, fill=WINDOW)
    # The doors: an outline, a handle, a small window; red corners on the middle one (the way out over the wing).
    for n, (y0, y1, z0, z1) in enumerate(DOORS):
        k.rrect(y0, y1, z0, z1, 0.10, outline=DOOR_LINE, width=0.045)
        k.rrect((y0 + y1) / 2 - 0.07, (y0 + y1) / 2 + 0.07, WINDOW_Z - hh, WINDOW_Z + hh, 0.03, fill=WINDOW)
        k.path([(y1 + 0.12, z0 + 0.62), (y1 + 0.30, z0 + 0.62)], 0.05, DOOR_LINE)
        if n == 1:
            for ya, yb in ((y0 + 0.06, y0 - 0.16), (y1 - 0.06, y1 + 0.16)):
                for za, zb in ((z0 - 0.06, z0 + 0.16), (z1 + 0.06, z1 - 0.16)):
                    k.path([(ya, zb), (ya, za), (yb, za)], 0.05, EXIT_RED)
    # The names and the registration.
    k.text(AIRLINE, *WORDMARK, BLUE, lean=0.16, weight=1.15, pitch=6.0)
    k.text("CONCORDE", *TYPE_NAME, BLUE, lean=0.16, weight=0.85)
    k.text(REGISTRATION, *FUSELAGE_REG, BLUE, weight=0.9)
    # The fin's emblem: a gold ring, and a white dart climbing through it with three gold lines behind.
    u, f, r = FIN_EMBLEM
    cy, cz = fin_at(u, f)
    fwd, up = np.array([np.cos(0.56), np.sin(0.56)]), np.array([-np.sin(0.56), np.cos(0.56)])
    at = lambda a, b: tuple(np.array([cy, cz]) + fwd * a * r + up * b * r)
    k.ring(cy, cz, r, 0.17 * r, GOLD)
    for b in (-0.34, 0.0, 0.34):
        k.path([at(-1.75, b), at(-0.80 - 0.5 * abs(b), b)], 0.085 * r, GOLD)
    dart = [(1.55, 0.0), (0.55, 0.20), (-0.35, 0.50), (-0.80, 0.78), (-0.52, 0.0), (-0.80, -0.78), (-0.35, -0.50), (0.55, -0.20)]
    k.polygon([at(a * 1.07 + 0.02, b * 1.16) for a, b in dart], BLUE)
    k.polygon([at(a, b) for a, b in dart], (240, 238, 230))
    return k.array()


def look_up(sheet, y, z, s):
    """The side sheet's paint at station y, height z (s as side_sheet)."""
    fx = np.clip(SIDE_W / 2 + s * (y - MID_Y) * PX - 0.5, 0, SIDE_W - 1.001)
    fy = np.clip((SIDE_TOP - z) * PX - 0.5, 0, SIDE_H - 1.001)
    x0, y0 = fx.astype(np.int32), fy.astype(np.int32)
    tx, ty = (fx - x0)[..., None], (fy - y0)[..., None]
    return (sheet[y0, x0] * (1 - tx) + sheet[y0, x0 + 1] * tx) * (1 - ty) + (sheet[y0 + 1, x0] * (1 - tx) + sheet[y0 + 1, x0 + 1] * tx) * ty


# ---------------------------------------------------------------- skin panels (as f4_paint.py, at this picture's scale)
def hashf(*ks, seed=0):
    """A number 0..1 for each set of whole numbers, the same every time."""
    h = np.full(np.shape(ks[0]), (seed * 0x9E3779B1 + 0x85EBCA6B) & 0xFFFFFFFF, np.uint32)
    for k in ks:
        h ^= np.asarray(k).astype(np.int64).astype(np.uint32)
        h *= np.uint32(0x9E3779B1)
        h ^= h >> np.uint32(15)
        h *= np.uint32(0x85EBCA6B)
        h ^= h >> np.uint32(13)
    return h.astype(np.float32) / 4294967295.0


def dots(along):
    """Fasteners along a row: on for a pixel and a half every RIVET_PITCH metres."""
    return (np.mod(along, RIVET_PITCH) < 0.065).astype(np.float32)


class Detail:
    """What is drawn into one view's paint: seams, fastener rows, stencilled
    lettering, the inside of access panels, and a number 0..1 for each skin
    panel (its paint aged a little differently)."""

    def __init__(self, v):
        self.seam, self.rivet, self.stencil, self.access = (np.zeros(v.ok.shape, np.float32) for _ in range(4))
        self.tone = np.full(v.ok.shape, 0.5, np.float32)


def panels(out, mask, a, b, breaks, b0, b1, pitch, seed, ma=1.0, mb=1.0):
    """Cut the surface under `mask` into skin panels. Seams run its whole length
    where `a` crosses `breaks`; between each pair of them others cross, about
    `pitch` apart in `b` (from b0 to b1), at places of their own in each bay.
    ma, mb: metres per unit of a and b. Every panel gets fasteners inside its
    edges, the wider ones a stiffener's row down the middle, some an access
    panel with its ring of fasteners, some a block of stencilling."""
    idx = np.nonzero(mask)
    if not len(idx[0]):
        return
    a, b = a[idx], b[idx]
    ma = ma[idx] if isinstance(ma, np.ndarray) else np.float32(ma)
    mb = mb[idx] if isinstance(mb, np.ndarray) else np.float32(mb)
    ab = np.concatenate(([-1e3], np.sort(np.asarray(breaks, np.float32)), [1e3])).astype(np.float32)
    i = np.clip(np.searchsorted(ab, a) - 1, 0, len(ab) - 2)
    lo_a, hi_a = ab[i], ab[i + 1]
    rng = np.random.default_rng(seed)
    n = int((b1 - b0) / (0.6 * pitch)) + 2
    table = np.full((len(ab) - 1, n), 1e3, np.float32)
    for row in table:
        p, k = b0 + rng.uniform(0.2, 1.0) * pitch, 0
        while p < b1 and k < n:
            row[k] = p
            p += rng.uniform(0.6, 1.4) * pitch
            k += 1
    t = table[i]
    below = b[:, None] > t
    j = below.sum(1)
    lo_b, hi_b = np.where(below, t, -1e3).max(1), np.where(below, 1e3, t).min(1)
    da, db = np.minimum(a - lo_a, hi_a - a) * ma, np.minimum(b - lo_b, hi_b - b) * mb   # metres from the nearest seam each way
    ha, hb = (hi_a - lo_a) / 2 * ma, (hi_b - lo_b) / 2 * mb                              # the panel's half size
    along_a, along_b = dots(a * ma), dots(b * mb)
    seam = np.maximum(line(da), line(db))
    rivet = np.maximum(line(da - RIVET_INSET) * along_b, line(db - RIVET_INSET) * along_a)
    rivet = np.maximum(rivet, line(ha - da) * along_b * (ha > 0.7) * (ha < 30) * (db > 0.1))
    rivet = np.maximum(rivet, line(hb - db) * along_a * (hb > 0.7) * (hb < 30) * (da > 0.1))
    h = lambda k: hashf(i, j, seed=seed * 16 + k)
    # An access panel: a rectangle with round corners, or an oval.
    ea, eb = np.minimum(ha * (0.35 + 0.30 * h(2)), 0.55), np.minimum(hb * (0.35 + 0.30 * h(3)), 0.55)
    has = (h(1) > 0.70) & (ha > 0.36) & (hb > 0.36) & (ha < 30) & (hb < 30)
    r = np.minimum(ea, eb) * np.where(h(4) > 0.7, 1.0, 0.3)
    qa, qb = np.abs(a - (lo_a + hi_a) / 2) * ma - ea + r, np.abs(b - (lo_b + hi_b) / 2) * mb - eb + r
    d = np.hypot(np.maximum(qa, 0), np.maximum(qb, 0)) + np.minimum(np.maximum(qa, qb), 0) - r
    seam = np.maximum(seam, 0.7 * line(d) * has)
    inside = (d < 0) & has
    # Stencilling: a row or two of dashes in a corner of the panel.
    la, lb = (a - lo_a) * ma - 0.20, (b - lo_b) * mb - 0.18
    rows, wide = 1 + (h(6) * 2).astype(np.int32), 0.35 + 0.6 * h(7)
    st = (h(5) < 0.34) & ~inside & (la > 0) & (la < wide) & (lb > 0) & (lb < rows * 0.11) & (np.mod(lb, 0.11) < 0.05) & (ha < 30) & (hb < 30)
    st &= hashf(np.floor(la / 0.085), np.floor(lb / 0.11), i, j, seed=seed) > 0.32
    out.seam[idx] = np.maximum(out.seam[idx], seam)
    out.rivet[idx] = np.maximum(out.rivet[idx], rivet)
    out.tone[idx] = h(0)
    out.access[idx] = inside
    out.stencil[idx] = st


def box_d(a, b, box, r=0.12):
    """Distance outside a round-cornered rectangle (a from, to; b from, to); negative inside."""
    a0, a1, b0, b1 = box
    qa = np.abs(a - (a0 + a1) / 2) - abs(a1 - a0) / 2 + r
    qb = np.abs(b - (b0 + b1) / 2) - abs(b1 - b0) / 2 + r
    return np.hypot(np.maximum(qa, 0), np.maximum(qb, 0)) + np.minimum(np.maximum(qa, qb), 0) - r


def smear(src, axis, step, decay, ok):
    """Drag `src` across the picture along `axis` (step 1 or -1), dying away by
    `decay` (a picture of its own) each pixel: what runs back from a leak."""
    out, dec, okm = np.moveaxis(src.copy(), axis, 0), np.moveaxis(decay, axis, 0), np.moveaxis(ok, axis, 0)
    for i in (range(1, len(out)) if step > 0 else range(len(out) - 2, -1, -1)):
        out[i] = np.maximum(out[i], out[i - step] * dec[i]) * okm[i]
    return np.moveaxis(out, 0, axis)


# ---------------------------------------------------------------- painting a view
def paint(v, sheets):
    sides = v.name in ("side", "right")
    rng = np.random.default_rng(("top", "bottom", "side", "right").index(v.name) + 1)

    def grain(r):
        """Fine random grain, blurred over r pixels: unit variance."""
        g = blur(rng.random(v.ok.shape).astype(np.float32), r)
        return (g - g.mean()) / (g.std() + 1e-6)

    body = v.piece("fuselage", "nose", "tail cone")
    wing = v.piece("wing", "elevon")
    fin = v.piece("fin", "rudder")
    nacelle = v.piece("intake")
    skin = v.mat("skin", "well")   # (the gear's doors shut over its wells, and take this paint)
    ax = np.abs(v.x)
    above, below, steep = v.nz > 0.45, v.nz < -0.45, np.abs(v.nx) > 0.45
    left = (v.x < 0) if not sides else np.full(v.ok.shape, v.name == "side")

    rgb = np.empty(v.ok.shape + (3,), np.float32)
    rgb[:] = WHITE

    # ---- the livery
    # The cheatline under the windows: it runs out to a point along the nose and sweeps up into the fin.
    y0, y1, rise, grow = CHEAT_RISE
    sweep = smoothstep(y0, y1, v.y) ** 1.3
    zc = CHEAT_Z + 0.9 * interp(np.maximum(v.y, 18.0), MID_LINE) + rise * sweep
    half = (CHEAT_HALF + grow * smoothstep(y0 - 1.0, y1, v.y)) * (1 - smoothstep(24.0, 31.0, v.y))
    edge = 0.022
    band = smoothstep(half + edge, half - edge, np.abs(v.z - zc)) * (half > 0.01) * body
    pin = line(v.z - (zc - half - PIN_GAP - PIN_WIDTH / 2), PIN_WIDTH / 2 + edge) * (half > 0.03) * body
    # The fin: blue behind a line a third of the way back, the gold pinstripe ahead of it.
    u = (v.z - FIN_Z) / FIN_H
    fin_c = 10.5 - 8.3 * u
    ff = (-12.5 - 9.9 * u - v.y) / fin_c
    band = np.maximum(band, smoothstep(-edge, edge, (ff - FIN_BLUE) * fin_c) * fin)
    pin = np.maximum(pin, line((ff - FIN_BLUE) * fin_c + 0.16, PIN_WIDTH / 2 + 0.03) * fin)
    rgb = mix(rgb, GOLD, pin)
    rgb = mix(rgb, BLUE, band)
    # Windows, doors, names, the emblem: from the side's own sheet.
    a = np.where(left[..., None], look_up(sheets[0], v.y, v.z, -1.0), look_up(sheets[1], v.y, v.z, 1.0))
    rgb = over(rgb, a, (body | fin) * skin * (v.nz > -0.3))
    # On the wings: the registration over the right and under the left; the red-edged way out over each wing.
    k = view_sheet(v)
    if v.name == "top":
        k.text(REGISTRATION, WING_REG[0], WING_REG[1], WING_REG[2], BLUE, weight=0.95)
        for s in (-1, 1):
            x0, x1, ya, yb = WALKWAY
            k.rrect(s * x0, s * x1, ya, yb, 0.05, outline=EXIT_RED, width=0.06)
            xm = s * (x0 + x1) / 2
            k.path([(xm, ya - 0.6), (xm, yb + 0.9)], 0.06, EXIT_RED)   # an arrow aft, toward the trailing edge
            k.polygon([(xm, yb + 0.5), (xm - 0.32, yb + 1.1), (xm + 0.32, yb + 1.1)], EXIT_RED)
    elif v.name == "bottom":
        k.text(REGISTRATION, -WING_REG[0], WING_REG[1], WING_REG[2], BLUE, weight=0.95)
    rgb = over(rgb, k.array(), wing * skin)
    # (No anti-glare panel: its nose was white to the tip.)

    # ---- bare metal
    le = interp(ax, WING_LE)
    te = -19.0 + 0.11 * ax
    from_le = le - v.y
    streaky = noise(v.pos, (0.35, 6.0, 0.35), 31)  # streaked along the airflow
    # The wing's leading edge, and the fin's.
    sweep_le = np.hypot(1.0, (interp(ax + 0.05, WING_LE) - interp(ax - 0.05, WING_LE)) / 0.1)   # (the strip is measured square to the edge)
    metal = np.maximum(wing * smoothstep(0.30, 0.20, from_le / sweep_le), fin * smoothstep(0.26, 0.18, ff * fin_c * 0.5))
    # The nacelles' aft ends, hot from the reheat pipes inside; the buckets and the jet pipes themselves.
    n_aft = nacelle * smoothstep(-13.6, -14.6, v.y)
    hot = np.maximum(n_aft, v.mat("exhaust", "ramp", "steel").astype(np.float32))
    m = np.empty_like(rgb)
    m[:] = STEEL
    heat = smoothstep(-14.5, JET_Y + 0.8, v.y)
    m = mix(m, STRAW, 0.5 * smoothstep(0.2, 0.9, heat + 0.12 * streaky))
    m = mix(m, BLUED, 0.35 * smoothstep(0.75, 1.0, heat + 0.10 * noise(v.pos, (1.2, 2.0, 1.2), 32)))
    m *= (1.0 + 0.07 * streaky)[..., None]
    rgb = mix(rgb, METAL, metal * (1 - band))
    rgb = mix(rgb, m, hot)
    # The intakes' lips are bare, their ramps and the splitter between each pair dark.
    lip = nacelle * smoothstep(-8.15, -7.95, v.y)
    rgb = mix(rgb, METAL, lip)
    bare = np.clip(metal + hot + lip, 0, 1)
    fresh = rgb.copy()  # as it left the paint shop

    # ---- the structure showing through: skin panels, fasteners, doors
    D = Detail(v)
    pan = v.ok & skin
    # The fuselage: frames by station; stringers at angles round its axis, so they meet across the views.
    mid = interp(v.y, MID_LINE)
    girth = np.maximum(np.hypot(v.x, v.z - mid), 0.2)
    panels(D, body & pan, v.y, np.arctan2(v.z - mid, ax), FRAMES, -1.6, 1.6, PANEL_PITCH[0], PANEL_SEED, 1.0, girth)
    # The wing: spars across the aircraft, ribs fore and aft; the upper and lower skins cut differently. The fin: spars by chord.
    for n, face in enumerate((v.nz >= 0, v.nz < 0)):
        panels(D, wing & pan & face, v.y, ax, SPARS, 0.9, 12.9, PANEL_PITCH[1], PANEL_SEED + 1 + n)
    panels(D, fin & pan, ff, v.z, FIN_SPARS, 1.3, FIN_Z + FIN_H + 0.1, PANEL_PITCH[2], PANEL_SEED + 3, fin_c, 1.0)
    panels(D, nacelle & pan, v.y, np.where(steep, v.z, ax), np.arange(-8.5, -18.0, -1.9), -3.0, 7.0, 0.9, PANEL_SEED + 4)
    seam, rivet = D.seam, D.rivet

    # Where two pieces of one surface meet (the elevons, the rudders, the nose's joint).
    joint = np.zeros(v.ok.shape, np.float32)
    depth = v.z if not sides else v.x
    for axis in (0, 1):
        d = (np.roll(v.piece_id, 1, axis) != v.piece_id) & v.ok & np.roll(v.ok, 1, axis) & (np.abs(np.roll(depth, 1, axis) - depth) < 0.25)
        joint = np.maximum(joint, d.astype(np.float32))
    seam = np.maximum(seam, np.clip(blur(joint, 0.6) * 1.6, 0, 1))
    # The radome's joint (it was white, like the rest); the rudders' and elevons' close-set ribs.
    seam = np.maximum(seam, 0.6 * line(v.y - 29.6) * body)
    rivet = np.maximum(rivet, (np.mod(v.z, 0.42) < 0.045) * dots(v.y) * v.piece("rudder"))
    rivet = np.maximum(rivet, (np.mod(ax, 0.50) < 0.045) * dots(v.y) * v.piece("elevon"))
    fairing = np.zeros(v.ok.shape, np.float32)
    if v.name == "bottom":
        # The gear's doors (the wheel bays, the legs' own, the nose's pair meeting on the centreline).
        for box in (GEAR_BAY, LEG_DOOR, NOSE_BAY):
            seam = np.maximum(seam, line(box_d(ax, v.y, box)))
        seam = np.maximum(seam, line(ax, 0.03) * (v.y < NOSE_BAY[2]) * (v.y > NOSE_BAY[3]))
        # Rows of oval tank doors under the wing, each with its ring of fasteners.
        for x0, x1, ya, yb in TANK_PANELS:
            qx, qy = np.mod(ax - x0, 1.1) - 0.55, np.mod(v.y - yb, 1.55) - 0.78
            d = np.hypot(qx / 0.27, qy / 0.40) - 1.0
            inside = (ax > x0) & (ax < x1) & (v.y < ya) & (v.y > yb) & (from_le > 1.2) & wing
            seam = np.maximum(seam, 0.55 * line(d * 0.30) * inside)
        # The fairings over the elevons' jacks.
        for fx, chord in ELEVON_FAIRINGS:
            d = np.hypot((ax - fx) / 0.15, (v.y - (-19.0 + 0.11 * fx + chord + 0.1)) / 0.85) - 1.0
            seam = np.maximum(seam, 0.8 * line(d * 0.15) * wing)
            fairing = np.maximum(fairing, smoothstep(0.0, -0.4, d) * wing)
    not_skin = v.mat("glass", "frame", "inlet", "rubber", "exhaust", "ramp", "steel", "strut", "hub")
    seam = np.clip(seam, 0, 1) * ~not_skin
    rivet = np.clip(rivet, 0, 1) * ~not_skin * (1 - seam)

    # ---- weathering (a well-kept airliner's: slight)
    # Every skin panel's white a little different, greyed in broad patches; a fine mottle so nowhere is one flat colour.
    fade = noise(v.pos, (4.0, 6.0, 3.0), 11)
    low = np.clip(-v.nz, 0, 1)   # how far underneath: where the dirt is left
    rgb *= (1.0 + 0.010 * fade + (PANEL_TONE[0] + PANEL_TONE[1] * low) * (D.tone - 0.5) + 0.008 * noise(v.pos, (0.5, 0.5, 0.5), 13))[..., None]
    warm = 0.5 + 0.5 * np.tanh(noise(v.pos, (2.2, 3.0, 2.2), 12))
    rgb *= 1.0 - (1 - bare)[..., None] * warm[..., None] * np.array([0.004, 0.010, 0.026], np.float32)   # the paint yellowing where the sun and the heat get at it
    rgb = mix(rgb, fresh * 1.01, 0.7 * ((D.tone > 0.92) | (D.access > 0)) * (1 - bare))                    # panels repainted since
    rgb *= (1.0 + 0.008 * grain(0.8) + 0.008 * grain(3.0) + 0.010 * grain(10.0))[..., None]
    # Grime streaked back along the airflow, more of it underneath; rain run down the fuselage's sides and the fin.
    streaks = smoothstep(0.5, 1.8, noise(v.pos, (0.30, 9.0, 0.30), 14) + 0.5 * noise(v.pos, (4.0, 12.0, 4.0), 15))
    dirty = np.clip(-v.nz, 0, 1)
    rgb = mix(rgb, GRIME, streaks * (0.015 + 0.10 * dirty) * (1 - band))
    rain = noise(v.pos, (16.0, 0.22, 6.0), 18) + 0.6 * noise(v.pos, (16.0, 2.5, 8.0), 19)
    rgb = mix(rgb, GRIME, 0.05 * smoothstep(0.7, 1.9, rain) * steep * (body | fin) * (1 - band))
    rgb = mix(rgb, GRIME, 0.05 * smoothstep(0.2, 1.4, noise(v.pos, (1.2, 1.6, 1.2), 24)) * dirty)           # the belly's general dirt
    # The way out over the wing: scuffed by feet.
    if v.name == "top":
        x0, x1, ya, yb = WALKWAY
        scuff = (box_d(ax, v.y, (x0, x1, ya, yb)) < 0) * wing * (0.5 + 0.3 * noise(v.pos, (0.6, 0.9, 0.6), 20) + 0.2 * grain(1.5))
        rgb = mix(rgb, GRIME, np.clip(0.16 * scuff, 0, 0.3))

    # What leaks: dirt and fluid dragged aft from seams and doors (and, on the sides, down by the rain).
    leaky = noise(v.pos, (1.5, 1.5, 1.5), 17)
    leak = np.clip(seam * (leaky > 0.5), 0, 1)
    wet = np.zeros(v.ok.shape, np.float32)
    if v.name == "bottom":
        for box in (GEAR_BAY, LEG_DOOR, NOSE_BAY):
            wet = np.maximum(wet, line(box_d(ax, v.y, box), 0.05) * (leaky > -0.6))
        wet = np.maximum(wet, nacelle * line(v.y + 13.0, 0.08))   # the engine bay doors' drains
    reach = np.clip(0.93 + 0.04 * noise(v.pos, (1.0, 1.0, 1.0), 23), 0.80, 0.985)
    axis, step = (1, 1 if v.name == "side" else -1) if sides else (0, 1)   # which way aft is in this picture
    trail = smear(np.maximum(leak, wet), axis, step, reach, v.ok)
    oil = smear(wet, axis, step, np.clip(reach + 0.03, 0, 0.99), v.ok)
    if sides:
        trail = np.maximum(trail, smear(leak * (leaky > 1.0), 0, 1, reach, v.ok))
    trail = blur(np.clip(trail - leak, 0, 1), 0.7) * ~not_skin
    rgb = mix(rgb, GRIME, np.clip(trail * (0.06 + 0.26 * dirty), 0, 0.5))
    rgb = mix(rgb, (70, 56, 38), np.clip(blur(oil, 0.8) * 0.40, 0, 0.5) * ~not_skin)

    # Soot: the Olympus smoked. Behind the jet pipes, over and under the wing between the elevons; grey up the rear fuselage and the fin's foot.
    r_jet = np.minimum(np.abs(ax - ENGINES[0]), np.abs(ax - ENGINES[1]))
    soot = wing * smoothstep(-16.2, -18.2, v.y) * np.exp(-((r_jet / 1.3) ** 2)) * (0.75 + 0.25 * streaky)
    soot = np.maximum(soot, nacelle * smoothstep(-16.0, NACELLE[3], v.y) * 0.8)
    soot = np.maximum(soot, 0.30 * body * smoothstep(-17.5, -22.0, v.y) * smoothstep(1.3, 0.2, v.z - mid) * (0.6 + 0.4 * streaky))
    rgb = mix(rgb, SOOT, np.clip(soot * 0.62, 0, 0.75))
    if v.name == "bottom":
        k = view_sheet(v)
        srng = np.random.default_rng(7)
        for _ in range(60):   # hydraulic fluid and oil run back under the belly and the nacelles
            a_ = srng.choice([-1, 1]) * (abs(srng.normal(0.0, 1.0)) if srng.random() < 0.5 else srng.uniform(4.1, 6.8))
            k.streak(a_, srng.uniform(-17.0, 12.0), srng.uniform(1.2, 5.0), srng.uniform(0.04, 0.09), (86, 70, 50), srng.uniform(50, 130))
        rgb = over(rgb, k.array(soft=1.0), skin.astype(np.float32))

    # Seams, fasteners and stencilling. A seam is a dark line with dirt gathered
    # along it and the paint a little light on its edge (the game takes dark
    # lines to lie below the skin round them).
    grime = blur(seam, 2.5) * (0.6 + 0.4 * noise(v.pos, (1.2, 2.4, 1.2), 16))
    rgb *= (1.0 - (SEAM_DIRT[0] + SEAM_DIRT[1] * dirty) * np.clip(grime * 2.2, 0, 1) * (1 + dirty))[..., None]
    rim = np.clip(np.roll(seam, (1, 1), (0, 1)) - seam, 0, 1)
    rgb *= (1.0 + 0.012 * rim)[..., None]
    rgb = mix(rgb, (60, 64, 76), 0.55 * D.stencil * (1 - bare) * (1 - band))
    rgb *= (1.0 - 0.07 * fairing)[..., None]   # the jacks' fairings stand in their own shadow
    rgb *= (1.0 - (RIVET_DARK[0] + RIVET_DARK[1] * np.maximum(bare, dirty)) * rivet)[..., None]
    rgb *= (1.0 - (SEAM_DARK[0] + SEAM_DARK[1] * np.maximum(bare, dirty)) * seam)[..., None]

    # What is not painted skin (the pictures are not projected onto it, but its edge shows).
    rgb = mix(rgb, GLASS, v.mat("glass").astype(np.float32))
    rgb = mix(rgb, FRAME, v.mat("frame", "inlet", "rubber").astype(np.float32))
    # The guide's shading, and shadow gathered where one piece meets another, so the shapes read.
    rgb *= (np.clip(v.shade, 0.6, 1.15) ** 0.10)[..., None]
    rgb *= (1.0 - 0.10 * np.clip(blur(joint, 4.0) * 3.0, 0, 1))[..., None]
    rgb = np.clip(rgb, 0, 250)
    rgb[~v.ok] = (255, 0, 255)  # (the background: the bake leaves it out)
    Image.fromarray(rgb.astype(np.uint8)).save(os.path.join(SRC, v.name + ".png"), optimize=True)
    print(f"painted {v.name}")


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
    paint_ = Image.open(os.path.join(SRC, view + ".png")).convert("RGB")
    if paint_.size != mask.size:
        paint_ = paint_.resize(mask.size, Image.LANCZOS)
    rgb = np.asarray(paint_)
    # Painted: inside the silhouette, less a pixel (where the picture's own edge
    # may fall short of the model's), and not the background's magenta (which
    # also shows through the gaps at the hinges).
    ok = (np.asarray(mask.filter(ImageFilter.MinFilter(3))) > 128) & ~((rgb[..., 0] > 240) & (rgb[..., 1] < 40) & (rgb[..., 2] > 240))
    out = Image.fromarray(np.clip(spread(rgb, ok), 0, 255).astype(np.uint8))
    print(f"{view}: {paint_.size[0]}x{paint_.size[1]}, {ok.mean() * 100:.0f}% painted")
    return out


if not BAKE_ONLY:
    for name in ("top", "bottom", "side", "right"):
        for kind, mode in (("guide", "L"), ("mask", "1")):  # as Blender wrote them: pack them
            path = os.path.join(SRC, f"{name}_{kind}.png")
            im = Image.open(path)
            if im.mode != mode:
                im.convert("L").point(lambda c: c if kind == "guide" else 255 * (c > 127)).convert(mode).save(path, optimize=True)
    sheets = (side_sheet(-1.0), side_sheet(1.0))
    for name in ("top", "bottom", "side", "right"):
        paint(View(name), sheets)
top, bottom, side, right = layer("top"), layer("bottom"), layer("side"), layer("right")
# Nose up like the other two, height running left to right in both: the left
# side is seen nose to the left, the right side nose to the right.
side = side.transpose(Image.Transpose.ROTATE_270)
right = right.transpose(Image.Transpose.FLIP_LEFT_RIGHT).transpose(Image.Transpose.ROTATE_270)
assert top.size == bottom.size == side.size == right.size, (top.size, bottom.size, side.size, right.size)
w, h = top.size
atlas = Image.new("RGB", (w * 4, h))
for k, im in enumerate((top, bottom, side, right)):
    atlas.paste(im, (w * k, 0))
atlas.save(OUT, quality=92, subsampling=0)
print("wrote", OUT, atlas.size, os.path.getsize(OUT), "bytes")
