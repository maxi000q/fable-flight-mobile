# The F-4E Phantom II for FableFlight, built procedurally in Blender.
#
#   blender -b --factory-startup --python assets/blender/f4.py -- <repo>/assets [--render <dir>] [--guides <dir>]
#
# Built the way the Hornet is (assets/blender/fa18.py, whose mesh toolkit this
# file carries a copy of): the airframe from lofted sections and NACA-section
# lifting surfaces, one object per piece that moves or breaks off on its own,
# saved as assets/blender/f4.blend and exported to assets/f4.bin.gz, which
# src/aircraft/f4 loads. That rebuild starts from scratch, so it throws away
# anything edited by hand in the .blend.
#
# To keep hand edits, export the .blend as it stands instead (save it first):
#
#   blender -b assets/blender/f4.blend --python assets/blender/f4.py -- <repo>/assets --export-only
#
# Edit freely: reshape meshes, add modifiers (they are applied on export),
# move or turn pieces (each carries its hinge along), assign the f4_
# materials. Keep the piece names and the "F-4E" collection: the game finds
# pieces by name (see EXPECTED below); objects with other names are ignored.
# How each piece moves is set in Rust by its name; each object records where
# its hinge is in its own frame (custom properties "pivot", "axis", "angle").
#
# The nose gear well is cut into the fuselage with a boolean modifier (the
# cutter in the "F-4E well cutters" collection, not exported) and its doors
# are patches of the skin itself. The main legs fold inward into the wing, as
# on the real aircraft: the wheel lies nearly flat in the root, under the
# bulge in the upper skin, behind a door that is a patch of the wing's lower
# skin; the strut carries its own door. Every export ends with a gear check
# that names any part of the stowed gear left outside the skin.
#
# --render writes preview pictures. --guides writes the four orthographic
# views (top, bottom, left side, right side) the paint is projected from, each
# with its silhouette mask and a geometry pass, <view>_geo.npz: what point of
# which piece each pixel shows (see assets/blender/f4_paint.py, which paints
# the pictures from them, and plane.wgsl).
# They cover the same rectangles as the Hornet's, so the game projects both
# aircraft's paint the same way.
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
# Pattern 17 is the painted skin (plane.wgsl): the paint projected from the
# aircraft's paint pictures; the countershaded vertex colour shows only where
# no picture faces the surface.
# Material 11 (exhaust) extra: 0 the nozzle petals, 1 inside the nozzle, 2 the
# flame holder; material 17 (the exhaust plume, drawn in the transparent pass)
# extra: 0 heat haze, 1 the flame, 2 its core, 3 discs across it.
MATS = {
    "skin": (15, 17, (0.30, 0.32, 0.30), 0),  # countershaded on export
    "radome": (15, 17, (0.02, 0.02, 0.022), 0),
    "heat": (3, 0, (0.40, 0.38, 0.36), 2),  # bare, heat-stained metal under the tail boom and on the stabilator's roots
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
    "well_dark": (10, 0, (0.05, 0.05, 0.05), 0),  # the main wells' openings in the wing, seen with the doors open
    "chrome": (3, 0, (0.85, 0.86, 0.88), 1),
    "steel": (3, 0, (0.32, 0.32, 0.33), 0),
    "rubber": (4, 0, (0.018, 0.018, 0.02), 0),
    "hub": (0, 0, (0.70, 0.70, 0.68), 0),
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
SKIN_LIGHT = (0.50, 0.52, 0.52)  # light grey underneath
SKIN_DARK = (0.10, 0.13, 0.08)  # dark green on top
COCKPIT_TUB = (0.03, 0.03, 0.034)

MAT = {}


def make_materials():
    for name, (gid, _pat, col, _x) in MATS.items():
        m = bpy.data.materials.get("f4_" + name) or bpy.data.materials.new("f4_" + name)
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



def paint_faces(g, inside, m):
    """Give the faces built so far whose centres `inside` accepts the finish m."""
    k = g.mi(m)
    for f in g.bm.faces:
        if inside(f.calc_center_median()):
            f.material_index = k


# ---------------------------------------------------------------- fuselage
# station y, half width, top, height of the widest point, bottom, superellipse exponent
# (kept in step with FUSE in src/aircraft/f4/mod.rs). The nose droops; the
# body widens round the two engines behind the intakes and ends where their
# nozzles come out (station -3.8), the tail boom carrying on above them.
FUSE = [
    (9.80, 0.000, -0.33, -0.33, -0.33, 2.0),
    (9.55, 0.110, -0.20, -0.32, -0.44, 2.0),
    (9.10, 0.220, -0.06, -0.31, -0.55, 2.0),
    (8.50, 0.320, 0.08, -0.30, -0.66, 2.0),
    (7.90, 0.400, 0.19, -0.29, -0.76, 2.0),
    (7.20, 0.455, 0.31, -0.27, -0.83, 2.1),
    (6.50, 0.500, 0.42, -0.25, -0.88, 2.2),
    (5.80, 0.540, 0.52, -0.22, -0.92, 2.3),
    (5.00, 0.580, 0.60, -0.20, -0.95, 2.4),
    (4.20, 0.620, 0.66, -0.18, -0.96, 2.5),
    (3.40, 0.660, 0.72, -0.16, -0.96, 2.6),
    (2.60, 0.720, 0.78, -0.15, -0.96, 2.8),
    (1.80, 1.000, 0.84, -0.15, -0.96, 3.2),
    (0.80, 1.300, 0.90, -0.15, -0.96, 3.6),
    (-0.50, 1.340, 0.93, -0.15, -0.95, 3.8),
    (-1.80, 1.280, 0.94, -0.15, -0.92, 3.6),
    (-3.00, 1.120, 0.95, -0.12, -0.86, 3.2),
    (-3.70, 1.020, 0.95, -0.05, -0.80, 3.0),
    (-4.05, 0.600, 0.95, 0.50, 0.10, 2.6),
    (-5.00, 0.520, 0.95, 0.54, 0.16, 2.4),
    (-6.50, 0.420, 0.93, 0.58, 0.26, 2.2),
    (-8.00, 0.270, 0.86, 0.62, 0.40, 2.0),
    (-8.90, 0.030, 0.66, 0.64, 0.62, 2.0),
]
RING_N = 44
# Round the cockpit opening the ring is laid out so that vertex TUB_K (and its
# mirror image) lies exactly on the opening's edge.
TUB_K = 6
ENGINE_X = 0.50  # the engines' centrelines
ENGINE_Z = -0.30
NOZZLE_Y = -3.80  # where the nozzles leave the body; their exits are 1 m further aft


def trough(y):
    """How deep the valley between the two engines is at station y: on top, underneath."""
    return 0.0, 0.12 * smoothstep(-1.5, -3.4, y) * smoothstep(-4.0, -3.75, y)


def p_lower(y, p):
    """The lower half's superellipse exponent (the same as the upper's here)."""
    return p


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


def blade(g, base, chord, height, t, down=False, m="skin"):
    """A swept blade antenna standing on `base`."""
    s = -1.0 if down else 1.0
    out = [base + V((0, chord / 2, 0)), base + V((0, -chord / 2, 0)), base + V((0, -chord / 2 - 0.02, s * height)), base + V((0, chord * 0.05, s * height))]
    g.plate(out, V((1, 0, 0)), t, m)



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


def wick(g, p, aft=V((0, -1, 0)), up=V((0, 0, 1))):
    """A static discharger trailing from point p: a fine rod on a little base."""
    g.box(p + aft * 0.02, (0.006, 0.025, 0.004), "skin", (up.cross(aft).normalized(), aft, up))
    tube(g, [p + aft * 0.03, p + aft * 0.14], 0.0035, "rubber", n=5)


def pair_axis(right_axis, s):
    """The axis a mirrored piece turns about so equal angles move both sides alike (as Builder::pair)."""
    return right_axis if s > 0 else -M(right_axis, -1)


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
    if s < 0:
        bmesh.ops.reverse_faces(g.bm, faces=g.bm.faces)  # mirrored: wound the other way
    # A hinge bracket or two along the hinge line, inside.
    for f in (0.2, 0.8):
        hp = h0.lerp(h1, f)
        g.box(hp + V((0, 0, 0.03)), (0.02, 0.03, 0.03), "well")
    g.finish(name, smooth=30, pivot=h0, axis=axis, angle=-open_angle, recalc=False)


# ---------------------------------------------------------------- the airframe
# The cockpits (front and rear, one opening): rows of the nose section the
# opening runs between, its floor.
TUB_Y = (6.2, 2.75)
TUB_FLOOR = 0.22
TUB_RIM = []  # per row of the opening: the ring's points from its right edge over the top to its left edge
# The turtleback behind the rear canopy, down to the fin: station y, half width, top.
SPINE = [(2.55, 0.495, 1.362), (2.20, 0.470, 1.340), (1.50, 0.430, 1.270), (0.60, 0.390, 1.170), (-0.60, 0.350, 1.080), (-1.80, 0.320, 1.020), (-3.00, 0.280, 0.985), (-3.90, 0.200, 0.960)]


def fuselage():
    idx = {round(r[0], 2): i for i, r in enumerate(FUSE)}
    for name, y0, y1 in (("radome", 9.80, 7.90), ("nose section", 7.90, 2.60), ("fuselage", 2.60, -3.00), ("tail cone", -3.00, -8.90)):
        g = Geo()
        rows = spline_rows(FUSE, idx[y0], idx[y1], 4)
        if name == "nose section":
            weight = lambda y: smoothstep(6.8, 6.3, y) * smoothstep(2.6, 2.85, y)
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
            # Bare metal under the tail boom, in the engines' exhaust.
            paint_faces(g, lambda c: c.y < -4.0 and c.z < table_at(FUSE, c.y)[3] - 0.03, "heat")
            tail_details(g)
        ob = g.finish(name)
        if name == "nose section":
            carve(ob, [nose_well()])

    # The turtleback behind the rear canopy.
    g = Geo()
    rings = []
    for y, w, top in spline_rows(SPINE, 0, len(SPINE) - 1, 3):
        base = fuse_top(y) - 0.10
        rings.append([V((w * math.cos(a), y, base + (top - base) * math.sin(a))) for a in (math.pi * j / 16 for j in range(17))])
    g.loft(rings, "skin")
    # Blade antenna and the upper anti-collision light.
    top = lambda y: table_at(SPINE, y)[2]
    blade(g, V((0, 1.0, top(1.0) - 0.01)), 0.22, 0.20, 0.010)
    g.box(V((0, -1.2, top(-1.2) + 0.012)), (0.03, 0.05, 0.02), "strobe")
    g.finish("spine")


# The nose gear well under the front cockpit: half width, aft end, front end, roof.
NOSE_WELL = (0.24, 4.80, 6.62, -0.30)


def nose_well():
    hw, y0, y1, zt = NOSE_WELL
    return cutter("nose well", [[V((-hw, y, -1.4)), V((hw, y, -1.4)), V((hw, y, zt)), V((-hw, y, zt))] for y in (y1, y0)])


def nose_details(g):
    # The gun: the M61's fairing under the nose, its muzzle at the front.
    rings = []
    for y, w, d in ((8.42, 0.0, 0.0), (8.34, 0.09, 0.08), (8.10, 0.14, 0.14), (7.60, 0.18, 0.19), (7.00, 0.19, 0.20), (6.50, 0.16, 0.14), (6.05, 0.0, 0.0)):
        zc = fuse_z(y, 0.0, upper=False) + 0.03
        rings.append([V((w * math.cos(a), y, zc - d * math.sin(a))) for a in (2 * math.pi * k / 16 for k in range(16))])
    g.loft(rings, "skin")
    g.box(V((0, 8.37, fuse_z(8.37, 0.0, upper=False) - 0.035)), (0.035, 0.03, 0.03), "inlet")
    # Gun gas vents along the fairing's sides.
    for s in (1, -1):
        for y in (7.75, 7.55, 7.35):
            g.box(V((s * 0.175, y, fuse_z(y, 0.0, upper=False) - 0.04)), (0.006, 0.06, 0.012), "inlet")
    # Angle-of-attack probe and total temperature probe low on the sides.
    for s in (1, -1):
        y = 7.45
        x = fuse_hw(y, -0.30)
        tube(g, [V((s * (x - 0.02), y, -0.30)), V((s * (x + 0.06), y, -0.30))], lambda t: 0.016 - 0.008 * t, "steel", n=8)
        # Formation light strip.
        y = 7.25
        x = fuse_hw(y, -0.05)
        g.box(V((s * (x + 0.002), y, -0.05)), (0.004, 0.30, 0.03), "slime")


def belly_details(g):
    # Blade antenna and the lower anti-collision light.
    blade(g, V((0, 2.2, fuse_z(2.2, 0, upper=False))), 0.20, 0.16, 0.010, down=True)
    g.box(V((0, 1.2, fuse_z(1.2, 0, upper=False) - 0.012)), (0.03, 0.05, 0.02), "strobe")
    for s in (1, -1):
        # Formation light strip on the side.
        y = -1.4
        x = fuse_hw(y, 0.30)
        g.box(V((s * (x + 0.002), y, 0.30)), (0.004, 0.36, 0.03), "slime")


def tail_details(g):
    # The arresting hook, stowed under the tail boom.
    zb = lambda y: fuse_z(y, 0.0, upper=False) - 0.045
    pts = [V((0, y, zb(y))) for y in (-4.4, -5.2, -6.0, -6.8)]
    tube(g, pts, lambda t: 0.045 - 0.012 * t, "steel", n=10)
    g.box(V((0, -6.92, zb(-6.9) - 0.02)), (0.05, 0.11, 0.035), "steel")
    g.box(V((0, -4.35, zb(-4.35) + 0.01)), (0.07, 0.10, 0.05), "heat")
    # The fuel dump mast and the parachute door's end at the boom's tip.
    tube(g, [V((0, -8.86, 0.64)), V((0, -9.02, 0.64))], lambda t: 0.035 - 0.02 * t, "skin", n=10)


def intakes():
    # station y, centre x, half width, half height, centre height: a tall box
    # with rounded corners standing off the fuselage's side at the lip, then
    # a bulge along the body's side over the wing, dying away toward the nozzles.
    T = [(4.20, 1.02, 0.30, 0.52, -0.05), (3.80, 1.00, 0.34, 0.55, -0.05), (3.40, 0.96, 0.38, 0.58, -0.06), (2.60, 0.92, 0.42, 0.64, -0.07), (1.60, 0.86, 0.47, 0.70, -0.08), (0.90, 0.83, 0.51, 0.72, -0.09), (0.10, 0.84, 0.52, 0.66, -0.11), (-0.70, 0.86, 0.50, 0.56, -0.13), (-1.60, 0.86, 0.44, 0.40, -0.15), (-2.40, 0.84, 0.32, 0.20, -0.16)]

    def ring(s, y, cx, hw, hh, zc, inset=0.0):
        # The lip is raked: its inboard edge is further forward than its outboard one.
        rake = 0.14 * smoothstep(3.6, 4.2, y)
        out = []
        for k in range(32):
            a = 2 * math.pi * k / 32
            c, sn = math.cos(a), math.sin(a)
            dx = (hw - inset) * math.copysign(abs(c) ** 0.62, c)
            out.append(V(((cx + dx) * s, y - rake * dx / hw, zc + (hh - inset) * math.copysign(abs(sn) ** 0.62, sn))))
        return out

    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()

        def face(n0, way):
            """Which way the faces added since n0 look: +1 away from the trunk's
            centreline (its end cap: aft), -1 in toward it (the duct's end:
            forward), 0 straight ahead (the lip). The trunk is an open funnel,
            which recalc_face_normals turns inside out."""
            g.bm.faces.ensure_lookup_table()
            for f in g.bm.faces[n0:]:
                c = f.calc_center_median()
                _, tx, _, _, tz = table_at(T, c.y)
                r = V((c.x - tx * s, 0, c.z - tz))
                if way == 0 or r.length < 0.05:
                    r, k = V((0, 1, 0)), (-1.0 if way > 0 else 1.0)
                else:
                    k = float(way)
                g.facing.append((f, c + r.normalized() * (10.0 * k)))

        rows = spline_rows(T, 0, len(T) - 1, 4)
        g.loft([ring(s, *r) for r in rows], "skin", False, True)
        face(0, 1)
        # The lip, and the duct running back from it into the dark.
        y, cx, hw, hh, zc = T[0]
        n0 = len(g.bm.faces)
        g.loft([ring(s, *T[0]), ring(s, *T[0], inset=0.028)], "skin", False, False)
        face(n0, 0)
        n0 = len(g.bm.faces)
        g.loft([ring(s, y, cx, hw, hh, zc, 0.028), ring(s, y - 0.5, cx, hw, hh, zc, 0.035), ring(s, y - 1.5, cx - 0.03, hw, hh, zc + 0.02, 0.10)], "inlet", False, True)
        face(n0, -1)
        # The fixed ramp ahead of the lip (it stands off the fuselage to keep its
        # boundary layer out of the duct) and the struts holding it there; the
        # variable ramp behind it, inside the lip.
        x = (cx - hw - 0.02) * s
        g.plate([V((x, 4.92, zc + 0.08)), V((x, 4.74, zc + 0.50)), V((x, 4.02, zc + 0.52)), V((x, 4.02, zc - 0.52)), V((x, 4.50, zc - 0.52))], V((1, 0, 0)), 0.008, "skin")
        g.plate([V((x, 4.05, zc + 0.48)), V((x, 4.05, zc - 0.48)), V(((cx - hw + 0.10) * s, 3.45, zc - 0.48)), V(((cx - hw + 0.10) * s, 3.45, zc + 0.48))], V((s, 0.2, 0)), 0.006, "skin")
        for zz in (zc + 0.26, zc - 0.24):
            x0 = fuse_hw(4.35, zz) - 0.01
            g.box(V(((x0 + cx - hw) / 2 * s, 4.32, zz)), (abs(cx - hw - x0) / 2 + 0.005, 0.14, 0.006), "skin")
        g.finish(side + " intake")


def nozzles():
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        c = V((ENGINE_X * s, NOZZLE_Y, ENGINE_Z))
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
    exit_y = NOZZLE_Y - 1.0

    def shell(prof, m, n, sub):
        for s in (1, -1):
            rows = spline_rows(prof, 0, len(prof) - 1, sub)
            lathe(g, V((ENGINE_X * s, exit_y, ENGINE_Z)), aft, [(max(r, 0.0), a) for a, r in rows], m, n=n, caps=(False, False))

    # distance aft of the nozzle exit, radius
    shell([(-0.05, 0.36), (1.0, 0.52), (3.0, 0.66), (6.0, 0.74), (9.0, 0.64), (11.0, 0.34), (12.0, 0.0)], "haze", 16, 2)
    shell([(-0.14, 0.325), (0.4, 0.37), (1.2, 0.36), (2.2, 0.32), (3.4, 0.26), (4.8, 0.18), (6.0, 0.09), (6.9, 0.0)], "flame", 20, 3)
    shell([(-0.14, 0.26), (0.5, 0.25), (1.3, 0.21), (2.3, 0.155), (3.4, 0.09), (4.5, 0.0)], "core", 20, 3)
    # Discs across the jet: what is seen of the flame looking up it from behind, where the shells are edge on.
    for s in (1, -1):
        for k in range(12):
            a = 0.12 + 5.2 * (k / 11) ** 1.3
            lathe(g, V((ENGINE_X * s, exit_y, ENGINE_Z)), aft, [(0.0, a), (0.36 * (1.0 - 0.75 * a / 5.6), a)], "disc", n=20, caps=(False, False))
    ob = g.finish("afterburner", smooth=80, recalc=False)
    ob.hide_render = True
    ob.display_type = "WIRE"


# ---------------------------------------------------------------- canopy
# (Everything inside the cockpits is built in Rust: src/aircraft/f4/pit.rs.)
# station y, half width at the sill, height of the top: the windscreen, the
# front canopy, the rear canopy, ending at the turtleback.
CANOPY = [(6.55, 0.36, 0.48), (6.25, 0.42, 0.80), (5.90, 0.47, 1.06), (5.55, 0.50, 1.22), (5.10, 0.52, 1.31), (4.60, 0.53, 1.35), (4.10, 0.54, 1.38), (3.60, 0.54, 1.39), (3.10, 0.53, 1.38), (2.70, 0.50, 1.36), (2.40, 0.46, 1.34)]
# The frames: the windscreen's arch, the fixed frame between the two canopies
# (from, to), the rear canopy's aft arch.
FRAME_WS = 5.72
FRAME_MID = (4.52, 4.24)
FRAME_AFT = 2.64


def canopy_arc(y, w, zt, grow=0.0, n=20):
    z0 = fuse_top(y) - 0.04
    return [V(((w + grow) * math.cos(a), y, z0 + (zt + grow - z0) * math.sin(a))) for a in (math.pi * k / n for k in range(n + 1))]


def canopy():
    g = Geo()
    rows = spline_rows(CANOPY, 0, len(CANOPY) - 1, 3)
    g.loft([canopy_arc(*r) for r in rows], "glass")
    g.finish("canopy", smooth=60)

    g = Geo()
    for yb in (FRAME_WS, FRAME_AFT):
        y, w, zt = table_at(CANOPY, yb)
        tube(g, canopy_arc(y, w, zt, grow=0.012, n=40), 0.028, "frame", n=16, ref=V((0, 1, 0)))
    # The fixed frame between the canopies: a band over the top, standing a little proud of the glass.
    band = []
    for k in range(5):
        y, w, zt = table_at(CANOPY, lerp(FRAME_MID[0], FRAME_MID[1], k / 4))
        out = canopy_arc(y, w, zt, grow=0.016, n=28)
        band.append(out + canopy_arc(y, w, zt, grow=-0.016, n=28)[::-1])
    g.loft(band, "frame")
    # The windscreen's posts either side of its flat centre panel.
    for k in (7, 13):
        pts = []
        for j in range(6):
            y, w, zt = table_at(CANOPY, lerp(FRAME_WS, 6.50, j / 5))
            pts.append(canopy_arc(y, w, zt, grow=0.010, n=20)[k])
        tube(g, pts, 0.020, "frame", n=10)
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
# Wing (kept in step with wing_sect in src/aircraft/f4/mod.rs): the tip's span
# station; the leading edge at the centreline and its sweep (m back per m
# out: 51.4 degrees, 45 at the quarter chord); the chords at the centreline
# and the tip; its height; the thickness ratio (times wing_tk). Outboard of
# W_FOLD the panel rises at W_DIH and its leading edge is W_DOG of the chord
# further forward (the dogtooth).
W_X1, W_LE0, W_SWEEP, W_C0, W_C1, W_Z0, W_T = 5.85, 4.30, 1.2526, 7.00, 1.40, -0.78, 0.05
W_FOLD, W_DIH, W_DOG = 4.05, math.radians(12.0), 0.10
FLAP = (1.40, 2.78)
AIL = (2.82, 4.05)
ILEF = (2.60, 4.05)  # the inner panel's leading-edge flap
OLEF = (4.05, 5.75)  # the outer panel's
TE_HINGE, ILEF_C, OLEF_C = 0.82, 0.10, 0.12
BRAKE = (1.45, 2.35, -1.38, -2.20)  # the speedbrake under each wing: span stations, front (its hinge) and back


def wing_sect(s, outer=None):
    def f(x):
        out = x > W_FOLD + 1e-6 if outer is None else outer
        c = lerp(W_C0, W_C1, x / W_X1)
        y = W_LE0 - W_SWEEP * x
        if out:
            return V((x * s, y + W_DOG * c, W_Z0 + (x - W_FOLD) * math.tan(W_DIH))), c * (1 + W_DOG), V((0, -1, 0)), V((-math.sin(W_DIH) * s, 0, math.cos(W_DIH)))
        return V((x * s, y, W_Z0)), c, V((0, -1, 0)), V((0, 0, 1))
    return f


def wing_tk(u, x):
    """The wing thins from 6.4% at the root to 3% at the tip."""
    return lerp(1.28, 0.62, abs(u) / W_X1)


def wing_z(x, y, top):
    """Height of the (right) inner wing's upper or lower surface at (x, y)."""
    le, c, _, _ = wing_sect(1, False)(x)
    xc = min(max((le.y - y) / c, 0.0), 1.0)
    h = c * max(naca_t(xc, W_T * wing_tk(x, xc)), 0.004)
    return le.z + (h if top else -h)


def wing_plate(g, s, x0, x1, y0, y1, drop, t, m, m_in, nx=6, ny=6):
    """A patch of the wing's lower skin `drop` below it, t thick (a door, shut).
    Returns its vertices."""
    lo = [[g.bm.verts.new(V((s * lerp(x0, x1, i / nx), lerp(y0, y1, j / ny), wing_z(lerp(x0, x1, i / nx), lerp(y0, y1, j / ny), False) - drop))) for i in range(nx + 1)] for j in range(ny + 1)]
    hi = [[g.bm.verts.new(v.co + V((0, 0, t))) for v in row] for row in lo]
    for j in range(ny):
        for i in range(nx):
            g.face([lo[j][i], lo[j][i + 1], lo[j + 1][i + 1], lo[j + 1][i]], m)
            g.face([hi[j][i], hi[j + 1][i], hi[j + 1][i + 1], hi[j][i + 1]], m_in)
    rim = [(j, 0) for j in range(ny)] + [(ny, i) for i in range(nx)] + [(j, nx) for j in range(ny, 0, -1)] + [(0, i) for i in range(nx, 0, -1)]
    for (ja, ia), (jb, ib) in zip(rim, rim[1:] + rim[:1]):
        g.face([lo[ja][ia], lo[jb][ib], hi[jb][ib], hi[ja][ia]], m_in)
    return [v for row in lo + hi for v in row]


def wings():
    for s, side in ((1, "right"), (-1, "left")):
        wi, wo = wing_sect(s, False), wing_sect(s, True)
        g = Geo()
        surface(g, wi, 0.0, FLAP[0], 0.0, 1.0, W_T, nu=4, tk=wing_tk)
        surface(g, wi, FLAP[0], ILEF[0], 0.0, TE_HINGE, W_T, nu=4, tk=wing_tk)
        surface(g, wi, ILEF[0], W_FOLD, ILEF_C, TE_HINGE, W_T, nu=5, tk=wing_tk)
        surface(g, wi, FLAP[1], AIL[0], TE_HINGE, 1.0, W_T, nu=2, tk=wing_tk)
        surface(g, wo, W_FOLD, OLEF[1], OLEF_C, 1.0, W_T, nu=6, tk=wing_tk)
        surface(g, wo, OLEF[1], W_X1, 0.0, 1.0, W_T, nu=2, tk=wing_tk)
        # The bulge in the upper skin over the stowed main wheel.
        sx, r, h = BULGE
        ball(g, V((s * sx, MAIN_AXLE.y, wing_z(sx, MAIN_AXLE.y, True) - 0.02)), r, "skin", n=20, squash=(1.0, 1.0, h / r))
        # The main well's opening, seen with its doors open.
        for x0, x1, y0, y1 in (MAIN_DOOR, LEG_DOOR):
            wing_plate(g, s, x0, x1, y0, y1, 0.002, 0.001, "well_dark", "well_dark")
        g.finish(side + " wing")

        for name, sect, (x0, x1), (a, b) in (("inner le flap", wi, ILEF, (0.0, ILEF_C)), ("le flap", wo, OLEF, (0.0, OLEF_C)), ("flap", wi, FLAP, (TE_HINGE, 1.0)), ("aileron", wi, AIL, (TE_HINGE, 1.0))):
            g = Geo()
            surface(g, sect, x0, x1, a, b, W_T, nu=8, tk=wing_tk)
            if name == "aileron":
                for xw in (3.3, 3.9):
                    wick(g, hinge(wi, xw, 1.0))
            xc = b if name.endswith("le flap") else a
            right = wing_sect(1, name == "le flap")
            p0, p1 = hinge(right, x0, xc), hinge(right, x1, xc)
            g.finish(side + " " + name, pivot=M(p0, s), axis=pair_axis((p1 - p0).normalized(), s))

        # The speedbrake: a panel of the lower skin ahead of the flap, hinged along its front edge.
        g = Geo()
        x0, x1, y0, y1 = BRAKE
        wing_plate(g, s, x0, x1, y0, y1, 0.004, 0.022, "skin", "door_in", nx=4, ny=4)
        g.finish(side + " speedbrake", smooth=30, pivot=V((s * (x0 + x1) / 2, y0, wing_z((x0 + x1) / 2, y0, False))), axis=V((1, 0, 0)))

        g = Geo()
        le, c, _, _ = wo(W_X1)
        g.box(V(((W_X1 - 0.03) * s, le.y - 0.25, le.z)), (0.03, 0.06, 0.02), "nav_green" if s > 0 else "nav_red")
        g.finish(side + " nav light", smooth=10)


# The stabilator: all-moving, drooping 23 degrees. Root and tip stations (x out
# from the centreline, measured level), leading edge and chord at each, the
# root's height.
S_X0, S_X1, S_ANH = 0.12, 2.70, math.radians(23.0)
S_LE, S_C, S_Z0 = (-6.10, -8.72), (2.80, 0.68), 0.60
S_PIVOT_Y = -7.45


def stab_sect(s):
    def f(x):
        k = (x - S_X0) / (S_X1 - S_X0)
        le = V((x * s, lerp(S_LE[0], S_LE[1], k), S_Z0 - math.tan(S_ANH) * (x - S_X0)))
        return le, lerp(S_C[0], S_C[1], k), V((0, -1, 0)), V((math.sin(S_ANH) * s, 0, math.cos(S_ANH)))
    return f


# The fin: its root on the tail boom and its tip (leading edge station, chord, height).
F_LE, F_C, F_Z = (-3.90, -7.75), (4.30, 1.25), (0.88, 2.885)  # (the tip fairing takes it to 2.95)
RUDDER = (0.0, 0.62, 0.78)  # from, to (fractions of the fin's height), hinge line (of the chord)


def fin_sect(u):
    le = V((0, lerp(F_LE[0], F_LE[1], u), lerp(F_Z[0], F_Z[1], u)))
    return le, lerp(F_C[0], F_C[1], u), V((0, -1, 0)), V((1, 0, 0))


def tail():
    for s, side in ((1, "right"), (-1, "left")):
        ss = stab_sect(s)
        g = Geo()
        surface(g, ss, S_X0, 0.95, 0.0, 1.0, 0.04, m="heat", nu=3)
        surface(g, ss, 0.95, S_X1, 0.0, 1.0, 0.04, nu=7)
        for xw in (1.4, 1.9):
            wick(g, hinge(ss, xw, 1.0))
        # The two halves are one slab on one shaft across the tail boom: they
        # turn about that level shaft, not each about its own drooping span.
        pv = V((S_X0, S_PIVOT_Y, S_Z0))
        g.finish(side + " stabilator", pivot=M(pv, s), axis=pair_axis(V((1, 0, 0)), s))

    g = Geo()
    u0, u1, xc = RUDDER
    # Thickened toward its root ahead of the rudder: a fillet into the tail boom.
    root = lambda u, x: 1.0 + 2.0 * smoothstep(0.14, -0.04, u) ** 2 * (1.0 - smoothstep(0.25, 0.68, x))
    surface(g, fin_sect, -0.05, u1, 0.0, xc, 0.04, nu=10, tk=root)
    surface(g, fin_sect, u1, 1.0, 0.0, 1.0, 0.04, nu=5)
    # The pitot head on the fin's leading edge, the antenna fairing along its tip.
    le, c, cdir, out = fin_sect(0.90)
    tube(g, [le + cdir * 0.10, le - cdir * 0.42], lambda t: 0.022 - 0.012 * t, "steel", n=8)
    le, c, cdir, out = fin_sect(1.0)
    up = V((0, 0, 1))
    tube(g, [le + cdir * (c * k) + up * 0.015 for k in (-0.06, 0.1, 0.5, 0.9, 1.10)], lambda t: 0.045 * (1.0 - (2 * t - 1) ** 4) + 0.006, "skin", n=10)
    wick(g, le + cdir * (c * 1.08) + up * 0.015, up=up)
    # Formation light strips.
    le, c, cdir, out = fin_sect(0.55)
    mid = le + cdir * (c * 0.40)
    for k in (1, -1):
        g.box(mid + out * (k * (c * naca_t(0.40, 0.04) + 0.002)), (0.004, 0.07, 0.26), "slime", (out, cdir, up))
    g.finish("fin")

    g = Geo()
    surface(g, fin_sect, u0, u1, xc, 1.0, 0.04, nu=6)
    p0, p1 = hinge(fin_sect, u0, xc), hinge(fin_sect, u1, xc)
    g.finish("rudder", pivot=p0, axis=(p1 - p0).normalized())

    g = Geo()
    g.box(V((0, -8.93, 0.72)), (0.04, 0.03, 0.025), "tail_white")
    g.finish("tail light", smooth=10)


# ---------------------------------------------------------------- landing gear (modelled down and locked)
# The nose leg swings aft into its well under the front cockpit. Each main
# leg hangs from the wing and swings inward, the wheel ending up level with
# its pivot, lying nearly flat in the wing's root (see `anim` in Rust).
NOSE_PIVOT = V((0, 6.42, -0.52))
NOSE_AXLE = V((0, 6.30, -1.82))
NOSE_R = 0.23
NOSE_STOWED_Z = -0.62  # height of the nose axle with the gear up
MAIN_PIVOT = V((2.87, -0.80, -0.74))
MAIN_AXLE = V((2.72, -0.80, -1.67))  # the wheel's centre, inboard of the strut
MAIN_R = 0.38
MAIN_HW = 0.095
_d = MAIN_AXLE - MAIN_PIVOT
MAIN_SWING = math.atan2(-_d.z, -_d.x)  # about the fore-and-aft axis: brings the wheel's centre level with the pivot
MAIN_STOWED = MAIN_PIVOT + Matrix.Rotation(MAIN_SWING, 3, V((0, 1, 0))) @ _d
# The doors, as patches of the wing's lower skin (span stations, front, back):
# over the wheel, hinged along its inboard edge; over the strut, carried by the leg.
MAIN_DOOR = (MAIN_STOWED.x - 0.41, MAIN_STOWED.x + 0.41, -0.39, -1.21)
LEG_DOOR = (MAIN_STOWED.x + 0.42, MAIN_PIVOT.x + 0.10, -0.66, -0.94)
BULGE = (MAIN_STOWED.x + 0.06, 0.46, 0.12)  # the bulge in the wing's upper skin over the wheel: span station, radius, height


def gear():
    # Nose: swings aft into the well under the front cockpit.
    l = (NOSE_AXLE - NOSE_PIVOT).length
    dz = NOSE_STOWED_Z - NOSE_PIVOT.z
    ax, ang = stow(NOSE_PIVOT, NOSE_AXLE, NOSE_PIVOT + V((0, -math.sqrt(l * l - dz * dz), dz)))
    along = (NOSE_PIVOT - NOSE_AXLE).normalized()
    low = NOSE_AXLE + along * 0.36
    g = Geo()
    # The trunnion across the top, the strut, the steering collar.
    tube(g, [NOSE_PIVOT + V((-0.17, 0, 0)), NOSE_PIVOT + V((0.17, 0, 0))], 0.045, "strut", n=10)
    tube(g, [NOSE_PIVOT, low], 0.062, "strut", n=14)
    lathe(g, NOSE_PIVOT - along * 0.70, -along, [(0.0, 0.0), (0.088, 0.0), (0.088, 0.12), (0.0, 0.12)], "strut", n=14)
    # Landing and taxi lights on a bracket ahead of the strut.
    fwd = (V((0, 1, 0)) - along * along.y).normalized()
    g.box(NOSE_PIVOT - along * 0.50 + fwd * 0.10, (0.055, 0.03, 0.04), "strut", (V((1, 0, 0)), fwd, along))
    g.box(NOSE_PIVOT - along * 0.50 + fwd * 0.132, (0.045, 0.004, 0.032), "landing", (V((1, 0, 0)), fwd, along))
    # Hydraulic lines down the back of the strut.
    for x in (-0.03, 0.03):
        tube(g, [NOSE_PIVOT + V((x, -0.065, 0)) - along * 0.05, low + V((x, -0.07, 0))], 0.008, "steel", n=5)
    g.finish("nose leg", pivot=NOSE_PIVOT, axis=ax, angle=ang)

    down = (NOSE_AXLE - NOSE_PIVOT).normalized()
    g = Geo()
    tube(g, [low + along * 0.10, NOSE_AXLE], 0.046, "chrome", n=12)
    g.box(NOSE_AXLE + along * 0.04, (0.05, 0.045, 0.05), "strut")
    tube(g, [NOSE_AXLE + V((-0.19, 0, 0)), NOSE_AXLE + V((0.19, 0, 0))], 0.028, "strut")
    scissors(g, low + V((0, 0.055, 0)), NOSE_AXLE + along * 0.06 + V((0, 0.045, 0)), V((0, 0.12, 0)))
    g.finish("nose fork", pivot=NOSE_AXLE, axis=down, parent="nose leg")

    g = Geo()
    for x in (-0.115, 0.115):
        wheel(g, NOSE_AXLE + V((x, 0, 0)), NOSE_R, 0.058)
    g.finish("nose wheel", pivot=NOSE_AXLE, axis=V((1, 0, 0)), parent="nose fork")

    # Mains.
    knee = V((MAIN_PIVOT.x, MAIN_PIVOT.y, MAIN_AXLE.z + 0.05))
    mid = MAIN_PIVOT.lerp(knee, 0.55)
    unstow = Matrix.Translation(MAIN_PIVOT) @ Matrix.Rotation(-MAIN_SWING, 4, V((0, 1, 0))) @ Matrix.Translation(-MAIN_PIVOT)
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        # The trunnion it swings on, the strut's outer cylinder and its oleo, the axle.
        tube(g, [M(MAIN_PIVOT + V((0, 0.15, 0)), s), M(MAIN_PIVOT + V((0, -0.15, 0)), s)], 0.04, "strut", n=10)
        tube(g, [M(MAIN_PIVOT, s), M(mid, s)], 0.056, "strut", n=14)
        tube(g, [M(mid, s), M(knee, s)], 0.036, "chrome", n=12)
        g.box(M(knee + V((-0.01, 0, 0.01)), s), (0.03, 0.04, 0.035), "strut")
        tube(g, [M(knee + V((0.02, 0, -0.05)), s), M(MAIN_AXLE + V((-0.02, 0, 0)), s)], 0.034, "steel", n=10)
        lathe(g, M(MAIN_AXLE + V((MAIN_HW + 0.012, 0, 0)), s), V((s, 0, 0)), [(0.0, 0.0), (0.17, 0.0), (0.17, 0.03), (0.0, 0.03)], "steel", n=16)
        scissors(g, M(mid + V((0, 0.06, 0.06)), s), M(knee + V((0, 0.06, 0.03)), s), M(V((0, 0.13, 0)), s))
        # The strut's door: a patch of the wing's lower skin, laid out shut and
        # swung down with the leg.
        x0, x1, y0, y1 = LEG_DOOR
        for v in wing_plate(g, 1, x0, x1, y0, y1, 0.006, 0.012, "skin", "door_in", nx=5, ny=3):
            v.co = M(unstow @ v.co, s)
        g.finish(side + " main leg", pivot=M(MAIN_PIVOT, s), axis=pair_axis(V((0, 1, 0)), s), angle=MAIN_SWING)

        g = Geo()
        wheel(g, M(MAIN_AXLE, s), MAIN_R, MAIN_HW)
        g.finish(side + " main wheel", pivot=M(MAIN_AXLE, s), axis=V((1, 0, 0)), parent=side + " main leg")
    print(f"main gear: swing {math.degrees(MAIN_SWING):.0f} deg, wheel stowed at {tuple(round(c, 3) for c in MAIN_STOWED)}")


def doors():
    hw, ny0, ny1, _ = NOSE_WELL
    gap = 0.005
    for s, side in ((1, "right"), (-1, "left")):
        # Nose: a pair meeting on the centreline, hinged at the well's sides.
        skin_door(side + " nose door", s, gap, hw - gap, ny1 - gap, ny0 + gap, hw, math.radians(86))
        # Main: over the wheel, hinged along its inboard edge; modelled open.
        x0, x1, y0, y1 = MAIN_DOOR
        g = Geo()
        vs = wing_plate(g, s, x0 + gap, x1 - gap, y0 - gap, y1 + gap, 0.006, 0.012, "skin", "door_in", nx=8, ny=8)
        h0 = V((s * x0, y0, wing_z(x0, y0, False) - 0.006))
        axis = pair_axis(V((0, 1, 0)), s)
        open_angle = math.radians(84)
        rot = Matrix.Translation(h0) @ Matrix.Rotation(open_angle, 4, axis) @ Matrix.Translation(-h0)
        for v in vs:
            v.co = rot @ v.co
        g.finish(side + " main door", smooth=30, pivot=h0, axis=axis, angle=-open_angle)


def check_gear():
    """Sweep the gear from up to down (as Rust `anim` moves it: the legs over
    0.15..0.85) and report any nose gear vertex inside the fuselage's skin but
    outside its well, and, stowed, any vertex not tucked in: the nose gear
    behind its doors, the main gear inside the wing. Run on every export: a
    hand edit that makes the gear poke through the skin shows up here."""
    obs = {o.name: o for o in COLL.all_objects}

    def local(n, g):
        o = obs[n]
        a = o.get("angle", 0.0)
        if abs(a) < 1e-6 or not n.endswith(" leg"):
            return Matrix.Identity(4)
        a *= 1 - smoothstep(0.15, 0.85, g)
        p = V(o["pivot"])
        return Matrix.Translation(p) @ Matrix.Rotation(a, 4, V(o["axis"])) @ Matrix.Translation(-p)

    def world(n, g):
        par = obs[n].get("parent")
        return (world(par, g) if par else Matrix.Identity(4)) @ local(n, g)

    def depth(p):
        """How far inside the fuselage's skin p is (negative: outside)."""
        if not FUSE[-1][0] < p.y < FUSE[0][0]:
            return -1.0
        return min(p.z - fuse_z(p.y, p.x, False), fuse_z(p.y, p.x, True) - p.z, fuse_hw(p.y, p.z) - abs(p.x))

    def in_well(p):
        hw, y0, y1, zt = NOSE_WELL
        return abs(p.x) < hw and y0 < p.y < y1 and p.z < zt

    def out_of_wing(p):
        """How far outside the wing's skin (and the bulge over the wheel) p is."""
        x = abs(p.x)
        sx, r, h = BULGE
        bulge = max(h * max(0.0, 1.0 - ((x - sx) ** 2 + (p.y - MAIN_AXLE.y) ** 2) / r ** 2) ** 0.5 - 0.02, 0.0)
        return max(wing_z(x, p.y, False) - 0.02 - p.z, p.z - wing_z(x, p.y, True) - bulge, 0.0)

    gear = [n for n in obs if "door" not in n and any(k in n for k in ("leg", "wheel", "fork"))]
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
                if "main" in n:
                    if g == 0.0:
                        e = out_of_wing(p)  # stowed: inside the wing
                else:
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
# Every piece the game looks for (PIECES in src/aircraft/f4/mod.rs); other objects are ignored.
EXPECTED = ["radome", "nose section", "fuselage", "tail cone", "spine", "canopy", "canopy frame", "fin", "rudder", "tail light", "nose leg", "nose fork", "nose wheel", "afterburner"] + [
    f"{s} {n}" for s in ("left", "right") for n in ("intake", "nozzle", "wing", "inner le flap", "le flap", "flap", "aileron", "speedbrake", "nav light", "stabilator", "main leg", "main wheel", "nose door", "main door")]


def game_mat(m):
    """Which game material a Blender material is: its "game" property, its f4_ name, or else the skin."""
    if m is None:
        return "skin"
    name = m.get("game") or (m.name[3:].split(".")[0] if m.name.startswith("f4_") else None)
    return name if name in MATS else "skin"


def export(path):
    out = bytearray(b"FFAC")
    obs = [o for o in COLL.all_objects if o.type == "MESH"]
    names = {o.name for o in obs}
    missing = [n for n in EXPECTED if n not in names]
    if missing:
        raise SystemExit(f"f4 export: the game needs these pieces, missing from the '{COLL.name}' collection: {', '.join(missing)}")
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
                        if 2.4 < p.y < 6.5 and p.z > 0.3 and abs(p.x) < table_at(CANOPY, p.y)[1] - 0.021:
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
    views = (("three_quarter", V((15, 17, 8)), 50, target), ("side", V((30, 0.01, 0.6)), 50, target), ("top", V((0.01, 0.3, 50)), 50, target), ("front", V((0.01, 30, 1.0)), 50, target), ("rear_low", V((-11, -16, -2.5)), 50, target),
             ("rear", V((4.0, -17, 2.6)), 60, V((0, -6, 0.2))), ("nozzles", V((2.4, -9.5, -0.2)), 70, V((0, -4.8, -0.2))), ("cockpit", V((3.2, 8.4, 2.9)), 75, V((0, 4.6, 0.9))), ("under", V((8, 10, -5.5)), 50, V((0, 0.5, -0.3))))
    for name, eye, lens, look in views:
        cam.location = eye
        cam.rotation_euler = (look - eye).to_track_quat("-Z", "Y").to_euler()
        cam_data.lens = lens
        sc.render.filepath = os.path.join(outdir, name + ".png")
        bpy.ops.render.render(write_still=True)


# The paint is projected straight down, straight up and from each side: these
# are the rectangles (m) those pictures cover, the same as the Hornet's
# (assets/blender/fa18.py), so plane.wgsl projects both the same way.
GUIDE_SPAN = 13.2   # across the top and bottom views; up the side views
GUIDE_LEN = 19.8    # fore and aft in all of them
GUIDE_Y = 0.4       # station at the middle of the pictures
GUIDE_Z = 1.0       # height at the middle of the side views
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
    # Gear up: only what the paint goes on.
    hidden = [o for o in COLL.all_objects if any(k in o.name for k in ("leg", "wheel", "fork", "door", "afterburner"))]
    for o in hidden:
        o.hide_render = True
    cam, cam_data = camera()
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = GUIDE_LEN
    short = round(GUIDE_PX * GUIDE_SPAN / GUIDE_LEN)
    # (The left side is seen nose to the left, the right side nose to the right.)
    views = (("top", V((0, GUIDE_Y, 40)), V((0, 0, -1)), V((0, 1, 0)), (short, GUIDE_PX)),
             ("bottom", V((0, GUIDE_Y, -40)), V((0, 0, 1)), V((0, 1, 0)), (short, GUIDE_PX)),
             ("side", V((-40, GUIDE_Y, GUIDE_Z)), V((1, 0, 0)), V((0, 0, 1)), (GUIDE_PX, short)),
             ("right", V((40, GUIDE_Y, GUIDE_Z)), V((-1, 0, 0)), V((0, 0, 1)), (GUIDE_PX, short)))
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
    geometry_pass(outdir, views)
    for o in hidden:
        o.hide_render = o.name == "afterburner"


def geometry_pass(outdir, views):
    """What each pixel of a guide is a picture of, for assets/blender/f4_paint.py
    (which paints by where on the aircraft a pixel is, not by hand):
    <view>_geo.npz with, for each pixel, the point of the model seen there
    (`depth`: its height seen from above or below, its x seen from a side, NaN
    off the model; the picture's own grid gives the rest), the surface's normal (`nrm`),
    and which piece and material it is (`piece`, `mat`: indices into `pieces`
    and `mats`). Rays are cast through the pixel centres of the renders above."""
    import numpy as np
    from mathutils.bvhtree import BVHTree
    deps = bpy.context.evaluated_depsgraph_get()
    obs = sorted((o for o in COLL.all_objects if o.type == "MESH" and not o.hide_render), key=lambda o: o.name)
    mats = sorted(MATS)
    verts, polys, owner = [], [], []
    for k, ob in enumerate(obs):
        me = ob.evaluated_get(deps).to_mesh()
        base = len(verts)
        verts += [ob.matrix_world @ v.co for v in me.vertices]
        for p in me.polygons:
            polys.append([base + i for i in p.vertices])
            owner.append((k, mats.index(game_mat(me.materials[p.material_index] if p.material_index < len(me.materials) else None))))
        ob.evaluated_get(deps).to_mesh_clear()
    tree = BVHTree.FromPolygons(verts, polys)
    for name, eye, look, up, (w, h) in views:
        right = look.cross(up)
        step = GUIDE_LEN / GUIDE_PX
        pos = np.full((h, w, 3), np.nan, np.float16)  # (to 5 mm: a pixel is 13)
        nrm = np.zeros((h, w, 3), np.int8)
        piece = np.full((h, w), 255, np.uint8)
        mat = np.full((h, w), 255, np.uint8)
        for j in range(h):
            row = eye + up * ((h / 2 - j - 0.5) * step)
            for i in range(w):
                hit, n, f, _ = tree.ray_cast(row + right * ((i + 0.5 - w / 2) * step), look)
                if hit is not None:
                    if n.dot(look) > 0:
                        n = -n
                    pos[j, i] = hit
                    nrm[j, i] = (round(n.x * 127), round(n.y * 127), round(n.z * 127))
                    piece[j, i], mat[j, i] = owner[f]
        np.savez_compressed(os.path.join(outdir, name + "_geo.npz"), depth=pos[..., 2 if abs(look.z) > 0.5 else 0], nrm=nrm, piece=piece, mat=mat, pieces=np.array([o.name for o in obs]), mats=np.array(mats))
        print(f"  {name}: geometry pass, {int((piece < 255).sum())} pixels on the model")


def main():
    global COLL, CUTS
    if EXPORT_ONLY:
        # Export the .blend that is open, edits and all, without rebuilding anything.
        COLL = bpy.data.collections.get("F-4E")
        if COLL is None:
            raise SystemExit("f4 export: no 'F-4E' collection in this file (open assets/blender/f4.blend)")
        export(os.path.join(ASSETS, "f4.bin.gz"))
        check_gear()
    else:
        for ob in list(bpy.data.objects):
            bpy.data.objects.remove(ob, do_unlink=True)
        for c in list(bpy.data.collections):
            bpy.data.collections.remove(c)
        COLL = bpy.data.collections.new("F-4E")
        bpy.context.scene.collection.children.link(COLL)
        CUTS = bpy.data.collections.new("F-4E well cutters")
        bpy.context.scene.collection.children.link(CUTS)
        make_materials()
        fuselage()
        intakes()
        nozzles()
        canopy()
        wings()
        tail()
        gear()
        doors()
        plume()
        os.makedirs(os.path.join(ASSETS, "blender"), exist_ok=True)
        bpy.context.preferences.filepaths.save_version = 0  # no f4.blend1 beside it
        export(os.path.join(ASSETS, "f4.bin.gz"))
        check_gear()
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ASSETS, "blender", "f4.blend"))
    if RENDER:
        render_previews(RENDER)
    if GUIDES:
        render_guides(GUIDES)


main()
