# Paints the Blackbird and bakes its paint into assets/sr71_paint.jpg (plain Python, with Pillow and NumPy):
#
#   python assets/blender/sr71_paint.py [--bake-only] [<dir of pictures>] [<out.jpg>]
#
# The paint is four pictures of the aircraft: from straight above, from
# straight below, from its left side and from its right side
# (assets/blender/sr71_paint/top.png, bottom.png, side.png, right.png). The
# game projects them back onto the skin (plane.wgsl, `hornet_paint`), so they
# must line up with the model: they are painted over the guides that
#
#   blender -b --factory-startup --python assets/blender/sr71.py -- <scratch dir> --guides assets/blender/sr71_paint
#
# renders (<view>_guide.png, the silhouette in <view>_mask.png, and
# <view>_geo.npz: how deep into the picture the model is at each pixel, its
# normal there, and which piece and material it is). As the Phantom's
# (f4_paint.py), they are painted here, by rule, from that geometry: a pixel's
# colour comes from where on the aircraft it is, so the panels meet across
# the views.
#
# The scheme is the SR-71A's in service: all over a black that is really a
# very dark indigo, the "iron ball" paint that shed heat and soaked up radar,
# baked toward grey-blue where the air heated it most and never quite the
# same from one panel to the next. Under it the structure shows: the wing's
# skins corrugated fore and aft so they could grow with the heat, the wedges
# of radar-absorbing plastic let into the chines and every leading and
# trailing edge (rows of triangles), big titanium panels held by thousands of
# flush fasteners. Almost no markings: thin red lines round the places it is
# safe to walk, a small star and "U.S. AIR FORCE" low on the forward fuselage,
# the serial in red on the fins' outer faces (made up: 17992 was never built;
# their inner faces are plain in the model, sr71.py `inner_faces`), small
# insignia on one wing above and the other below. The ejector flaps and the
# back of each nacelle are bare metal, tinted by the heat; underneath, the
# fuel it wept from every tank seam on the ground has run aft and dried.
#
# (Freshly rendered guides and masks are packed, grey and one-bit, on the
# first run here.) The pictures can be touched up or replaced in any paint
# program (keep the background white): --bake-only then bakes them as they
# stand, without painting them again.
#
# The bake is the Hornet's (fa18_paint.py): each picture is cut to the
# model's silhouette, the paint at its edge spread outward over the
# background, and the four laid side by side, nose up: top | bottom | left |
# right. The side views are turned so that both are laid out alike (height
# across, the nose at the top), which is how the game looks them up.
import os
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
BAKE_ONLY = "--bake-only" in sys.argv
SRC = ARGS[0] if ARGS else os.path.join(HERE, "sr71_paint")
OUT = ARGS[1] if len(ARGS) > 1 else os.path.join(os.path.dirname(HERE), "sr71_paint.jpg")

# The rectangles the pictures cover (GUIDE_* in sr71.py): metres across and
# fore and aft, the station and height at their middle, pixels per metre
# (a pixel is 23 mm).
SPAN, LEN, MID_Y, MID_Z = 24.0, 36.0, 1.63, 1.0
PX = 1536 / LEN

# ---------------------------------------------------------------- the scheme (sRGB; metres, Blender frame: x right, y forward, z up)
BLACK = (37, 39, 47)          # the paint: a very dark indigo
HEATED = (66, 72, 88)         # baked toward grey-blue where it ran hottest
DIELECTRIC = (41, 37, 36)     # the plastic wedges and the nose: a browner, flatter black
TITANIUM = (104, 106, 112)    # where the paint has worn through
METAL = (112, 108, 104)       # bare metal: the ejector flaps, the back of the nacelles
STRAW, BLUED, PURPLE, SOOT = (146, 118, 76), (70, 78, 116), (98, 70, 104), (22, 21, 21)
GLASS, FRAME = (26, 29, 32), (22, 23, 25)
RED = (126, 30, 28)           # walkway lines, the serial, the lettering: a dull red that hardly shows from any distance
WALK_LINE = 0.022             # how wide the walkway lines are (one pixel)
PANEL_TONE = 0.30             # how far one panel's black may differ from the next's (more, and from far off it is a quilt)
INSIGNIA_BLUE, INSIGNIA_WHITE, INSIGNIA_RED = (27, 40, 68), (214, 214, 208), (150, 34, 38)
FUEL_WET, FUEL_DRY = (20, 20, 22), (84, 74, 60)   # JP-7: fresh, and what it leaves when it dries

SERIAL = "17992"                               # (made up: the last built was 17981)
FIN_SERIAL = (-9.45, 1.92, 0.36)               # station, height of its middle, digit height
LETTERING = "U.S. AIR FORCE"
FUSELAGE_LETTERING = (5.05, 0.32, 0.21)        # station, height, letter height
FUSELAGE_STAR = (7.60, 0.33, 0.20)             # station, height, the star's radius
WING_STAR_TOP = (-6.95, -9.62, 0.31)           # x, station, radius: upper left wing
WING_STAR_BOTTOM = (6.90, -9.55, 0.34)         # lower right wing
# "NO STEP": red lines round where it is safe to walk. Each wing: |x| from, to; station from, to.
WALK_BOXES = ((1.98, 3.02, -1.60, -9.90), (5.25, 6.20, -7.30, -10.05))
WALK_SPINE = (0.52, 7.90, -9.60)               # and a line each side of the fuselage's top: |x|; station from, to

# ---------------------------------------------------------------- the structure under the paint
# The fuselage (FUSE in sr71.py): station, half width to the chine's edge, half width of the round body.
FUSE = ((16.70, 0.00, 0.00), (16.40, 0.10, 0.07), (15.80, 0.24, 0.17), (15.00, 0.40, 0.28), (14.00, 0.58, 0.40), (13.00, 0.74, 0.50), (12.00, 0.88, 0.58),
        (11.00, 0.98, 0.64), (10.00, 1.06, 0.70), (9.00, 1.14, 0.75), (8.00, 1.22, 0.79), (6.50, 1.36, 0.81), (5.00, 1.52, 0.81), (3.20, 1.66, 0.81),
        (1.20, 1.72, 0.81), (-2.00, 1.72, 0.86), (-4.20, 1.72, 0.90), (-6.40, 1.72, 0.86), (-9.00, 1.68, 0.76), (-11.00, 1.50, 0.58), (-13.00, 0.90, 0.32), (-14.74, 0.04, 0.02))
WING_Z = -0.10                                 # the wing's plane, and the chines'
WING_LE = (4.2, 1.732)                         # leading edge: station at x = 0, m back per m out (60 degrees)
WING_TE = (-11.6, 4.95, -10.9, 8.47)           # trailing edge: straight to x = 4.95, then forward to the tip
NACELLE = (4.12, -0.10, 0.88)                  # each nacelle's centreline (x, z) and radius
FIN = (0.66, 2.68, -6.40, 2.80, 5.00)          # fins: root height, height, leading edge station at the root, its rake, root chord
# Seams that run right across a surface: the fuselage's and nacelles' frames
# and the wing's spars, by station. Between each pair others cross, at places
# of their own in every bay (PANEL_SEED: another number, another aircraft's panels).
FRAMES = (15.9, 15.1, 14.0, 13.1, 12.4, 11.6, 10.9, 10.1, 9.3, 8.5, 7.7, 6.9, 6.1, 5.3, 4.5, 3.7, 2.9, 2.1, 1.2, 0.3, -0.6, -1.5, -2.4, -3.3, -4.2,
          -5.1, -6.0, -6.9, -7.8, -8.7, -9.6, -10.4, -11.0, -11.8, -12.6, -13.4)
NACELLE_FRAMES = (2.2, 1.5, 0.6, -0.4, -1.4, -2.4, -3.4, -4.4, -5.4, -6.4, -7.3, -8.2, -9.0, -9.7, -10.3)
SPARS = (3.3, 2.4, 1.5, 0.5, -0.5, -1.5, -2.5, -3.5, -4.5, -5.5, -6.5, -7.5, -8.5, -9.4, -10.35)
FIN_SPARS = (0.12, 0.30, 0.45, 0.62, 0.80)     # (the fin turns about 0.45 of its chord)
PANEL_SEED = 71
PANEL_PITCH = (0.62, 0.50, 0.78, 0.62, 0.55)   # how far apart the crossing seams are: round the fuselage, across a chine, along the wing, round a nacelle, up a fin (m)
RIVET_PITCH, RIVET_INSET = 0.094, 0.050        # fasteners along every seam: 4 pixels apart, this far in from the seam
CORRUGATION = 0.094                            # the wing skins' corrugations, crest to crest (4 pixels; nearer 5 cm on the aircraft)
# The wedges of radar-absorbing plastic let into the edges: how deep the row is, and each wedge's width along the edge.
WEDGE_CHINE, WEDGE_WING, WEDGE_ELEVON, WEDGE_FIN = (0.26, 0.42), (0.40, 0.30), (0.20, 0.24), (0.22, 0.30)
# Things with a place of their own. On top: |x| from, to; station from, to.
RECEPTACLE = (0.0, 0.16, 5.60, 4.55)           # the air refuelling receptacle's doors behind the cockpits
TOP_PANELS = ((0.0, 0.30, 7.60, 6.40), (0.0, 0.34, 3.60, 2.30), (0.0, 0.30, -0.80, -2.20), (0.0, 0.30, -6.00, -7.40), (2.10, 2.90, -3.00, -4.40), (5.40, 6.30, -8.00, -8.90))
BELLY_PANELS = ((0.0, 0.42, 12.9, 10.4), (0.0, 0.50, 6.80, 5.10), (0.0, 0.52, 4.60, 2.60), (0.0, 0.34, 1.00, -1.00), (0.0, 0.34, -6.50, -8.50), (1.90, 2.90, -1.20, -2.90), (5.30, 6.40, -7.80, -9.20))
SIDE_PANELS = ((15.2, 14.2, -0.05, 0.25), (13.6, 12.7, 0.00, 0.34), (8.20, 7.00, -0.05, 0.22), (6.60, 5.90, 0.42, 0.66), (3.40, 2.95, 0.05, 0.50))   # station from, to; height from, to
TANK_SEAMS = (6.1, 3.7, 1.2, -1.5, -4.2, -6.9, -9.6)   # where the fuselage's tanks join: the fuel weeps here
BYPASS_DOORS = (1.75, 1.45)                    # each nacelle's forward bypass doors: a band of louvres (stations)
BLEED_SLOTS = (-3.2, -3.9)                     # and the tertiary doors round its middle


# ---------------------------------------------------------------- a picture's geometry
class View:
    """One guide: for each pixel, the point of the model it shows (x, y, z), the
    surface's normal there, and what it is a picture of."""

    def __init__(self, name):
        self.name = name
        g = np.load(os.path.join(SRC, name + "_geo.npz"))
        self.ok = g["piece"] < 255
        # The point each pixel shows: where the pixel is in the picture's plane, and its depth.
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
        self._mat = lambda *names: np.isin(g["mat"], [mats.index(n) for n in names])
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


def line(d, w=0.020):
    """1 on a line, falling to 0 `w` metres from it; d the distance from it."""
    return 1.0 - smoothstep(w * 0.45, w, np.abs(d))


def lines_at(v, stations, w=0.020):
    out = np.zeros(v.shape, np.float32)
    for s in stations:
        out = np.maximum(out, line(v - s, w))
    return out


def blur(a, r):
    return np.asarray(Image.fromarray(np.clip(a * 255, 0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(r)), np.float32) / 255.0


def fuse_at(y):
    """The fuselage at station y: half width to the chine's edge, half width of the round body."""
    t = np.asarray(FUSE, np.float32)[::-1]
    return np.interp(y, t[:, 0], t[:, 1]).astype(np.float32), np.interp(y, t[:, 0], t[:, 2]).astype(np.float32)


# ---------------------------------------------------------------- lettering and insignia
# Block capitals as the USAF stencilled them: strokes on a grid 4 wide and 6 high.
GLYPHS = {
    "U": [[(0, 6), (0, 1), (1, 0), (3, 0), (4, 1), (4, 6)]],
    "S": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 4), (1, 3), (3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "A": [[(0, 0), (0, 4.4), (1.2, 6), (2.8, 6), (4, 4.4), (4, 0)], [(0, 2.3), (4, 2.3)]],
    "I": [[(2, 0), (2, 6)]],
    "R": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)], [(2, 3), (4, 0)]],
    "F": [[(0, 0), (0, 6), (4, 6)], [(0, 3.2), (3, 3.2)]],
    "O": [[(1, 0), (3, 0), (4, 1), (4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0)]],
    "C": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0), (3, 0), (4, 1)]],
    "E": [[(4, 6), (0, 6), (0, 0), (4, 0)], [(0, 3.1), (3, 3.1)]],
    ".": [[(1.8, 0), (2.2, 0)]],
    " ": [],
    "1": [[(1, 5), (2, 6), (2, 0)], [(1, 0), (3, 0)]],
    "2": [[(0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (0, 0.9), (0, 0), (4, 0)]],
    "7": [[(0, 6), (4, 6), (4, 5), (1.5, 0)]],
    "9": [[(0, 1), (1, 0), (3, 0), (4, 1), (4, 5), (3, 6), (1, 6), (0, 5), (0, 3.6), (1, 2.6), (4, 2.6)]],
}
SS = 4  # the markings are drawn this many times too big and scaled down, for clean edges


class Markings:
    """What is painted over the black in one view: drawn in the picture's own
    plane, positions and sizes in metres."""

    def __init__(self, view):
        self.v = view
        self.im = Image.new("RGBA", (view.w * SS, view.h * SS), (0, 0, 0, 0))
        self.d = ImageDraw.Draw(self.im)

    def p(self, a, b):
        x, y = self.v.px(a, b)
        return x * SS, y * SS

    def text(self, s, a, b, height, colour):
        """Letters `height` m tall centred on (a, b), reading left to right in the picture."""
        cx, cy = self.p(a, b)
        u = height * PX * SS / 6.0
        pitch = 5.3 * u
        x0 = cx - (len(s) * pitch - 1.3 * u) / 2
        for k, ch in enumerate(s):
            for stroke in GLYPHS[ch]:
                pts = [(x0 + k * pitch + gx * u, cy + (3 - gy) * u) for gx, gy in stroke]
                self.d.line(pts, fill=colour + (255,), width=max(int(round(1.05 * u)), 1), joint="curve")
                for q in (pts[0], pts[-1]):  # square the ends
                    self.d.rectangle([q[0] - 0.5 * u, q[1] - 0.5 * u, q[0] + 0.5 * u, q[1] + 0.5 * u], fill=colour + (255,))

    def star(self, a, b, r):
        """The national insignia: star in a disc, a bar each side, the red stripe of 1947."""
        cx, cy = self.p(a, b)
        R = r * PX * SS
        e = R / 8
        d = self.d
        d.ellipse([cx - R - e, cy - R - e, cx + R + e, cy + R + e], fill=INSIGNIA_BLUE + (255,))
        top = cy - R * np.sin(np.radians(18))  # the bars' top edge: level with the star's arms
        d.rectangle([cx - 2 * R - e, top - e, cx + 2 * R + e, top + R / 2 + e], fill=INSIGNIA_BLUE + (255,))
        d.rectangle([cx - 2 * R, top, cx + 2 * R, top + R / 2], fill=INSIGNIA_WHITE + (255,))
        d.rectangle([cx - 2 * R, top + R / 6, cx + 2 * R, top + R / 3], fill=INSIGNIA_RED + (255,))
        d.ellipse([cx - R, cy - R, cx + R, cy + R], fill=INSIGNIA_BLUE + (255,))
        pts = []
        for k in range(10):
            ang = np.radians(-90 + 36 * k)
            rr = R if k % 2 == 0 else R * 0.382
            pts.append((cx + rr * np.cos(ang), cy + rr * np.sin(ang)))
        d.polygon(pts, fill=INSIGNIA_WHITE + (255,))

    def outline(self, a0, a1, b0, b1, colour, width):
        p0, p1 = self.p(a0, b0), self.p(a1, b1)
        self.d.rectangle([min(p0[0], p1[0]), min(p0[1], p1[1]), max(p0[0], p1[0]), max(p0[1], p1[1])], outline=colour + (255,), width=max(int(round(width * PX * SS)), 1))

    def rule(self, a0, b0, a1, b1, colour, width):
        self.d.line([self.p(a0, b0), self.p(a1, b1)], fill=colour + (255,), width=max(int(round(width * PX * SS)), 1))

    def streak(self, a, b, length, width, colour, alpha, drift=0.0):
        """A stain run back along the airflow from (a, b), fading out; `drift`: how far it wanders sideways."""
        n = 12
        for k in range(n):
            t0, t1 = k / n, (k + 1) / n
            p0, p1 = self.p(a + drift * t0 * t0, b - length * t0), self.p(a + drift * t1 * t1, b - length * t1)
            self.d.line([p0, p1], fill=colour + (int(alpha * (1 - t0) ** 1.5),), width=max(int(round(width * (1 + 1.5 * t0) * PX * SS)), 1))

    def dashes(self, a, b, width, rows, height, colour, seed):
        """Small stencilling, too small to read: rows of dashes from (a, b), letters `height` m tall."""
        rng = np.random.default_rng(seed)
        cx, cy = self.p(a, b)
        u = PX * SS
        for r in range(rows):
            x, end = cx, cx + width * u * rng.uniform(0.6, 1.0)
            y = cy + r * height * 2.1 * u
            while x < end:
                w = rng.uniform(1.5, 5.0) * height * u
                self.d.line([(x, y), (min(x + w, end), y)], fill=colour + (255,), width=max(int(round(height * u)), 1))
                x += w + rng.uniform(0.6, 1.4) * height * u

    def over(self, rgb, where, soft=0.0, strength=1.0):
        im = self.im.filter(ImageFilter.GaussianBlur(soft * SS)) if soft else self.im
        a = np.asarray(im.resize((self.v.w, self.v.h), Image.LANCZOS), np.float32)
        return mix(rgb, a[..., :3], a[..., 3] / 255.0 * where * strength)


# ---------------------------------------------------------------- skin panels (as f4_paint.py)
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
    return (np.mod(along, RIVET_PITCH) < 0.036).astype(np.float32)


class Detail:
    """What is drawn into one view's paint: seams, fastener rows, stencilled
    lettering, the inside of access panels, and two numbers 0..1 for each
    skin panel (how its paint has aged, and how it takes the light)."""

    def __init__(self, v):
        self.seam, self.rivet, self.stencil, self.access = (np.zeros(v.ok.shape, np.float32) for _ in range(4))
        self.tone = np.full(v.ok.shape, 0.5, np.float32)
        self.sheen = np.full(v.ok.shape, 0.5, np.float32)


def panels(out, mask, a, b, breaks, b0, b1, pitch, seed, ma=1.0, mb=1.0):
    """Cut the surface under `mask` into skin panels. Seams run its whole length
    where `a` crosses `breaks`; between each pair of them others cross, about
    `pitch` apart in `b` (from b0 to b1), at places of their own in each bay.
    ma, mb: metres per unit of a and b. Every panel gets fasteners inside its
    edges, the wider ones a second row down the middle, some an access panel
    with its ring of fasteners, some a block of stencilling."""
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
    rivet = np.maximum(line(da - RIVET_INSET, 0.022) * along_b, line(db - RIVET_INSET, 0.022) * along_a)
    rivet = np.maximum(rivet, line(ha - da, 0.022) * along_b * (ha > 0.34) * (ha < 9) * (db > 0.05))
    rivet = np.maximum(rivet, line(hb - db, 0.022) * along_a * (hb > 0.34) * (hb < 9) * (da > 0.05))
    h = lambda k: hashf(i, j, seed=seed * 16 + k)
    # An access panel: a rectangle with round corners, or an oval.
    ea, eb = np.minimum(ha * (0.35 + 0.30 * h(2)), 0.48), np.minimum(hb * (0.35 + 0.30 * h(3)), 0.48)
    has = (h(1) > 0.70) & (ha > 0.20) & (hb > 0.20) & (ha < 9) & (hb < 9)
    r = np.minimum(ea, eb) * np.where(h(4) > 0.7, 1.0, 0.3)
    qa, qb = np.abs(a - (lo_a + hi_a) / 2) * ma - ea + r, np.abs(b - (lo_b + hi_b) / 2) * mb - eb + r
    d = np.hypot(np.maximum(qa, 0), np.maximum(qb, 0)) + np.minimum(np.maximum(qa, qb), 0) - r
    seam = np.maximum(seam, 0.7 * line(d, 0.020) * has)
    rivet = np.maximum(rivet, line(d + 0.045, 0.022) * np.where(qa > qb, along_b, along_a) * has)
    inside = (d < 0) & has
    # Stencilling: a few rows of dashes in a corner of the panel.
    la, lb = (a - lo_a) * ma - 0.10, (b - lo_b) * mb - 0.09
    rows, wide = 1 + (h(6) * 3).astype(np.int32), 0.16 + 0.30 * h(7)
    st = (h(5) < 0.22) & ~inside & (la > 0) & (la < wide) & (lb > 0) & (lb < rows * 0.05) & (np.mod(lb, 0.05) < 0.024) & (ha < 9) & (hb < 9)
    st &= hashf(np.floor(la / 0.045), np.floor(lb / 0.05), i, j, seed=seed) > 0.32
    out.seam[idx] = np.maximum(out.seam[idx], seam)
    out.rivet[idx] = np.maximum(out.rivet[idx], rivet)
    out.tone[idx] = h(0)
    out.sheen[idx] = h(9)
    out.access[idx] = inside
    out.stencil[idx] = st


def box_d(a, b, box, r=0.06):
    """Distance outside a round-cornered rectangle (a from, to; b from, to); negative inside."""
    a0, a1, b0, b1 = box
    qa = np.abs(a - (a0 + a1) / 2) - abs(a1 - a0) / 2 + r
    qb = np.abs(b - (b0 + b1) / 2) - abs(b1 - b0) / 2 + r
    return np.hypot(np.maximum(qa, 0), np.maximum(qb, 0)) + np.minimum(np.maximum(qa, qb), 0) - r


def boxes(a, b, list_, w=0.020, r=0.06):
    """The outlines of round-cornered rectangles."""
    out = np.zeros(a.shape, np.float32)
    for box in list_:
        out = np.maximum(out, line(box_d(a, b, box, r), w))
    return out


def smear(src, axis, step, decay, ok):
    """Drag `src` across the picture along `axis` (step 1 or -1), dying away by
    `decay` (a picture of its own) each pixel: what runs back from a leak."""
    out, dec, okm = np.moveaxis(src.copy(), axis, 0), np.moveaxis(decay, axis, 0), np.moveaxis(ok, axis, 0)
    for i in (range(1, len(out)) if step > 0 else range(len(out) - 2, -1, -1)):
        out[i] = np.maximum(out[i], out[i - step] * dec[i]) * okm[i]
    return np.moveaxis(out, 0, axis)


def wedges(d, along, depth, width):
    """A row of triangles let into an edge, their bases on it: `d` the distance
    in from the edge, `along` the distance along it. Returns (inside a wedge,
    on a wedge's outline, in the row at all)."""
    t = np.mod(along / width, 1.0)
    reach = depth * (1.0 - np.abs(2.0 * t - 1.0))
    row = (d >= 0) & (d < depth)
    return ((d < reach) & row).astype(np.float32), line(d - reach, 0.022) * row, row.astype(np.float32)


# ---------------------------------------------------------------- painting a view
def paint(v):
    sides = v.name in ("side", "right")
    rng = np.random.default_rng(("top", "bottom", "side", "right").index(v.name) + 1)

    def grain(r):
        """Fine random grain, blurred over r pixels: unit variance."""
        g = blur(rng.random(v.ok.shape).astype(np.float32), r)
        return (g - g.mean()) / (g.std() + 1e-6)

    body = v.piece("fuselage", "nose section", "tail cone", "radome", "canopy frame")
    wing = v.piece("wing", "elevon")
    elevon = v.piece("elevon")
    nac = v.piece("nacelle", "spike")
    fin = v.piece("fin")
    ax = np.abs(v.x)
    above, below, steep = v.nz > 0.45, v.nz < -0.45, np.abs(v.nx) > 0.45
    # (Everything that is not glass, a lamp or bare metal takes the paint: the wells' doors shut over them and show what is painted there.)
    not_skin = v.mat("glass", "frame", "inlet", "exhaust", "nozzle_in", "burner", "steel", "chrome", "strobe", "nav_red", "nav_green", "tail_white")
    skin = v.ok & ~not_skin
    hw, rb = fuse_at(v.y)
    flat = body & (ax > rb + 0.02) & (np.abs(v.nz) > 0.6)       # the chines: the shelf either side of the round body
    # Where a point is on its surface. The wing: how far behind its leading edge and ahead of its trailing edge.
    le = WING_LE[0] - WING_LE[1] * ax
    te = np.where(ax < WING_TE[1], WING_TE[0], WING_TE[0] + (WING_TE[2] - WING_TE[0]) * (ax - WING_TE[1]) / (WING_TE[3] - WING_TE[1]))
    d_le, d_te = (le - v.y) * 0.5, v.y - te                    # (the edge is swept 60 degrees: half the distance along the chord)
    # A nacelle: the angle round its centreline. A fin: how far up it, and back along its chord.
    r_nac = np.hypot(ax - NACELLE[0], v.z - NACELLE[1])
    a_nac = np.arctan2(v.z - NACELLE[1], ax - NACELLE[0])
    fu = (v.z - FIN[0]) / FIN[1]
    fin_c = FIN[4] - FIN[3] * fu
    ff = (FIN[2] - FIN[3] * fu - v.y) / np.maximum(fin_c, 0.5)

    # ---- the structure: skin panels, fasteners, doors
    D = Detail(v)
    pan = v.ok & skin
    round_ = body & ~flat
    girth = np.maximum(np.hypot(v.x, v.z - WING_Z), 0.15)
    panels(D, round_ & pan, v.y, np.arctan2(v.z - WING_Z, ax), FRAMES, -1.6, 1.6, PANEL_PITCH[0], PANEL_SEED, 1.0, girth)
    for n, face in enumerate((v.nz >= 0, v.nz < 0)):   # the upper and lower skins are cut differently
        panels(D, flat & pan & face, v.y, ax, FRAMES, 0.0, 1.9, PANEL_PITCH[1], PANEL_SEED + 1 + n)
        panels(D, wing & pan & face, v.y, ax, SPARS, 0.6, 8.6, PANEL_PITCH[2], PANEL_SEED + 3 + n)
    panels(D, nac & pan, v.y, a_nac, NACELLE_FRAMES, -3.2, 3.2, PANEL_PITCH[3], PANEL_SEED + 5, 1.0, np.maximum(r_nac, 0.2))
    panels(D, fin & pan, ff, v.z, FIN_SPARS, 0.5, 3.5, PANEL_PITCH[4], PANEL_SEED + 6, np.maximum(fin_c, 0.5), 1.0)
    seam, rivet = D.seam, D.rivet

    # Where two pieces of one surface meet (the elevons, the fins on their stubs, the doors).
    joint = np.zeros(v.ok.shape, np.float32)
    depth = v.z if not sides else v.x
    for axis in (0, 1):
        d = (np.roll(v.piece_id, 1, axis) != v.piece_id) & v.ok & np.roll(v.ok, 1, axis) & (np.abs(np.roll(depth, 1, axis) - depth) < 0.10)
        joint = np.maximum(joint, d.astype(np.float32))
    seam = np.maximum(seam, np.clip(blur(joint, 0.6) * 1.6, 0, 1))
    # Where the round body rises out of the chines; the nose's joint; panels and doors with places of their own.
    seam = np.maximum(seam, line(ax - rb - 0.02, 0.022) * body * (v.y > 1.2) * (v.y < 15.6) * (np.abs(v.nz) > 0.3))
    seam = np.maximum(seam, lines_at(v.y, (14.0,), 0.03) * body)
    seam = np.maximum(seam, boxes(ax, v.y, TOP_PANELS + (RECEPTACLE,)) * (body | wing) * above)
    seam = np.maximum(seam, line(ax, 0.02) * above * body * (v.y < RECEPTACLE[2]) * (v.y > RECEPTACLE[3]))   # (the receptacle's two doors meet on the centreline)
    seam = np.maximum(seam, boxes(ax, v.y, BELLY_PANELS) * (body | wing) * below)
    seam = np.maximum(seam, boxes(v.y, v.z, SIDE_PANELS) * body * steep)
    # The nacelles' bypass doors and bleed slots: bands of louvres round them.
    louvres = np.zeros(v.ok.shape, np.float32)
    for (y0, y1), n in ((BYPASS_DOORS, 14.0), (BLEED_SLOTS, 9.0)):
        band = nac * skin * (v.y < y0) * (v.y > y1)
        louvres = np.maximum(louvres, band * (np.mod(a_nac * n / np.pi, 1.0) < 0.55) * (np.abs(v.y - (y0 + y1) / 2) < (y0 - y1) / 2 - 0.04))
        seam = np.maximum(seam, lines_at(v.y, (y0, y1), 0.02) * nac)
    # The fins: the line they turn on, and the close-set ribs' fasteners.
    rivet = np.maximum(rivet, (np.mod(v.z, 0.19) < 0.026) * dots(v.y) * fin * (ff > 0.12) * (ff < 0.9))

    # The wedges of radar-absorbing plastic along the chines and every edge.
    wd = np.zeros(v.ok.shape, np.float32)      # inside a wedge
    wl = np.zeros(v.ok.shape, np.float32)      # on its outline
    wz = np.zeros(v.ok.shape, np.float32)      # in the row
    for where, d, along, (deep, wide) in ((flat * (v.y > 1.3) * (v.y < 15.2), hw - ax, v.y, WEDGE_CHINE), (wing & ~elevon, d_le, ax, WEDGE_WING),
                                         (elevon, d_te, ax, WEDGE_ELEVON), (fin * (fu > 0.2), ff * fin_c * 0.75, v.z, WEDGE_FIN)):
        a, b, c = wedges(d, along, deep, wide)
        w = np.asarray(where, np.float32) * skin
        wd, wl, wz = np.maximum(wd, a * w), np.maximum(wl, b * w), np.maximum(wz, c * w)
    seam = np.maximum(seam * (1 - wz), 0.85 * wl)
    rivet *= 1 - wz
    seam = np.clip(seam, 0, 1) * ~not_skin
    rivet = np.clip(rivet, 0, 1) * ~not_skin * (1 - seam)

    # The wing skins' corrugations, fore and aft: inboard of the nacelles and on the outer panels.
    corrugated = wing * skin * (1 - wz) * (d_le > WEDGE_WING[0] + 0.05) * (((ax > 1.80) & (ax < 3.20)) | (ax > 5.05)) * (np.abs(v.nz) > 0.5)
    ridge = np.cos(2 * np.pi * ax / CORRUGATION)

    # ---- the paint
    rgb = np.empty(v.ok.shape + (3,), np.float32)
    rgb[:] = BLACK
    # Every panel its own black: some browner, some bluer, some lighter (repainted, or baked longer).
    rgb *= (1.0 + PANEL_TONE * (D.tone - 0.5))[..., None]
    rgb = mix(rgb, np.asarray(BLACK, np.float32) * (1.08, 1.0, 0.86), 0.5 * smoothstep(0.55, 1.0, D.sheen))
    rgb = mix(rgb, np.asarray(BLACK, np.float32) * (0.90, 0.97, 1.16), 0.5 * smoothstep(0.45, 0.0, D.sheen))
    # Baked toward grey-blue by the heat: the nose and the chines' edges, the leading edges, the nacelles' lips and their hot back halves, in patches.
    hot = np.maximum.reduce([smoothstep(12.5, 16.5, v.y) * body, 0.8 * smoothstep(0.50, 0.0, hw - ax) * flat, 0.9 * smoothstep(0.9, 0.0, d_le) * wing,
                             smoothstep(1.6, 2.8, v.y) * nac, 0.75 * smoothstep(-5.5, -9.5, v.y) * nac, 0.7 * smoothstep(0.30, 0.0, ff) * fin])
    patch = noise(v.pos, (1.3, 1.9, 1.1), 11)
    rgb = mix(rgb, HEATED, np.clip(hot * (0.50 + 0.22 * patch) + 0.16 * smoothstep(0.4, 1.8, patch) + 0.12 * (D.tone > 0.90), 0, 0.85))
    # The dielectric parts: the nose, and the wedges in their rows.
    rgb = mix(rgb, DIELECTRIC, 0.75 * v.mat("radome"))
    rgb = mix(rgb, DIELECTRIC, 0.55 * wz)
    rgb *= (1.0 - 0.24 * wd + 0.10 * wz * (1 - wd))[..., None]
    # Corrugations: light on the crests, dark in the troughs.
    rgb *= (1.0 + 0.17 * ridge * corrugated)[..., None]

    # Bare metal: the back of each nacelle, shading from the paint; the ejector flaps. Tinted in bands by the heat.
    metal = np.maximum(nac * smoothstep(-9.75, -10.25, v.y), v.mat("exhaust", "nozzle_in", "burner").astype(np.float32))
    streaky = noise(v.pos, (0.16, 2.6, 0.16), 31)
    band = (-10.2 - v.y) / 2.2 + 0.10 * streaky + 0.06 * noise(v.pos, (0.5, 0.5, 0.5), 32)
    m = np.empty_like(rgb)
    m[:] = METAL
    m = mix(m, STRAW, 0.75 * smoothstep(0.05, 0.30, band) * (1 - smoothstep(0.42, 0.60, band)))
    m = mix(m, PURPLE, 0.60 * smoothstep(0.38, 0.55, band) * (1 - smoothstep(0.62, 0.78, band)))
    m = mix(m, BLUED, 0.70 * smoothstep(0.60, 0.80, band))
    m = mix(m, SOOT, 0.75 * smoothstep(0.86, 1.0, band))
    m *= (1.0 + 0.09 * streaky)[..., None]
    m = mix(m, (62, 60, 60), 0.55 * v.mat("exhaust", "nozzle_in", "burner"))   # (the flaps themselves are not painted: only their edge shows)
    rgb = mix(rgb, m, metal)
    fresh = rgb.copy()

    # Markings: few, and small.
    k = Markings(v)
    if v.name == "top":
        k.star(*WING_STAR_TOP)
        for s in (-1, 1):
            for x0, x1, y0, y1 in WALK_BOXES:
                k.outline(s * x0, s * x1, y0, y1, RED, WALK_LINE)
                k.dashes(s * (x0 + x1) / 2 - 0.22, y0 - 0.20, 0.44, 1, 0.034, RED, int(x0 * 10))   # "NO STEP"
            k.rule(s * WALK_SPINE[0], WALK_SPINE[1], s * WALK_SPINE[0], WALK_SPINE[2], RED, WALK_LINE)
    elif v.name == "bottom":
        k.star(*WING_STAR_BOTTOM)
    else:
        k.star(*FUSELAGE_STAR)
        k.text(LETTERING, *FUSELAGE_LETTERING, RED)
        k.text(SERIAL, *FIN_SERIAL, RED)
        k.dashes(12.9, 0.02, 0.44, 2, 0.028, RED, 3)     # the rescue instructions under the canopy
    on = skin.astype(np.float32) * (1 - wz)
    rgb = k.over(rgb, on * (body | wing | fin | nac), strength=0.88)

    # ---- weathering
    # Chalked and greyed in patches, more on top; the mottle of a paint that was never even.
    lum = rgb @ np.array([0.30, 0.59, 0.11], np.float32)
    chalk = smoothstep(0.2, 1.6, noise(v.pos, (0.55, 0.8, 0.55), 12) + 0.5 * patch) * (0.10 + 0.14 * np.clip(v.nz, 0, 1)) * (1 - metal)
    rgb = mix(rgb, lum[..., None] * 0.5 + rgb * 0.5 + 16.0, chalk)
    rgb *= (1.0 + 0.07 * noise(v.pos, (0.9, 1.3, 0.9), 13) + 0.05 * noise(v.pos, (0.22, 0.22, 0.22), 14))[..., None]
    rgb *= (1.0 + 0.055 * grain(0.8) + 0.05 * grain(2.5) + 0.04 * grain(8.0))[..., None]
    # Panels repainted since: blacker and cleaner.
    rgb = mix(rgb, fresh * 0.80, 0.8 * ((D.tone < 0.08) | ((D.access > 0) & (D.tone > 0.6))) * (1 - metal))
    # Blotches where fluid was spilt and wiped; streaks dragged aft by the air; rain run down the sides.
    rgb *= (1.0 - 0.14 * smoothstep(0.9, 1.6, noise(v.pos, (0.40, 0.50, 0.40), 24)) + 0.10 * smoothstep(1.0, 1.9, noise(v.pos, (0.6, 0.8, 0.6), 25)))[..., None]
    streaks = noise(v.pos, (0.12, 3.4, 0.12), 15) + 0.5 * noise(v.pos, (1.6, 4.5, 1.6), 16)
    rgb *= (1.0 - 0.12 * smoothstep(0.5, 1.8, streaks) + 0.10 * smoothstep(0.6, 1.9, -streaks))[..., None]
    rain = noise(v.pos, (7.0, 0.10, 2.6), 18) + 0.6 * noise(v.pos, (7.0, 1.0, 3.0), 19)
    rgb *= (1.0 - (0.12 * smoothstep(0.7, 1.9, rain) - 0.09 * smoothstep(0.7, 1.9, -rain)) * steep * (body | fin | nac))[..., None]
    # Boots: the walk areas scuffed grey.
    if v.name == "top":
        scuff = np.zeros(v.ok.shape, np.float32)
        for box in WALK_BOXES:
            scuff = np.maximum(scuff, (box_d(ax, v.y, box) < -0.04) * wing)
        scuff = np.maximum(scuff, (ax < WALK_SPINE[0] - 0.03) * body * above * (v.y < WALK_SPINE[1]) * (v.y > WALK_SPINE[2]))
        rgb = mix(rgb, (74, 74, 76), np.clip(scuff * (0.16 + 0.10 * noise(v.pos, (0.3, 0.5, 0.3), 20) + 0.07 * grain(1.5)), 0, 0.4))

    # What leaks. On top and on the sides, dirt dragged aft from seams and louvres.
    leaky = noise(v.pos, (0.6, 0.6, 0.6), 17)
    leak = np.clip(seam * (leaky > 0.45) + louvres, 0, 1)
    reach = np.clip(0.93 + 0.04 * noise(v.pos, (0.4, 0.4, 0.4), 23), 0.80, 0.985)
    axis, step = (1, 1 if v.name == "side" else -1) if sides else (0, 1)   # which way aft is in this picture
    trail = blur(np.clip(smear(leak, axis, step, reach, v.ok) - leak, 0, 1), 0.7) * ~not_skin
    rgb *= (1.0 - 0.30 * np.clip(trail, 0, 1))[..., None]
    # Underneath, the fuel: it weeps from every tank seam on the ground and runs aft in flight, wet and dark where fresh, a dull tan film where it has dried.
    if v.name == "bottom":
        tank = (body | wing) & (v.y < 6.3) & (v.y > -10.0)
        weep = seam * tank * (leaky > -0.3) + 0.9 * lines_at(v.y, TANK_SEAMS, 0.05) * body * (leaky > -0.9)
        far = np.clip(reach + 0.045, 0, 0.993)
        run = blur(smear(np.clip(weep, 0, 1), 0, 1, far, v.ok), 0.8) * ~not_skin
        rgb = mix(rgb, FUEL_DRY, np.clip(0.34 * run * (0.6 + 0.4 * noise(v.pos, (0.10, 1.5, 0.10), 26)), 0, 0.5))
        rgb = mix(rgb, FUEL_WET, np.clip(0.75 * blur(smear(np.clip(weep * (leaky > 0.5), 0, 1), 0, 1, reach, v.ok), 0.6), 0, 0.8) * ~not_skin)
        k = Markings(v)
        srng = np.random.default_rng(7)
        for _ in range(90):
            a = srng.choice([-1, 1]) * abs(srng.normal(0.0, 1.6))
            dry = srng.random() < 0.6
            k.streak(a, srng.uniform(-9.0, 6.0), srng.uniform(0.8, 3.2), srng.uniform(0.02, 0.05), FUEL_DRY if dry else FUEL_WET, srng.uniform(50, 120) if dry else srng.uniform(110, 200), srng.normal(0, 0.12))
        for s in (-1, 1):  # from the wing tanks inboard of the nacelles, and the nacelles' drains
            for _ in range(22):
                k.streak(s * srng.uniform(1.8, 3.2), srng.uniform(-9.5, -1.0), srng.uniform(0.6, 2.4), srng.uniform(0.02, 0.045), FUEL_DRY, srng.uniform(50, 120), srng.normal(0, 0.10))
            for _ in range(10):
                k.streak(s * (NACELLE[0] + srng.normal(0, 0.25)), srng.uniform(-8.5, 1.0), srng.uniform(0.6, 2.0), srng.uniform(0.02, 0.04), FUEL_WET, srng.uniform(90, 170), srng.normal(0, 0.06))
        rgb = k.over(rgb, on * ~not_skin, soft=1.0)

    # The paint worn through to the titanium: the nose's tip, the edges, round seams and opened panels.
    worn = np.maximum.reduce([wing * smoothstep(0.07, 0.015, d_le), fin * smoothstep(0.03, 0.006, ff), flat * smoothstep(0.06, 0.015, hw - ax) * (v.y > 1.3), v.piece("nacelle") * smoothstep(2.60, 2.78, v.y) * (v.y < 2.82)])
    g = grain(0.9)
    chip = worn * (g > 0.0) + (blur(seam, 1.0) > 0.2) * (g > 1.7) + (blur(D.access, 1.0) > 0.1) * (blur(D.access, 1.0) < 0.9) * (g > 1.0)
    chip = np.clip(chip, 0, 1) * (1 - metal) * ~not_skin
    rgb = mix(rgb, TITANIUM, (0.30 + 0.45 * worn) * chip)

    # Fittings: the louvres' slats in shadow.
    rgb *= (1.0 - 0.50 * louvres)[..., None]
    # Seams, fasteners and stencilling. A seam is a dark line with dirt gathered
    # along it and the paint worn light on its edge (the game takes dark lines
    # to lie below the skin round them). On black the fasteners show light: the
    # paint rubs off their heads first.
    grime = blur(seam, 2.5) * (0.6 + 0.4 * noise(v.pos, (0.5, 0.9, 0.5), 21))
    rgb *= (1.0 - 0.26 * np.clip(grime * 2.2, 0, 1))[..., None]
    rim = np.clip(np.roll(seam, (1, 1), (0, 1)) - seam, 0, 1)
    rgb *= (1.0 + 0.30 * rim)[..., None]
    rgb = mix(rgb, RED, 0.40 * (D.stencil > 0) * (1 - metal) * (1 - wz) * (hashf(np.floor(v.y / 0.9), np.floor(ax / 0.9), seed=5) > 0.8))
    rgb = mix(rgb, (104, 104, 102), 0.28 * (D.stencil > 0) * (1 - metal) * (1 - wz) * (hashf(np.floor(v.y / 0.9), np.floor(ax / 0.9), seed=5) <= 0.5))
    bright = hashf(np.floor(v.y / RIVET_PITCH), np.floor(ax / RIVET_PITCH), np.floor(v.z / RIVET_PITCH), seed=9)
    rgb *= (1.0 + rivet * np.where(bright > 0.45, 0.55, -0.40))[..., None]
    rgb *= (1.0 - 0.60 * seam)[..., None]

    # Soot behind the jet pipes: the elevons and the trailing edge either side of each nozzle.
    soot = smoothstep(-9.6, -11.4, v.y) * np.exp(-(((ax - NACELLE[0]) / 1.5) ** 2)) * (0.7 + 0.3 * streaky) * (wing | nac)
    rgb = mix(rgb, SOOT, np.clip(0.55 * soot, 0, 0.7) * (1 - 0.5 * metal))

    # What is not painted skin (the pictures are not projected onto it, but its edge shows).
    rgb = mix(rgb, GLASS, v.mat("glass").astype(np.float32))
    rgb = mix(rgb, FRAME, v.mat("frame", "inlet").astype(np.float32))
    rgb = mix(rgb, (150, 150, 152), v.mat("steel", "chrome").astype(np.float32))
    # The guide's shading, and shadow gathered where one piece meets another, so the shapes read.
    rgb *= (np.clip(v.shade, 0.5, 1.3) ** 0.45)[..., None]
    rgb *= (1.0 - 0.25 * np.clip(blur(joint, 4.0) * 3.0, 0, 1))[..., None]
    rgb = np.clip(rgb, 6, 232)  # (pure white is the background)
    rgb[~v.ok] = 255
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
    # Painted: inside the silhouette (less a pixel or two, where the picture's
    # own edge may fall short of the model's) and not the white of the background.
    inside = np.asarray(mask.filter(ImageFilter.MinFilter(5))) > 128
    ok = inside & (rgb.min(axis=2) < 236)
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
        paint(View(name))
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
