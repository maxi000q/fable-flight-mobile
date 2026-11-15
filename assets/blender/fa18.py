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
# The gear wells are cut into the fuselage with boolean modifiers (cutters in
# the "FA-18C well cutters" collection, not exported) and the gear doors are
# patches of the skin itself. Every export ends with a gear check: a sweep of
# the legs from up to down that names any part passing through the skin.
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
    "strut": (0, 0, (0.78, 0.78, 0.76), 0),
    "door_in": (0, 0, (0.74, 0.74, 0.72), 0),
    "well": (0, 0, (0.62, 0.63, 0.60), 0),  # inside the gear wells
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

    def finish(self, name, smooth=38.0, pivot=None, axis=None, angle=0.0, recalc=True, parent=None, coll=None):
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
        (coll or COLL).objects.link(ob)
        # The hinge, in this object's own (local) frame: moving or turning the
        # object in Blender carries its hinge along.
        ob["pivot"] = list(pivot or V((0, 0, 0)))
        ob["axis"] = list((axis or V((1, 0, 0))).normalized())
        ob["angle"] = float(angle)
        if parent:
            ob["parent"] = parent  # what it hangs from (as PIECES in Rust), for previews
        return ob


def cutter(name, rings):
    """A solid lofted through closed rings that a well is cut with: kept in its
    own collection, out of the export. What it cuts takes its finish (well)."""
    g = Geo()
    g.loft(rings, "well")
    ob = g.finish(name, smooth=30, coll=CUTS)
    ob.hide_render = True
    ob.display_type = "WIRE"
    return ob


def carve(ob, cutters, smooth=38.0):
    """Cut these solids out of a piece's skin, leaving their walls inside it (the wells)."""
    for c in cutters:
        md = ob.modifiers.new("well", "BOOLEAN")
        md.operation, md.solver, md.object = "DIFFERENCE", "EXACT", c
        md.material_mode = "TRANSFER"
        md.use_hole_tolerant = True
    deps = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(ob.evaluated_get(deps))
    ob.modifiers.clear()
    old = ob.data
    ob.data = me
    bpy.data.meshes.remove(old)
    for p in me.polygons:
        p.use_smooth = True
    me.set_sharp_from_angle(angle=math.radians(smooth))


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
    (1.00, 0.960, 0.68, -0.10, -0.72, 3.1),
    (-0.50, 1.060, 0.64, -0.11, -0.74, 3.2),
    (-2.00, 1.100, 0.60, -0.12, -0.74, 3.2),
    (-3.50, 1.100, 0.55, -0.12, -0.72, 3.2),
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


def p_lower(y, p):
    """The lower half's superellipse exponent: boxier under the intake ducts,
    where the main wheels stow."""
    return p + 1.6 * smoothstep(3.0, 1.6, y) * smoothstep(-4.8, -3.2, y)


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
    pl = p_lower(y, p)
    pts = []
    for a in angs:
        c, s = math.cos(a), math.sin(a)
        e = p if s >= 0 else pl
        x = hw * math.copysign(abs(c) ** (2 / e), c)
        f = abs(s) ** (2 / e)
        between = math.exp(-(x / 0.22) ** 2) * f
        z = mid + (top - mid) * f - t_top * between if s >= 0 else mid - (mid - bot) * f + t_bot * between
        pts.append(V((x, y, z)))
    return pts


def fuse_top(y):
    return table_at(FUSE, y)[2]


def fuse_z(y, x, upper=True):
    """Height of the fuselage skin at station y, x out from the centreline."""
    _, hw, top, mid, bot, p = table_at(FUSE, y)
    if not upper:
        p = p_lower(y, p)
    f = max(0.0, 1.0 - (min(abs(x), hw) / max(hw, 1e-6)) ** p) ** (1 / p)
    t_top, t_bot = trough(y)
    between = math.exp(-(x / 0.22) ** 2) * f
    return mid + (top - mid) * f - t_top * between if upper else mid - (mid - bot) * f + t_bot * between


def fuse_hw(y, z):
    """Half width of the fuselage at station y, height z."""
    _, hw, top, mid, bot, p = table_at(FUSE, y)
    if z < mid:
        p = p_lower(y, p)
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
        ob = g.finish(name)
        if name == "nose section":
            carve(ob, [nose_well()])
        if name == "fuselage":
            carve(ob, [main_well(1), main_well(-1)])

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


# The gear wells: the nose well under the cockpit (half width, aft end, front
# end, roof) and a main well each side under the intake duct (inboard edge,
# front, back, roof; its outboard edge is where the skin is MAIN_WELL_Z high).
NOSE_WELL = (0.25, 4.85, 6.62, 0.08)
MAIN_WELL = (0.14, -0.05, -2.20, 0.0)
MAIN_WELL_Z = -0.55
MAIN_DOOR_SPLIT = -0.62  # where the strut door ends and the main door starts


def nose_well():
    hw, y0, y1, zt = NOSE_WELL
    return cutter("nose well", [[V((-hw, y, -1.2)), V((hw, y, -1.2)), V((hw, y, zt)), V((-hw, y, zt))] for y in (y1, y0)])


def main_well_out(y):
    return fuse_hw(y, MAIN_WELL_Z)


def main_well_x(y, z):
    """The main well's outboard wall: straight up from the opening's edge (the
    skin's facets round the lower corner leave no room to lean it out)."""
    return main_well_out(y)


def main_well(s):
    xi, y0, y1, zt = MAIN_WELL
    rings = []
    for k in range(7):
        y = lerp(y0, y1, k / 6)
        xo = main_well_out(y)
        rings.append([V((xi * s, y, -1.3)), V((xo * s, y, -1.3)), V((xo * s, y, MAIN_WELL_Z)), V((main_well_x(y, zt) * s, y, zt)), V((xi * s, y, zt))])
    return cutter(("right" if s > 0 else "left") + " main well", rings)


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
    # station y, centre x, half width, half height, centre height. Aft of the
    # lip the trunk climbs and narrows toward the engine face, fairing into the
    # fuselage's side ahead of the main gear well.
    T = [(2.75, 1.08, 0.26, 0.36, -0.36), (2.30, 1.08, 0.29, 0.39, -0.37), (1.30, 1.06, 0.30, 0.40, -0.37), (0.60, 0.99, 0.27, 0.37, -0.33), (0.08, 0.84, 0.19, 0.28, -0.25)]

    def ring(s, y, cx, hw, hh, zc, inset=0.0):
        # The lip is raked: its top is further forward than its bottom.
        rake = 0.20 * smoothstep(2.1, 2.75, y)
        out = []
        for k in range(32):
            a = 2 * math.pi * k / 32
            c, sn = math.cos(a), math.sin(a)
            dz = (hh - inset) * math.copysign(abs(sn) ** 0.55, sn)
            out.append(V(((cx + (hw - inset) * math.copysign(abs(c) ** 0.55, c)) * s, y + rake * dz / hh, zc + dz)))
        return out

    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        rows = spline_rows(T, 0, len(T) - 1, 5)
        g.loft([ring(s, *r) for r in rows], "skin", False, True)
        # The lip, and the duct running back from it into the dark.
        y, cx, hw, hh, zc = T[0]
        g.loft([ring(s, *T[0]), ring(s, *T[0], inset=0.028)], "skin", False, False)
        g.loft([ring(s, y, cx, hw, hh, zc, 0.028), ring(s, y - 0.5, cx, hw, hh, zc, 0.035), ring(s, y - 1.5, cx - 0.03, hw, hh, zc + 0.02, 0.10)], "inlet", False, True)
        # The splitter plate that keeps the fuselage's boundary layer out of the
        # duct, and the struts holding it off the fuselage.
        x = (cx - hw - 0.012) * s
        g.plate([V((x, 3.25, zc + 0.05)), V((x, 3.02, zc + 0.40)), V((x, 2.35, zc + 0.40)), V((x, 2.35, zc - 0.40)), V((x, 2.60, zc - 0.40))], V((1, 0, 0)), 0.008, "skin")
        for zz in (zc + 0.22, zc - 0.18):
            x0 = fuse_hw(2.55, zz) - 0.01
            g.box(V(((x0 + cx - hw) / 2 * s, 2.50, zz)), (abs(cx - hw - x0) / 2 + 0.005, 0.12, 0.006), "skin")
        g.finish(side + " intake")


# The leading-edge extension and the fillet it becomes along the wing root:
# station y, how far out its edge is. Ahead of LEX_JOIN the edge is the LEX's
# sharp one; behind it the edge lies on the wing's upper surface and the
# surface is the fillet between the wing and the fuselage.
LEX_EDGE = [(6.40, 0.30), (6.05, 0.42), (5.75, 0.50), (5.30, 0.62), (4.40, 0.80), (3.10, 1.02), (1.90, 1.28), (1.05, 1.50), (0.62, 1.63),
            (0.30, 1.52), (-0.30, 1.40), (-1.20, 1.31), (-2.20, 1.25), (-2.95, 1.20)]
LEX_JOIN = 0.62


def wing_top(x, y):
    """Height of the (right) wing's upper surface at (x, y)."""
    le, c, _, up = wing_sect(1)(x)
    xc = min(max((le.y - y) / c, 0.0), 1.0)
    return le.z + c * max(naca_t(xc, W_T), 0.004) * up.z


def lex_section(y, xe):
    """The LEX's edge height ze, how high above it the surface meets the
    fuselage (h), and the root's x there."""
    if y >= LEX_JOIN:
        ze = lerp(0.085, 0.03, smoothstep(0.6, 6.0, y))
    else:
        ze = wing_top(xe, y) - 0.006
    h = 0.04 + 0.28 * smoothstep(6.3, 3.8, y) * smoothstep(-2.95, -1.0, y)
    xr = min(fuse_hw(y, ze + h) - 0.015, xe - 0.02)
    return ze, h, xr


def lex_top(x, y):
    """Height of the LEX/fillet's upper surface at (x, y), right side."""
    xe = table_at(LEX_EDGE, y)[1]
    ze, h, xr = lex_section(y, xe)
    t = min(max((x - xr) / max(xe - xr, 1e-3), 0.0), 1.0)
    return ze + 0.004 + h * (1 - t) ** 2.2


def lexes():
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        rings = []
        for y, xe in spline_rows(LEX_EDGE, 0, len(LEX_EDGE) - 1, 4):
            ze, h, xr = lex_section(y, xe)
            wing = y < LEX_JOIN
            # concave from the fuselage's side out to the sharp edge, flat underneath
            top = [V((lerp(xr, xe, t), y, ze + 0.004 + h * (1 - t) ** 2.2)) for t in (0.0, 0.1, 0.22, 0.36, 0.52, 0.68, 0.84)]
            zb = ze - (0.09 if wing else 0.05)
            xb = min(fuse_hw(y, zb) - 0.015, xe - 0.03)
            under = [V((xe, y, ze + 0.004)), V((xe, y, ze - 0.004)), V((lerp(xe, xb, 0.45), y, ze - (0.05 if wing else 0.012))), V((xb, y, zb))]
            rings.append([M(p, s) for p in top + under])
        g.loft(rings, "skin")
        # The fence on top of the LEX (added to the C to tame tail buffeting).
        x = 1.16
        y0, y1 = 1.86, 1.22
        z0, z1 = lex_top(x, y0), lex_top(x, y1)
        g.plate([V((x * s, y0, z0 - 0.02)), V((x * s, y1, z1 - 0.02)), V((x * s, y1 + 0.06, z1 + 0.13)), V((x * s, y0 - 0.22, z0 + 0.17))], V((1, 0, 0)), 0.005, "skin")
        g.finish(side + " lex", smooth=30)


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


# ---------------------------------------------------------------- canopy
# (Everything inside the cockpit is built in Rust: src/aircraft/fa18/pit.rs.)
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
        tube(g, canopy_arc(y, w, zt, grow=0.012, n=40), 0.028, "frame", n=16, ref=V((0, 1, 0)))
    for s in (1, -1):
        pts = [V((s * (r[1] + 0.01), r[0], fuse_top(r[0]) - 0.03)) for r in spline_rows(CANOPY, 0, len(CANOPY) - 1, 4)]
        tube(g, pts, 0.03, "frame", n=16)
        # The sill: the canopy's rail standing on the fuselage, painted outside, black inside.
        rows = spline_rows(CANOPY, 0, len(CANOPY) - 1, 2)
        outer = [[V((s * (w + 0.018), y, fuse_z(y, w) - 0.04)), V((s * (w + 0.018), y, fuse_top(y) - 0.02)), V((s * (w - 0.022), y, fuse_top(y) - 0.02))] for y, w, _ in rows]
        inner = [[V((s * (w - 0.022), y, fuse_top(y) - 0.02)), V((s * (w - 0.022), y, fuse_z(y, w) - 0.04))] for y, w, _ in rows]
        g.strip(outer, "skin", toward=V((s * 8.0, 4.5, 6.0)))
        g.strip(inner, "cockpit", toward=V((0, 4.5, 0.8)))
    g.finish("canopy frame", smooth=80)



# ---------------------------------------------------------------- lifting surfaces
def naca_t(x, t):
    x = max(0.0, min(1.0, x))
    return 5 * t * (0.2969 * math.sqrt(x) - 0.1260 * x - 0.3516 * x * x + 0.2843 * x ** 3 - 0.1036 * x ** 4)


def foil_ring(sect, t, a, b, m=14):
    """A closed section from chord fraction a to b; t the thickness ratio, or a
    function of the chord fraction (a root fillet)."""
    le, c, cdir, up = sect
    tt = t if callable(t) else (lambda x: t)
    xs = [a + (b - a) * (1 - math.cos(math.pi * k / m)) / 2 for k in range(m + 1)]
    upper = [le + cdir * (c * x) + up * max(c * naca_t(x, tt(x)), 0.004) for x in xs]
    lower = [le + cdir * (c * x) - up * max(c * naca_t(x, tt(x)), 0.004) for x in reversed(xs)]
    return upper + lower


def surface(g, sect, u0, u1, a, b, t, m="skin", nu=6, tk=None):
    """`tk(u, x)`: how much thicker than t the section at u is at chord fraction x."""
    rings = []
    for k in range(nu):
        u = lerp(u0, u1, k / (nu - 1))
        rings.append(foil_ring(sect(u), (lambda x, u=u: t * tk(u, x)) if tk else t, a, b))
    g.loft(rings, m)


def hinge(sect, u, xc):
    le, c, cdir, _ = sect(u)
    return le + cdir * (c * xc)


# Wing (kept in step with wing_sect in src/aircraft/fa18/mod.rs)
W_X0, W_X1, W_LE0, W_SWEEP, W_C0, W_C1, W_Z0, W_ANH, W_T = 0.9, 5.75, 1.0, 0.503, 4.15, 1.65, 0.12, 0.052, 0.05
FLAP = (1.3, 3.45)
AIL = (3.5, 5.6)
LEF = (1.62, 5.6)  # from where the LEX meets the leading edge


def wing_sect(s):
    def f(x):
        k = (x - W_X0) / (W_X1 - W_X0)
        le = V((x * s, W_LE0 - W_SWEEP * (x - W_X0), W_Z0 - W_ANH * (x - W_X0)))
        return le, lerp(W_C0, W_C1, k), V((0, -1, 0)), V((W_ANH * s, 0, 1)).normalized()
    return f


def wick(g, p, aft=V((0, -1, 0)), up=V((0, 0, 1))):
    """A static discharger trailing from point p: a fine rod on a little base."""
    g.box(p + aft * 0.02, (0.006, 0.025, 0.004), "skin", (up.cross(aft).normalized(), aft, up))
    tube(g, [p + aft * 0.03, p + aft * 0.14], 0.0035, "rubber", n=5)


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
        surface(g, ws, W_X0, FLAP[0], 0.0, 1.0, W_T, nu=3)
        surface(g, ws, FLAP[0], LEF[0], 0.0, 0.75, W_T, nu=2)
        surface(g, ws, LEF[0], AIL[1], 0.15, 0.75, W_T, nu=10)
        surface(g, ws, FLAP[1], AIL[0], 0.75, 1.0, W_T, nu=2)
        surface(g, ws, AIL[1], W_X1, 0.0, 1.0, W_T, nu=3)
        pylon(g, s, 2.35)
        pylon(g, s, 3.55)
        g.finish(side + " wing")

        for name, (x0, x1), (a, b) in (("le flap", LEF, (0.0, 0.15)), ("flap", FLAP, (0.75, 1.0)), ("aileron", AIL, (0.75, 1.0))):
            g = Geo()
            surface(g, ws, x0, x1, a, b, W_T, nu=8)
            if name == "aileron":
                for xw in (4.75, 5.35):
                    wick(g, hinge(ws, xw, 1.0))
            xc = 0.15 if name == "le flap" else 0.75
            p0, p1 = hinge(wing_sect(1), x0, xc), hinge(wing_sect(1), x1, xc)
            g.finish(side + " " + name, pivot=M(p0, s), axis=pair_axis((p1 - p0).normalized(), s))

        # Wingtip launch rail with a Sidewinder on it: a slab with chamfered ends,
        # the shoe the missile hangs from under it.
        g = Geo()
        rings = []
        for y, hw, top, bot in ((-0.95, 0.012, -0.12, -0.13), (-1.15, 0.045, -0.10, -0.19), (-1.30, 0.048, -0.10, -0.20), (-2.80, 0.048, -0.10, -0.20), (-2.95, 0.04, -0.11, -0.17)):
            rings.append([V((5.80 * s - hw, y, top)), V((5.80 * s + hw, y, top)), V((5.80 * s + hw, y, bot)), V((5.80 * s - hw, y, bot))])
        g.loft(rings, "skin")
        g.box(V((5.86 * s, -2.05, -0.20)), (0.022, 0.70, 0.012), "steel")
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
        for xw in (2.85, 3.22):
            wick(g, hinge(stab_sect(s), xw, 1.0))
        pv = V((0.85, -6.15, -0.12))
        g.finish(side + " stabilator", pivot=M(pv, s), axis=pair_axis(V((1, 0, -0.033)).normalized(), s))

        fs = fin_sect(s)
        g = Geo()
        # Thickened toward its root ahead of the rudder: a fillet into the fuselage.
        root = lambda u, x: 1.0 + 2.0 * smoothstep(0.16, -0.04, u) ** 2 * (1.0 - smoothstep(0.25, 0.68, x))
        surface(g, fs, -0.06, 0.6, 0.0, 0.7, 0.045, nu=10, tk=root)
        surface(g, fs, 0.6, 1.0, 0.0, 1.0, 0.045, nu=5)
        # The antenna fairing along the fin's tip, and the formation light strip below it.
        le, c, cdir, out = fs(1.0)
        up = V((math.sin(FIN_CANT) * s, 0, math.cos(FIN_CANT)))
        tube(g, [le + cdir * (c * k) + up * 0.02 for k in (-0.12, 0.1, 0.5, 0.9, 1.12)], lambda t: 0.05 * (1.0 - (2 * t - 1) ** 4) + 0.006, "skin", n=10)
        wick(g, le + cdir * (c * 1.1) + up * 0.02, up=up)
        wick(g, le + cdir * (c * 0.98) + up * -0.05, up=up)
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
# The nose leg swings forward into its well under the cockpit. Each main leg
# swings aft while its lower part (oleo, trailing arm, axle) twists about the
# strut, so the wheel ends up lying almost flat in the well under the intake
# duct, as on the real aircraft. Everything stays inside the wells, which the
# doors close over (they open first and shut last: see `anim` in Rust).
NOSE_PIVOT = V((0, 5.02, -0.42))
NOSE_AXLE = V((0, 4.80, -1.67))
NOSE_R = 0.28
NOSE_STOWED_Z = -0.25  # height of the nose axle with the gear up
MAIN_PIVOT = V((0.84, -0.24, -0.20))
MAIN_KNEE = V((1.16, -0.36, -1.24))  # bottom of the strut, where the trailing arm hangs
MAIN_AXLE = V((1.57, -0.60, -1.57))
MAIN_ARM_X = 1.38  # the trailing arm runs aft and out to the inboard side of the wheel
MAIN_R = 0.38
MAIN_STOW_AT = V((0.56, -1.66, -0.36))  # where the main wheels stow, under the intake ducts
MAIN_STOW_AXLE = V((0.30, 0.0, 1.0))  # and their axles: nearly upright, the wheels nearly flat


def wheel(g, c, r, hw):
    lathe(g, c, V((1, 0, 0)), [(0.0, -hw * 0.85), (r * 0.55, -hw * 0.85), (r * 0.55, -hw), (r - 0.04, -hw), (r, -hw * 0.6), (r, hw * 0.6), (r - 0.04, hw), (r * 0.55, hw), (r * 0.55, hw * 0.85), (0.0, hw * 0.85)], "rubber", n=28)
    lathe(g, c, V((1, 0, 0)), [(0.0, -hw * 0.9), (r * 0.56, -hw * 0.9), (r * 0.56, hw * 0.9), (0.0, hw * 0.9)], "hub", n=20)
    lathe(g, c, V((1, 0, 0)), [(0.0, -hw * 1.02), (r * 0.16, -hw * 1.02), (r * 0.20, -hw * 0.9), (r * 0.20, hw * 0.9), (r * 0.16, hw * 1.02), (0.0, hw * 1.02)], "steel", n=12)
    # Hub bolts round the flange.
    for k in range(8):
        a = 2 * math.pi * k / 8
        for side in (-1, 1):
            p = c + V((side * hw * 0.92, math.cos(a) * r * 0.36, math.sin(a) * r * 0.36))
            g.box(p, (0.008, 0.012, 0.012), "steel")


def stow(pivot, down, up):
    d0, d1 = down - pivot, up - pivot
    return d0.cross(d1).normalized(), d0.angle(d1)


def scissors(g, top, bottom, back, m="strut"):
    """Torque links: two arms from the strut's ends meeting behind it."""
    knee = (top + bottom) / 2 + back
    for a, b in ((top, knee), (knee, bottom)):
        tube(g, [a, b], 0.014, m, n=6)
    g.box(knee, (0.03, 0.02, 0.02), m)


def main_stow():
    """How a (right) main leg stows: the strut's swing (axis, angle) and the lower
    part's twist about the strut (angle). Searched for: the wheel lying as flat
    as it can, its centre near MAIN_STOW_AT, in the well under the intake."""
    global _STOW
    if _STOW:
        return _STOW
    s = (MAIN_KNEE - MAIN_PIVOT).normalized()
    want = MAIN_STOW_AXLE.normalized()
    best = None
    for el in range(-30, 31, 2):
        for az in range(-20, 11, 2):
            e, z = math.radians(el), math.radians(az)
            s1 = V((math.sin(z) * math.cos(e), -math.cos(z) * math.cos(e), math.sin(e)))
            ax = s.cross(s1).normalized()
            sw = s.angle(s1)
            rs = Matrix.Rotation(sw, 3, ax)
            for d in range(-180, 180, 2):
                r = rs @ Matrix.Rotation(math.radians(d), 3, s)
                a1 = MAIN_PIVOT + r @ (MAIN_AXLE - MAIN_PIVOT)
                score = abs((r @ V((1, 0, 0))).dot(want)) - 3.0 * (a1 - MAIN_STOW_AT).length
                if best is None or score > best[0]:
                    best = (score, ax, sw, s, math.radians(d))
    _STOW = best[1:]
    return _STOW


_STOW = None


def main_stowed(p):
    """Where point p of the (right) main leg's lower part ends up with the gear up."""
    ax, sw, s, tw = main_stow()
    r = Matrix.Rotation(sw, 3, ax) @ Matrix.Rotation(tw, 3, s)
    return MAIN_PIVOT + r @ (p - MAIN_PIVOT)


def gear():
    # Nose: swings forward into the well under the cockpit.
    l = (NOSE_AXLE - NOSE_PIVOT).length
    dz = NOSE_STOWED_Z - NOSE_PIVOT.z
    ax, ang = stow(NOSE_PIVOT, NOSE_AXLE, NOSE_PIVOT + V((0, math.sqrt(l * l - dz * dz), dz)))
    along = (NOSE_PIVOT - NOSE_AXLE).normalized()
    low = NOSE_AXLE + along * 0.34
    g = Geo()
    # The trunnion across the top, the strut, the steering collar.
    tube(g, [NOSE_PIVOT + V((-0.17, 0, 0)), NOSE_PIVOT + V((0.17, 0, 0))], 0.045, "strut", n=10)
    tube(g, [NOSE_PIVOT, low], 0.068, "strut", n=14)
    lathe(g, NOSE_PIVOT - along * 0.62, -along, [(0.0, 0.0), (0.095, 0.0), (0.095, 0.12), (0.0, 0.12)], "strut", n=14)
    # The launch bar, folded up against the front of the strut, and its actuator.
    fwd = (V((0, 1, 0)) - along * along.y).normalized()
    bar0 = low + fwd * 0.07 + along * 0.06
    tube(g, [bar0, bar0 + along * 0.62 + fwd * 0.10], 0.024, "strut", n=8)
    g.box(bar0 + along * 0.64 + fwd * 0.11, (0.05, 0.03, 0.025), "steel", (V((1, 0, 0)), fwd, along))
    tube(g, [NOSE_PIVOT - along * 0.25 + fwd * 0.06, bar0 + along * 0.40 + fwd * 0.09], 0.016, "chrome", n=6)
    # Landing and taxi lights on a bracket ahead of the strut.
    g.box(NOSE_PIVOT - along * 0.78 + fwd * 0.11, (0.055, 0.03, 0.04), "strut", (V((1, 0, 0)), fwd, along))
    g.box(NOSE_PIVOT - along * 0.78 + fwd * 0.142, (0.045, 0.004, 0.032), "landing", (V((1, 0, 0)), fwd, along))
    g.box(NOSE_PIVOT - along * 0.66 + fwd * 0.11 + V((0.09, 0, 0)), (0.025, 0.025, 0.025), "landing", (V((1, 0, 0)), fwd, along))
    # Hydraulic lines down the back of the strut.
    for x in (-0.03, 0.03):
        tube(g, [NOSE_PIVOT + V((x, -0.07, 0)) - along * 0.05, low + V((x, -0.075, 0))], 0.008, "steel", n=5)
    g.finish("nose leg", pivot=NOSE_PIVOT, axis=ax, angle=ang)

    down = (NOSE_AXLE - NOSE_PIVOT).normalized()
    g = Geo()
    tube(g, [low + along * 0.10, NOSE_AXLE], 0.052, "chrome", n=12)
    g.box(NOSE_AXLE + along * 0.04, (0.07, 0.05, 0.05), "strut")
    tube(g, [NOSE_AXLE + V((-0.22, 0, 0)), NOSE_AXLE + V((0.22, 0, 0))], 0.03, "strut")
    scissors(g, low + V((0, -0.06, 0)), NOSE_AXLE + along * 0.06 + V((0, -0.05, 0)), V((0, -0.13, 0)))
    g.finish("nose fork", pivot=NOSE_AXLE, axis=down, parent="nose leg")

    g = Geo()
    for x in (-0.13, 0.13):
        wheel(g, NOSE_AXLE + V((x, 0, 0)), NOSE_R, 0.075)
    g.finish("nose wheel", pivot=NOSE_AXLE, axis=V((1, 0, 0)), parent="nose fork")

    # Mains.
    ax, sw, sax, tw = main_stow()
    s_dir = (MAIN_KNEE - MAIN_PIVOT).normalized()
    mid = MAIN_PIVOT + (MAIN_KNEE - MAIN_PIVOT) * 0.52
    arm_end = V((MAIN_ARM_X, MAIN_AXLE.y, MAIN_AXLE.z))
    for s, side in ((1, "right"), (-1, "left")):
        # The strut's outer cylinder, the trunnion it swings on, the retraction
        # actuator's lug.
        g = Geo()
        tube(g, [M(MAIN_PIVOT + V((0, 0.16, 0.02)), s), M(MAIN_PIVOT + V((0, -0.16, -0.02)), s)], 0.05, "strut", n=10)
        tube(g, [M(MAIN_PIVOT, s), M(mid, s)], 0.088, "strut", n=14)
        lathe(g, M(mid, s), M(-s_dir, s), [(0.0, 0.0), (0.105, 0.0), (0.105, 0.07), (0.0, 0.07)], "strut", n=14)
        lug = MAIN_PIVOT + s_dir * 0.22 + V((0, -0.10, 0))
        g.box(M(lug, s), (0.03, 0.05, 0.05), "strut")
        for x in (-0.035, 0.035):
            tube(g, [M(MAIN_PIVOT + V((x, -0.09, 0)), s), M(mid + V((x, -0.10, 0)), s)], 0.009, "steel", n=5)
        g.finish(side + " main leg", pivot=M(MAIN_PIVOT, s), axis=pair_axis(ax, s), angle=sw)

        # The lower part, which twists as the leg swings up: the oleo, the
        # trailing arm and its shock link, the axle and brake.
        g = Geo()
        tube(g, [M(mid - s_dir * 0.05, s), M(MAIN_KNEE, s)], 0.062, "chrome", n=12)
        sd = M(s_dir, s)
        bx = (V((1, 0, 0)) - sd * sd.x).normalized()
        g.box(M(MAIN_KNEE, s), (0.06, 0.06, 0.07), "strut", (bx, sd.cross(bx), sd))
        tube(g, [M(MAIN_KNEE, s), M(arm_end, s)], 0.055, "strut", n=10)
        tube(g, [M(mid.lerp(MAIN_KNEE, 0.45) + V((0.0, -0.07, 0)), s), M(MAIN_KNEE.lerp(arm_end, 0.7) + V((0, 0, 0.05)), s)], 0.032, "strut", n=8)
        tube(g, [M(arm_end + V((-0.02, 0, 0)), s), M(MAIN_AXLE + V((0.13, 0, 0)), s)], 0.04, "steel", n=10)
        lathe(g, M(MAIN_AXLE + V((-0.115, 0, 0)), s), V((s, 0, 0)), [(0.0, 0.0), (0.17, 0.0), (0.17, 0.05), (0.0, 0.05)], "steel", n=16)
        scissors(g, M(mid + s_dir * 0.08 + V((0, 0.07, 0)), s), M(MAIN_KNEE + V((0, 0.07, 0.04)), s), M(V((0.02, 0.15, 0)), s))
        g.finish(side + " main strut", pivot=M(MAIN_PIVOT, s), axis=pair_axis(sax, s), angle=tw, parent=side + " main leg")

        g = Geo()
        wheel(g, M(MAIN_AXLE, s), MAIN_R, 0.14)
        g.finish(side + " main wheel", pivot=M(MAIN_AXLE, s), axis=V((1, 0, 0)), parent=side + " main strut")
    a1 = main_stowed(MAIN_AXLE)
    print(f"main gear: swing {math.degrees(sw):.0f} deg, twist {math.degrees(tw):.0f} deg, wheel stowed at {tuple(round(c, 3) for c in a1)}, axle {tuple(round(c, 2) for c in main_stowed(MAIN_AXLE + V((1, 0, 0))) - a1)}")


def skin_door(name, s, x0, x1, y0, y1, hinge_x, open_angle, nx=10, ny=12, t=0.014):
    """A gear door cut from the skin itself (x0..x1 out from the centreline on
    side s, stations y0..y1), so that shut it lies flush in its opening; hinged
    along the line x = hinge_x. Modelled open: it turns by open_angle about the
    hinge, its free edge going down. x1 may be a function of y (the edge of an
    opening that widens)."""
    xe = x1 if callable(x1) else (lambda y: x1)
    ys = [lerp(y0, y1, k / ny) for k in range(ny + 1)]
    outer = [[V((s * x, y, fuse_z(y, x, upper=False) - 0.002)) for x in (lerp(x0, xe(y), (1 - math.cos(math.pi * k / nx)) / 2) for k in range(nx + 1))] for y in ys]

    def inward(p):
        mid = table_at(FUSE, p.y)[3]
        return p + (V((0, p.y, mid)) - p).normalized() * t

    inner = [[inward(p) for p in row] for row in outer]
    g = Geo()
    vo = [[g.bm.verts.new(p) for p in row] for row in outer]
    vi = [[g.bm.verts.new(p) for p in row] for row in inner]
    for j in range(ny):
        for i in range(nx):
            g.face([vo[j][i], vo[j][i + 1], vo[j + 1][i + 1], vo[j + 1][i]], "skin")
            g.face([vi[j][i], vi[j + 1][i], vi[j + 1][i + 1], vi[j][i + 1]], "door_in")
    rim = [(j, 0) for j in range(ny)] + [(ny, i) for i in range(nx)] + [(j, nx) for j in range(ny, 0, -1)] + [(0, i) for i in range(nx, 0, -1)]
    for (ja, ia), (jb, ib) in zip(rim, rim[1:] + rim[:1]):
        g.face([vo[ja][ia], vo[jb][ib], vi[jb][ib], vi[ja][ia]], "door_in")
    h0 = V((s * hinge_x, y0, fuse_z(y0, hinge_x, upper=False)))
    h1 = V((s * hinge_x, y1, fuse_z(y1, hinge_x, upper=False)))
    axis = (h1 - h0).normalized()
    xf = x0 + xe((y0 + y1) / 2) - hinge_x
    free = V((s * xf, (y0 + y1) / 2, fuse_z((y0 + y1) / 2, xf, upper=False)))
    rot = Matrix.Translation(h0) @ Matrix.Rotation(open_angle, 4, axis) @ Matrix.Translation(-h0)
    if (rot @ free).z > free.z:
        axis = -axis
        rot = Matrix.Translation(h0) @ Matrix.Rotation(open_angle, 4, axis) @ Matrix.Translation(-h0)
    for v in g.bm.verts:
        v.co = rot @ v.co
    # A hinge bracket or two along the hinge line, inside.
    for f in (0.2, 0.8):
        hp = h0.lerp(h1, f)
        g.box(hp + V((0, 0, 0.03)), (0.02, 0.03, 0.03), "well")
    g.finish(name, smooth=30, pivot=h0, axis=axis, angle=-open_angle, recalc=False)


def doors():
    hw, ny0, ny1, _ = NOSE_WELL
    xi, my0, my1, _ = MAIN_WELL
    gap = 0.005
    for s, side in ((1, "right"), (-1, "left")):
        # Nose: a pair meeting on the centreline, hinged at the well's sides.
        skin_door(side + " nose door", s, gap, hw - gap, ny1 - gap, ny0 + gap, hw, math.radians(86))
        # Mains, hinged along the inboard edge: a short door beside the strut that
        # stays open, and the long one behind it that shuts again once the leg
        # is down (Rust `anim`).
        xo = lambda y: main_well_out(y) - gap
        skin_door(side + " strut door", s, xi + gap, xo, my0 - gap, MAIN_DOOR_SPLIT + gap, xi, math.radians(92), nx=14, ny=5)
        skin_door(side + " main door", s, xi + gap, xo, MAIN_DOOR_SPLIT - gap, my1 + gap, xi, math.radians(92), nx=14)


def check_gear():
    """Sweep the gear from up to down (as Rust `anim` moves it: the legs and the
    main struts' twist over 0.15..0.85) and report any gear vertex inside the
    fuselage's skin but outside the wells, or, stowed, not tucked in behind
    the doors. Run on every export: a hand edit that makes the gear poke
    through the skin shows up here."""
    obs = {o.name: o for o in COLL.all_objects}

    def local(n, g):
        o = obs[n]
        a = o.get("angle", 0.0)
        if abs(a) < 1e-6 or not (n.endswith(" leg") or n.endswith("main strut")):
            return Matrix.Identity(4)
        a *= 1 - smoothstep(0.15, 0.85, g)
        p = V(o["pivot"])
        return Matrix.Translation(p) @ Matrix.Rotation(a, 4, V(o["axis"])) @ Matrix.Translation(-p)

    def world(n, g):
        par = obs[n].get("parent")
        return (world(par, g) if par else Matrix.Identity(4)) @ local(n, g)

    def depth(p):
        """How far inside the skin p is (negative: outside)."""
        if not FUSE[-1][0] < p.y < FUSE[0][0]:
            return -1.0
        return min(p.z - fuse_z(p.y, p.x, False), fuse_z(p.y, p.x, True) - p.z, fuse_hw(p.y, p.z) - abs(p.x))

    def in_well(p):
        hw, y0, y1, zt = NOSE_WELL
        if abs(p.x) < hw and y0 < p.y < y1 and p.z < zt:
            return True
        xi, y0, y1, zt = MAIN_WELL
        return xi < abs(p.x) < main_well_x(p.y, p.z) and y1 < p.y < y0 and p.z < zt

    gear = [n for n in obs if "door" not in n and any(k in n for k in ("leg", "strut", "wheel", "fork"))]
    bad = 0
    for k in range(21):
        g = k / 20
        for n in gear:
            m = world(n, g) @ obs[n].matrix_world
            worst, at = 0.0, None
            for v in obs[n].data.vertices:
                p = m @ v.co
                d = depth(p)
                e = 0.0
                if d > 0.0 and not in_well(p):
                    e = 1.0  # inside the fuselage, out of its well
                if g == 0.0 and d < 0.018:
                    e = max(e, 0.018 - d)  # stowed: inside the skin, clear of the shut doors
                if e > worst:
                    worst, at = e, v.co
            if worst > 0.005:
                bad += 1
                what = "out of its well" if worst >= 1.0 else f"{worst * 100:.0f} cm outside the skin"
                print(f"  gear check: {n} at gear {g:.2f}: {what} (vertex at {tuple(round(c, 2) for c in at)} as modelled)")
    print("gear check:", "clear of the skin all the way up and down" if not bad else f"{bad} clashes")


# ---------------------------------------------------------------- export
# Every piece the game looks for (PIECES in src/aircraft/fa18/mod.rs); other objects are ignored.
EXPECTED = ["radome", "nose section", "fuselage", "tail cone", "spine", "canopy", "canopy frame", "speedbrake", "tail light", "nose leg", "nose fork", "nose wheel", "afterburner"] + [
    f"{s} {n}" for s in ("left", "right") for n in ("intake", "lex", "nozzle", "wing", "le flap", "flap", "aileron", "tip rail", "nav light", "stabilator", "fin", "rudder", "main leg", "main strut", "main wheel", "nose door", "strut door", "main door")]


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
    hidden = [o for o in COLL.all_objects if any(k in o.name for k in ("leg", "strut", "wheel", "fork", "door", "tip rail", "afterburner"))]
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
    global COLL, CUTS
    if EXPORT_ONLY:
        # Export the .blend that is open, edits and all, without rebuilding anything.
        COLL = bpy.data.collections.get("FA-18C")
        if COLL is None:
            raise SystemExit("fa18 export: no 'FA-18C' collection in this file (open assets/blender/fa18.blend)")
        export(os.path.join(ASSETS, "fa18.bin.gz"))
        check_gear()
    else:
        for ob in list(bpy.data.objects):
            bpy.data.objects.remove(ob, do_unlink=True)
        for c in list(bpy.data.collections):
            bpy.data.collections.remove(c)
        COLL = bpy.data.collections.new("FA-18C")
        bpy.context.scene.collection.children.link(COLL)
        CUTS = bpy.data.collections.new("FA-18C well cutters")
        bpy.context.scene.collection.children.link(CUTS)
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
        bpy.context.preferences.filepaths.save_version = 0  # no fa18.blend1 beside it
        export(os.path.join(ASSETS, "fa18.bin.gz"))
        check_gear()
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ASSETS, "blender", "fa18.blend"))
    if RENDER:
        render_previews(RENDER)
    if GUIDES:
        render_guides(GUIDES)


main()
