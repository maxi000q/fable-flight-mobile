# The F/A-18C Hornet for FableFlight, built procedurally in Blender.
#
#   blender -b --factory-startup --python assets/blender/fa18.py -- <repo>/assets [--render <dir>] [--guides <dir>]
#
# Builds the airframe from lofted sections and NACA-section lifting surfaces,
# one object per piece that moves or breaks off on its own, saves it as
# assets/blender/fa18.blend and exports assets/fa18.bin.gz, which
# src/aircraft/fa18 loads. That rebuild starts from scratch, so it throws
# away anything edited by hand in the .blend.
#
# To keep hand edits, export the .blend as it stands instead (save it first):
#
#   blender -b assets/blender/fa18.blend --python assets/blender/fa18.py -- <repo>/assets --export-only
#
# Edit freely: reshape meshes, add modifiers (they are applied on export),
# move or turn pieces (each carries its hinge along), assign the fa18_
# materials. Keep the piece names and the "FA-18C" collection: the game finds
# pieces by name (see EXPECTED below); objects with other names are ignored.
# How each piece moves is set in Rust by its name; each object records where
# its hinge is in its own frame (custom properties "pivot", "axis", "angle").
#
# --render writes preview pictures. --guides writes the three orthographic
# views (top, bottom, left side) the paint is projected from, each with its
# silhouette mask; assets/blender/fa18_paint.py turns painted versions of
# them into assets/fa18_paint.jpg (see there and plane.wgsl, `hornet_paint`).
#
# Blender frame here: +X right, +Y forward (nose), +Z up, origin at the centre
# of gravity, metres. The game's body frame is +X right, +Y up, -Z forward,
# so a point (x, y, z) here is (x, z, -y) there.
import bpy, bmesh, math, struct, gzip, sys, os
from mathutils import Vector as V, Matrix

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
ASSETS = os.path.abspath(argv[0]) if argv else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RENDER = argv[argv.index("--render") + 1] if "--render" in argv else None
GUIDES = argv[argv.index("--guides") + 1] if "--guides" in argv else None
EXPORT_ONLY = "--export-only" in argv


def lerp(a, b, t):
    return a + (b - a) * t


def smoothstep(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3.0 - 2.0 * t)


def M(v, s):
    """Mirror to side s (+1 right, -1 left)."""
    return V((v.x * s, v.y, v.z))


def game(v):
    return (v.x, v.z, -v.y)


# ---------------------------------------------------------------- materials
# name: (game material id, surface pattern, linear colour, extra); ids as airframe::mat.
# Pattern 17 is the Hornet's painted skin (plane.wgsl P_HORNET): the paint
# projected from assets/fa18_paint.jpg; the countershaded vertex colour shows
# only where no picture faces the surface.
# Material 11 (exhaust) extra: 0 the nozzle petals, 1 inside the nozzle, 2 the
# flame holder; material 17 (the exhaust plume, drawn in the transparent pass)
# extra: 0 heat haze, 1 the flame, 2 its core, 3 discs across it.
MATS = {
    "skin": (15, 17, (0.24, 0.28, 0.33), 0),  # two-tone ghost grey, countershaded on export
    "radome": (15, 17, (0.21, 0.23, 0.26), 0),
    "inlet": (10, 0, (0.02, 0.02, 0.022), 0),
    "exhaust": (11, 0, (0.30, 0.28, 0.26), 0),
    "nozzle_in": (11, 0, (0.06, 0.055, 0.05), 1),
    "burner": (11, 0, (0.10, 0.09, 0.08), 2),
    "glass": (7, 0, (0.10, 0.10, 0.10), 0),
    "frame": (15, 0, (0.07, 0.075, 0.08), 0),
    "cockpit": (10, 0, (0.03, 0.03, 0.034), 0),
    "console": (15, 0, (0.055, 0.058, 0.06), 0),
    "seat": (15, 0, (0.05, 0.055, 0.05), 0),
    "seat_pad": (6, 0, (0.07, 0.085, 0.05), 0),
    "handle": (0, 0, (0.75, 0.55, 0.02), 0),
    "screen": (8, 0, (0.10, 0.95, 0.35), 8),
    "suit": (15, 0, (0.10, 0.115, 0.07), 0),
    "vest": (15, 0, (0.06, 0.07, 0.05), 0),
    "helmet": (0, 0, (0.50, 0.51, 0.50), 0),
    "visor": (0, 0, (0.012, 0.012, 0.016), 0),
    "mask": (4, 0, (0.03, 0.03, 0.032), 0),
    "strut": (0, 0, (0.78, 0.78, 0.76), 0),
    "door_in": (0, 0, (0.74, 0.74, 0.72), 0),
    "chrome": (3, 0, (0.85, 0.86, 0.88), 1),
    "steel": (3, 0, (0.32, 0.32, 0.33), 0),
    "rubber": (4, 0, (0.018, 0.018, 0.02), 0),
    "hub": (0, 0, (0.70, 0.70, 0.68), 0),
    "missile": (0, 0, (0.70, 0.70, 0.68), 0),
    "seeker": (10, 0, (0.04, 0.04, 0.05), 0),
    "nav_red": (8, 0, (1.0, 0.06, 0.05), 0),
    "nav_green": (8, 0, (0.05, 1.0, 0.25), 1),
    "tail_white": (8, 0, (1.0, 1.0, 1.0), 2),
    "landing": (8, 0, (1.0, 1.0, 0.95), 5),
    "slime": (8, 0, (0.45, 1.0, 0.25), 9),  # formation light strips: pale panels by day, glowing with the navigation lights at night
    "strobe": (8, 0, (1.0, 0.10, 0.06), 4),
    "haze": (17, 0, (1.0, 1.0, 1.0), 0),
    "flame": (17, 0, (1.0, 0.6, 0.3), 1),
    "core": (17, 0, (0.7, 0.8, 1.0), 2),
    "disc": (17, 0, (1.0, 0.8, 0.6), 3),
}
SKIN_LIGHT = (0.24, 0.28, 0.33)  # FS 36375 underneath
SKIN_DARK = (0.12, 0.145, 0.17)  # FS 36320 on top
COCKPIT_TUB = (0.03, 0.03, 0.034)

MAT = {}


def make_materials():
    for name, (gid, _pat, col, _x) in MATS.items():
        m = bpy.data.materials.get("fa18_" + name) or bpy.data.materials.new("fa18_" + name)
        srgb = tuple(max(c, 0.0) ** (1 / 2.2) for c in col)
        m.diffuse_color = (*srgb, 0.3 if gid in (7, 17) else 1.0)
        m["game"] = name
        MAT[name] = m


# ---------------------------------------------------------------- mesh building
class Geo:
    def __init__(self):
        self.bm = bmesh.new()
        self.mats = []
        self.facing = []  # (face, point it must face): open sheets, whose outside cannot be worked out

    def mi(self, m):
        if m not in self.mats:
            self.mats.append(m)
        return self.mats.index(m)

    def face(self, vs, m, toward=None):
        try:
            f = self.bm.faces.new(vs)
        except ValueError:
            return None
        f.material_index = self.mi(m)
        if toward is not None:
            self.facing.append((f, toward))
        return f

    def poly(self, pts, m, toward=None):
        """A flat face through these points."""
        return self.face([self.bm.verts.new(p) for p in pts], m, toward)

    def loft(self, rings, m, cap0=True, cap1=True, cap_mat=None, skip=None):
        """Join closed rings of equal length; a ring that is a single point closes to it.
        `skip(row, i)`: leave out the face between ring `row` and the next at segment i."""
        vr = []
        for r in rings:
            if max((p - r[0]).length for p in r) < 1e-5:
                vr.append([self.bm.verts.new(r[0])])
            else:
                vr.append([self.bm.verts.new(p) for p in r])
        n = len(rings[0])
        for row, (a, b) in enumerate(zip(vr, vr[1:])):
            for i in range(n):
                j = (i + 1) % n
                if skip and skip(row, i):
                    continue
                if len(a) == 1 and len(b) == 1:
                    continue
                if len(a) == 1:
                    self.face([a[0], b[i], b[j]], m)
                elif len(b) == 1:
                    self.face([a[i], a[j], b[0]], m)
                else:
                    self.face([a[i], a[j], b[j], b[i]], m)
        if cap0 and len(vr[0]) > 2:
            self.face(vr[0], cap_mat or m)
        if cap1 and len(vr[-1]) > 2:
            self.face(vr[-1], cap_mat or m)

    def strip(self, rows, m, toward=None):
        """Join open rows of points of equal length (a sheet, not a tube), facing `toward`."""
        vr = [[self.bm.verts.new(p) for p in r] for r in rows]
        for a, b in zip(vr, vr[1:]):
            for i in range(len(a) - 1):
                self.face([a[i], a[i + 1], b[i + 1], b[i]], m, toward)

    def box(self, c, h, m, basis=None):
        ax, ay, az = basis or (V((1, 0, 0)), V((0, 1, 0)), V((0, 0, 1)))
        vs = [self.bm.verts.new(c + ax * (h[0] * sx) + ay * (h[1] * sy) + az * (h[2] * sz)) for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)]
        for q in ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)):
            self.face([vs[k] for k in q], m)

    def plate(self, outline, normal, t, m, m_back=None):
        """A flat plate: this outline (3D points in one plane) made 2t thick along
        `normal`; `m_back` is the finish of the face away from the normal."""
        normal = normal.normalized()
        a = [self.bm.verts.new(p + normal * t) for p in outline]
        b = [self.bm.verts.new(p - normal * t) for p in outline]
        self.face(a, m)
        self.face(b[::-1], m_back or m)
        n = len(outline)
        for i in range(n):
            j = (i + 1) % n
            self.face([a[i], b[i], b[j], a[j]], m)

    def finish(self, name, smooth=38.0, pivot=None, axis=None, angle=0.0, recalc=True):
        bm = self.bm
        if recalc:
            bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        for f, toward in self.facing:
            f.normal_update()
            if (toward - f.calc_center_median()).dot(f.normal) < 0:
                f.normal_flip()
        me = bpy.data.meshes.new(name)
        bm.to_mesh(me)
        bm.free()
        for m in self.mats:
            me.materials.append(MAT[m])
        for p in me.polygons:
            p.use_smooth = True
        me.set_sharp_from_angle(angle=math.radians(smooth))
        ob = bpy.data.objects.new(name, me)
        COLL.objects.link(ob)
        # The hinge, in this object's own (local) frame: moving or turning the
        # object in Blender carries its hinge along.
        ob["pivot"] = list(pivot or V((0, 0, 0)))
        ob["axis"] = list((axis or V((1, 0, 0))).normalized())
        ob["angle"] = float(angle)
        return ob


def frame_at(t, ref=None):
    t = t.normalized()
    if ref is None:
        ref = V((0, 0, 1)) if abs(t.z) < 0.9 else V((1, 0, 0))
    u = t.cross(ref).normalized()
    return u, t.cross(u)


def tube(g, pts, r, m, n=10, caps=True, ref=None):
    rings = []
    for i, p in enumerate(pts):
        t = pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]
        u, w = frame_at(t, ref)
        rr = r(i / (len(pts) - 1)) if callable(r) else r
        rings.append([p + (u * math.cos(a) + w * math.sin(a)) * rr for a in (2 * math.pi * k / n for k in range(n))])
    g.loft(rings, m, caps, caps)


def lathe(g, c, axis, prof, m, n=24, cap_mat=None, caps=(True, True), zig=None):
    """Revolve (radius, distance along axis) about the axis through c. `zig`, one
    value for each point of the profile: every other point round that ring sits
    this fraction further in (overlapping petals)."""
    axis = axis.normalized()
    u, w = frame_at(axis)
    zig = zig or [0.0] * len(prof)
    rings = [[c + axis * a + (u * math.cos(t) + w * math.sin(t)) * (r * (1.0 - z * (k % 2))) for k, t in ((k, 2 * math.pi * k / n) for k in range(n))] for (r, a), z in zip(prof, zig)]
    g.loft(rings, m, caps[0], caps[1], cap_mat)


def ball(g, c, r, m, n=16, squash=(1.0, 1.0, 1.0)):
    rings = []
    for j in range(n // 2 + 1):
        t = math.pi * j / (n // 2)
        rings.append([c + V((r * math.sin(t) * math.cos(a) * squash[0], r * math.sin(t) * math.sin(a) * squash[1], -r * math.cos(t) * squash[2])) for a in (2 * math.pi * k / n for k in range(n))])
    g.loft(rings, m, False, False)


def catmull(p0, p1, p2, p3, t):
    t2, t3 = t * t, t * t * t
    return 0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 + (-p0 + 3 * p1 - 3 * p2 + p3) * t3)


def spline_rows(table, i0, i1, sub):
    """Rows of a station table from station i0 to i1, Catmull-Rom between them."""
    out = []
    n = len(table)
    for i in range(i0, i1):
        a, b, c, d = table[max(i - 1, 0)], table[i], table[i + 1], table[min(i + 2, n - 1)]
        for k in range(sub):
            t = k / sub
            out.append(tuple(catmull(a[j], b[j], c[j], d[j], t) for j in range(len(b))))
    out.append(table[i1])
    return out


def table_at(table, y):
    """Linear interpolation of a station table (stations by descending y)."""
    if y >= table[0][0]:
        return table[0]
    for a, b in zip(table, table[1:]):
        if a[0] >= y >= b[0]:
            t = (a[0] - y) / (a[0] - b[0])
            return tuple(lerp(a[k], b[k], t) for k in range(len(a)))
    return table[-1]


# ---------------------------------------------------------------- fuselage
# station y, half width, top, height of the widest point, bottom, superellipse exponent
# (kept in step with FUSE in src/aircraft/fa18/mod.rs)
FUSE = [
    (8.90, 0.000, -0.03, -0.03, -0.03, 2.0),
    (8.65, 0.120, 0.07, -0.04, -0.15, 2.0),
    (8.20, 0.240, 0.17, -0.05, -0.27, 2.0),
    (7.70, 0.345, 0.28, -0.07, -0.39, 2.1),
    (7.20, 0.430, 0.39, -0.08, -0.48, 2.2),
    (6.50, 0.515, 0.51, -0.09, -0.56, 2.3),
    (5.80, 0.575, 0.60, -0.09, -0.61, 2.4),
    (5.00, 0.615, 0.65, -0.08, -0.63, 2.6),
    (4.00, 0.650, 0.68, -0.06, -0.65, 2.7),
    (3.00, 0.700, 0.70, -0.06, -0.67, 2.8),
    (2.00, 0.800, 0.70, -0.08, -0.70, 3.0),
    (1.00, 0.920, 0.68, -0.10, -0.72, 3.1),
    (-0.50, 1.010, 0.64, -0.11, -0.74, 3.2),
    (-2.00, 1.070, 0.60, -0.12, -0.74, 3.2),
    (-3.50, 1.090, 0.55, -0.12, -0.72, 3.2),
    (-5.00, 1.070, 0.48, -0.10, -0.66, 3.0),
    (-6.20, 1.020, 0.40, -0.08, -0.58, 2.8),
    (-7.00, 0.950, 0.33, -0.06, -0.50, 2.6),
    (-7.40, 0.900, 0.28, -0.05, -0.45, 2.5),
]
RING_N = 44
# Round the cockpit opening the ring is laid out so that vertex TUB_K (and its
# mirror image) lies exactly on the opening's edge.
TUB_K = 6
ENGINE_X = 0.52  # the engines' centrelines
ENGINE_Z = -0.05


def trough(y):
    """How deep the valley between the two engines is at station y: on top, underneath."""
    return 0.13 * smoothstep(-5.2, -7.2, y), 0.15 * smoothstep(-2.6, -6.8, y)


def tub_cut(y):
    """Half width of the cockpit opening at station y."""
    return table_at(CANOPY, y)[1] - 0.03


def fuse_ring(y, hw, top, mid, bot, p, n=RING_N, tub=0.0):
    hw = max(hw, 0.0)
    q = n // 4
    angs = [2 * math.pi * k / n for k in range(n)]
    if tub > 0.0 and hw > 1e-6:
        # x = hw cos(a)^(2/p) = the opening's edge
        a0 = math.acos(min(tub_cut(y) / hw, 0.98) ** (p / 2))
        for i in range(q + 1):
            a = a0 * i / TUB_K if i <= TUB_K else a0 + (math.pi / 2 - a0) * (i - TUB_K) / (q - TUB_K)
            angs[i] = lerp(angs[i], a, tub)
            angs[2 * q - i] = math.pi - angs[i]
    t_top, t_bot = trough(y)
    pts = []
    for a in angs:
        c, s = math.cos(a), math.sin(a)
        x = hw * math.copysign(abs(c) ** (2 / p), c)
        f = abs(s) ** (2 / p)
        between = math.exp(-(x / 0.22) ** 2) * f
        z = mid + (top - mid) * f - t_top * between if s >= 0 else mid - (mid - bot) * f + t_bot * between
        pts.append(V((x, y, z)))
    return pts


def fuse_top(y):
    return table_at(FUSE, y)[2]


def fuse_z(y, x, upper=True):
    """Height of the fuselage skin at station y, x out from the centreline."""
    _, hw, top, mid, bot, p = table_at(FUSE, y)
    f = max(0.0, 1.0 - (min(abs(x), hw) / max(hw, 1e-6)) ** p) ** (1 / p)
    t_top, t_bot = trough(y)
    between = math.exp(-(x / 0.22) ** 2) * f
    return mid + (top - mid) * f - t_top * between if upper else mid - (mid - bot) * f + t_bot * between


def fuse_hw(y, z):
    """Half width of the fuselage at station y, height z."""
    _, hw, top, mid, bot, p = table_at(FUSE, y)
    f = min(1.0, abs(z - mid) / max((top - mid) if z >= mid else (mid - bot), 1e-6))
    return hw * max(0.0, 1.0 - f ** p) ** (1 / p)


# The cockpit: rows of the nose section the opening runs between, its floor.
TUB_Y = (6.0, 3.2)
TUB_FLOOR = 0.16
TUB_RIM = []  # per row of the opening: the ring's points from its right edge over the top to its left edge


def fuselage():
    idx = {round(r[0], 2): i for i, r in enumerate(FUSE)}
    for name, y0, y1 in (("radome", 8.90, 7.20), ("nose section", 7.20, 3.00), ("fuselage", 3.00, -5.00), ("tail cone", -5.00, -7.40)):
        g = Geo()
        rows = spline_rows(FUSE, idx[y0], idx[y1], 4)
        if name == "nose section":
            weight = lambda y: smoothstep(6.6, 6.0, y) * smoothstep(3.0, 3.25, y)
            rings = [fuse_ring(*r, tub=weight(r[0])) for r in rows]
            inside = [TUB_Y[1] < r[0] < TUB_Y[0] for r in rows]
            TUB_RIM.extend(rings[i][TUB_K:RING_N // 2 - TUB_K + 1] for i in range(len(rows)) if inside[i])
            g.loft(rings, "skin", skip=lambda row, i: inside[row] and inside[row + 1] and TUB_K <= i < RING_N // 2 - TUB_K)
            nose_details(g)
        else:
            g.loft([fuse_ring(*r) for r in rows], "radome" if name == "radome" else "skin")
        if name == "fuselage":
            belly_details(g)
        if name == "tail cone":
            tail_details(g)
        g.finish(name)

    # The dorsal spine behind the canopy.
    g = Geo()
    rings = []
    for k in range(15):
        f = k / 14
        y = lerp(3.0, -3.5, f)
        w = lerp(0.34, 0.20, f)
        base = fuse_top(y) - 0.10
        top = fuse_top(y) + lerp(0.18, 0.035, f ** 0.8)
        rings.append([V((w * math.cos(a), y, base + (top - base) * math.sin(a))) for a in (math.pi * j / 16 for j in range(17))])
    g.loft(rings, "skin")
    # Blade antenna and the upper anti-collision light.
    blade(g, V((0, 0.9, fuse_top(0.9) + 0.07)), 0.20, 0.17, 0.010)
    g.box(V((0, -1.6, fuse_top(-1.6) + 0.085)), (0.03, 0.05, 0.02), "strobe")
    g.finish("spine")


def blade(g, base, chord, height, t, down=False, m="skin"):
    """A swept blade antenna standing on `base`."""
    s = -1.0 if down else 1.0
    out = [base + V((0, chord / 2, 0)), base + V((0, -chord / 2, 0)), base + V((0, -chord / 2 - 0.02, s * height)), base + V((0, chord * 0.05, s * height))]
    g.plate(out, V((1, 0, 0)), t, m)


def nose_details(g):
    # The five blades of the IFF antenna ahead of the windscreen.
    for x in (-0.18, -0.09, 0.0, 0.09, 0.18):
        y = 6.72
        g.box(V((x, y, fuse_z(y, x) + 0.03)), (0.004, 0.05, 0.035), "skin")
    # Gun muzzle fairing and gas vents on top of the nose.
    g.box(V((0, 7.08, fuse_top(7.08) + 0.004)), (0.045, 0.16, 0.012), "inlet")
    # Angle-of-attack probes and pitot heads low on the sides.
    for s in (1, -1):
        y = 6.95
        x = fuse_hw(y, -0.20)
        tube(g, [V((s * (x - 0.02), y, -0.20)), V((s * (x + 0.07), y + 0.02, -0.22)), V((s * (x + 0.08), y + 0.26, -0.22))], lambda t: 0.014 - 0.006 * t, "steel", n=8)
        y = 6.2
        x = fuse_hw(y, 0.05)
        tube(g, [V((s * (x - 0.02), y, 0.05)), V((s * (x + 0.05), y, 0.05))], lambda t: 0.016 - 0.008 * t, "steel", n=8)
        # Formation light strip.
        y = 6.75
        x = fuse_hw(y, 0.0)
        g.box(V((s * (x + 0.002), y, 0.0)), (0.004, 0.30, 0.03), "slime")


def belly_details(g):
    # Blade antenna and the lower anti-collision light.
    blade(g, V((0, 1.9, fuse_z(1.9, 0, upper=False))), 0.20, 0.16, 0.010, down=True)
    g.box(V((0, -0.2, fuse_z(-0.2, 0, upper=False) - 0.012)), (0.03, 0.05, 0.02), "strobe")
    for s in (1, -1):
        # Formation light strip on the side.
        y = -4.4
        x = fuse_hw(y, 0.12)
        g.box(V((s * (x + 0.002), y, 0.12)), (0.004, 0.32, 0.03), "slime")


def tail_details(g):
    # The arresting hook, stowed in the valley between the engines.
    zb = lambda y: fuse_z(y, 0.0, upper=False) - 0.045
    pts = [V((0, y, zb(y))) for y in (-5.1, -5.8, -6.5, -7.1)]
    tube(g, pts, lambda t: 0.045 - 0.012 * t, "steel", n=10)
    g.box(V((0, -7.22, zb(-7.2) - 0.02)), (0.05, 0.11, 0.035), "steel")
    g.box(V((0, -5.05, zb(-5.05) + 0.01)), (0.07, 0.10, 0.05), "skin")


def intakes():
    # station y, centre x, half width, half height
    T = [(2.75, 1.08, 0.26, 0.36), (2.30, 1.08, 0.29, 0.39), (1.00, 1.06, 0.30, 0.40), (-0.50, 1.03, 0.27, 0.38), (-1.90, 0.95, 0.14, 0.30)]
    ZC = -0.30

    def ring(s, y, cx, hw, hh, inset=0.0):
        # The lip is raked: its top is further forward than its bottom.
        rake = 0.20 * smoothstep(2.1, 2.75, y)
        out = []
        for k in range(28):
            a = 2 * math.pi * k / 28
            c, sn = math.cos(a), math.sin(a)
            dz = (hh - inset) * math.copysign(abs(sn) ** 0.55, sn)
            out.append(V(((cx + (hw - inset) * math.copysign(abs(c) ** 0.55, c)) * s, y + rake * dz / hh, ZC + dz)))
        return out

    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        rows = spline_rows(T, 0, len(T) - 1, 4)
        g.loft([ring(s, *r) for r in rows], "skin", False, True)
        # The lip, and the duct running back from it into the dark.
        lip = ring(s, *T[0])
        g.loft([lip, ring(s, *T[0], inset=0.028)], "skin", False, False)
        y, cx, hw, hh = T[0]
        g.loft([ring(s, y, cx, hw, hh, 0.028), ring(s, y - 0.5, cx, hw, hh, 0.035), ring(s, y - 1.5, cx - 0.03, hw, hh, 0.10)], "inlet", False, True)
        # The splitter plate that keeps the fuselage's boundary layer out of the duct.
        x = (cx - hw - 0.012) * s
        g.plate([V((x, 3.25, ZC + 0.05)), V((x, 3.02, ZC + 0.40)), V((x, 2.35, ZC + 0.40)), V((x, 2.35, ZC - 0.40)), V((x, 2.60, ZC - 0.40))], V((1, 0, 0)), 0.008, "skin")
        g.finish(side + " intake")


def lexes():
    # station y, how far out the sharp edge is
    edge = [(6.05, 0.42), (5.75, 0.50), (5.30, 0.62), (4.40, 0.80), (3.10, 1.02), (1.90, 1.28), (1.05, 1.50), (0.60, 1.63)]
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        rings = []
        for y, xe in spline_rows(edge, 0, len(edge) - 1, 4):
            zc = lerp(0.12, 0.02, smoothstep(0.6, 6.0, y))
            # thick where it meets the fuselage, a sharp edge outboard, flat underneath
            rings.append([V((0.38 * s, y, zc + 0.055)), V((lerp(0.38, xe, 0.55) * s, y, zc + 0.034)), V((xe * s, y, zc + 0.005)), V((xe * s, y, zc - 0.005)), V((0.38 * s, y, zc - 0.035))])
        g.loft(rings, "skin")
        g.finish(side + " lex", smooth=25)


def nozzles():
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        c = V((ENGINE_X * s, -7.0, ENGINE_Z))
        aft = V((0, -1, 0))
        # The fixed shroud, then the overlapping petals closing in to the exit.
        lathe(g, c, aft, [(0.0, 0.0), (0.47, 0.0), (0.475, 0.13), (0.462, 0.15)], "exhaust", n=30, caps=(True, False))
        lathe(g, c, aft, [(0.462, 0.15), (0.455, 0.45), (0.365, 1.0), (0.338, 1.0)], "exhaust", n=30, caps=(False, False), zig=[0.0, 0.03, 0.03, 0.03])
        # Inside: the divergent flaps, the throat, the afterburner liner.
        lathe(g, c, aft, [(0.338, 1.0), (0.30, 0.80), (0.335, 0.60), (0.385, 0.44)], "nozzle_in", n=30, caps=(False, False), zig=[0.03, 0.03, 0.015, 0.0])
        # The flame holder's gutters and the turbine's exit cone behind them.
        lathe(g, c, aft, [(0.385, 0.44), (0.385, 0.42), (0.31, 0.42), (0.30, 0.47), (0.27, 0.47), (0.26, 0.42), (0.19, 0.42), (0.18, 0.46), (0.15, 0.46), (0.14, 0.42), (0.11, 0.42), (0.085, 0.50), (0.0, 0.66)], "burner", n=30, caps=(False, False))
        for k in range(8):
            a = 2 * math.pi * (k + 0.5) / 8
            d = V((math.cos(a), 0, math.sin(a)))
            g.box(c + aft * 0.445 + d * 0.25, (0.135, 0.02, 0.012), "burner", (d, aft, d.cross(aft)))
        g.finish(side + " nozzle", smooth=20)


def plume():
    """The exhaust, drawn in the game's transparent pass: shells and discs whose
    brightness plane.wgsl works out from the engines (material 17, `plume`).
    Haze first, then the flames, their cores and the discs: they are drawn in
    this order."""
    g = Geo()
    aft = V((0, -1, 0))

    def shell(prof, m, n, sub):
        for s in (1, -1):
            rows = spline_rows(prof, 0, len(prof) - 1, sub)
            lathe(g, V((ENGINE_X * s, -8.0, ENGINE_Z)), aft, [(max(r, 0.0), a) for a, r in rows], m, n=n, caps=(False, False))

    # distance aft of the nozzle exit, radius
    shell([(-0.05, 0.36), (1.0, 0.52), (3.0, 0.66), (6.0, 0.74), (9.0, 0.64), (11.0, 0.34), (12.0, 0.0)], "haze", 16, 2)
    shell([(-0.14, 0.325), (0.4, 0.37), (1.2, 0.36), (2.2, 0.32), (3.4, 0.26), (4.8, 0.18), (6.0, 0.09), (6.9, 0.0)], "flame", 20, 3)
    shell([(-0.14, 0.26), (0.5, 0.25), (1.3, 0.21), (2.3, 0.155), (3.4, 0.09), (4.5, 0.0)], "core", 20, 3)
    # Discs across the jet: what is seen of the flame looking up it from behind, where the shells are edge on.
    for s in (1, -1):
        for k in range(12):
            a = 0.12 + 5.2 * (k / 11) ** 1.3
            lathe(g, V((ENGINE_X * s, -8.0, ENGINE_Z)), aft, [(0.0, a), (0.36 * (1.0 - 0.75 * a / 5.6), a)], "disc", n=20, caps=(False, False))
    ob = g.finish("afterburner", smooth=80, recalc=False)
    ob.hide_render = True
    ob.display_type = "WIRE"


# ---------------------------------------------------------------- canopy and cockpit
# station y, half width at the sill, height of the top
CANOPY = [(6.25, 0.40, 0.64), (6.00, 0.45, 0.80), (5.70, 0.50, 1.00), (5.40, 0.52, 1.12), (5.00, 0.53, 1.20), (4.50, 0.53, 1.22), (4.00, 0.52, 1.20), (3.50, 0.48, 1.12), (3.10, 0.42, 0.98), (2.70, 0.34, 0.80)]


def canopy_arc(y, w, zt, grow=0.0, n=20):
    z0 = fuse_top(y) - 0.04
    return [V(((w + grow) * math.cos(a), y, z0 + (zt + grow - z0) * math.sin(a))) for a in (math.pi * k / n for k in range(n + 1))]


def canopy():
    g = Geo()
    rows = spline_rows(CANOPY, 0, len(CANOPY) - 1, 3)
    g.loft([canopy_arc(*r) for r in rows], "glass")
    g.finish("canopy", smooth=60)

    g = Geo()
    for yb in (5.55, 3.45):
        y, w, zt = table_at(CANOPY, yb)
        tube(g, canopy_arc(y, w, zt, grow=0.012), 0.028, "frame", n=8, ref=V((0, 1, 0)))
    for s in (1, -1):
        pts = [V((s * (r[1] + 0.01), r[0], fuse_top(r[0]) - 0.03)) for r in spline_rows(CANOPY, 0, len(CANOPY) - 1, 2)]
        tube(g, pts, 0.03, "frame", n=8)
        # The sill: the canopy's rail standing on the fuselage, painted outside, black inside.
        rows = spline_rows(CANOPY, 0, len(CANOPY) - 1, 2)
        outer = [[V((s * (w + 0.018), y, fuse_z(y, w) - 0.04)), V((s * (w + 0.018), y, fuse_top(y) - 0.02)), V((s * (w - 0.022), y, fuse_top(y) - 0.02))] for y, w, _ in rows]
        inner = [[V((s * (w - 0.022), y, fuse_top(y) - 0.02)), V((s * (w - 0.022), y, fuse_z(y, w) - 0.04))] for y, w, _ in rows]
        g.strip(outer, "skin", toward=V((s * 8.0, 4.5, 6.0)))
        g.strip(inner, "cockpit", toward=V((0, 4.5, 0.8)))
    g.finish("canopy frame")

    cockpit()


def cockpit():
    g = Geo()
    # The tub: walls down from the opening's edge, the floor, a bulkhead at each end.
    rim = TUB_RIM
    floor = lambda p: V((math.copysign(min(abs(p.x) - 0.03, 0.40), p.x), p.y, TUB_FLOOR))
    mid = V((0, (TUB_Y[0] + TUB_Y[1]) / 2, 0.6))
    g.strip([[r[0], floor(r[0])] for r in rim], "cockpit", toward=mid)
    g.strip([[r[-1], floor(r[-1])] for r in rim], "cockpit", toward=mid)
    g.strip([[floor(r[0]), floor(r[-1])] for r in rim], "cockpit", toward=mid)
    for r in (rim[0], rim[-1]):
        g.poly(list(r) + [floor(r[-1]), floor(r[0])], "cockpit", toward=mid)

    # Side consoles, the instrument panel with its three displays, the glareshield.
    for s in (1, -1):
        g.box(V((s * 0.36, 4.75, 0.40)), (0.085, 0.80, 0.11), "console")
    g.box(V((0, 5.52, 0.47)), (0.40, 0.035, 0.25), "console")
    for x, z, w, h in ((-0.21, 0.55, 0.085, 0.085), (0.21, 0.55, 0.085, 0.085), (0.0, 0.40, 0.08, 0.08), (0.0, 0.615, 0.07, 0.035)):
        g.box(V((x, 5.482, z)), (w, 0.004, h), "screen")
    g.box(V((0, 5.76, 0.735)), (0.38, 0.27, 0.018), "cockpit")  # glareshield, low enough to see over
    g.box(V((0, 5.70, 0.775)), (0.10, 0.07, 0.03), "cockpit")  # HUD projector
    for x in (-0.09, 0.09):
        g.box(V((x, 5.66, 0.86)), (0.008, 0.008, 0.07), "frame")  # HUD combiner posts
    # Stick and throttles.
    tube(g, [V((0, 4.93, TUB_FLOOR)), V((0, 4.90, 0.46))], 0.014, "frame", n=8)
    g.box(V((0, 4.895, 0.50)), (0.022, 0.024, 0.05), "cockpit")
    g.box(V((-0.34, 4.98, 0.545)), (0.035, 0.06, 0.035), "cockpit")

    # The ejection seat: back, headrest, pan, cushions, the firing handle between the knees.
    g.box(V((0, 4.13, 0.60)), (0.21, 0.05, 0.42), "seat")
    g.box(V((0, 4.15, 1.045)), (0.13, 0.075, 0.075), "seat")
    g.box(V((0, 4.38, 0.33)), (0.21, 0.24, 0.06), "seat")
    g.box(V((0, 4.19, 0.66)), (0.17, 0.02, 0.30), "seat_pad")
    g.box(V((0, 4.615, 0.43)), (0.06, 0.016, 0.035), "handle")
    for x in (-0.145, 0.145):
        g.box(V((x, 4.16, 1.14)), (0.012, 0.03, 0.03), "handle")  # canopy breakers

    # The pilot.
    g.box(V((0, 4.30, 0.60)), (0.17, 0.105, 0.20), "suit")  # torso
    g.box(V((0, 4.30, 0.80)), (0.215, 0.09, 0.045), "suit")  # shoulders
    g.box(V((0, 4.40, 0.62)), (0.145, 0.02, 0.15), "vest")  # survival vest and harness
    tube(g, [V((0, 4.32, 0.83)), V((0, 4.33, 0.91))], 0.055, "suit", n=10)
    head = V((0, 4.33, 0.985))
    ball(g, head, 0.132, "helmet", n=18, squash=(0.95, 1.05, 1.0))
    # Visor: a dark cap over the front of the helmet; the oxygen mask and its hose below it.
    lathe(g, head + V((0, 0.012, 0.022)), V((0, 1, 0)), [(0.133 * math.sin(t), 0.133 * math.cos(t)) for t in (math.radians(a) for a in (0, 15, 30, 45, 60, 72))], "visor", n=18, caps=(False, False))
    g.box(head + V((0, 0.125, -0.06)), (0.045, 0.03, 0.045), "mask")
    tube(g, [head + V((0, 0.13, -0.10)), head + V((0.04, 0.10, -0.20)), V((0.07, 4.41, 0.70))], 0.018, "mask", n=8)
    for s in (1, -1):
        elbow = V((s * 0.275, 4.52, 0.565))
        hand = V((0.03, 4.87, 0.51)) if s > 0 else V((-0.33, 4.92, 0.57))
        tube(g, [V((s * 0.215, 4.31, 0.78)), elbow], 0.052, "suit", n=10)
        tube(g, [elbow, hand], lambda t: 0.046 - 0.008 * t, "suit", n=10)
        ball(g, hand, 0.045, "mask", n=10)
        knee = V((s * 0.14, 4.84, 0.50))
        tube(g, [V((s * 0.10, 4.36, 0.44)), knee], 0.072, "suit", n=10)
        tube(g, [knee, V((s * 0.13, 5.22, 0.22))], 0.056, "suit", n=10)
    g.finish("cockpit", smooth=30)


# ---------------------------------------------------------------- lifting surfaces
def naca_t(x, t):
    x = max(0.0, min(1.0, x))
    return 5 * t * (0.2969 * math.sqrt(x) - 0.1260 * x - 0.3516 * x * x + 0.2843 * x ** 3 - 0.1036 * x ** 4)


def foil_ring(sect, t, a, b, m=14):
    le, c, cdir, up = sect
    xs = [a + (b - a) * (1 - math.cos(math.pi * k / m)) / 2 for k in range(m + 1)]
    upper = [le + cdir * (c * x) + up * max(c * naca_t(x, t), 0.004) for x in xs]
    lower = [le + cdir * (c * x) - up * max(c * naca_t(x, t), 0.004) for x in reversed(xs)]
    return upper + lower


def surface(g, sect, u0, u1, a, b, t, m="skin", nu=6):
    g.loft([foil_ring(sect(lerp(u0, u1, k / (nu - 1))), t, a, b) for k in range(nu)], m)


def hinge(sect, u, xc):
    le, c, cdir, _ = sect(u)
    return le + cdir * (c * xc)


# Wing (kept in step with wing_sect in src/aircraft/fa18/mod.rs)
W_X0, W_X1, W_LE0, W_SWEEP, W_C0, W_C1, W_Z0, W_ANH, W_T = 0.9, 5.75, 1.0, 0.503, 4.15, 1.65, 0.12, 0.052, 0.05
FLAP = (1.3, 3.45)
AIL = (3.5, 5.6)
LEF = (1.3, 5.6)


def wing_sect(s):
    def f(x):
        k = (x - W_X0) / (W_X1 - W_X0)
        le = V((x * s, W_LE0 - W_SWEEP * (x - W_X0), W_Z0 - W_ANH * (x - W_X0)))
        return le, lerp(W_C0, W_C1, k), V((0, -1, 0)), V((W_ANH * s, 0, 1)).normalized()
    return f


def pair_axis(right_axis, s):
    """The axis a mirrored piece turns about so equal angles move both sides alike (as Builder::pair)."""
    return right_axis if s > 0 else -M(right_axis, -1)


def pylon(g, s, x):
    """A stores pylon under the wing at span station x."""
    le, c, _, _ = wing_sect(s)(x)
    zt = le.z + 0.01
    y0 = le.y - 0.40
    rings = []
    for dy, depth, w in ((0.0, 0.03, 0.012), (-0.22, 0.20, 0.036), (-0.55, 0.30, 0.042), (-1.45, 0.30, 0.042), (-1.75, 0.16, 0.030), (-1.95, 0.03, 0.012)):
        y = y0 + dy
        rings.append([V((x * s - w, y, zt)), V((x * s + w, y, zt)), V((x * s + w, y, zt - depth)), V((x * s - w, y, zt - depth))])
    g.loft(rings, "skin")


def wings():
    for s, side in ((1, "right"), (-1, "left")):
        ws = wing_sect(s)
        g = Geo()
        surface(g, ws, W_X0, LEF[0], 0.0, 1.0, W_T, nu=3)
        surface(g, ws, LEF[0], AIL[1], 0.15, 0.75, W_T, nu=10)
        surface(g, ws, FLAP[1], AIL[0], 0.75, 1.0, W_T, nu=2)
        surface(g, ws, AIL[1], W_X1, 0.0, 1.0, W_T, nu=3)
        pylon(g, s, 2.35)
        pylon(g, s, 3.55)
        g.finish(side + " wing")

        for name, (x0, x1), (a, b) in (("le flap", LEF, (0.0, 0.15)), ("flap", FLAP, (0.75, 1.0)), ("aileron", AIL, (0.75, 1.0))):
            g = Geo()
            surface(g, ws, x0, x1, a, b, W_T, nu=8)
            xc = 0.15 if name == "le flap" else 0.75
            p0, p1 = hinge(wing_sect(1), x0, xc), hinge(wing_sect(1), x1, xc)
            g.finish(side + " " + name, pivot=M(p0, s), axis=pair_axis((p1 - p0).normalized(), s))

        # Wingtip launch rail with a Sidewinder on it.
        g = Geo()
        g.box(V((5.80 * s, -2.05, -0.15)), (0.045, 0.85, 0.05), "skin")
        c = V((5.90 * s, -2.75, -0.20))
        lathe(g, c, V((0, 1, 0)), [(0.0, 0.0), (0.0635, 0.0), (0.0635, 2.62)], "missile", n=16)
        lathe(g, c, V((0, 1, 0)), [(0.0635, 2.62), (0.058, 2.72), (0.040, 2.80), (0.0, 2.86)], "seeker", n=16)
        for k in range(4):
            a = math.radians(45 + 90 * k)
            d = V((math.cos(a), 0, math.sin(a)))
            for yc, span, chord in ((-2.55, 0.20, 0.30), (-0.05, 0.15, 0.20)):
                g.box(c + V((0, yc + 2.75, 0)) + d * (0.0635 + span / 2), (span / 2, chord / 2, 0.006), "missile", (d, V((0, 1, 0)), d.cross(V((0, 1, 0)))))
        g.finish(side + " tip rail", smooth=30)

        g = Geo()
        g.box(V((5.70 * s, -1.38, -0.125)), (0.035, 0.05, 0.022), "nav_green" if s > 0 else "nav_red")
        g.finish(side + " nav light", smooth=10)


def stab_sect(s):
    def f(x):
        k = (x - 0.85) / (3.3 - 0.85)
        le = V((x * s, lerp(-5.2, -7.2, k), lerp(-0.12, -0.20, k)))
        return le, lerp(2.5, 0.9, k), V((0, -1, 0)), V((0.033 * s, 0, 1)).normalized()
    return f


FIN_CANT = math.radians(20.0)


def fin_sect(s):
    ca, sa = math.cos(FIN_CANT), math.sin(FIN_CANT)
    def f(u):
        p = V((0.95, 0.0, 0.25)) + V((sa, 0, ca)) * (2.75 * u)
        le = V((p.x * s, -3.7 - 1.93 * u, p.z))
        return le, lerp(2.7, 1.1, u), V((0, -1, 0)), V((ca * s, 0, -sa))
    return f


def tail():
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        surface(g, stab_sect(s), 0.85, 3.3, 0.0, 1.0, 0.04, nu=8)
        pv = V((0.85, -6.15, -0.12))
        g.finish(side + " stabilator", pivot=M(pv, s), axis=pair_axis(V((1, 0, -0.033)).normalized(), s))

        fs = fin_sect(s)
        g = Geo()
        surface(g, fs, 0.0, 0.6, 0.0, 0.7, 0.045, nu=6)
        surface(g, fs, 0.6, 1.0, 0.0, 1.0, 0.045, nu=5)
        # The antenna fairing along the fin's tip, and the formation light strip below it.
        le, c, cdir, out = fs(1.0)
        up = V((math.sin(FIN_CANT) * s, 0, math.cos(FIN_CANT)))
        tube(g, [le + cdir * (c * k) + up * 0.02 for k in (-0.12, 0.1, 0.5, 0.9, 1.12)], lambda t: 0.05 * (1.0 - (2 * t - 1) ** 4) + 0.006, "skin", n=10)
        le, c, cdir, out = fs(0.74)
        mid = le + cdir * (c * 0.42)
        for side_k in (1, -1):
            g.box(mid + out * (side_k * (c * naca_t(0.42, 0.045) + 0.002)), (0.004, 0.07, 0.26), "slime", (out, cdir, up))
        g.finish(side + " fin")

        g = Geo()
        surface(g, fs, 0.0, 0.6, 0.7, 1.0, 0.045, nu=6)
        p0, p1 = hinge(fs, 0.0, 0.7), hinge(fs, 0.6, 0.7)
        # Both rudders swing the same way for the same command: no pair flip here.
        g.finish(side + " rudder", pivot=p0, axis=(p1 - p0).normalized())

    # The dorsal speedbrake between the fins.
    g = Geo()
    rings = []
    for k in range(5):
        y = lerp(-3.6, -4.9, k / 4)
        z = fuse_top(y) + 0.005
        rings.append([V((-0.38, y, z)), V((0.38, y, z)), V((0.38, y, z + 0.035)), V((-0.38, y, z + 0.035))])
    g.loft(rings, "skin")
    g.finish("speedbrake", smooth=20, pivot=V((0, -3.6, fuse_top(-3.6) + 0.02)), axis=V((1, 0, 0)))

    g = Geo()
    g.box(V((0, -7.43, 0.10)), (0.04, 0.03, 0.025), "tail_white")
    g.finish("tail light", smooth=10)


# ---------------------------------------------------------------- landing gear (modelled down and locked)
NOSE_PIVOT = V((0, 5.0, -0.50))
NOSE_AXLE = V((0, 4.80, -1.67))
NOSE_R = 0.28
MAIN_PIVOT = V((1.15, -0.25, -0.45))
MAIN_AXLE = V((1.55, -0.60, -1.57))
MAIN_R = 0.38
# where each wheel centre ends up with the gear up
NOSE_STOWED = V((0, 6.18, -0.32))
MAIN_STOWED = V((0.85, -1.45, -0.35))


def wheel(g, c, r, hw):
    lathe(g, c, V((1, 0, 0)), [(0.0, -hw * 0.85), (r * 0.55, -hw * 0.85), (r * 0.55, -hw), (r - 0.04, -hw), (r, -hw * 0.6), (r, hw * 0.6), (r - 0.04, hw), (r * 0.55, hw), (r * 0.55, hw * 0.85), (0.0, hw * 0.85)], "rubber", n=28)
    lathe(g, c, V((1, 0, 0)), [(0.0, -hw * 0.9), (r * 0.56, -hw * 0.9), (r * 0.56, hw * 0.9), (0.0, hw * 0.9)], "hub", n=20)
    lathe(g, c, V((1, 0, 0)), [(0.0, -hw * 1.02), (r * 0.16, -hw * 1.02), (r * 0.20, -hw * 0.9), (r * 0.20, hw * 0.9), (r * 0.16, hw * 1.02), (0.0, hw * 1.02)], "steel", n=12)


def stow(pivot, down, up):
    d0, d1 = down - pivot, up - pivot
    return d0.cross(d1).normalized(), d0.angle(d1)


def scissors(g, top, bottom, back, m="strut"):
    """Torque links: two arms from the strut's ends meeting behind it."""
    knee = (top + bottom) / 2 + back
    for a, b in ((top, knee), (knee, bottom)):
        tube(g, [a, b], 0.014, m, n=6)


def gear():
    # Nose: retracts forward into the nose.
    ax, ang = stow(NOSE_PIVOT, NOSE_AXLE, NOSE_STOWED)
    g = Geo()
    top = NOSE_PIVOT
    along = (NOSE_PIVOT - NOSE_AXLE).normalized()
    low = NOSE_AXLE + along * 0.32
    tube(g, [top, low], 0.065, "strut")
    tube(g, [V((0, 4.25, -0.55)), V((0, 4.93, -1.05))], 0.035, "strut")
    tube(g, [low + V((0, 0.02, 0.05)), V((0, 5.52, -1.60))], 0.028, "strut")  # launch bar
    g.box(V((0, 4.99, -1.18)), (0.05, 0.04, 0.04), "landing")
    g.box(V((0.085, 4.97, -0.98)), (0.03, 0.03, 0.03), "landing")  # taxi light
    # The forward door rides on the strut.
    fwd = V((0, 1, 0)) - along * along.y
    fwd.normalize()
    g.box(top - along * 0.42 + fwd * 0.10, (0.15, 0.008, 0.36), "door_in", (V((1, 0, 0)), fwd, along))
    g.finish("nose leg", pivot=NOSE_PIVOT, axis=ax, angle=ang)

    down = (NOSE_AXLE - NOSE_PIVOT).normalized()
    g = Geo()
    tube(g, [low + along * 0.08, NOSE_AXLE], 0.05, "chrome")
    tube(g, [NOSE_AXLE + V((-0.22, 0, 0)), NOSE_AXLE + V((0.22, 0, 0))], 0.03, "strut")
    scissors(g, low + V((0, -0.05, 0)), NOSE_AXLE + along * 0.06 + V((0, -0.04, 0)), V((0, -0.13, 0)))
    g.finish("nose fork", pivot=NOSE_AXLE, axis=down)

    g = Geo()
    for x in (-0.13, 0.13):
        wheel(g, NOSE_AXLE + V((x, 0, 0)), NOSE_R, 0.075)
    g.finish("nose wheel", pivot=NOSE_AXLE, axis=V((1, 0, 0)))

    # Mains: swing aft and in under the intakes.
    for s, side in ((1, "right"), (-1, "left")):
        ax, ang = stow(MAIN_PIVOT, MAIN_AXLE, MAIN_STOWED)
        g = Geo()
        a = M(MAIN_AXLE, s)
        strut = (MAIN_PIVOT - MAIN_AXLE).normalized()
        knee = M(MAIN_AXLE + strut * 0.12, s)
        tube(g, [M(MAIN_PIVOT, s), knee], 0.08, "strut")
        tube(g, [M(V((1.0, 0.40, -0.50)), s), M(V((1.42, -0.45, -1.10)), s)], 0.04, "strut")
        tube(g, [M(V((0.80, -0.75, -0.48)), s), M(MAIN_AXLE + strut * 0.55, s)], 0.03, "strut")  # side brace
        tube(g, [knee, a + V((-0.10 * s, 0, 0))], 0.06, "chrome")
        scissors(g, M(MAIN_AXLE + strut * 0.50 + V((0, -0.07, 0)), s), a + V((-0.10 * s, -0.06, 0.02)), V((0, -0.16, 0)))
        # The strut door rides on the leg, outboard of it.
        sd = M(strut, s)
        out = V((s, 0, 0)) - sd * (sd.x * s)
        out.normalize()
        g.box(M(MAIN_AXLE + strut * 0.62, s) + out * 0.12, (0.008, 0.17, 0.40), "door_in", (out, sd.cross(out), sd))
        g.finish(side + " main leg", pivot=M(MAIN_PIVOT, s), axis=pair_axis(ax, s), angle=ang)

        g = Geo()
        wheel(g, a + V((0.02 * s, 0, 0)), MAIN_R, 0.14)
        g.finish(side + " main wheel", pivot=a, axis=V((1, 0, 0)))


def door(name, h0, h1, d_open, d_closed, outside):
    """A gear door hinged along h0..h1, modelled open (its free edge `d_open` from
    the hinge); it closes to `d_closed`, just inside the skin. Painted on the
    face toward `outside`, white on the other."""
    g = Geo()
    g.plate([h0, h1, h1 + d_open, h0 + d_open], outside, 0.008, "skin", m_back="door_in")
    ax, ang = stow(h0, h0 + d_open, h0 + d_closed)
    along = (h1 - h0).normalized()
    g.finish(name, smooth=20, pivot=h0, axis=along if along.dot(ax) > 0 else -along, angle=ang)


def doors():
    for s, side in ((1, "right"), (-1, "left")):
        # Nose: one each side of the well, hanging down beside the leg.
        z = lambda y: fuse_z(y, 0.26, upper=False) + 0.006
        door(side + " nose door", V((0.26 * s, 6.40, z(6.40))), V((0.26 * s, 5.05, z(5.05))), V((0.05 * s, 0, -0.25)), V((-0.255 * s, 0, -0.015)), V((s, 0, 0)))
        # Mains: hinged inboard under the intake, hanging between the leg and the belly.
        door(side + " main door", V((0.84 * s, 0.30, -0.672)), V((0.84 * s, -1.20, -0.672)), V((-0.04 * s, 0, -0.28)), V((0.283 * s, 0, 0.0)), V((-s, 0, 0)))


# ---------------------------------------------------------------- export
# Every piece the game looks for (PIECES in src/aircraft/fa18/mod.rs); other objects are ignored.
EXPECTED = ["radome", "nose section", "fuselage", "tail cone", "spine", "canopy", "canopy frame", "cockpit", "speedbrake", "tail light", "nose leg", "nose fork", "nose wheel", "afterburner"] + [
    f"{s} {n}" for s in ("left", "right") for n in ("intake", "lex", "nozzle", "wing", "le flap", "flap", "aileron", "tip rail", "nav light", "stabilator", "fin", "rudder", "main leg", "main wheel", "nose door", "main door")]


def game_mat(m):
    """Which game material a Blender material is: its "game" property, its fa18_ name, or else the skin."""
    if m is None:
        return "skin"
    name = m.get("game") or (m.name[5:].split(".")[0] if m.name.startswith("fa18_") else None)
    return name if name in MATS else "skin"


def export(path):
    out = bytearray(b"FFAC")
    obs = [o for o in COLL.all_objects if o.type == "MESH"]
    names = {o.name for o in obs}
    missing = [n for n in EXPECTED if n not in names]
    if missing:
        raise SystemExit(f"fa18 export: the game needs these pieces, missing from the '{COLL.name}' collection: {', '.join(missing)}")
    deps = bpy.context.evaluated_depsgraph_get()
    out += struct.pack("<II", 1, len(obs))
    total = 0
    for ob in obs:
        ob_eval = ob.evaluated_get(deps)
        me = ob_eval.to_mesh()  # with its modifiers applied
        mw = ob.matrix_world
        nm = mw.to_3x3().inverted().transposed()
        pivot = mw @ V(ob.get("pivot", (0, 0, 0)))
        axis = (mw.to_3x3() @ V(ob.get("axis", (1, 0, 0)))).normalized()
        me.calc_loop_triangles()
        verts, index, idx = [], [], {}
        for tri in me.loop_triangles:
            mat = game_mat(me.materials[tri.material_index] if tri.material_index < len(me.materials) else None)
            gid, pat, col, extra = MATS[mat]
            for k in range(3):
                vi = tri.vertices[k]
                n = (nm @ V(tri.split_normals[k])).normalized()
                key = (vi, round(n.x, 3), round(n.y, 3), round(n.z, 3), mat)
                if key not in idx:
                    p = mw @ me.vertices[vi].co
                    c = col
                    if mat == "skin":
                        t = smoothstep(-0.15, 0.45, n.z)
                        c = tuple(lerp(SKIN_LIGHT[j], SKIN_DARK[j], t) for j in range(3))
                        if 3.0 < p.y < 6.2 and p.z > 0.3 and abs(p.x) < table_at(CANOPY, p.y)[1] - 0.021:
                            c = COCKPIT_TUB  # under the canopy
                    idx[key] = len(verts)
                    verts.append((*game(p), *game(n), *c, float(gid), float(pat), 0.3, float(extra)))
                index.append(idx[key])
        ob_eval.to_mesh_clear()
        name = ob.name.encode()
        out += struct.pack("<I", len(name)) + name
        out += struct.pack("<7f", *game(pivot), *game(axis), float(ob.get("angle", 0.0)))
        out += struct.pack("<I", len(verts))
        for v in verts:
            out += struct.pack("<13f", *v)
        out += struct.pack("<I", len(index))
        out += struct.pack("<%dI" % len(index), *index)
        total += len(index) // 3
        print(f"  {ob.name}: {len(verts)} vertices, {len(index) // 3} triangles")
    with gzip.open(path, "wb", compresslevel=9) as f:
        f.write(bytes(out))
    print("wrote", path, len(out), "bytes raw,", total, "triangles")


# ---------------------------------------------------------------- pictures
def camera():
    sc = bpy.context.scene
    cam_data = bpy.data.cameras.new("cam")
    cam = bpy.data.objects.new("cam", cam_data)
    sc.collection.objects.link(cam)
    sc.camera = cam
    return cam, cam_data


def render_previews(outdir):
    os.makedirs(outdir, exist_ok=True)
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_WORKBENCH"
    sc.display.shading.light = "STUDIO"
    sc.display.shading.color_type = "MATERIAL"
    sc.display.shading.show_cavity = True
    sc.render.resolution_x, sc.render.resolution_y = 1280, 800
    sc.render.film_transparent = False
    cam, cam_data = camera()
    target = V((0, 0, 0.3))
    views = (("three_quarter", V((14, 15, 7)), 50, target), ("side", V((26, 0.01, 0.6)), 50, target), ("top", V((0.01, 0.3, 46)), 50, target), ("front", V((0.01, 26, 1.0)), 50, target), ("rear_low", V((-10, -14, -2.5)), 50, target),
             ("rear", V((3.5, -15, 2.2)), 60, V((0, -6, 0.2))), ("nozzles", V((2.2, -11.5, 0.9)), 70, V((0, -7.5, 0.0))), ("cockpit", V((3.0, 7.6, 2.6)), 75, V((0, 4.6, 0.8))), ("under", V((7, 9, -5)), 50, V((0, 0.5, -0.3))))
    for name, eye, lens, look in views:
        cam.location = eye
        cam.rotation_euler = (look - eye).to_track_quat("-Z", "Y").to_euler()
        cam_data.lens = lens
        sc.render.filepath = os.path.join(outdir, name + ".png")
        bpy.ops.render.render(write_still=True)


# The paint is projected straight down, straight up and from the side: these
# are the rectangles (m) those three pictures cover. plane.wgsl (`hornet_paint`)
# and fa18_paint.py use the same numbers.
GUIDE_SPAN = 13.2   # across the top and bottom views; up the side view
GUIDE_LEN = 19.8    # fore and aft in all three
GUIDE_Y = 0.4       # station at the middle of the pictures
GUIDE_Z = 1.0       # height at the middle of the side view
GUIDE_PX = 1536     # pixels fore and aft


def render_guides(outdir):
    os.makedirs(outdir, exist_ok=True)
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_WORKBENCH"
    sc.render.film_transparent = False
    sc.view_settings.view_transform = "Standard"
    world = sc.world or bpy.data.worlds.new("guide")
    sc.world = world
    sh = sc.display.shading
    # Gear up and nothing on the rails: only what the paint goes on.
    hidden = [o for o in COLL.all_objects if any(k in o.name for k in ("leg", "wheel", "fork", "door", "tip rail", "afterburner"))]
    for o in hidden:
        o.hide_render = True
    cam, cam_data = camera()
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = GUIDE_LEN
    short = round(GUIDE_PX * GUIDE_SPAN / GUIDE_LEN)
    views = (("top", V((0, GUIDE_Y, 40)), V((0, 0, -1)), V((0, 1, 0)), (short, GUIDE_PX)),
             ("bottom", V((0, GUIDE_Y, -40)), V((0, 0, 1)), V((0, 1, 0)), (short, GUIDE_PX)),
             ("side", V((-40, GUIDE_Y, GUIDE_Z)), V((1, 0, 0)), V((0, 0, 1)), (GUIDE_PX, short)))
    for name, eye, look, up, (w, h) in views:
        cam.location = eye
        right = look.cross(up)
        cam.rotation_euler = Matrix((right, up, -look)).transposed().to_euler()
        sc.render.resolution_x, sc.render.resolution_y = w, h
        for kind in ("guide", "mask"):
            if kind == "guide":
                sh.light, sh.color_type, sh.single_color = "STUDIO", "SINGLE", (0.62, 0.64, 0.66)
                sh.show_cavity, sh.cavity_type, sh.show_object_outline = True, "BOTH", True
                world.color = (1, 1, 1)
            else:
                sh.light, sh.color_type, sh.single_color = "FLAT", "SINGLE", (1, 1, 1)
                sh.show_cavity, sh.show_object_outline = False, False
                world.color = (0, 0, 0)
            sc.render.filepath = os.path.join(outdir, f"{name}_{kind}.png")
            bpy.ops.render.render(write_still=True)
    for o in hidden:
        o.hide_render = o.name == "afterburner"


def main():
    global COLL
    if EXPORT_ONLY:
        # Export the .blend that is open, edits and all, without rebuilding anything.
        COLL = bpy.data.collections.get("FA-18C")
        if COLL is None:
            raise SystemExit("fa18 export: no 'FA-18C' collection in this file (open assets/blender/fa18.blend)")
        export(os.path.join(ASSETS, "fa18.bin.gz"))
    else:
        for ob in list(bpy.data.objects):
            bpy.data.objects.remove(ob, do_unlink=True)
        COLL = bpy.data.collections.new("FA-18C")
        bpy.context.scene.collection.children.link(COLL)
        make_materials()
        fuselage()
        intakes()
        lexes()
        nozzles()
        canopy()
        wings()
        tail()
        gear()
        doors()
        plume()
        os.makedirs(os.path.join(ASSETS, "blender"), exist_ok=True)
        export(os.path.join(ASSETS, "fa18.bin.gz"))
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ASSETS, "blender", "fa18.blend"))
    if RENDER:
        render_previews(RENDER)
    if GUIDES:
        render_guides(GUIDES)


main()
