# Paints the Phantom and bakes its paint into assets/f4_paint.jpg (plain Python, with Pillow and NumPy):
#
#   python assets/blender/f4_paint.py [--bake-only] [<dir of pictures>] [<out.jpg>]
#
# The paint is four pictures of the aircraft: from straight above, from
# straight below, from its left side and from its right side
# (assets/blender/f4_paint/top.png, bottom.png, side.png, right.png). The game
# projects them back onto the skin (plane.wgsl, `hornet_paint`, layers 3 to
# 6), so they must line up with the model: they are painted over the guides that
#
#   blender -b --factory-startup --python assets/blender/f4.py -- <scratch dir> --guides assets/blender/f4_paint
#
# renders (<view>_guide.png, the silhouette in <view>_mask.png, and
# <view>_geo.npz: how deep into the picture the model is at each pixel, its
# normal there, and which piece and material it is). Where the Hornet's
# pictures were painted by an image model, these are painted here, by rule,
# from that geometry: a pixel's colour comes from where on the aircraft it
# is, so the camouflage and the panel lines meet across the views. The scheme
# is the USAF's Southeast Asia camouflage of the Vietnam years: tan and two
# greens over light grey, a black radome, bare metal round the jet pipes. Its
# tail code and serial are made up.
#
# Over the scheme goes what makes it a used aircraft seen close: the skin cut
# into panels along its frames, spars and ribs, a row of fasteners inside
# every seam, access panels and stencilling, the doors, vents and fillers
# that have places of their own; then the weather: paint chalked and faded
# panel by panel, repainted patches, chips on the leading edges, rain and
# airflow streaks, dirt and oil dragged aft from what leaks, gun gas and jet
# soot. The game takes the darker lines to lie below the skin round them
# (plane.wgsl), so the seams and fasteners are relief there, not only ink.
# (Freshly rendered guides and masks are packed, grey and one-bit, on the
# first run here: they are a third of the size.)
#
# The pictures can be touched up or replaced in any paint program (keep the
# background white): --bake-only then bakes them as they stand, without
# painting them again.
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
SRC = ARGS[0] if ARGS else os.path.join(HERE, "f4_paint")
OUT = ARGS[1] if len(ARGS) > 1 else os.path.join(os.path.dirname(HERE), "f4_paint.jpg")

# The rectangles the pictures cover (GUIDE_* in f4.py): metres across and
# fore and aft, the station and height at their middle, pixels per metre.
SPAN, LEN, MID_Y, MID_Z = 13.2, 19.8, 0.4, 1.0
PX = 1536 / LEN

# ---------------------------------------------------------------- the scheme (sRGB; metres, Blender frame: x right, y forward, z up)
TAN = (166, 131, 96)         # FS 30219
GREEN = (80, 93, 60)         # FS 34102
DARK_GREEN = (52, 64, 50)    # FS 34079
GREY = (197, 199, 189)       # FS 36622, underneath
BLACK = (27, 27, 29)         # radome, anti-glare panel
METAL = (128, 124, 117)      # bare titanium and steel round the jet pipes
STRAW, BLUED, SOOT = (150, 122, 80), (84, 86, 112), (30, 28, 27)
GLASS, FRAME = (34, 38, 40), (26, 27, 28)
INSIGNIA_BLUE, INSIGNIA_WHITE, INSIGNIA_RED = (27, 40, 68), (226, 226, 220), (150, 34, 38)
WARNING_RED = (168, 36, 34)
WALKWAY = (48, 48, 46)

CAMO_SEED = 4               # another number, another aircraft's pattern
CAMO_SIZE = (4.2, 5.6, 3.0)  # how big the blotches are across, fore and aft, and up
DEMARCATION = -0.50          # height where the grey starts on the fuselage, and how much it waves
DEMARCATION_WAVE = 0.07

TAIL_CODE, SERIAL = "XF", "65-999"            # (made up)
FIN_CODE = (-6.90, 1.40, 0.50)                # station, height of its middle, letter height (clear of the formation light strip)
FIN_SERIAL = (-7.60, 2.42, 0.16)
FUSELAGE_STAR = (1.30, 0.02, 0.30)            # station, height, the star's radius
WING_STAR_TOP = (-3.00, -1.95, 0.36)          # x, station, radius: upper left wing
WING_STAR_BOTTOM = (3.30, -1.95, 0.40)        # lower right wing
WING_USAF_TOP = (3.00, -2.00, 0.62)           # x, station, letter height: upper right wing
WING_USAF_BOTTOM = (-3.50, -2.00, 0.66)       # lower left wing
INTAKE_STRIPE, TURBINE_STRIPE = 5.05, -2.45   # stations of the red warning stripes round the fuselage
SEAT_TRIANGLES = ((5.50, 0.26), (3.80, 0.40))  # station, height: under each cockpit's sill
WALKWAY_BOX = (1.42, 2.15, 1.00, -2.30)       # x from, to; station from, to (each wing root)

# ---------------------------------------------------------------- the structure under the paint
# What the panel lines and fastener rows follow. Each surface is cut into skin
# panels: seams that run its whole length (the fuselage's frames by station;
# the spars of the wing and tail by chord fraction), and between each pair of
# them the seams that cross, at places of their own in every bay (PANEL_SEED:
# another number, another aircraft's panels), so no two bays are cut alike.
FRAMES = (9.30, 8.90, 8.50, 7.90, 7.55, 7.20, 6.85, 6.50, 6.20, 5.80, 5.40, 5.00, 4.60, 4.20, 3.80, 3.40, 3.00, 2.60, 2.20, 1.80, 1.30, 0.80, 0.30,
          -0.10, -0.50, -0.95, -1.40, -1.80, -2.15, -2.45, -2.80, -3.20, -3.70, -4.05, -4.50, -5.00, -5.50, -6.00, -6.50, -7.00, -7.50, -8.00, -8.50)
WING_SPARS, WING_FOLD = (0.12, 0.25, 0.44, 0.63, 0.82), 4.05
STAB_SPARS = (0.12, 0.35, 0.62, 0.85)
FIN_SPARS, FIN_CAP = (0.10, 0.28, 0.52, 0.78), 2.72   # (the rudder hinges at 0.78; the fin's cap is a dielectric panel)
PANEL_SEED = 21
# How far apart the crossing seams are: round the fuselage (m of girth), along the wing, the stabilator and up the fin (m).
PANEL_PITCH = (0.46, 0.58, 0.40, 0.36)
RIVET_PITCH, RIVET_INSET = 0.052, 0.030   # fasteners along every seam: 4 pixels apart, this far in from the seam

# Things with a place of their own (m, Blender frame). On the fuselage's sides:
# station from, to; height from, to. Underneath and on top: |x| from, to;
# station from, to.
SIDE_PANELS = ((7.75, 7.35, -0.50, -0.20), (6.95, 6.60, -0.45, -0.05), (6.05, 5.45, -0.30, 0.10), (4.95, 4.55, -0.55, -0.15), (3.10, 2.30, -0.45, 0.05),
               (1.60, 1.00, -0.50, -0.25), (0.40, -0.30, 0.20, 0.50), (-0.80, -1.50, -0.45, 0.00), (-2.00, -2.35, 0.25, 0.60), (-2.75, -3.30, -0.50, 0.10),
               (-4.40, -4.90, 0.50, 0.80), (-5.60, -6.20, 0.50, 0.78), (-7.00, -7.50, 0.55, 0.78))
BELLY_PANELS = ((0.08, 0.34, 8.10, 6.30), (0.0, 0.25, 5.60, 4.90), (0.30, 0.80, 3.60, 2.70), (2.70, 3.30, -0.60, -1.30), (3.00, 3.80, -1.90, -2.60), (4.30, 4.90, -2.20, -2.90),
                (1.50, 2.30, 1.10, 0.20), (0.0, 0.12, 1.20, -0.60), (1.95, 2.30, 0.55, -1.00), (3.15, 3.40, -1.20, -2.20))
TOP_PANELS = ((0.0, 0.20, 2.10, 0.90), (0.0, 0.22, -0.40, -1.60), (2.70, 3.50, 0.10, -0.60), (4.30, 4.95, -2.30, -2.95), (1.45, 2.40, 1.20, 0.40))
GUN_BAY = (0.05, 0.40, 7.85, 6.25)                               # the gun bay's doors, either side of the fairing under the nose
GUN_MUZZLE = 8.37                                                # station of the muzzle: gun gas soots the belly behind it
SPARROW_WELLS = ((0.86, 1.08, 3.90, 0.30), (0.50, 0.72, -0.30, -3.50))  # the four missile recesses under the fuselage
ENGINE_DOORS = ((0.76, 1.26, 0.20, -1.35), (0.76, 1.22, -1.40, -3.05))  # engine bay doors: the oil and hydraulic leaks start here
AUX_AIR_DOORS = (0.08, 0.42, -0.95, -1.55)                       # auxiliary air doors on the belly
RAMP_HOLES = (4.90, 4.05)                                        # the intake ramps' boundary-layer bleed holes (stations)
BLEED_LOUVRES = (3.30, 2.70)                                     # and the louvres the bled air leaves by, on top of and under each duct
SPOILERS = (1.90, 3.90, 0.63, 0.80)                              # upper wing: span stations, chord fractions
WHEEL_BULGE = (1.988, -0.80, 0.47)                               # the bulge over each main wheel: x, station, radius
FILLERS = ((0.0, 1.55, 0.065), (0.0, 0.15, 0.065), (0.0, -1.35, 0.065), (2.55, -0.95, 0.055), (3.45, -1.75, 0.055))  # fuel fillers on top: |x|, station, radius
RECEPTACLE = (0.0, 0.13, 0.95, 0.20)                             # the air refuelling receptacle's door on the spine
RESCUE = (5.78, 0.00)                                            # station, height of the rescue arrow under the front canopy
DATA_BLOCK = (5.50, -0.28)                                       # and of the stencilled data block (left side only, clear of the nose art)
CHIP = (150, 152, 149)                                           # paint chipped to the metal, and to its primer
PRIMER = (120, 126, 78)
RESCUE_YELLOW = (190, 160, 40)
FUEL_RED = (150, 40, 36)
GRIME = (44, 38, 30)
NOSE_ART = (7.82, -0.66)                 # the nose art: station of its forward edge, height of its bottom (left side only)
NOSE_ART_NAME = "SMOKIN' SUE"             # (made up: for the trail a pair of J79s leaves)
NOSE_ART_MISSIONS = 14                   # bombs painted for sorties flown
ART_SKIN, ART_SHADE, ART_LINE = (226, 172, 138), (188, 126, 100), (46, 26, 22)
ART_SUIT, ART_SUIT_LIGHT, ART_HAIR, ART_HAIR_LIGHT = (184, 30, 34), (226, 86, 76), (196, 120, 34), (232, 176, 70)
ART_RIBBON, ART_LETTER, ART_BOMB = (222, 206, 160), (120, 22, 26), (214, 180, 52)
ART_SIZE = (1.86, 0.92)                  # metres: along the fuselage, and up it
ART_SS = 12                              # the art is drawn this many times too big and scaled down


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
        self.pieces = pieces
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


def line(d, w=0.011):
    """1 on a line, falling to 0 `w` metres from it; d the distance from it."""
    return 1.0 - smoothstep(w * 0.45, w, np.abs(d))


def lines_at(v, stations, w=0.011):
    out = np.zeros(v.shape, np.float32)
    for s in stations:
        out = np.maximum(out, line(v - s, w))
    return out


def blur(a, r):
    return np.asarray(Image.fromarray(np.clip(a * 255, 0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(r)), np.float32) / 255.0


# ---------------------------------------------------------------- lettering and insignia
# Block capitals as the USAF stencilled them: strokes on a grid 4 wide and 6 high.
GLYPHS = {
    "U": [[(0, 6), (0, 1), (1, 0), (3, 0), (4, 1), (4, 6)]],
    "S": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 4), (1, 3), (3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "A": [[(0, 0), (0, 4.4), (1.2, 6), (2.8, 6), (4, 4.4), (4, 0)], [(0, 2.3), (4, 2.3)]],
    "F": [[(0, 0), (0, 6), (4, 6)], [(0, 3.2), (3, 3.2)]],
    "X": [[(0, 0), (4, 6)], [(0, 6), (4, 0)]],
    "0": [[(1, 0), (3, 0), (4, 1), (4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0)]],
    "1": [[(1, 5), (2, 6), (2, 0)], [(1, 0), (3, 0)]],
    "2": [[(0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (0, 0.9), (0, 0), (4, 0)]],
    "3": [[(0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (3, 3), (1.5, 3)], [(3, 3), (4, 2), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "4": [[(3, 0), (3, 6), (0, 2), (0, 1.7), (4, 1.7)]],
    "5": [[(4, 6), (0, 6), (0, 3.4), (3, 3.4), (4, 2.4), (4, 1), (3, 0), (1, 0), (0, 1)]],
    "6": [[(4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0), (3, 0), (4, 1), (4, 2.4), (3, 3.4), (0, 3.4)]],
    "7": [[(0, 6), (4, 6), (4, 5), (1.5, 0)]],
    "8": [[(1, 3), (0, 4), (0, 5), (1, 6), (3, 6), (4, 5), (4, 4), (3, 3), (1, 3), (0, 2), (0, 1), (1, 0), (3, 0), (4, 1), (4, 2), (3, 3)]],
    "9": [[(0, 1), (1, 0), (3, 0), (4, 1), (4, 5), (3, 6), (1, 6), (0, 5), (0, 3.6), (1, 2.6), (4, 2.6)]],
    "-": [[(0.8, 3), (3.2, 3)]],
}
SS = 3  # the markings are drawn this many times too big and scaled down, for clean edges


class Markings:
    """What is painted over the camouflage in one view: drawn in the picture's
    own plane, positions and sizes in metres."""

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

    def triangle(self, a, b, size):
        """Ejection seat warning: a red triangle, point down, edged in white."""
        cx, cy = self.p(a, b)
        for s, col in ((size * 1.25, INSIGNIA_WHITE), (size, WARNING_RED)):
            R = s * PX * SS / 2
            self.d.polygon([(cx - R, cy - R * 0.75), (cx + R, cy - R * 0.75), (cx, cy + R)], fill=col + (255,))

    def outline(self, a0, a1, b0, b1, colour, width):
        p0, p1 = self.p(a0, b0), self.p(a1, b1)
        self.d.rectangle([min(p0[0], p1[0]), min(p0[1], p1[1]), max(p0[0], p1[0]), max(p0[1], p1[1])], outline=colour + (255,), width=max(int(round(width * PX * SS)), 1))

    def streak(self, a, b, length, width, colour, alpha):
        """A stain run back along the airflow from (a, b), fading out."""
        n = 12
        for k in range(n):
            t0, t1 = k / n, (k + 1) / n
            p0, p1 = self.p(a, b - length * t0), self.p(a, b - length * t1)
            self.d.line([p0, p1], fill=colour + (int(alpha * (1 - t0) ** 1.5),), width=max(int(round(width * (1 + 1.5 * t0) * PX * SS)), 1))

    def arrow(self, a, b, length, colour):
        """An arrow along the fuselage, pointing forward to (a + length / 2, b)."""
        t, n = a + length / 2, a + length / 2 - 0.14
        pts = [(t, b), (n, b + 0.075), (n, b + 0.028), (a - length / 2, b + 0.028), (a - length / 2, b - 0.028), (n, b - 0.028), (n, b - 0.075)]
        self.d.polygon([self.p(*q) for q in pts], fill=colour + (255,))

    def dashes(self, a, b, width, rows, height, colour, seed):
        """Small stencilling, too small to read: rows of dashes centred on (a, b), letters `height` m tall."""
        rng = np.random.default_rng(seed)
        cx, cy = self.p(a, b)
        u = PX * SS
        for r in range(rows):
            x, end = cx - width / 2 * u, cx + width / 2 * u * rng.uniform(0.6, 1.0)
            y = cy + r * height * 2.1 * u
            while x < end:
                w = rng.uniform(1.5, 5.0) * height * u
                self.d.line([(x, y), (min(x + w, end), y)], fill=colour + (255,), width=max(int(round(height * u)), 1))
                x += w + rng.uniform(0.6, 1.4) * height * u

    def over(self, rgb, where, soft=0.0):
        im = self.im.filter(ImageFilter.GaussianBlur(soft * SS)) if soft else self.im
        a = np.asarray(im.resize((self.v.w, self.v.h), Image.LANCZOS), np.float32)
        return mix(rgb, a[..., :3], a[..., 3] / 255.0 * where)


# ---------------------------------------------------------------- nose art
# Brush capitals for the name: strokes on a grid 4 wide and 6 high.
BRUSH = {
    "S": [[(4, 5.2), (3, 6), (1, 6), (0, 5), (0.3, 3.8), (3.7, 2.4), (4, 1), (3, 0), (1, 0), (0, 0.9)]],
    "M": [[(0, 0), (0.3, 6), (2, 2.4), (3.7, 6), (4, 0)]],
    "O": [[(2, 0), (0.6, 0.8), (0, 3), (0.6, 5.2), (2, 6), (3.4, 5.2), (4, 3), (3.4, 0.8), (2, 0)]],
    "K": [[(0, 0), (0.2, 6)], [(4, 6), (0.2, 2.6)], [(1.5, 3.7), (4, 0)]],
    "I": [[(2, 0), (2, 6)]],
    "N": [[(0, 0), (0.2, 6), (3.8, 0), (4, 6)]],
    "U": [[(0, 6), (0, 1.6), (1, 0), (3, 0), (4, 1.6), (4, 6)]],
    "E": [[(4, 6), (0, 6), (0, 0), (4, 0)], [(0, 3.1), (3, 3.1)]],
    "'": [[(2.2, 6.4), (1.6, 4.6)]],
    " ": [],
}


def spline(pts, sub=12):
    """A smooth curve through the points (Catmull-Rom); each point may carry more than x and y (a radius)."""
    p = np.asarray(pts, np.float32)
    out = []
    for i in range(len(p) - 1):
        a, b, c, d = p[max(i - 1, 0)], p[i], p[i + 1], p[min(i + 2, len(p) - 1)]
        for t in np.linspace(0, 1, sub, endpoint=False):
            out.append(0.5 * ((2 * b) + (-a + c) * t + (2 * a - 5 * b + 4 * c - d) * t * t + (-a + 3 * b - 3 * c + d) * t ** 3))
    out.append(p[-1])
    return np.asarray(out)


def pin_up(ppm):
    """The pin-up on the nose, as a crew chief would have painted her: an RGBA
    picture `ppm` pixels to the metre, ART_SIZE across, the aircraft's nose to
    the left. She sits leaning back on one arm, a knee up, a hand behind her
    head, in a red swimsuit and heels; her name on a ribbon underneath, and a
    row of bombs for the missions flown. Drawn on a grid 150 by 74 from the
    picture's lower left corner, as limbs (curves with a width) and outlines,
    each part inked round its edge before it is filled."""
    w, h = int(round(ART_SIZE[0] * ppm)), int(round(ART_SIZE[1] * ppm))
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    s = w / 150.0
    ink = 0.9  # the outline's width
    P = lambda x, y: (x * s, h - y * s)

    def limb(pts, colour, grow=0.0):
        """A curve through (x, y, half width) points, round at both ends."""
        for x, y, r in spline(pts, 24):
            r = (r + grow) * s
            cx, cy = P(x, y)
            d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=colour + (255,))

    def shape(pts, colour, grow=0.0, smooth=True):
        q = spline(pts + pts[:1], 10)[:, :2] if smooth else np.asarray(pts, np.float32)
        if grow:
            d.line([P(x, y) for x, y in q] + [P(*q[0])], fill=ART_LINE + (255,), width=max(int(round(2 * grow * s)), 1), joint="curve")
        d.polygon([P(x, y) for x, y in q], fill=colour + (255,))

    def part(pts, colour, lit=None, lit_shift=(0.0, 0.0), lit_scale=0.45):
        limb(pts, ART_LINE, ink)
        limb(pts, colour)
        if lit:  # a highlight along the limb's upper side
            limb([(x + lit_shift[0], y + lit_shift[1], r * lit_scale) for x, y, r in pts], lit)

    Y0 = 16.0  # the line she sits on, above the ribbon
    # Her hair, falling behind her shoulders.
    hair_back = [(111, Y0 + 44), (118, Y0 + 44), (124, Y0 + 37), (126, Y0 + 28), (131, Y0 + 20), (127, Y0 + 17), (121, Y0 + 21), (115, Y0 + 24), (110, Y0 + 32)]
    shape(hair_back, ART_HAIR, ink)
    # The arm she leans on, behind her; the far leg, stretched out toward the nose, toes pointed.
    part([(109, Y0 + 30, 2.3), (116, Y0 + 19, 2.0), (120, Y0 + 9, 1.7), (123, Y0 + 1.5, 1.5)], ART_SHADE)
    part([(90, Y0 + 5.5, 5.4), (70, Y0 + 5.0, 4.3), (52, Y0 + 3.8, 3.3), (36, Y0 + 3.2, 2.4), (24, Y0 + 2.6, 1.5)], ART_SHADE)
    shape([(25, Y0 + 4.2), (17, Y0 + 3.6), (11, Y0 + 1.2), (17, Y0 + 0.6), (26, Y0 + 1.0)], ART_SUIT, ink)  # the far shoe
    # Her body: the swimsuit from bust to hip, shoulders and chest above it.
    torso = [(84, Y0 + 2), (97, Y0 + 0.5), (103, Y0 + 6), (103, Y0 + 14), (106, Y0 + 22), (111, Y0 + 29), (110.5, Y0 + 34), (103.5, Y0 + 35.5), (97.5, Y0 + 32), (93.2, Y0 + 27.5), (92.6, Y0 + 22.5), (91.5, Y0 + 16), (83, Y0 + 9)]
    shape(torso, ART_SKIN, ink)
    suit = [(84, Y0 + 2), (97, Y0 + 0.5), (103, Y0 + 6), (103, Y0 + 14), (106, Y0 + 22), (109.5, Y0 + 27.5), (104, Y0 + 30.0), (98.5, Y0 + 30.5), (93.2, Y0 + 27.5), (92.6, Y0 + 22.5), (91.5, Y0 + 16), (83, Y0 + 9)]
    shape(suit, ART_SUIT, ink * 0.6)
    limb([(95.6, Y0 + 27, 1.7), (95.0, Y0 + 20, 1.2), (92.5, Y0 + 12.5, 1.2)], ART_SUIT_LIGHT)  # the light on it
    for k in range(4):  # and its white polka dots
        for j in range(2 + k % 2):
            cx, cy = P(100.2 + 3.0 * j - 1.5 * (k % 2) - 0.9 * k, Y0 + 25.5 - 5.4 * k)
            r = 0.75 * s
            d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(240, 232, 220, 255))
    # The near leg: thigh up to the raised knee, shin down to the heel.
    part([(92, Y0 + 7.5, 6.0), (80, Y0 + 15, 5.4), (69, Y0 + 23.5, 4.3)], ART_SKIN, (244, 200, 168), (0.6, 1.4))
    part([(69, Y0 + 23.5, 4.1), (63, Y0 + 15, 3.7), (57, Y0 + 7, 2.6), (54, Y0 + 3.0, 1.7)], ART_SKIN, (244, 200, 168), (-0.9, 0.3), 0.4)
    shape([(55.5, Y0 + 4.6), (51.5, Y0 + 5.0), (45, Y0 + 2.2), (40, Y0 + 0.2), (46, Y0 - 0.4), (52, Y0 + 0.4), (56.5, Y0 + 1.6)], ART_SUIT, ink)  # the near shoe
    d.line([P(55.8, Y0 + 1.8), P(56.6, Y0 - 1.6)], fill=ART_LINE + (255,), width=max(int(round(0.9 * s)), 1))  # its heel
    part([(98.0, Y0 + 45.5, 2.0), (102.5, Y0 + 51.5, 1.8), (109.5, Y0 + 51.0, 1.7)], ART_SKIN)  # the near forearm, its hand behind her head
    # Neck and head, turned to look out at whoever walks up to the aircraft.
    part([(107.5, Y0 + 33, 2.1), (109.5, Y0 + 37.5, 1.9)], ART_SKIN)
    head = (110.0, Y0 + 42.5)
    shape([(head[0] - 4.6, head[1] + 0.2), (head[0] - 3.7, head[1] + 3.6), (head[0] - 0.2, head[1] + 5.0), (head[0] + 3.6, head[1] + 3.6), (head[0] + 4.6, head[1] - 0.2), (head[0] + 2.6, head[1] - 4.0), (head[0] - 1.2, head[1] - 5.4), (head[0] - 3.8, head[1] - 3.4)], ART_SKIN, ink)
    # Her face: eyes, brows, a red smile, a touch of rouge.
    for ex in (-2.3, 0.9):
        cx, cy = P(head[0] + ex, head[1] + 0.3)
        d.ellipse([cx - 0.75 * s, cy - 0.6 * s, cx + 0.75 * s, cy + 0.6 * s], fill=ART_LINE + (255,))
        d.line([P(head[0] + ex - 1.2, head[1] + 1.7), P(head[0] + ex + 1.1, head[1] + 2.0)], fill=ART_LINE + (255,), width=max(int(round(0.4 * s)), 1))
    d.line([P(*q) for q in spline([(head[0] - 2.5, head[1] - 2.1), (head[0] - 1.0, head[1] - 3.0), (head[0] + 0.9, head[1] - 2.2)], 8)[:, :2]], fill=ART_SUIT + (255,), width=max(int(round(0.95 * s)), 1), joint="curve")
    cx, cy = P(head[0] + 2.2, head[1] - 1.3)
    d.ellipse([cx - 1.0 * s, cy - 0.7 * s, cx + 1.0 * s, cy + 0.7 * s], fill=(232, 140, 124, 255))
    # Hair over her brow and down the side of her face, with the light on its waves.
    hair = [(head[0] - 5.1, head[1] + 1.8), (head[0] - 4.1, head[1] + 5.6), (head[0], head[1] + 7.6), (head[0] + 5.0, head[1] + 6.2), (head[0] + 7.5, head[1] + 1.5), (head[0] + 8.0, head[1] - 4.5), (head[0] + 5.1, head[1] - 7.0), (head[0] + 4.1, head[1] - 2.0), (head[0] + 3.1, head[1] + 2.2), (head[0] - 0.3, head[1] + 3.6), (head[0] - 3.1, head[1] + 2.4)]
    shape(hair, ART_HAIR, ink)
    limb([(head[0] - 2.5, head[1] + 5.3, 0.6), (head[0] + 1.5, head[1] + 6.0, 0.8), (head[0] + 5.5, head[1] + 3.5, 0.7), (head[0] + 6.4, head[1] - 2.5, 0.6)], ART_HAIR_LIGHT)
    cx, cy = P(head[0] + 4.6, head[1] + 5.2)  # a red flower in it
    d.ellipse([cx - 1.7 * s, cy - 1.7 * s, cx + 1.7 * s, cy + 1.7 * s], fill=ART_LINE + (255,))
    d.ellipse([cx - 1.25 * s, cy - 1.25 * s, cx + 1.25 * s, cy + 1.25 * s], fill=ART_SUIT_LIGHT + (255,))
    limb([(121, Y0 + 35, 0.6), (124, Y0 + 28, 0.7), (127.5, Y0 + 21, 0.5)], ART_HAIR_LIGHT)
    # The near arm's upper part, raised.
    part([(104.0, Y0 + 32.5, 2.6), (100.5, Y0 + 39.5, 2.3), (98.0, Y0 + 45.5, 2.1)], ART_SKIN, (244, 200, 168), (-0.7, 0.2), 0.4)

    # Her name on a ribbon, its ends folded back and notched.
    r0, r1, x0, x1 = 1.5, 12.5, 13.0, 136.0
    wave = lambda x: 1.2 * np.sin((x - x0) / (x1 - x0) * 2 * np.pi)
    for sx, xe in ((-1, x0), (1, x1)):  # the tails behind
        shape([(xe + sx * 9, r0 + 2.0 + wave(xe)), (xe - sx * 4, r0 + 2.0 + wave(xe)), (xe - sx * 4, r1 + 2.0 + wave(xe)), (xe + sx * 9, r1 + 2.0 + wave(xe)), (xe + sx * 5.5, (r0 + r1) / 2 + 2.0 + wave(xe))], tuple(int(c * 0.72) for c in ART_RIBBON), ink * 0.7, smooth=False)
    xs = np.linspace(x0, x1, 40)
    shape([(x, r1 + wave(x)) for x in xs] + [(x, r0 + wave(x)) for x in xs[::-1]], ART_RIBBON, ink * 0.7, smooth=False)
    u = (r1 - r0 - 3.6) / 6.0  # letter grid unit, cm
    pitch = 5.25 * u
    start = (x0 + x1) / 2 - (len(NOSE_ART_NAME) * pitch - 1.25 * u) / 2
    for k, ch in enumerate(NOSE_ART_NAME):
        for stroke in BRUSH[ch]:
            pts = [(start + k * pitch + gx * u + 0.16 * gy * u, r0 + 1.8 + gy * u) for gx, gy in stroke]  # (slanted like brush lettering)
            pts = [(x, y + wave(x)) for x, y in pts]
            limb([(x, y, 0.5 * u * (1.0 + 0.25 * np.cos(i * 1.7))) for i, (x, y) in enumerate(pts)] if len(pts) > 2 else [(x, y, 0.5 * u) for x, y in pts], ART_LETTER)
    # The missions: two rows of little bombs ahead of her, over her legs.
    for n in range(NOSE_ART_MISSIONS):
        bx, by = 8.0 + 5.4 * (n % 7), Y0 + 31.0 - 9.0 * (n // 7)
        limb([(bx, by + 4.2, 0.5), (bx, by + 2.6, 1.4), (bx, by + 0.6, 1.2), (bx, by - 0.4, 0.5)], ART_BOMB)
        d.polygon([P(bx - 1.6, by + 6.0), P(bx + 1.6, by + 6.0), P(bx, by + 3.6)], fill=ART_BOMB + (255,))
    return im


def nose_art(v, rgb, where):
    """Paint her onto the left side of the nose (before the weathering, so the
    fasteners, the grime and the fading run over her like the rest of the paint)."""
    art = pin_up(PX * ART_SS)
    w, h = int(round(ART_SIZE[0] * PX)), int(round(ART_SIZE[1] * PX))
    x, y = v.px(NOSE_ART[0], NOSE_ART[1] + ART_SIZE[1])
    layer_ = Image.new("RGBA", (v.w, v.h), (0, 0, 0, 0))
    layer_.paste(art.resize((w, h), Image.LANCZOS), (int(round(x)), int(round(y))))
    a = np.asarray(layer_, np.float32)
    # Brushed on by hand: not quite opaque, and worn thin in places.
    thin = 0.95 - 0.30 * smoothstep(1.0, 2.0, noise(v.pos, (0.06, 0.06, 0.06), 41))
    return mix(rgb, a[..., :3], a[..., 3] / 255.0 * thin * where)


# ---------------------------------------------------------------- skin panels
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
    return (np.mod(along, RIVET_PITCH) < 0.020).astype(np.float32)


class Detail:
    """What is drawn into one view's paint: seams, fastener rows, stencilled
    lettering (1 dark, -1 light), the inside of access panels, and a number
    0..1 for each skin panel (its paint aged a little differently)."""

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
    rivet = np.maximum(line(da - RIVET_INSET, 0.012) * along_b, line(db - RIVET_INSET, 0.012) * along_a)
    rivet = np.maximum(rivet, line(ha - da, 0.012) * along_b * (ha > 0.24) * (ha < 9) * (db > 0.03))
    rivet = np.maximum(rivet, line(hb - db, 0.012) * along_a * (hb > 0.24) * (hb < 9) * (da > 0.03))
    h = lambda k: hashf(i, j, seed=seed * 16 + k)
    # An access panel: a rectangle with round corners, or an oval.
    ea, eb = np.minimum(ha * (0.35 + 0.30 * h(2)), 0.42), np.minimum(hb * (0.35 + 0.30 * h(3)), 0.42)
    has = (h(1) > 0.74) & (ha > 0.13) & (hb > 0.13) & (ha < 9) & (hb < 9)
    r = np.minimum(ea, eb) * np.where(h(4) > 0.7, 1.0, 0.3)
    qa, qb = np.abs(a - (lo_a + hi_a) / 2) * ma - ea + r, np.abs(b - (lo_b + hi_b) / 2) * mb - eb + r
    d = np.hypot(np.maximum(qa, 0), np.maximum(qb, 0)) + np.minimum(np.maximum(qa, qb), 0) - r
    seam = np.maximum(seam, 0.6 * line(d, 0.011) * has)
    rivet = np.maximum(rivet, line(d + 0.024, 0.012) * np.where(qa > qb, along_b, along_a) * has)
    inside = (d < 0) & has
    # Stencilling: a few rows of dashes in a corner of the panel.
    la, lb = (a - lo_a) * ma - 0.07, (b - lo_b) * mb - 0.06
    rows, wide = 2 + (h(6) * 3).astype(np.int32), 0.10 + 0.22 * h(7)
    st = (h(5) < 0.30) & ~inside & (la > 0) & (la < wide) & (lb > 0) & (lb < rows * 0.034) & (np.mod(lb, 0.034) < 0.015) & (ha < 9) & (hb < 9)
    st &= hashf(np.floor(la / 0.024), np.floor(lb / 0.034), i, j, seed=seed) > 0.32
    out.seam[idx] = np.maximum(out.seam[idx], seam)
    out.rivet[idx] = np.maximum(out.rivet[idx], rivet)
    out.tone[idx] = h(0)
    out.access[idx] = inside
    out.stencil[idx] = st * np.where(h(8) > 0.45, 1.0, -1.0)


def box_d(a, b, box, r=0.05):
    """Distance outside a round-cornered rectangle (a from, to; b from, to); negative inside."""
    a0, a1, b0, b1 = box
    qa = np.abs(a - (a0 + a1) / 2) - abs(a1 - a0) / 2 + r
    qb = np.abs(b - (b0 + b1) / 2) - abs(b1 - b0) / 2 + r
    return np.hypot(np.maximum(qa, 0), np.maximum(qb, 0)) + np.minimum(np.maximum(qa, qb), 0) - r


def boxes(a, b, list_, w=0.010, r=0.05):
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


# ---------------------------------------------------------------- painting a view
def wing_chord(v):
    """Chord fraction (0 the leading edge) of a point of the wing, its span station, and the chord there (m)."""
    ax = np.abs(v.x)
    c = 7.00 - 5.60 * ax / 5.85
    le = 4.30 - 1.2526 * ax
    outer = ax > WING_FOLD
    le = np.where(outer, le + 0.10 * c, le)  # the dogtooth
    c = np.where(outer, 1.10 * c, c)
    return (le - v.y) / c, ax, c


def paint(v):
    sides = v.name in ("side", "right")
    rng = np.random.default_rng(("top", "bottom", "side", "right").index(v.name) + 1)

    def grain(r):
        """Fine random grain, blurred over r pixels: unit variance."""
        g = blur(rng.random(v.ok.shape).astype(np.float32), r)
        return (g - g.mean()) / (g.std() + 1e-6)

    body = v.piece("fuselage", "nose section", "tail cone", "spine", "intake", "radome", "canopy frame")
    wing = v.piece("wing", "flap", "aileron", "speedbrake")
    stab = v.piece("stabilator")
    fin = v.piece("fin", "rudder")
    ax = np.abs(v.x)
    above, below, steep = v.nz > 0.45, v.nz < -0.45, np.abs(v.nx) > 0.45

    # The camouflage: blotches of three colours from two smooth fields over the
    # model, their edges soft and a little ragged (sprayed freehand).
    edge = 0.10 * noise(v.pos, (0.8, 0.9, 0.7), CAMO_SEED + 2) + 0.03 * noise(v.pos, (0.12, 0.12, 0.12), CAMO_SEED + 3)
    f1 = noise(v.pos, CAMO_SIZE, CAMO_SEED) + edge
    f2 = noise(v.pos, CAMO_SIZE, CAMO_SEED + 1) - edge
    rgb = np.empty(v.ok.shape + (3,), np.float32)
    rgb[:] = GREEN
    rgb = mix(rgb, TAN, smoothstep(-0.16, -0.04, f2))
    rgb = mix(rgb, DARK_GREEN, smoothstep(0.28, 0.40, f1))
    # Light grey underneath: the under surfaces, and the fuselage below a wavy line.
    wave = DEMARCATION + DEMARCATION_WAVE * (np.sin(v.y * 1.9) + 0.5 * np.sin(v.y * 4.3 + 1.0))
    under = np.where(body, np.maximum(smoothstep(wave + 0.03, wave - 0.03, v.z), smoothstep(-0.75, -0.9, v.nz) * (v.z < 0.1)), smoothstep(-0.12, -0.3, v.nz))
    under = np.where(fin, 0.0, under)
    rgb = mix(rgb, GREY, under)
    # Black: the radome, and the anti-glare panel ahead of the windscreen.
    glare = v.piece("nose section") * smoothstep(0.20, 0.35, v.nz) * (1 - smoothstep(-0.02, 0.02, ax - (0.17 + 0.04 * (7.9 - v.y)))) * smoothstep(6.42, 6.47, v.y)
    rgb = mix(rgb, BLACK, np.maximum(glare, v.mat("radome").astype(np.float32)))

    # Bare metal: under the tail boom and behind the jet pipes, the stabilators' roots.
    aft = smoothstep(-3.45, -3.62, v.y)
    metal = np.maximum(v.piece("tail cone") * aft * smoothstep(0.38, 0.22, v.nz), v.piece("fuselage") * aft * smoothstep(0.40, 0.30, v.z))
    metal = np.maximum(metal, stab * smoothstep(1.00, 0.93, ax))
    metal = np.maximum(metal, v.mat("heat", "exhaust", "steel").astype(np.float32))
    r_jet = np.hypot(ax - 0.50, v.z + 0.30)  # distance from the nearer engine's centreline
    heat = smoothstep(-3.4, -5.4, v.y) * np.exp(-((r_jet / 1.1) ** 2))
    streaky = noise(v.pos, (0.14, 2.5, 0.14), 31)  # streaked along the airflow
    m = np.empty_like(rgb)
    m[:] = METAL
    m = mix(m, STRAW, 0.55 * smoothstep(0.15, 0.8, heat + 0.10 * streaky))
    m = mix(m, BLUED, 0.35 * smoothstep(0.6, 1.0, heat + 0.08 * noise(v.pos, (0.9, 1.6, 0.9), 32)))
    m *= (1.0 + 0.07 * streaky)[..., None]
    rgb = mix(rgb, m, metal)
    # The fin's cap: a dielectric panel over the antennas, in its own dull grey-green.
    cap = fin * smoothstep(FIN_CAP - 0.01, FIN_CAP + 0.01, v.z)
    rgb = mix(rgb, (66, 72, 62), 0.45 * cap)

    # Markings.
    k = Markings(v)
    if v.name == "top":
        k.star(*WING_STAR_TOP)
        k.text("USAF", *WING_USAF_TOP, INSIGNIA_BLUE)
        for s in (-1, 1):
            x0, x1, y0, y1 = WALKWAY_BOX
            k.outline(s * x0, s * x1, y0, y1, WALKWAY, 0.03)
    elif v.name == "bottom":
        k.star(*WING_STAR_BOTTOM)
        k.text("USAF", *WING_USAF_BOTTOM, INSIGNIA_BLUE)
    else:
        k.star(*FUSELAGE_STAR)
        k.text(TAIL_CODE, *FIN_CODE, INSIGNIA_WHITE)
        k.text(SERIAL, *FIN_SERIAL, BLACK)
        for y, z in SEAT_TRIANGLES:
            k.triangle(y, z, 0.17)
        # The rescue arrow under the front canopy with its instructions, and
        # (left side) the data block: rows of small stencilling.
        k.arrow(*RESCUE, 0.42, RESCUE_YELLOW)
        k.dashes(RESCUE[0] - 0.50, RESCUE[1] - 0.09, 0.40, 3, 0.016, RESCUE_YELLOW, 3)
        if v.name == "side":
            k.dashes(*DATA_BLOCK, 0.34, 5, 0.014, BLACK, 5)
    on = v.mat("skin").astype(np.float32)
    rgb = k.over(rgb, on * ((fin | body) if sides else (wing | body)))
    if v.name == "side":
        rgb = nose_art(v, rgb, on * body)
    # The red warning stripes round the fuselage: at the intakes, and in the plane of the turbines.
    stripes = np.maximum(line(v.y - INTAKE_STRIPE, 0.035) * v.piece("nose section"), line(v.y - TURBINE_STRIPE, 0.03) * v.piece("fuselage") * (np.sin(v.z * 40) > -0.3))
    rgb = mix(rgb, WARNING_RED, stripes * (1 - under) * (1 - glare) * steep)
    fresh = rgb.copy()  # as it left the paint shop

    # ---- the structure showing through: skin panels, fasteners, doors, vents
    D = Detail(v)
    pan = v.ok & v.mat("skin", "heat")
    # The fuselage: frames by station; stringers at angles round its axis (the boom's is higher), so they meet across the views.
    zc = 0.55 * smoothstep(-3.7, -4.2, v.y)
    girth = np.maximum(np.hypot(v.x, v.z - zc), 0.15)
    panels(D, body & pan, v.y, np.arctan2(v.z - zc, ax), FRAMES, -1.6, 1.6, PANEL_PITCH[0], PANEL_SEED, 1.0, girth)
    # Wing, stabilators and fin: spars by chord fraction, ribs along the span; the upper and lower skins cut differently.
    frac, _, chord = wing_chord(v)
    ks = (ax - 0.12) / 2.58
    fs, stab_c = (-6.10 - 2.62 * ks - v.y) / (2.80 - 2.12 * ks), 2.80 - 2.12 * ks
    u = (v.z - 0.88) / 2.005
    ff, fin_c = (-3.90 - 3.85 * u - v.y) / (4.30 - 3.05 * u), 4.30 - 3.05 * u
    for n, face in enumerate((v.nz >= 0, v.nz < 0)):
        panels(D, wing & pan & face, frac, ax, WING_SPARS, 0.0, 5.9, PANEL_PITCH[1], PANEL_SEED + 1 + n, chord, 1.0)
        panels(D, stab & pan & face, fs, ax, STAB_SPARS, 0.1, 2.75, PANEL_PITCH[2], PANEL_SEED + 3 + n, stab_c, 1.0)
    panels(D, fin & pan, ff, v.z, FIN_SPARS, 0.9, 2.95, PANEL_PITCH[3], PANEL_SEED + 5, fin_c, 1.0)
    seam, rivet = D.seam, D.rivet

    # Where two pieces of one surface meet (the control surfaces, the doors).
    joint = np.zeros(v.ok.shape, np.float32)
    depth = v.z if not sides else v.x
    for axis in (0, 1):
        d = (np.roll(v.piece_id, 1, axis) != v.piece_id) & v.ok & np.roll(v.ok, 1, axis) & (np.abs(np.roll(depth, 1, axis) - depth) < 0.10)
        joint = np.maximum(joint, d.astype(np.float32))
    seam = np.maximum(seam, np.clip(blur(joint, 0.6) * 1.6, 0, 1))
    # Doors and panels with places of their own.
    seam = np.maximum(seam, boxes(v.y, v.z, SIDE_PANELS) * body * steep)
    under_side = (body | wing) & below
    seam = np.maximum(seam, boxes(ax, v.y, BELLY_PANELS + (GUN_BAY, AUX_AIR_DOORS) + ENGINE_DOORS) * under_side)
    seam = np.maximum(seam, boxes(ax, v.y, SPARROW_WELLS, r=0.10) * under_side)
    seam = np.maximum(seam, boxes(ax, v.y, TOP_PANELS + (RECEPTACLE,)) * (body | wing) * above)
    for door in ENGINE_DOORS:  # their latches
        rivet = np.maximum(rivet, line(box_d(ax, v.y, door) + 0.035, 0.014) * dots(v.y + ax) * under_side)
    # The wing: the fold joint with its double row of fasteners, the spoilers, the bulge over each wheel.
    seam = np.maximum(seam, line(ax - WING_FOLD, 0.02) * wing)
    rivet = np.maximum(rivet, lines_at(ax, (WING_FOLD - 0.05, WING_FOLD + 0.05), 0.012) * dots(v.y) * wing)
    x0, x1, c0, c1 = SPOILERS
    seam = np.maximum(seam, line(np.maximum(np.abs(ax - (x0 + x1) / 2) - (x1 - x0) / 2, (np.abs(frac - (c0 + c1) / 2) - (c1 - c0) / 2) * chord)) * wing * above)
    seam = np.maximum(seam, line(np.hypot(ax - WHEEL_BULGE[0], v.y - WHEEL_BULGE[1]) - WHEEL_BULGE[2], 0.012) * wing * above)
    # The rudder's and stabilators' close-set ribs: rows of fasteners.
    rivet = np.maximum(rivet, (np.mod(v.z, 0.13) < 0.013) * dots(v.y) * v.piece("rudder"))
    rivet = np.maximum(rivet, (np.mod(ax, 0.17) < 0.013) * dots(v.y) * stab * (fs > 0.12) * (fs < 0.85))
    seam = np.maximum(seam, line(v.z - FIN_CAP, 0.012) * fin)
    # Fuel fillers on top, ringed in red; the radome's joint and its tip cap.
    ring = np.zeros(v.ok.shape, np.float32)
    for fx, fy, fr in FILLERS:
        d = np.hypot(ax - fx, v.y - fy) - fr
        seam = np.maximum(seam, line(d, 0.011) * above)
        ring = np.maximum(ring, line(d - 0.035, 0.013) * above)
    seam = np.maximum(seam, lines_at(v.y, (9.45,), 0.012) * v.mat("radome"))
    # The intake ramps' bleed holes, and the louvres the bled air leaves by; gun gas louvres' surrounds.
    intake = v.piece("intake")
    holes = intake * steep * (v.y < RAMP_HOLES[0]) * (v.y > RAMP_HOLES[1]) * (np.abs(v.z + 0.05) < 0.45) * (np.mod(v.y, 0.039) < 0.016) * (np.mod(v.z, 0.039) < 0.016)
    louvre_box = (0.74, 1.20, BLEED_LOUVRES[0], BLEED_LOUVRES[1])
    louvres = intake * (above | below) * (box_d(ax, v.y, louvre_box, 0.03) < 0) * (np.mod(v.y, 0.075) < 0.032)
    seam = np.maximum(seam, boxes(ax, v.y, (louvre_box,), r=0.03) * intake * (above | below))
    # The formation light strips' surrounds.
    slime = v.mat("slime").astype(np.float32)
    seam = np.maximum(seam, 0.7 * np.clip(blur(slime, 1.5) * 4, 0, 1) * (1 - slime))
    not_skin = v.mat("glass", "frame", "inlet", "rubber", "slime")
    seam = np.clip(seam, 0, 1) * ~not_skin
    rivet = np.clip(np.maximum(rivet, holes), 0, 1) * ~not_skin * (1 - seam)

    # ---- weathering
    # The paint faded and chalked in patches, more on top; every skin panel aged a little differently.
    fade = noise(v.pos, (1.6, 2.2, 1.4), 11)
    chalk = smoothstep(0.0, 1.4, noise(v.pos, (0.5, 0.7, 0.5), 12) + 0.6 * fade + 1.2 * (D.tone < 0.12)) * (0.16 + 0.16 * np.clip(v.nz, 0, 1)) * (1 - metal)
    lum = rgb @ np.array([0.30, 0.59, 0.11], np.float32)
    rgb = mix(rgb, lum[..., None] * 0.35 + rgb * 0.65 + 20.0, chalk)
    rgb *= (1.0 + 0.05 * fade + 0.16 * (D.tone - 0.5) + 0.025 * noise(v.pos, (0.12, 0.12, 0.12), 13))[..., None]
    # Panels repainted since (fresher and darker), and touched-up patches round some access panels.
    rgb = mix(rgb, fresh * 0.94, 0.8 * ((D.tone > 0.93) | ((D.access > 0) & (D.tone > 0.7))) * (1 - metal))
    # Grain: the paint's own mottle and the dust on it, so nowhere is one flat colour.
    rgb *= (1.0 + 0.030 * grain(0.8) + 0.035 * grain(3.0) + 0.03 * grain(9.0))[..., None]
    # Blotchy stains, where fluids were spilt and wiped.
    rgb *= (1.0 - 0.10 * smoothstep(0.9, 1.6, noise(v.pos, (0.28, 0.34, 0.28), 24)) - 0.05 * smoothstep(0.8, 1.8, noise(v.pos, (0.7, 0.9, 0.7), 25)))[..., None]
    # Grime streaked back along the airflow; rain run down the fuselage's sides and the fin.
    streaks = smoothstep(0.5, 1.8, noise(v.pos, (0.09, 3.0, 0.09), 14) + 0.5 * noise(v.pos, (1.5, 4.0, 1.5), 15))
    rgb *= (1.0 - streaks * (0.09 + 0.07 * under))[..., None]
    rain = noise(v.pos, (6.0, 0.07, 2.4), 18) + 0.6 * noise(v.pos, (6.0, 0.9, 3.0), 19)
    rgb *= (1.0 - (0.10 * smoothstep(0.7, 1.9, rain) - 0.06 * smoothstep(0.7, 1.9, -rain)) * steep * (body | fin))[..., None]
    # Walkways: scuffed by boots.
    if v.name == "top":
        x0, x1, y0, y1 = WALKWAY_BOX
        scuff = (box_d(ax, v.y, (x0, x1, y0, y1)) < 0) * wing * (0.5 + 0.35 * noise(v.pos, (0.25, 0.4, 0.25), 20) + 0.25 * grain(1.5))
        rgb = mix(rgb, (70, 68, 62), np.clip(0.30 * scuff, 0, 0.5))

    # What leaks: dirt and fluid dragged aft from seams, doors and vents (and, on the sides, down by the rain).
    leaky = noise(v.pos, (0.5, 0.5, 0.5), 17)
    leak = np.clip(seam * (leaky > 0.35) + 0.8 * louvres + holes + ring, 0, 1)
    wet = np.zeros(v.ok.shape, np.float32)
    for door in ENGINE_DOORS + (AUX_AIR_DOORS, GUN_BAY):
        wet = np.maximum(wet, line(box_d(ax, v.y, door), 0.02) * under_side * (leaky > -0.6))
    for well in SPARROW_WELLS:
        wet = np.maximum(wet, 0.6 * line(box_d(ax, v.y, well, 0.10), 0.02) * under_side)
    reach = np.clip(0.925 + 0.045 * noise(v.pos, (0.3, 0.3, 0.3), 23), 0.80, 0.985)
    axis, step = (1, 1 if v.name == "side" else -1) if sides else (0, 1)   # which way aft is in this picture
    trail = smear(np.maximum(leak, wet), axis, step, reach, v.ok)
    oil = smear(wet, axis, step, np.clip(reach + 0.035, 0, 0.992), v.ok)
    if sides:
        trail = np.maximum(trail, smear(leak * (leaky > 0.9), 0, 1, reach, v.ok))
    trail = blur(np.clip(trail - leak, 0, 1), 0.7) * ~not_skin
    rgb = mix(rgb, GRIME, np.clip(trail * (0.20 + 0.14 * under), 0, 0.6))
    rgb = mix(rgb, (52, 40, 26), np.clip(blur(oil, 0.8) * 0.42 * under, 0, 0.6) * ~not_skin)

    # Paint chipped to the primer and the metal: the leading edges and intake lips, round seams and opened panels.
    worn = np.maximum.reduce([wing * smoothstep(0.035, 0.008, frac), stab * smoothstep(0.045, 0.010, fs), fin * smoothstep(0.035, 0.008, ff), intake * smoothstep(4.02, 4.17, v.y) * (ax > 0.80) * (v.mat("skin") > 0)])
    g = grain(0.9)
    chip = worn * (g > 0.2) + (blur(seam, 1.0) > 0.2) * (g > 1.9) + (blur(D.access, 1.0) > 0.1) * (blur(D.access, 1.0) < 0.9) * (g > 0.9)
    chip = np.clip(chip, 0, 1) * (1 - metal) * ~not_skin * (1 - v.mat("radome"))
    rgb = mix(rgb, mix(np.broadcast_to(np.asarray(PRIMER, np.float32), rgb.shape).copy(), CHIP, (grain(1.2) > 0.2).astype(np.float32)), (0.45 + 0.4 * worn) * chip)

    # Fittings: the bleed louvres' slats and the Sparrow recesses in shadow, the auxiliary air doors, the fillers' rings.
    rgb *= (1.0 - 0.55 * louvres)[..., None]
    for well in SPARROW_WELLS:
        d = box_d(ax, v.y, well, 0.10)
        rgb *= (1.0 - 0.24 * smoothstep(0.0, -0.05, d) * under_side)[..., None]
    rgb *= (1.0 - 0.10 * (box_d(ax, v.y, AUX_AIR_DOORS) < 0) * under_side)[..., None]
    rgb = mix(rgb, FUEL_RED, 0.8 * ring * (1 - metal))

    # Seams, fasteners and stencilling. A seam is a dark line with dirt gathered
    # along it and the paint worn light on its edge (the game takes dark lines
    # to lie below the skin round them).
    grime = blur(seam, 3.0) * (0.6 + 0.4 * noise(v.pos, (0.4, 0.8, 0.4), 16))
    rgb *= (1.0 - 0.20 * np.clip(grime * 2.2, 0, 1) * (1 + under))[..., None]
    rim = np.clip(np.roll(seam, (1, 1), (0, 1)) - seam, 0, 1)
    rgb *= (1.0 + 0.10 * rim)[..., None]
    rgb = mix(rgb, (24, 24, 24), 0.75 * (D.stencil > 0) * (1 - metal))
    rgb = mix(rgb, (214, 208, 186), 0.60 * (D.stencil < 0) * (1 - metal) * (1 - under))
    rgb *= (1.0 - 0.34 * rivet)[..., None]
    rgb *= (1.0 - 0.52 * seam)[..., None]

    # Stains: gun gas behind the muzzle; soot behind the jet pipes; hydraulic fluid and oil run back under the belly.
    gun = np.exp(-((v.x / 0.26) ** 2)) * smoothstep(GUN_MUZZLE + 0.05, GUN_MUZZLE - 0.30, v.y) * smoothstep(5.0, 7.7, v.y) * body * (v.z < -0.45)
    rgb = mix(rgb, SOOT, np.clip(gun * (0.55 + 0.25 * streaky), 0, 0.8))
    soot = smoothstep(-4.3, -5.6, v.y) * np.exp(-((r_jet / 0.75) ** 2)) * (0.75 + 0.25 * streaky)
    rgb = mix(rgb, SOOT, np.clip(soot, 0, 0.85) * np.maximum(metal, stab))
    if v.name == "bottom":
        k = Markings(v)
        srng = np.random.default_rng(7)
        for _ in range(46):
            a = srng.choice([-1, 1]) * abs(srng.normal(0.0, 1.5))
            k.streak(a, srng.uniform(-3.2, 6.5), srng.uniform(0.4, 1.6), srng.uniform(0.012, 0.03), (58, 44, 30), srng.uniform(70, 170))
        for s in (-1, 1):  # round the wheel wells and the speedbrakes
            for _ in range(7):
                k.streak(s * srng.uniform(1.5, 2.9), srng.uniform(-2.2, -1.2), srng.uniform(0.5, 1.3), srng.uniform(0.015, 0.03), (50, 40, 30), srng.uniform(60, 130))
        rgb = k.over(rgb, on, soft=1.2)

    # What is not painted skin (the pictures are not projected onto it, but its edge shows).
    rgb = mix(rgb, GLASS, v.mat("glass").astype(np.float32))
    rgb = mix(rgb, FRAME, v.mat("frame", "inlet", "rubber").astype(np.float32))  # (not the wheel wells: their doors shut over them, and take this paint)
    # The guide's shading, and shadow gathered where one piece meets another, so the shapes read.
    rgb *= (np.clip(v.shade, 0.5, 1.25) ** 0.40)[..., None]
    rgb *= (1.0 - 0.22 * np.clip(blur(joint, 5.0) * 3.0, 0, 1))[..., None]
    rgb = np.clip(rgb, 0, 232)  # (pure white is the background)
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
    # Painted: inside the silhouette (less a few pixels, where the picture's
    # own edge may fall short of the model's) and not the white of the background.
    inside = np.asarray(mask.filter(ImageFilter.MinFilter(7))) > 128
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
