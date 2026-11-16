# The SR-71A Blackbird for FableFlight, built procedurally in Blender.
#
#   blender -b --factory-startup --python assets/blender/sr71.py -- <repo>/assets [--render <dir>] [--guides <dir>]
#
# Builds the airframe from lofted sections and thin lifting surfaces, one
# object per piece that moves or breaks off on its own, saves it as
# assets/blender/sr71.blend and exports assets/sr71.bin.gz, which
# src/aircraft/sr71 loads. That rebuild starts from scratch, so it throws
# away anything edited by hand in the .blend.
#
# To keep hand edits, export the .blend as it stands instead (save it first):
#
#   blender -b assets/blender/sr71.blend --python assets/blender/sr71.py -- <repo>/assets --export-only
#
# Edit freely: reshape meshes, add modifiers (they are applied on export),
# move or turn pieces (each carries its hinge along), assign the sr71_
# materials. Keep the piece names and the "SR-71A" collection: the game finds
# pieces by name (see EXPECTED below); objects with other names are ignored.
# How each piece moves is set in Rust by its name; each object records where
# its hinge is in its own frame (custom properties "pivot", "axis", "angle").
#
# The toolkit (Geo and what follows it) is the Hornet's (fa18.py). What is the
# Blackbird's own: a round fuselage with a sharp chine each side that runs
# from the nose into the delta wing's leading edge, the two nacelles half way
# out the wing with their inlet spikes (which slide) and ejector nozzles, the
# all-moving fins on top of them leaning in, elevons inboard and outboard of
# the nacelles, and main legs with three wheels abreast that swing inboard
# into the fuselage.
#
# The gear wells are cut into the fuselage and the wings with boolean
# modifiers (cutters in the "SR-71A well cutters" collection, not exported);
# the fuselage's gear doors are patches of the skin itself, and the door
# under each wing rides on its leg. Every export ends with a gear check: a
# sweep of the legs from up to down that names any part passing through the
# skin.
#
# --render writes preview pictures. --guides writes the four orthographic
# views (top, bottom, left side, right side) the paint is projected from, each
# with its silhouette mask and what each of its pixels is a picture of.
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
# Pattern 17 is the painted skin (plane.wgsl P_HORNET): the paint projected
# from the aircraft's pictures; the vertex colour shows only where no picture
# faces the surface.
# Material 11 (exhaust) extra: 0 the nozzle petals outside, 1 inside the
# nozzle, 2 the flame holder; material 17 (the exhaust plume, drawn in the
# transparent pass) extra: 0 heat haze, 1 the flame, 2 its core, 3 discs across it.
MATS = {
    "skin": (15, 17, (0.022, 0.023, 0.027), 0),  # the black of its heat-shedding paint, a shade lighter underneath on export
    "radome": (15, 17, (0.020, 0.021, 0.024), 0),
    "skin_in": (15, 0, (0.022, 0.023, 0.027), 0),  # the fins' inner faces: plain paint (seen from the side they would show the other fin's serial)
    "inlet": (10, 0, (0.02, 0.02, 0.022), 0),
    "exhaust": (11, 0, (0.30, 0.28, 0.26), 0),
    "nozzle_in": (11, 0, (0.06, 0.055, 0.05), 1),
    "burner": (11, 0, (0.10, 0.09, 0.08), 2),
    "glass": (7, 0, (0.10, 0.10, 0.10), 0),
    "frame": (15, 0, (0.03, 0.031, 0.034), 0),
    "cockpit": (10, 0, (0.03, 0.03, 0.034), 0),
    "strut": (3, 0, (0.62, 0.63, 0.64), 0),  # its gear is bare, silvered metal against the heat
    "door_in": (2, 0, (0.60, 0.61, 0.62), 0),
    "well": (2, 0, (0.50, 0.51, 0.52), 0),  # inside the gear wells
    "chrome": (3, 0, (0.85, 0.86, 0.88), 1),
    "steel": (3, 0, (0.32, 0.32, 0.33), 0),
    "rubber": (2, 0, (0.55, 0.56, 0.57), 0),  # the tyres too: aluminium powder in the rubber
    "hub": (0, 0, (0.70, 0.70, 0.68), 0),
    "nav_red": (8, 0, (1.0, 0.06, 0.05), 0),
    "nav_green": (8, 0, (0.05, 1.0, 0.25), 1),
    "tail_white": (8, 0, (1.0, 1.0, 1.0), 2),
    "landing": (8, 0, (1.0, 1.0, 0.95), 5),
    "strobe": (8, 0, (1.0, 0.10, 0.06), 4),
    "haze": (17, 0, (1.0, 1.0, 1.0), 0),
    "flame": (17, 0, (1.0, 0.6, 0.3), 1),
    "core": (17, 0, (0.7, 0.8, 1.0), 2),
    "disc": (17, 0, (1.0, 0.8, 0.6), 3),
    "chute_white": (6, 0, (0.78, 0.78, 0.74), 0),  # the drag chute's ribbons, its lines
    "chute_orange": (6, 0, (0.80, 0.22, 0.03), 0),
}
SKIN_LIGHT = (0.030, 0.031, 0.035)  # underneath
SKIN_DARK = (0.020, 0.021, 0.025)  # on top
COCKPIT_TUB = (0.03, 0.03, 0.034)

MAT = {}


def make_materials():
    for name, (gid, _pat, col, _x) in MATS.items():
        m = bpy.data.materials.get("sr71_" + name) or bpy.data.materials.new("sr71_" + name)
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

def circle(c, r, y, n=28):
    """A ring of radius r about the fore-aft line through c, at station y."""
    return [V((c.x + r * math.cos(t), y, c.z + r * math.sin(t))) for t in (2 * math.pi * k / n for k in range(n))]


def tube_skin(g, rings, m, c, out=True):
    """An open tube lofted through rings about the fore-aft line through c, its
    faces made to face away from that line (or, the inside of a duct, in toward
    it): the outside of an open tube cannot be worked out from the mesh."""
    vr = [[g.bm.verts.new(p) for p in r] for r in rings]
    n = len(rings[0])
    for a, b in zip(vr, vr[1:]):
        for i in range(n):
            j = (i + 1) % n
            f = g.face([a[i], a[j], b[j], b[i]], m)
            if f is not None:
                fc = f.calc_center_median()
                r = V((fc.x - c.x, 0, fc.z - c.z))
                g.facing.append((f, fc + r.normalized() * (10.0 if out else -10.0)))


# ---------------------------------------------------------------- fuselage
# station y, half width (out to the chine's edge), top, height of the chine,
# bottom, half width of the round body the chines stand out from. Kept in step
# with FUSE in src/aircraft/sr71/mod.rs. From station 1.2 aft the chine has
# become the wing's leading edge and the table is the body inside the wing
# root; round the main wheels (station -4.2) the body is a little fuller than
# the real one, to take them.
FUSE = [
    (16.70, 0.00, -0.10, -0.10, -0.10, 0.00),
    (16.40, 0.10, -0.02, -0.10, -0.17, 0.07),
    (15.80, 0.24, 0.10, -0.10, -0.27, 0.17),
    (15.00, 0.40, 0.24, -0.10, -0.38, 0.28),
    (14.00, 0.58, 0.38, -0.10, -0.48, 0.40),
    (13.00, 0.74, 0.50, -0.10, -0.56, 0.50),
    (12.00, 0.88, 0.60, -0.10, -0.63, 0.58),
    (11.00, 0.98, 0.68, -0.10, -0.69, 0.64),
    (10.00, 1.06, 0.74, -0.10, -0.74, 0.70),
    (9.00, 1.14, 0.78, -0.10, -0.78, 0.75),
    (8.00, 1.22, 0.80, -0.10, -0.80, 0.79),
    (6.50, 1.36, 0.81, -0.10, -0.81, 0.81),
    (5.00, 1.52, 0.81, -0.10, -0.81, 0.81),
    (3.20, 1.66, 0.81, -0.10, -0.81, 0.81),
    (1.20, 1.72, 0.81, -0.10, -0.81, 0.81),
    (-2.00, 1.72, 0.81, -0.10, -0.83, 0.86),
    (-4.20, 1.72, 0.82, -0.10, -0.86, 0.90),
    (-6.40, 1.72, 0.81, -0.10, -0.83, 0.86),
    (-9.00, 1.68, 0.72, -0.10, -0.70, 0.76),
    (-11.00, 1.50, 0.54, -0.10, -0.48, 0.58),
    (-13.00, 0.90, 0.28, -0.10, -0.28, 0.32),
    (-14.74, 0.04, -0.07, -0.10, -0.13, 0.02),
]
# A ring: NU segments from the chine's edge over the top to the centreline (NA
# of them across the chine, the rest round the body), NL likewise underneath.
NU, NL, NA, NB = 16, 12, 5, 4
# Round the cockpit opening the ring is laid out so that vertex TUB_K (and its
# mirror image) lies exactly on the opening's edge.
TUB_K = 9
CHINE_T = (0.15, 0.11)  # how thick the chine is where it leaves the body: above its edge, below
ENGINE_X = 4.12  # the engines' centrelines
ENGINE_Z = -0.10


def tub_cut(y):
    """Half width of the cockpit opening at station y."""
    return table_at(CANOPY, y)[1] - 0.03


def section_h(row, ax, upper=True):
    """How far above (or below) the chine's height a section's skin is, ax out
    from the centreline: the round body, and the chine tapering from it to a
    sharp edge, run together."""
    y, hw, top, mid, bot, rb = row
    if hw < 1e-6:
        return 0.0
    ax = min(abs(ax), hw)
    e = (top - mid) if upper else (mid - bot)
    body = e * math.sqrt(max(0.0, 1.0 - (ax / rb) ** 2)) if rb > 1e-6 and ax < rb else 0.0
    chine = min(CHINE_T[0 if upper else 1], e) * (1.0 - ax / hw) ** 0.75
    return (body ** 3 + chine ** 3) ** (1 / 3)


def section_z(row, x, upper=True):
    return row[3] + section_h(row, x, upper) * (1.0 if upper else -1.0)


def fuse_ring(row, tub=0.0):
    y, hw, rb = row[0], max(row[1], 0.0), row[5]
    rbx = min(rb, hw * 0.92)
    xs = [lerp(hw, rbx, i / NA) if i <= NA else rbx * math.cos(math.pi / 2 * (i - NA) / (NU - NA)) for i in range(NU + 1)]
    xs[NU] = 0.0
    if tub > 0.0 and hw > 1e-6:
        cut = min(tub_cut(y), hw * 0.95)
        for i in range(NU + 1):
            x = lerp(hw, cut, (i / TUB_K) ** 0.7) if i <= TUB_K else cut * (1 - (i - TUB_K) / (NU - TUB_K))
            xs[i] = lerp(xs[i], x, tub)
    pts = [V((x, y, section_z(row, x))) for x in xs]
    pts += [V((-x, y, section_z(row, x))) for x in reversed(xs[:-1])]
    xl = [lerp(hw, rbx, j / NB) if j <= NB else rbx * math.cos(math.pi / 2 * (j - NB) / (NL - NB)) for j in range(1, NL + 1)]
    xl[-1] = 0.0
    pts += [V((-x, y, section_z(row, x, False))) for x in xl]
    pts += [V((x, y, section_z(row, x, False))) for x in reversed(xl[:-1])]
    return pts


def fuse_top(y):
    return table_at(FUSE, y)[2]


def fuse_z(y, x, upper=True):
    """Height of the fuselage skin at station y, x out from the centreline."""
    return section_z(table_at(FUSE, y), x, upper)


def fuse_hw(y, z):
    """Half width of the fuselage at station y, height z."""
    row = table_at(FUSE, y)
    upper = z >= row[3]
    want = abs(z - row[3])
    if want >= section_h(row, 0.0, upper):
        return 0.0
    lo, hi = 0.0, row[1]
    for _ in range(30):
        m = (lo + hi) / 2
        if section_h(row, m, upper) >= want:
            lo = m
        else:
            hi = m
    return lo


# The cockpits (the pilot's, and the reconnaissance systems officer's behind
# it) share one opening in the skin: rows of the nose section it runs between,
# its floor.
TUB_Y = (12.0, 9.0)
TUB_FLOOR = 0.02
TUB_RIM = []  # per row of the opening: the ring's points from its right edge over the top to its left edge
WELLS = {}


def fuselage():
    idx = {round(r[0], 2): i for i, r in enumerate(FUSE)}
    WELLS["nose"] = nose_well()
    for s in (1, -1):
        WELLS[s] = main_well(s)
    for name, y0, y1 in (("radome", 16.70, 14.00), ("nose section", 14.00, 6.50), ("fuselage", 6.50, -11.00), ("tail cone", -11.00, -14.74)):
        g = Geo()
        rows = spline_rows(FUSE, idx[y0], idx[y1], 4)
        if name == "nose section":
            weight = lambda y: smoothstep(12.5, 12.0, y) * smoothstep(8.5, 9.0, y)
            rings = [fuse_ring(r, tub=weight(r[0])) for r in rows]
            inside = [TUB_Y[1] < r[0] < TUB_Y[0] for r in rows]
            TUB_RIM.extend(rings[i][TUB_K:2 * NU - TUB_K + 1] for i in range(len(rows)) if inside[i])
            g.loft(rings, "skin", skip=lambda row, i: inside[row] and inside[row + 1] and TUB_K <= i < 2 * NU - TUB_K)
            # The upper anti-collision light, behind the cockpits.
            g.box(V((0, 7.6, fuse_top(7.6) + 0.012)), (0.03, 0.05, 0.02), "strobe")
        else:
            g.loft([fuse_ring(r) for r in rows], "radome" if name == "radome" else "skin")
        if name == "radome":
            # The pitot boom.
            tube(g, [V((0, 16.62, -0.10)), V((0, 17.2, -0.10)), V((0, 18.0, -0.10))], lambda t: 0.035 - 0.024 * t, "steel", n=10)
        if name == "fuselage":
            # The lower anti-collision light.
            g.box(V((0, 2.0, fuse_z(2.0, 0, upper=False) - 0.012)), (0.03, 0.05, 0.02), "strobe")
        ob = g.finish(name)
        if name == "nose section":
            carve(ob, [WELLS["nose"]])
        if name == "fuselage":
            carve(ob, [WELLS[1], WELLS[-1]])

    g = Geo()
    g.box(V((0, -14.72, -0.10)), (0.04, 0.03, 0.025), "tail_white")
    g.finish("tail light", smooth=10)


# The gear wells: the nose well behind the cockpits (half width, aft end, front
# end, roof), and a main well each side that runs from the fuselage out under
# the wing to the leg's hinge (inboard edge, outboard edge, front, back; its
# roof in the fuselage, as far out as MAIN_WELL_IN, and under the wing).
NOSE_WELL = (0.30, 7.20, 9.60, -0.12)
MAIN_WELL = (0.02, 2.62, -3.76, -4.64)
MAIN_WELL_IN = 0.78
MAIN_WELL_ROOF = (0.36, 0.05)


def nose_well():
    hw, y0, y1, zt = NOSE_WELL
    return cutter("nose well", [[V((-hw, y, -1.4)), V((hw, y, -1.4)), V((hw, y, zt)), V((-hw, y, zt))] for y in (y1, y0)])


def main_well(s):
    xi, xo, y0, y1 = MAIN_WELL
    zi, zo = MAIN_WELL_ROOF
    ring = lambda y: [V((xi * s, y, -1.5)), V((xo * s, y, -1.5)), V((xo * s, y, zo)), V((MAIN_WELL_IN * s, y, zo)), V((MAIN_WELL_IN * s, y, zi)), V((xi * s, y, zi))]
    return cutter(("right" if s > 0 else "left") + " main well", [ring(y0), ring(y1)])


# ---------------------------------------------------------------- nacelles
# station y, radius: from the inlet's sharp lip back to where the ejector
# nozzle starts.
NACELLE = [(2.80, 0.70), (2.40, 0.765), (1.60, 0.83), (0.30, 0.88), (-3.00, 0.88), (-7.00, 0.88), (-9.50, 0.895), (-10.90, 0.88)]
NAC_CHINE = (2.45, -4.75, 0.30)  # the chine along a nacelle's outboard side: from, to (where the wing takes over), how far it stands out
SPIKE_TIP = 4.10   # the inlet spike's point, forward (subsonic); it slides SPIKE_TRAVEL aft by Mach 3.2
SPIKE_TRAVEL = 0.66
NOZZLE_Y0, NOZZLE_Y = -10.90, -12.40  # the ejector nozzle: where it starts, its exit
NOZZLE_K = (0.88 / 0.475, 1.5)        # how much bigger than the Hornet's it is inside (liner, flame holder): across, along
EJECTOR_FLAPS = 0.86                  # how far along the ejector its trailing-edge flaps start
BLOW_IN = (14, 0.16, 0.50, 0.25)      # the blow-in doors round it: how many, from and to along it, how wide


def nacelles():
    for s, side in ((1, "right"), (-1, "left")):
        c = V((ENGINE_X * s, 0, ENGINE_Z))
        g = Geo()
        tube_skin(g, [circle(c, r, y) for y, r in spline_rows(NACELLE, 0, len(NACELLE) - 1, 3)], "skin", c)
        # The lip, the duct running back from it into the dark, and its end.
        y, r = NACELLE[0]
        a, b = [g.bm.verts.new(p) for p in circle(c, r, y)], [g.bm.verts.new(p) for p in circle(c, r - 0.025, y)]
        for i in range(len(a)):
            j = (i + 1) % len(a)
            f = g.face([a[i], a[j], b[j], b[i]], "skin")
            g.facing.append((f, f.calc_center_median() + V((0, 10, 0))))
        tube_skin(g, [circle(c, r - 0.025, y), circle(c, r - 0.01, y - 0.6), circle(c, 0.60, y - 2.0)], "inlet", c, out=False)
        g.poly(circle(c, 0.60, y - 2.0), "inlet", toward=c + V((0, 20, 0)))
        # The chine carried on along the nacelle's outboard side, into the outer wing's leading edge.
        rings = []
        for k in range(15):
            y = lerp(NAC_CHINE[0], NAC_CHINE[1], k / 14)
            f = (NAC_CHINE[0] - y) / (NAC_CHINE[0] - NAC_CHINE[1])
            w = NAC_CHINE[2] * min(1.0, f / 0.3) ** 0.7 + 0.004
            t = 0.02 + 0.07 * min(1.0, f / 0.2)
            x0 = ENGINE_X + table_at(NACELLE, y)[1] - 0.06
            rings.append([V((s * x0, y, ENGINE_Z + t)), V((s * (x0 + 0.06 + w), y, ENGINE_Z + 0.004)), V((s * (x0 + 0.06 + w), y, ENGINE_Z - 0.004)), V((s * x0, y, ENGINE_Z - t))])
        g.loft(rings, "skin")
        # The stub the fin turns on.
        fs = fin_sect(s)
        n0 = len(g.bm.faces)
        surface(g, fs, -0.07, FIN_SPLIT, 0.0, 1.0, FIN_T, nu=3)
        inner_faces(g, s, n0)
        g.finish(side + " nacelle")

        # The spike: a cone that slides aft as the speed rises, keeping the shock on the lip.
        g = Geo()
        tip = V((ENGINE_X * s, SPIKE_TIP, ENGINE_Z))
        lathe(g, tip, V((0, -1, 0)), [(0.0, 0.0), (0.09, 0.32), (0.24, 0.95), (0.37, 1.55), (0.42, 1.95), (0.38, 2.40), (0.30, 2.95)], "skin", n=24, caps=(False, True))
        g.finish(side + " spike", smooth=40, pivot=tip, axis=V((0, -1, 0)))


def nozzles():
    kr, ka = NOZZLE_K
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        c = V((ENGINE_X * s, NOZZLE_Y0, ENGINE_Z))
        aft = V((0, -1, 0))
        sc = lambda prof: [(r * kr, a * ka) for r, a in prof]
        # The ejector: the nacelle carries on as a fixed shroud, with a ring of
        # blow-in doors round it (open, on the ground and at low speed, to
        # feed the ejector), and ends in a ring of short free-floating flaps
        # that the stream holds half shut until it goes supersonic.
        lathe(g, c, aft, [(0.0, 0.0), (0.872, 0.0), (0.884, 0.30), (0.880, 0.62), (0.866, EJECTOR_FLAPS), (0.846, EJECTOR_FLAPS)], "exhaust", n=32, caps=(True, False))
        lathe(g, c, aft, [(0.846, EJECTOR_FLAPS - 0.05), (0.835, EJECTOR_FLAPS + 0.05), (0.775, 1.16), (0.676, NOZZLE_Y0 - NOZZLE_Y), (0.626, NOZZLE_Y0 - NOZZLE_Y)], "exhaust", n=32, caps=(False, False), zig=[0.0, 0.035, 0.045, 0.05, 0.05])
        for k in range(BLOW_IN[0]):
            a = 2 * math.pi * (k + 0.5) / BLOW_IN[0]
            d = V((math.cos(a), 0, math.sin(a)))
            t = d.cross(aft)
            mid = (BLOW_IN[1] + BLOW_IN[2]) / 2
            # (a dark opening, its door hinged in at the front edge)
            g.box(c + aft * mid + d * 0.884, (0.004, (BLOW_IN[2] - BLOW_IN[1]) / 2, BLOW_IN[3] / 2), "inlet", (d, aft, t))
            g.box(c + aft * (BLOW_IN[1] + 0.012) + d * 0.886, (0.006, 0.012, BLOW_IN[3] / 2 + 0.012), "exhaust", (d, aft, t))
        # Inside: the flaps, the afterburner's nozzle, its liner.
        lathe(g, c, aft, sc([(0.338, 1.0), (0.30, 0.80), (0.335, 0.60), (0.385, 0.44)]), "nozzle_in", n=30, caps=(False, False), zig=[0.03, 0.03, 0.015, 0.0])
        # The flame holder's gutters and the turbine's exit cone behind them.
        lathe(g, c, aft, sc([(0.385, 0.44), (0.385, 0.42), (0.31, 0.42), (0.30, 0.47), (0.27, 0.47), (0.26, 0.42), (0.19, 0.42), (0.18, 0.46), (0.15, 0.46), (0.14, 0.42), (0.11, 0.42), (0.085, 0.50), (0.0, 0.66)]), "burner", n=30, caps=(False, False))
        for k in range(8):
            a = 2 * math.pi * (k + 0.5) / 8
            d = V((math.cos(a), 0, math.sin(a)))
            g.box(c + aft * (0.445 * ka) + d * (0.25 * kr), (0.135 * kr, 0.02 * ka, 0.012 * kr), "burner", (d, aft, d.cross(aft)))
        g.finish(side + " nozzle", smooth=20)


def plume():
    """The exhaust, drawn in the game's transparent pass: shells and discs whose
    brightness plane.wgsl works out from the engines (material 17, `plume`).
    Haze first, then the flames, their cores and the discs: they are drawn in
    this order. The Hornet's, wider by the nozzles' size and two and a half
    times as long: a J58's flame is a long one, full of shock diamonds."""
    g = Geo()
    aft = V((0, -1, 0))
    kr, kl = NOZZLE_K[0], 2.5

    def shell(prof, m, n, sub):
        for s in (1, -1):
            rows = spline_rows(prof, 0, len(prof) - 1, sub)
            lathe(g, V((ENGINE_X * s, NOZZLE_Y, ENGINE_Z)), aft, [(max(r, 0.0) * kr, a * kl) for a, r in rows], m, n=n, caps=(False, False))

    # distance aft of the nozzle exit, radius (the Hornet's, before scaling)
    shell([(-0.05, 0.36), (1.0, 0.52), (3.0, 0.66), (6.0, 0.74), (9.0, 0.64), (11.0, 0.34), (12.0, 0.0)], "haze", 16, 2)
    shell([(-0.14, 0.325), (0.4, 0.37), (1.2, 0.36), (2.2, 0.32), (3.4, 0.26), (4.8, 0.18), (6.0, 0.09), (6.9, 0.0)], "flame", 20, 3)
    shell([(-0.14, 0.26), (0.5, 0.25), (1.3, 0.21), (2.3, 0.155), (3.4, 0.09), (4.5, 0.0)], "core", 20, 3)
    # Discs across the jet: what is seen of the flame looking up it from behind, where the shells are edge on.
    for s in (1, -1):
        for k in range(12):
            a = 0.12 + 5.2 * (k / 11) ** 1.3
            lathe(g, V((ENGINE_X * s, NOZZLE_Y, ENGINE_Z)), aft, [(0.0, a * kl), (0.36 * (1.0 - 0.75 * a / 5.6) * kr, a * kl)], "disc", n=20, caps=(False, False))
    ob = g.finish("afterburner", smooth=80, recalc=False)
    ob.hide_render = True
    ob.display_type = "WIRE"


# The drag chute, streamed: a ribbon canopy CHUTE_D across on its lines, from
# the bay on top of the rear fuselage. It rides high, clear of the runway.
# (The game draws this piece only while the chute is out.)
CHUTE_BAY = -11.6
CHUTE_D, CHUTE_DEPTH = 12.0, 3.4
CHUTE_AT = V((0, -30.2, 4.45))   # the middle of the canopy's mouth
CHUTE_LINES = 13.5


def chute():
    g = Geo()
    a = V((0, CHUTE_BAY, fuse_top(CHUTE_BAY) - 0.02))
    axis = (CHUTE_AT - a).normalized()
    u, w = frame_at(axis)
    R = CHUTE_D / 2
    # The ribbons: rings round the canopy with slots between them, a vent at the top.
    n = 11
    for k in range(n):
        f0, f1 = math.radians(lerp(90.0, 11.0, k / n)), math.radians(lerp(90.0, 11.0, (k + 0.72) / n))
        prof = [(R * math.sin(f), CHUTE_DEPTH * math.cos(f)) for f in (f0, (f0 + f1) / 2, f1)]
        prof += [(r - 0.03, d - 0.012) for r, d in reversed(prof)] + prof[:1]
        lathe(g, CHUTE_AT, axis, prof, "chute_orange" if k % 2 else "chute_white", n=36, caps=(False, False))
    # The lines from the mouth in to where they meet, and the riser from there to the bay.
    meet = CHUTE_AT - axis * math.sqrt(CHUTE_LINES ** 2 - R ** 2)
    for k in range(18):
        t = 2 * math.pi * (k + 0.5) / 18
        tube(g, [meet, CHUTE_AT + (u * math.cos(t) + w * math.sin(t)) * (R - 0.02)], 0.014, "chute_white", n=3)
    tube(g, [a, meet], 0.035, "chute_white", n=6)
    ob = g.finish("drag chute", smooth=60, pivot=a, axis=V((0, -1, 0)))
    ob.hide_render = True


# ---------------------------------------------------------------- canopies
# (Everything inside the cockpits is built in Rust: src/aircraft/sr71/pit.rs.)
# station y, half width at the sill, height of the top: the pilot's canopy
# (its windscreen a V of two flat panes), the frame between the two at
# CANOPY_SPLIT, and the reconnaissance systems officer's, which has glass only
# in its sides.
CANOPY = [(12.35, 0.20, 0.60), (12.00, 0.32, 0.86), (11.60, 0.40, 1.06), (11.20, 0.43, 1.14), (10.70, 0.44, 1.16), (10.20, 0.44, 1.16), (9.70, 0.44, 1.15), (9.20, 0.42, 1.10), (8.80, 0.34, 0.98), (8.50, 0.22, 0.86)]
CANOPY_SPLIT = (10.56, 10.34)
PANES = 6  # flat panes round the canopy: two a side, two on top


def sill_z(y, w):
    """Height of the canopy's edge: on the skin, which falls away either side of the top."""
    return fuse_z(y, w) - 0.02


def canopy_arc(y, w, zt, grow=0.0, n=PANES):
    z0 = sill_z(y, w)
    return [V(((w + grow) * math.cos(a), y, z0 + (zt + grow - z0) * math.sin(a))) for a in (math.pi * k / n for k in range(n + 1))]


def canopy():
    g = Geo()
    rows = spline_rows(CANOPY, 0, len(CANOPY) - 1, 3)
    g.strip([canopy_arc(*r) for r in rows], "glass", toward=V((0, 10.5, 30.0)))
    g.finish("canopy", smooth=25, recalc=False)

    g = Geo()
    top = lambda y: table_at(CANOPY, y)
    # The windscreen's bow and its knife-edge centre post, the frames of the
    # two canopies where they meet, and the last one.
    for yb in (11.55, CANOPY_SPLIT[0], CANOPY_SPLIT[1], 8.95):
        tube(g, canopy_arc(*top(yb), grow=0.010), 0.028, "frame", n=10, ref=V((0, 1, 0)))
    tube(g, [V((0, y, top(y)[2] + 0.012)) for y in (12.35, 12.15, 11.9, 11.7, 11.55)], 0.024, "frame", n=10)
    for a, b in ((11.55, CANOPY_SPLIT[0]), (CANOPY_SPLIT[1], 8.95)):
        tube(g, [V((0, y, top(y)[2] + 0.012)) for y in (a, (a + b) / 2, b)], 0.022, "frame", n=8)
    # Between the canopies, and over the back seat: skin, not glass.
    def roof(ya, yb, k0, k1, n=6):
        rows = []
        for i in range(n + 1):
            arc = canopy_arc(*top(lerp(ya, yb, i / n)), grow=0.012)
            rows.append(arc[k0:k1 + 1])
        g.strip(rows, "skin", toward=V((0, 10.0, 30.0)))
    roof(CANOPY_SPLIT[0], CANOPY_SPLIT[1], 0, PANES, n=1)
    roof(CANOPY_SPLIT[1], 8.95, 2, PANES - 2)
    roof(8.95, 8.50, 0, PANES, n=3)
    for s in (1, -1):
        rows = spline_rows(CANOPY, 0, len(CANOPY) - 1, 4)
        tube(g, [V((s * (w + 0.004), y, sill_z(y, w) + 0.012)) for y, w, _ in rows], 0.026, "frame", n=12)
        inner = [[V((s * (w - 0.03), y, sill_z(y, w) + 0.02)), V((s * (w - 0.03), y, sill_z(y, w) - 0.10))] for y, w, _ in spline_rows(CANOPY, 0, len(CANOPY) - 1, 2)]
        g.strip(inner, "cockpit", toward=V((0, 10.5, 0.8)))
    g.finish("canopy frame", smooth=60)


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


# Wing (kept in step with wing_sect in src/aircraft/sr71/mod.rs): a delta
# whose leading edge, swept 60 degrees, carries on from the chines. Span
# stations of its root (inside the fuselage) and tip; the leading edge on the
# centreline and its sweep (m back per m out); the trailing edge: straight
# across out to W_KINK (just outboard of the nacelle), then swept forward to
# the tip; height (it is flat: no dihedral); thickness (a fraction of the
# chord, but no more than W_MAX m: the long chords inboard would make a slab of it).
W_X0, W_X1, W_LE0, W_SWEEP, W_TE0, W_TE1, W_KINK, W_Z, W_T, W_MAX = 0.7, 8.47, 4.2, 1.732, -11.6, -10.9, 4.95, -0.10, 0.035, 0.36
IN_ELEVON = (1.76, 3.22)   # between the fuselage and the nacelle
OUT_ELEVON = (5.02, 8.20)  # outboard of the nacelle
ELEVON_C = (1.25, 0.50)    # the elevons' chord: inboard, and at the outer one's tip


def wing_te(x):
    return W_TE0 if x <= W_KINK else lerp(W_TE0, W_TE1, (x - W_KINK) / (W_X1 - W_KINK))


def wing_chord(x):
    return W_LE0 - W_SWEEP * x - wing_te(x)


def wing_t(x):
    """The wing's thickness ratio at span station x."""
    return min(W_T, W_MAX / wing_chord(x))


def wing_surface(g, sect, u0, u1, a, b, nu):
    """The wing between span stations u0 and u1, from chord fraction a(u) to b(u)."""
    us = [lerp(u0, u1, k / (nu - 1)) for k in range(nu)]
    g.loft([foil_ring(sect(u), wing_t(u), a(u), b(u)) for u in us], "skin")


def tef_frac(x):
    """Chord fraction of the elevons' hinge."""
    c = ELEVON_C[0] if x <= W_KINK else lerp(ELEVON_C[0], ELEVON_C[1], (x - OUT_ELEVON[0]) / (OUT_ELEVON[1] - OUT_ELEVON[0]))
    return 1.0 - c / wing_chord(x)


def wing_sect(s):
    def f(x):
        return V((x * s, W_LE0 - W_SWEEP * x, W_Z)), wing_chord(x), V((0, -1, 0)), V((0, 0, 1))
    return f


def well_surface(g, sect, us, b, y0, y1):
    """As `wing_surface` from the leading edge to chord fraction b(u), through the
    span stations us, but with the lower skin left open between stations y0
    and y1: a gear well's mouth."""
    n1, n2, n3 = 5, 2, 8
    rings = []
    for u in us:
        le, c, cdir, up = sect(u)
        f0, f1 = (le.y - y0) / c, (le.y - y1) / c
        xs = [f0 * (1 - math.cos(math.pi * k / n1)) / 2 for k in range(n1)] + [lerp(f0, f1, k / n2) for k in range(n2)] + [lerp(f1, b(u), k / n3) for k in range(n3 + 1)]
        t = wing_t(u)
        upper = [le + cdir * (c * x) + up * max(c * naca_t(x, t), 0.004) for x in xs]
        lower = [le + cdir * (c * x) - up * max(c * naca_t(x, t), 0.004) for x in reversed(xs)]
        rings.append(upper + lower)
    k = n1 + n2 + n3 + 1  # points along each skin; the lower skin's segment from xs[j] to xs[j + 1] is segment 2k - 2 - j of the ring
    g.loft(rings, "skin", skip=lambda row, i: n1 <= 2 * k - 2 - i < n1 + n2)


def wing_half(x, y):
    """Half the wing's thickness at span station x, station y (0 off the wing)."""
    if not W_X0 <= abs(x) <= W_X1:
        return 0.0
    c = wing_chord(abs(x))
    xc = (W_LE0 - W_SWEEP * abs(x) - y) / c
    return max(c * naca_t(xc, wing_t(abs(x))), 0.004) if 0.0 <= xc <= 1.0 else 0.0


def wick(g, p, aft=V((0, -1, 0)), up=V((0, 0, 1))):
    """A static discharger trailing from point p: a fine rod on a little base."""
    g.box(p + aft * 0.02, (0.006, 0.025, 0.004), "skin", (up.cross(aft).normalized(), aft, up))
    tube(g, [p + aft * 0.03, p + aft * 0.14], 0.0035, "steel", n=5)


def pair_axis(right_axis, s):
    """The axis a mirrored piece turns about so equal angles move both sides alike (as Builder::pair)."""
    return right_axis if s > 0 else -M(right_axis, -1)


def wings():
    zero = lambda x: 0.0
    one = lambda x: 1.0
    for s, side in ((1, "right"), (-1, "left")):
        ws = wing_sect(s)
        g = Geo()
        # Over the main gear's well the lower skin is left open, and the well
        # is given its roof and its end walls.
        xi, xo, y0, y1 = MAIN_WELL
        zr = MAIN_WELL_ROOF[1]
        wing_surface(g, ws, W_X0, MAIN_WELL_IN, zero, one, 2)
        well_surface(g, ws, [MAIN_WELL_IN, 1.27, IN_ELEVON[0] - 0.001, IN_ELEVON[0] + 0.001, 2.19, xo], lambda x: 1.0 if x < IN_ELEVON[0] else tef_frac(x), y0, y1)
        wing_surface(g, ws, xo, IN_ELEVON[1], zero, tef_frac, 3)
        inside = V((1.7 * s, (y0 + y1) / 2, -0.4))
        g.poly([V((MAIN_WELL_IN * s, y0, zr)), V((xo * s, y0, zr)), V((xo * s, y1, zr)), V((MAIN_WELL_IN * s, y1, zr))], "well", toward=inside)
        for y in (y0, y1):
            g.poly([V((x * s, y, W_Z - wing_half(x, y) if k == 0 else zr)) for k, x in ((0, MAIN_WELL_IN), (0, 1.7), (0, xo), (1, xo), (1, MAIN_WELL_IN))], "well", toward=inside)
        wing_surface(g, ws, IN_ELEVON[1], OUT_ELEVON[0], zero, one, 4)
        wing_surface(g, ws, OUT_ELEVON[0], OUT_ELEVON[1], zero, tef_frac, 9)
        # (the tip rounded off in plan)
        tip = lambda x: 1.0 - math.sqrt(max(0.0, 1.0 - ((x - OUT_ELEVON[1]) / (W_X1 - OUT_ELEVON[1])) ** 2))
        wing_surface(g, ws, OUT_ELEVON[1], W_X1, lambda x: 0.36 * tip(x), lambda x: 1.0 - 0.46 * tip(x), 7)
        g.finish(side + " wing")

        for name, (x0, x1) in (("inner elevon", IN_ELEVON), ("outer elevon", OUT_ELEVON)):
            g = Geo()
            wing_surface(g, ws, x0, x1, tef_frac, one, 8)
            if name == "outer elevon":
                for xw in (6.2, 7.6):
                    wick(g, hinge(ws, xw, 1.0))
            p0, p1 = hinge(wing_sect(1), x0, tef_frac(x0)), hinge(wing_sect(1), x1, tef_frac(x1))
            g.finish(side + " " + name, pivot=M(p0, s), axis=pair_axis((p1 - p0).normalized(), s))

        g = Geo()
        le, c, cdir, up = ws(W_X1 - 0.03)
        g.box(le + cdir * (c * 0.25), (0.035, 0.05, 0.02), "nav_green" if s > 0 else "nav_red")
        g.finish(side + " nav light", smooth=10)


# The fins: one on each nacelle, leaning in 15 degrees. All of a fin above
# FIN_SPLIT (a fraction of its height) turns, about the line up it at FIN_HINGE
# of its chord; below is a stub fixed to the nacelle.
FIN_CANT = math.radians(15.0)
FIN_BASE = V((ENGINE_X, 0.0, 0.66))
FIN_LEN = 2.775
FIN_LE0, FIN_SWEEP, FIN_C0, FIN_C1, FIN_T = -6.40, 2.80, 5.00, 2.20, 0.035
FIN_SPLIT, FIN_HINGE = 0.16, 0.45


def fin_sect(s):
    ca, sa = math.cos(FIN_CANT), math.sin(FIN_CANT)
    def f(u):
        p = FIN_BASE + V((-sa, 0, ca)) * (FIN_LEN * u)
        le = V((p.x * s, FIN_LE0 - FIN_SWEEP * u, p.z))
        return le, lerp(FIN_C0, FIN_C1, u), V((0, -1, 0)), V((ca * s, 0, sa))
    return f


def inner_faces(g, s, first=0):
    """Give a fin's inboard face (of the faces from `first` on) the plain finish.
    The paint is projected from each side, so a fin's inner face would show
    what the other fin has on its outer one: its serial."""
    out = V((math.cos(FIN_CANT) * s, 0, math.sin(FIN_CANT)))
    base = V((FIN_BASE.x * s, 0, FIN_BASE.z))
    g.bm.faces.ensure_lookup_table()
    for f in list(g.bm.faces)[first:]:
        if (f.calc_center_median() - base).dot(out) < -1e-4:
            f.material_index = g.mi("skin_in")


def fins():
    ca, sa = math.cos(FIN_CANT), math.sin(FIN_CANT)
    for s, side in ((1, "right"), (-1, "left")):
        fs = fin_sect(s)
        g = Geo()
        surface(g, fs, FIN_SPLIT + 0.006, 1.0, 0.0, 1.0, FIN_T, nu=9)
        inner_faces(g, s)
        up = V((-sa * s, 0, ca))
        le, c, cdir, out = fs(1.0)
        wick(g, le + cdir * c - up * 0.05, up=up)
        # Both fins swing the same way for the same command: no pair flip here.
        g.finish(side + " fin", pivot=hinge(fs, FIN_SPLIT, FIN_HINGE), axis=up)


# ---------------------------------------------------------------- landing gear (modelled down and locked)
# The nose leg swings forward into its well behind the cockpits. Each main leg
# hangs from the wing between the fuselage and the nacelle and swings inboard,
# its three wheels ending up standing one above another inside the fuselage.
# The door under the wing rides on the leg; the fuselage's doors open first
# and shut last (see `anim` in Rust).
GROUND = -2.30
NOSE_PIVOT = V((0, 7.60, -0.45))
NOSE_AXLE = V((0, 7.45, -2.06))
NOSE_R = 0.24
NOSE_STOWED_Z = -0.40  # height of the nose axle with the gear up
MAIN_Y = -4.20
MAIN_PIVOT = V((2.34, MAIN_Y, -0.02))
MAIN_KNEE = V((2.55, MAIN_Y, -1.52))  # bottom of the strut, over the wheels
MAIN_AXLE = V((2.55, MAIN_Y, -1.95))
MAIN_R = 0.35
MAIN_HW = 0.09      # half a main tyre's width
MAIN_PITCH = 0.26   # from one main wheel to the next along the axle
MAIN_STOW_AT = V((0.40, MAIN_Y, -0.13))  # where the middle wheel stows


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
    """How a (right) main leg stows: the axis and angle of the one swing that takes its axle to MAIN_STOW_AT."""
    d = MAIN_AXLE - MAIN_PIVOT
    return stow(MAIN_PIVOT, MAIN_AXLE, MAIN_PIVOT + (MAIN_STOW_AT - MAIN_PIVOT).normalized() * d.length)


def gear():
    # Nose: swings forward into the well behind the cockpits.
    l = (NOSE_AXLE - NOSE_PIVOT).length
    dz = NOSE_STOWED_Z - NOSE_PIVOT.z
    ax, ang = stow(NOSE_PIVOT, NOSE_AXLE, NOSE_PIVOT + V((0, math.sqrt(l * l - dz * dz), dz)))
    along = (NOSE_PIVOT - NOSE_AXLE).normalized()
    low = NOSE_AXLE + along * 0.40
    g = Geo()
    # The trunnion across the top, the strut, the steering collar.
    tube(g, [NOSE_PIVOT + V((-0.17, 0, 0)), NOSE_PIVOT + V((0.17, 0, 0))], 0.04, "strut", n=10)
    tube(g, [NOSE_PIVOT, low], 0.058, "strut", n=14)
    lathe(g, NOSE_PIVOT - along * 0.60, -along, [(0.0, 0.0), (0.08, 0.0), (0.08, 0.12), (0.0, 0.12)], "strut", n=14)
    fwd = (V((0, 1, 0)) - along * along.y).normalized()
    # Landing and taxi lights on a bracket ahead of the strut.
    g.box(NOSE_PIVOT - along * 0.78 + fwd * 0.095, (0.055, 0.03, 0.04), "strut", (V((1, 0, 0)), fwd, along))
    g.box(NOSE_PIVOT - along * 0.78 + fwd * 0.127, (0.045, 0.004, 0.032), "landing", (V((1, 0, 0)), fwd, along))
    # Hydraulic lines down the back of the strut.
    for x in (-0.03, 0.03):
        tube(g, [NOSE_PIVOT + V((x, -0.065, 0)) - along * 0.05, low + V((x, -0.065, 0))], 0.008, "steel", n=5)
    g.finish("nose leg", pivot=NOSE_PIVOT, axis=ax, angle=ang)

    down = (NOSE_AXLE - NOSE_PIVOT).normalized()
    g = Geo()
    tube(g, [low + along * 0.10, NOSE_AXLE], 0.045, "chrome", n=12)
    g.box(NOSE_AXLE + along * 0.04, (0.06, 0.05, 0.05), "strut")
    tube(g, [NOSE_AXLE + V((-0.20, 0, 0)), NOSE_AXLE + V((0.20, 0, 0))], 0.028, "strut")
    scissors(g, low + V((0, -0.055, 0)), NOSE_AXLE + along * 0.06 + V((0, -0.05, 0)), V((0, -0.12, 0)))
    g.finish("nose fork", pivot=NOSE_AXLE, axis=down, parent="nose leg")

    g = Geo()
    for x in (-0.125, 0.125):
        wheel(g, NOSE_AXLE + V((x, 0, 0)), NOSE_R, 0.065)
    g.finish("nose wheel", pivot=NOSE_AXLE, axis=V((1, 0, 0)), parent="nose fork")

    # Mains.
    ax, sw = main_stow()
    up_rot = Matrix.Translation(MAIN_PIVOT) @ Matrix.Rotation(-sw, 4, ax) @ Matrix.Translation(-MAIN_PIVOT)  # from stowed to down
    s_dir = (MAIN_KNEE - MAIN_PIVOT).normalized()
    mid = MAIN_PIVOT + (MAIN_KNEE - MAIN_PIVOT) * 0.50
    arm = MAIN_PITCH / 2
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        # The trunnion, the strut and its oleo, the yoke whose two arms come
        # down between the wheels to the axle, the brakes.
        tube(g, [M(MAIN_PIVOT + V((0, 0.15, 0)), s), M(MAIN_PIVOT + V((0, -0.15, 0)), s)], 0.04, "strut", n=10)
        tube(g, [M(MAIN_PIVOT, s), M(mid, s)], 0.06, "strut", n=14)
        lathe(g, M(mid, s), M(-s_dir, s), [(0.0, 0.0), (0.072, 0.0), (0.072, 0.07), (0.0, 0.07)], "strut", n=14)
        tube(g, [M(mid - s_dir * 0.05, s), M(MAIN_KNEE, s)], 0.045, "chrome", n=12)
        tube(g, [M(MAIN_KNEE + V((-arm, 0, 0)), s), M(MAIN_KNEE + V((arm, 0, 0)), s)], 0.03, "strut", n=10)
        for x in (-arm, arm):
            tube(g, [M(MAIN_KNEE + V((x, 0, 0)), s), M(MAIN_AXLE + V((x, 0, 0)), s)], 0.026, "strut", n=8)
        tube(g, [M(MAIN_AXLE + V((-MAIN_PITCH - 0.02, 0, 0)), s), M(MAIN_AXLE + V((MAIN_PITCH + 0.02, 0, 0)), s)], 0.028, "steel", n=10)
        scissors(g, M(mid + s_dir * 0.08 + V((0, 0.055, 0)), s), M(MAIN_KNEE + V((0, 0.05, 0.02)), s), M(V((0.0, 0.11, 0)), s))
        for x in (-0.025, 0.025):
            tube(g, [M(MAIN_PIVOT + V((x, -0.062, -0.05)), s), M(mid + V((x, -0.068, 0)), s)], 0.008, "steel", n=5)
        g.finish(side + " main leg", pivot=M(MAIN_PIVOT, s), axis=pair_axis(ax, s), angle=sw)

        g = Geo()
        for k in (-1, 0, 1):
            wheel(g, M(MAIN_AXLE + V((k * MAIN_PITCH, 0, 0)), s), MAIN_R, MAIN_HW)
        g.finish(side + " main wheel", pivot=M(MAIN_AXLE, s), axis=V((1, 0, 0)), parent=side + " main leg")

        # The door under the wing, on the leg: built where it lies shut, flush
        # with the wing's lower skin, then swung down with the leg.
        g = Geo()
        xi, xo, y0, y1 = MAIN_WELL
        gap = 0.006
        xs = [lerp(MAIN_WELL_IN + 0.02, xo - gap, k / 6) for k in range(7)]
        ys = [lerp(y0 - gap, y1 + gap, k / 4) for k in range(5)]
        for dz, m, t in ((-0.003, "skin", -9.0), (0.011, "door_in", 9.0)):
            rows = [[up_rot @ V((x, y, W_Z - wing_half(x, y) + dz)) for x in xs] for y in ys]
            g.strip([[M(p, s) for p in r] for r in rows], m, toward=M(up_rot @ V((1.7, MAIN_Y, W_Z + t)), s))
        g.finish(side + " strut door", smooth=30, pivot=M(MAIN_PIVOT, s), axis=pair_axis(ax, s), recalc=False, parent=side + " main leg")
    r = Matrix.Rotation(sw, 3, ax)
    a1 = MAIN_PIVOT + r @ (MAIN_AXLE - MAIN_PIVOT)
    print(f"main gear: swing {math.degrees(sw):.0f} deg about {tuple(round(c, 2) for c in ax)}, middle wheel stowed at {tuple(round(c, 3) for c in a1)}, axle {tuple(round(c, 2) for c in r @ V((1, 0, 0)))}")


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


def doors():
    hw, ny0, ny1, _ = NOSE_WELL
    xi, xo, my0, my1 = MAIN_WELL
    gap = 0.005
    for s, side in ((1, "right"), (-1, "left")):
        # Nose: a pair meeting on the centreline, hinged at the well's sides.
        skin_door(side + " nose door", s, gap, hw - gap, ny1 - gap, ny0 + gap, hw, math.radians(86))
        # Mains: the fuselage's door, hinged beside the centreline, which shuts
        # again once the leg is down (Rust `anim`).
        skin_door(side + " main door", s, xi + gap, MAIN_WELL_IN - gap, my0 - gap, my1 + gap, xi + gap, math.radians(84), nx=12, ny=6)


def check_gear():
    """Sweep the gear from up to down (as Rust `anim` moves it: the legs over
    0.15..0.85) and report any gear vertex inside the skin (the fuselage's or
    the wing's) but outside the wells, or, stowed, not tucked in behind the
    doors. Run on every export: a hand edit that makes the gear poke through
    the skin shows up here."""
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
        """How far inside the skin p is (negative: outside)."""
        d = -1.0
        if FUSE[-1][0] < p.y < FUSE[0][0]:
            d = min(p.z - fuse_z(p.y, p.x, False), fuse_z(p.y, p.x, True) - p.z, fuse_hw(p.y, p.z) - abs(p.x))
        h = wing_half(p.x, p.y)
        return max(d, h - abs(p.z - W_Z)) if h > 0.0 else d

    def in_well(p):
        hw, y0, y1, zt = NOSE_WELL
        if abs(p.x) < hw and y0 < p.y < y1 and p.z < zt:
            return True
        xi, xo, y0, y1 = MAIN_WELL
        return xi < abs(p.x) < xo and y1 < p.y < y0 and p.z < MAIN_WELL_ROOF[0 if abs(p.x) < MAIN_WELL_IN else 1] + 0.005

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
                if d > 0.0 and not in_well(p):
                    e = 1.0  # inside the skin, out of its well
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
# Every piece the game looks for (PIECES in src/aircraft/sr71/mod.rs); other objects are ignored.
EXPECTED = ["radome", "nose section", "fuselage", "tail cone", "canopy", "canopy frame", "tail light", "nose leg", "nose fork", "nose wheel", "afterburner", "drag chute"] + [
    f"{s} {n}" for s in ("left", "right") for n in ("nacelle", "spike", "nozzle", "wing", "inner elevon", "outer elevon", "nav light", "fin", "main leg", "main wheel", "strut door", "nose door", "main door")]


def game_mat(m):
    """Which game material a Blender material is: its "game" property, its sr71_ name, or else the skin."""
    if m is None:
        return "skin"
    name = m.get("game") or (m.name[5:].split(".")[0] if m.name.startswith("sr71_") else None)
    return name if name in MATS else "skin"


def export(path):
    out = bytearray(b"FFAC")
    obs = [o for o in COLL.all_objects if o.type == "MESH"]
    names = {o.name for o in obs}
    missing = [n for n in EXPECTED if n not in names]
    if missing:
        raise SystemExit(f"sr71 export: the game needs these pieces, missing from the '{COLL.name}' collection: {', '.join(missing)}")
    deps = bpy.context.evaluated_depsgraph_get()
    out += struct.pack("<II", 1, len(obs))
    total = 0
    lo, hi = V((1e9, 1e9, 1e9)), V((-1e9, -1e9, -1e9))
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
                        if CANOPY[-1][0] < p.y < CANOPY[0][0] and p.z > 0.3 and abs(p.x) < table_at(CANOPY, p.y)[1] - 0.021:
                            c = COCKPIT_TUB  # under the canopy
                    if ob.name not in ("afterburner", "drag chute"):
                        for j in range(3):
                            lo[j], hi[j] = min(lo[j], p[j]), max(hi[j], p[j])
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
    print(f"bounds: length {hi.y - lo.y:.2f} m (y {lo.y:.2f} .. {hi.y:.2f}), span {hi.x - lo.x:.2f} m, height {hi.z - lo.z:.2f} m (z {lo.z:.2f} .. {hi.z:.2f})")


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
    target = V((0, 1.0, 0.3))
    views = (("three_quarter", V((26, 30, 13)), 50, target), ("side", V((52, 1.0, 0.6)), 50, target), ("top", V((0.01, 1.3, 86)), 50, target), ("front", V((0.01, 44, 1.5)), 70, V((0, 0, 0.5))), ("rear_low", V((-18, -30, -3.0)), 50, target),
             ("rear", V((7.5, -28, 5.0)), 60, V((0, -9, 0.4))), ("nozzles", V((7.0, -19.0, 1.4)), 70, V((3.0, -11.5, 0.0))), ("cockpit", V((3.4, 14.4, 2.6)), 75, V((0, 10.6, 0.7))), ("under", V((13, 16, -9)), 50, V((0, 0.5, -0.3))),
             ("inlet", V((7.5, 9.5, 1.2)), 70, V((4.0, 3.6, -0.1))), ("gear", V((9.0, 2.0, -1.6)), 60, V((1.5, -4.0, -1.2))))
    for name, eye, lens, look in views:
        cam.location = eye
        cam.rotation_euler = (look - eye).to_track_quat("-Z", "Y").to_euler()
        cam_data.lens = lens
        sc.render.filepath = os.path.join(outdir, name + ".png")
        bpy.ops.render.render(write_still=True)
    # And with the drag chute out.
    ch = bpy.data.objects["drag chute"]
    ch.hide_render = False
    eye, look = V((30, -44, 9)), V((0, -18, 2.5))
    cam.location = eye
    cam.rotation_euler = (look - eye).to_track_quat("-Z", "Y").to_euler()
    cam_data.lens = 50
    sc.render.filepath = os.path.join(outdir, "chute.png")
    bpy.ops.render.render(write_still=True)
    ch.hide_render = True


# The paint is projected straight down, straight up and from each side: these
# are the rectangles (m) those four pictures cover (the Blackbird is far longer
# than the Hornet, so they are larger than fa18.py's). plane.wgsl and the paint
# script use the same numbers.
GUIDE_SPAN = 24.0   # across the top and bottom views; up the side views
GUIDE_LEN = 36.0    # fore and aft in all four
GUIDE_Y = 1.63      # station at the middle of the pictures
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
    hidden = [o for o in COLL.all_objects if any(k in o.name for k in ("leg", "wheel", "fork", "door", "afterburner", "chute"))]
    for o in hidden:
        o.hide_render = True
    cam, cam_data = camera()
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = GUIDE_LEN
    short = round(GUIDE_PX * GUIDE_SPAN / GUIDE_LEN)
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
        o.hide_render = o.name in ("afterburner", "drag chute")


def geometry_pass(outdir, views):
    """What each pixel of a guide is a picture of, for assets/blender/sr71_paint.py
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
        COLL = bpy.data.collections.get("SR-71A")
        if COLL is None:
            raise SystemExit("sr71 export: no 'SR-71A' collection in this file (open assets/blender/sr71.blend)")
        export(os.path.join(ASSETS, "sr71.bin.gz"))
        check_gear()
    else:
        for ob in list(bpy.data.objects):
            bpy.data.objects.remove(ob, do_unlink=True)
        for c in list(bpy.data.collections):
            bpy.data.collections.remove(c)
        COLL = bpy.data.collections.new("SR-71A")
        bpy.context.scene.collection.children.link(COLL)
        CUTS = bpy.data.collections.new("SR-71A well cutters")
        bpy.context.scene.collection.children.link(CUTS)
        make_materials()
        fuselage()
        nacelles()
        nozzles()
        canopy()
        wings()
        fins()
        gear()
        doors()
        plume()
        chute()
        os.makedirs(os.path.join(ASSETS, "blender"), exist_ok=True)
        bpy.context.preferences.filepaths.save_version = 0  # no sr71.blend1 beside it
        export(os.path.join(ASSETS, "sr71.bin.gz"))
        check_gear()
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ASSETS, "blender", "sr71.blend"))
    if RENDER:
        render_previews(RENDER)
    if GUIDES:
        render_guides(GUIDES)


main()
