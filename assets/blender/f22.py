# The F-22A Raptor for FableFlight, built procedurally in Blender.
#
#   blender -b --factory-startup --python assets/blender/f22.py -- <repo>/assets [--render <dir>] [--guides <dir>]
#
# Builds the airframe from lofted sections and NACA-section lifting surfaces,
# one object per piece that moves or breaks off on its own, saves it as
# assets/blender/f22.blend and exports assets/f22.bin.gz, which
# src/aircraft/f22 loads. That rebuild starts from scratch, so it throws
# away anything edited by hand in the .blend.
#
# To keep hand edits, export the .blend as it stands instead (save it first):
#
#   blender -b assets/blender/f22.blend --python assets/blender/f22.py -- <repo>/assets --export-only
#
# Edit freely: reshape meshes, add modifiers (they are applied on export),
# move or turn pieces (each carries its hinge along), assign the f22_
# materials. Keep the piece names and the "F-22A" collection: the game finds
# pieces by name (see EXPECTED below); objects with other names are ignored.
# How each piece moves is set in Rust by its name; each object records where
# its hinge is in its own frame (custom properties "pivot", "axis", "angle").
#
# The toolkit (Geo and what follows it) is the Hornet's (fa18.py). What is the
# Raptor's own: a fuselage of faceted sections (a chine each side, flat facets
# above and below it) instead of superellipses, caret intakes under the chine,
# the clipped-diamond wing and tails, fins canted out, and flat
# thrust-vectoring nozzles.
#
# The gear wells are cut into the fuselage with boolean modifiers (cutters in
# the "F-22A well cutters" collection, not exported) and the gear doors are
# patches of the skin itself. Every export ends with a gear check: a sweep of
# the legs from up to down that names any part passing through the skin.
#
# --render writes preview pictures. --guides writes the four orthographic
# views (top, bottom, left side, right side) the paint is projected from, each
# with its silhouette mask.
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
# from the aircraft's pictures; the countershaded vertex colour shows only
# where no picture faces the surface.
# Material 11 (exhaust) extra: 0 the nozzle flaps outside, 1 inside the
# nozzle, 2 the flame holder; material 17 (the exhaust plume, drawn in the
# transparent pass) extra: 0 heat haze, 1 the flame, 2 its core, 3 discs across it.
MATS = {
    "skin": (15, 17, (0.20, 0.22, 0.24), 0),  # two greys, countershaded on export
    "radome": (15, 17, (0.18, 0.20, 0.22), 0),
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
SKIN_LIGHT = (0.22, 0.24, 0.26)  # underneath
SKIN_DARK = (0.15, 0.165, 0.18)  # on top
COCKPIT_TUB = (0.03, 0.03, 0.034)

MAT = {}


def make_materials():
    for name, (gid, _pat, col, _x) in MATS.items():
        m = bpy.data.materials.get("f22_" + name) or bpy.data.materials.new("f22_" + name)
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
# station y, half width (at the chine), top, height of the chine, bottom, half
# width of the flat on top, of the flat underneath, how far the upper and the
# lower facets bulge (1: flat). The first five columns are kept in step with
# FUSE in src/aircraft/f22/mod.rs.
FUSE = [
    (9.90, 0.00, -0.10, -0.10, -0.10, 0.00, 0.00, 1.6, 1.5),
    (9.60, 0.15, 0.02, -0.10, -0.23, 0.02, 0.03, 1.6, 1.5),
    (9.10, 0.32, 0.16, -0.10, -0.39, 0.04, 0.06, 1.6, 1.5),
    (8.40, 0.50, 0.31, -0.09, -0.55, 0.07, 0.10, 1.6, 1.5),
    (7.60, 0.68, 0.45, -0.07, -0.69, 0.12, 0.16, 1.6, 1.5),
    (6.80, 0.82, 0.55, -0.04, -0.79, 0.20, 0.22, 1.6, 1.5),
    (5.90, 0.94, 0.62, 0.00, -0.86, 0.28, 0.30, 1.6, 1.5),
    (5.00, 1.03, 0.66, 0.05, -0.92, 0.34, 0.40, 1.5, 1.4),
    (4.20, 1.12, 0.67, 0.09, -0.95, 0.40, 0.55, 1.5, 1.4),
    (3.20, 1.50, 0.66, 0.14, -0.98, 0.55, 0.95, 1.4, 1.3),
    (2.00, 1.86, 0.64, 0.18, -1.00, 0.75, 1.40, 1.3, 1.2),
    (0.50, 1.92, 0.62, 0.18, -1.00, 0.88, 1.45, 1.3, 1.2),
    (-1.50, 1.92, 0.58, 0.16, -0.98, 0.92, 1.45, 1.3, 1.2),
    (-3.50, 1.92, 0.52, 0.12, -0.92, 0.95, 1.42, 1.3, 1.2),
    (-5.00, 1.94, 0.44, 0.08, -0.80, 1.00, 1.40, 1.3, 1.2),
    (-6.00, 1.96, 0.36, 0.04, -0.60, 1.36, 1.36, 1.3, 1.2),
]
# A ring: NU segments from the chine over the top to the centreline (NA of
# them up the facet, the rest across the flat), NL likewise underneath.
NU, NL, NA, NB = 12, 10, 7, 5
# Round the cockpit opening the ring is laid out so that vertex TUB_K (and its
# mirror image) lies exactly on the opening's edge.
TUB_K = 5
ENGINE_X = 0.78  # the engines' centrelines
ENGINE_Z = -0.12


def trough(y):
    """How deep the valley between the two engines is at station y: on top, underneath."""
    return 0.10 * smoothstep(-3.5, -5.5, y), 0.12 * smoothstep(-2.5, -5.5, y)


def tub_cut(y):
    """Half width of the cockpit opening at station y."""
    return table_at(CANOPY, y)[1] - 0.03


def section_z(row, x, upper=True):
    """Height of a section's skin x out from the centreline: flat across the
    middle, then a facet down (or up) to the chine."""
    y, hw, top, mid, bot, wt, wb, ku, kl = row
    ax = min(abs(x), hw)
    w, e, k = (wt, top, ku) if upper else (wb, bot, kl)
    if ax <= w or hw - w < 1e-6:
        z = e
    else:
        z = mid + (e - mid) * (1.0 - (1.0 - (hw - ax) / (hw - w)) ** k)
    t_top, t_bot = trough(y)
    between = math.exp(-(x / 0.25) ** 2)
    return z - t_top * between if upper else z + t_bot * between


def fuse_ring(row, tub=0.0):
    y, hw, wt, wb = row[0], max(row[1], 0.0), row[5], row[6]
    xs = [lerp(hw, wt, i / NA) if i <= NA else wt * (1 - (i - NA) / (NU - NA)) for i in range(NU + 1)]
    if tub > 0.0 and hw > 1e-6:
        cut = min(tub_cut(y), hw * 0.95)
        for i in range(NU + 1):
            x = lerp(hw, cut, i / TUB_K) if i <= TUB_K else cut * (1 - (i - TUB_K) / (NU - TUB_K))
            xs[i] = lerp(xs[i], x, tub)
    pts = [V((x, y, section_z(row, x))) for x in xs]
    pts += [V((-x, y, section_z(row, x))) for x in reversed(xs[:-1])]
    xl = [lerp(hw, wb, j / NB) if j <= NB else wb * (1 - (j - NB) / (NL - NB)) for j in range(1, NL + 1)]
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
    _, hw, top, mid, bot, wt, wb, ku, kl = table_at(FUSE, y)
    w, e, k = (wt, top, ku) if z >= mid else (wb, bot, kl)
    f = min(1.0, abs(z - mid) / max(abs(e - mid), 1e-6))
    return hw - (hw - w) * (1.0 - (1.0 - f) ** (1 / k))


# The cockpit: rows of the nose section the opening runs between, its floor.
TUB_Y = (6.9, 4.2)
TUB_FLOOR = 0.12
TUB_RIM = []  # per row of the opening: the ring's points from its right edge over the top to its left edge


def fuselage():
    idx = {round(r[0], 2): i for i, r in enumerate(FUSE)}
    for name, y0, y1 in (("radome", 9.90, 7.60), ("nose section", 7.60, 3.20), ("fuselage", 3.20, -5.00), ("tail cone", -5.00, -6.00)):
        g = Geo()
        rows = spline_rows(FUSE, idx[y0], idx[y1], 4)
        if name == "nose section":
            weight = lambda y: smoothstep(7.3, 6.9, y) * smoothstep(3.8, 4.2, y)
            rings = [fuse_ring(r, tub=weight(r[0])) for r in rows]
            inside = [TUB_Y[1] < r[0] < TUB_Y[0] for r in rows]
            TUB_RIM.extend(rings[i][TUB_K:2 * NU - TUB_K + 1] for i in range(len(rows)) if inside[i])
            g.loft(rings, "skin", skip=lambda row, i: inside[row] and inside[row + 1] and TUB_K <= i < 2 * NU - TUB_K)
            nose_details(g)
        else:
            g.loft([fuse_ring(r) for r in rows], "radome" if name == "radome" else "skin")
        if name == "radome":
            # The pitot probes either side of the nose.
            for s in (1, -1):
                y = 9.0
                x = fuse_hw(y, -0.16)
                tube(g, [V((s * (x - 0.02), y, -0.16)), V((s * (x + 0.06), y + 0.02, -0.18)), V((s * (x + 0.07), y + 0.22, -0.18))], lambda t: 0.012 - 0.005 * t, "steel", n=8)
        if name == "fuselage":
            belly_details(g)
        if name == "tail cone":
            booms(g)
        ob = g.finish(name)
        if name == "nose section":
            carve(ob, [nose_well()])
        if name == "fuselage":
            carve(ob, [main_well(1), main_well(-1)])


# The gear wells: the nose well under the cockpit (half width, aft end, front
# end, roof) and a main well each side (inboard edge, front, back, roof; its
# outboard edge is where the skin is MAIN_WELL_Z high).
NOSE_WELL = (0.22, 4.95, 6.80, 0.05)
MAIN_WELL = (0.80, 0.85, -1.25, 0.05)
MAIN_WELL_Z = -0.60
MAIN_DOOR_SPLIT = -0.62  # where the main door ends and the strut door starts


def nose_well():
    hw, y0, y1, zt = NOSE_WELL
    return cutter("nose well", [[V((-hw, y, -1.4)), V((hw, y, -1.4)), V((hw, y, zt)), V((-hw, y, zt))] for y in (y1, y0)])


def main_well_out(y):
    return fuse_hw(y, MAIN_WELL_Z)


def main_well_x(y, z):
    """The main well's outboard wall: straight up from the opening's edge."""
    return main_well_out(y)


def main_well(s):
    xi, y0, y1, zt = MAIN_WELL
    rings = []
    for k in range(7):
        y = lerp(y0, y1, k / 6)
        xo = main_well_out(y)
        rings.append([V((xi * s, y, -1.5)), V((xo * s, y, -1.5)), V((xo * s, y, MAIN_WELL_Z)), V((main_well_x(y, zt) * s, y, zt)), V((xi * s, y, zt))])
    return cutter(("right" if s > 0 else "left") + " main well", rings)


def nose_details(g):
    for s in (1, -1):
        # Formation light strip on the forebody's side, under the chine.
        y = 6.3
        z = -0.30
        x = fuse_hw(y, z)
        g.box(V((s * (x + 0.002), y, z)), (0.004, 0.30, 0.03), "slime")
    # The upper anti-collision light, behind the canopy.
    g.box(V((0, 3.45, fuse_top(3.45) + 0.012)), (0.03, 0.05, 0.02), "strobe")


def belly_details(g):
    # The lower anti-collision light.
    g.box(V((0, -0.2, fuse_z(-0.2, 0, upper=False) - 0.012)), (0.03, 0.05, 0.02), "strobe")


def booms(g):
    """The tail booms outboard of the nozzles, which carry the fins and the
    stabilators: they run aft of the fuselage's end to a thin edge."""
    _, hw, top, mid, bot = FUSE[-1][:5]
    for s in (1, -1):
        rings = []
        for k in range(7):
            f = k / 6
            y = lerp(-6.0, -7.6, f)
            t = 1.0 - 0.94 * smoothstep(0.0, 1.0, f)
            xo = lerp(hw, 1.82, f)
            zt, zb = mid + (top - mid) * t, mid + (bot - mid) * t
            rings.append([V((1.38 * s, y, zt)), V((1.62 * s, y, zt)), V((xo * s, y, mid + 0.02 * t)), V((xo * s, y, mid - 0.02 * t)), V((1.62 * s, y, zb)), V((1.38 * s, y, zb))])
        g.loft(rings, "skin")


def intakes():
    # The caret inlets under the chine: a parallelogram, its sides leaning in
    # toward the bottom like the fuselage's own, its lip swept back from the
    # upper inboard corner. station y, then (x, z) of the upper inboard, upper
    # outboard, lower outboard and lower inboard corners. Aft, the trunk fairs
    # into the fuselage, which has widened to take it in.
    T = [(4.90, 1.00, 0.08, 1.78, 0.14, 1.42, -0.86, 0.66, -0.80),
         (4.00, 1.00, 0.10, 1.82, 0.15, 1.43, -0.91, 0.68, -0.84),
         (3.10, 1.00, 0.13, 1.85, 0.17, 1.43, -0.95, 0.74, -0.88),
         (2.20, 1.00, 0.15, 1.86, 0.18, 1.41, -0.97, 0.84, -0.90),
         (1.40, 1.00, 0.15, 1.84, 0.17, 1.38, -0.95, 0.90, -0.90)]
    NS = 8  # points along each side

    def ring(s, row, inset=0.0):
        y = row[0]
        cs = [V((row[1 + 2 * k], 0, row[2 + 2 * k])) for k in range(4)]
        mid = (cs[0] + cs[1] + cs[2] + cs[3]) / 4
        cs = [c + (mid - c).normalized() * (inset * 1.4) for c in cs]
        rake = smoothstep(3.3, 4.9, y)
        out = []
        for k in range(4):
            a, b = cs[k], cs[(k + 1) % 4]
            for j in range(NS):
                p = a.lerp(b, j / NS)
                dy = -rake * (0.90 * max(p.x - row[1], 0.0) + 0.50 * (row[2] - p.z))
                out.append(V((p.x * s, y + dy, p.z)))
        return out

    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        rows = spline_rows(T, 0, len(T) - 1, 4)
        g.loft([ring(s, r) for r in rows], "skin", False, True)
        # The lip, and the duct running back from it into the dark.
        g.loft([ring(s, T[0]), ring(s, T[0], inset=0.025)], "skin", False, False)
        back = lambda dy: (T[0][0] - dy,) + tuple(T[0][1:])
        deep = table_at(T, 3.2)
        g.loft([ring(s, T[0], 0.025), ring(s, back(0.5), 0.03), ring(s, (3.2,) + tuple(deep[1:]), 0.10)], "inlet", False, True)
        g.finish(side + " intake", smooth=30)


def nozzles():
    """The two-dimensional nozzles: an upper and a lower flap between fixed side
    walls, each flap's edge a chevron. One piece an engine, hinged across its
    front so that it can be pitched (thrust vectoring)."""
    y0, y1, tip = -6.0, -7.3, 0.25
    hw, hh0, hh1 = 0.56, 0.43, 0.22
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        c = V((ENGINE_X * s, 0, ENGINE_Z))
        far = V((ENGINE_X * s, -6.7, ENGINE_Z))
        xs = [-hw, -hw / 2, 0.0, hw / 2, hw]
        ye = lambda x: y1 - tip * (1.0 - abs(x) / hw)
        for v in (1, -1):
            out0 = [c + V((x, y0, v * hh0)) for x in xs]
            out1 = [c + V((x, ye(x), v * hh1)) for x in xs]
            in0 = [c + V((x, y0, v * (hh0 - 0.04))) for x in xs]
            in1 = [c + V((x, ye(x), v * (hh1 - 0.015))) for x in xs]
            for k in range(4):
                g.poly([out0[k], out0[k + 1], out1[k + 1], out1[k]], "exhaust", toward=far + V((0, 0, v * 9.0)))
                g.poly([in0[k], in0[k + 1], in1[k + 1], in1[k]], "nozzle_in", toward=far)
                g.poly([out1[k], out1[k + 1], in1[k + 1], in1[k]], "exhaust", toward=far + V((0, -9.0, 0)))
        for x in (-hw, hw):
            wall = [c + V((x, y0, hh0)), c + V((x, y1, hh1)), c + V((x, y1, -hh1)), c + V((x, y0, -hh0))]
            g.plate(wall, V((math.copysign(1.0, x), 0, 0)), 0.012, "exhaust", "nozzle_in")
        # Deep inside: the duct's end, the flame holder's gutters and the turbine's exit cone.
        yb = y0 - 0.38
        hb = lerp(hh0, hh1, 0.38 / (y0 - y1))
        g.poly([c + V((-hw, yb, hb)), c + V((hw, yb, hb)), c + V((hw, yb, -hb)), c + V((-hw, yb, -hb))], "nozzle_in", toward=far)
        aft = V((0, -1, 0))
        k = 0.80
        prof = [(0.385, 0.44), (0.385, 0.42), (0.31, 0.42), (0.30, 0.47), (0.27, 0.47), (0.26, 0.42), (0.19, 0.42), (0.18, 0.46), (0.15, 0.46), (0.14, 0.42), (0.11, 0.42), (0.085, 0.50), (0.0, 0.66)]
        lathe(g, c + V((0, y0, 0)), aft, [(r * k, a) for r, a in prof], "burner", n=30, caps=(False, False))
        g.finish(side + " nozzle", smooth=20, pivot=c + V((0, y0, 0)), axis=V((1, 0, 0)))


NOZZLE_Y = -7.3  # the nozzles' exit (the chevrons' points are 0.25 m further aft)
PLUME_SQUASH = (1.30, 0.65)  # the jets leave flat nozzles: wider than they are tall


def plume():
    """The exhaust, drawn in the game's transparent pass: shells and discs whose
    brightness plane.wgsl works out from the engines (material 17, `plume`).
    Haze first, then the flames, their cores and the discs: they are drawn in
    this order."""
    g = Geo()

    def ring(s, a, r, n):
        c = V((ENGINE_X * s, NOZZLE_Y - a, ENGINE_Z))
        return [c + V((math.cos(t) * r * PLUME_SQUASH[0], 0, math.sin(t) * r * PLUME_SQUASH[1])) for t in (2 * math.pi * k / n for k in range(n))]

    def shell(prof, m, n, sub):
        for s in (1, -1):
            rows = spline_rows(prof, 0, len(prof) - 1, sub)
            g.loft([ring(s, a, max(r, 0.0), n) for a, r in rows], m, False, False)

    # distance aft of the nozzle exit, radius
    shell([(-0.05, 0.36), (1.0, 0.52), (3.0, 0.66), (6.0, 0.74), (9.0, 0.64), (11.0, 0.34), (12.0, 0.0)], "haze", 16, 2)
    shell([(-0.14, 0.325), (0.4, 0.37), (1.2, 0.36), (2.2, 0.32), (3.4, 0.26), (4.8, 0.18), (6.0, 0.09), (6.9, 0.0)], "flame", 20, 3)
    shell([(-0.14, 0.26), (0.5, 0.25), (1.3, 0.21), (2.3, 0.155), (3.4, 0.09), (4.5, 0.0)], "core", 20, 3)
    # Discs across the jet: what is seen of the flame looking up it from behind, where the shells are edge on.
    for s in (1, -1):
        for k in range(12):
            a = 0.12 + 5.2 * (k / 11) ** 1.3
            g.loft([ring(s, a, 0.0, 20), ring(s, a, 0.36 * (1.0 - 0.75 * a / 5.6), 20)], "disc", False, False)
    ob = g.finish("afterburner", smooth=80, recalc=False)
    ob.hide_render = True
    ob.display_type = "WIRE"


# ---------------------------------------------------------------- canopy
# (Everything inside the cockpit is built in Rust: src/aircraft/f22/pit.rs.)
# station y, half width at the sill, height of the top
CANOPY = [(7.30, 0.20, 0.52), (7.00, 0.33, 0.74), (6.60, 0.42, 0.98), (6.20, 0.46, 1.14), (5.70, 0.48, 1.23), (5.20, 0.48, 1.25), (4.70, 0.46, 1.18), (4.30, 0.40, 1.04), (3.95, 0.30, 0.86), (3.70, 0.18, 0.72)]


def sill_z(y, w):
    """Height of the canopy's edge: on the skin, which slopes away either side of the top."""
    return fuse_z(y, w) - 0.02


def canopy_arc(y, w, zt, grow=0.0, n=20):
    z0 = sill_z(y, w)
    return [V(((w + grow) * math.cos(a), y, z0 + (zt + grow - z0) * math.sin(a))) for a in (math.pi * k / n for k in range(n + 1))]


def canopy():
    # One blown piece, no bow in front of the pilot.
    g = Geo()
    rows = spline_rows(CANOPY, 0, len(CANOPY) - 1, 3)
    g.loft([canopy_arc(*r) for r in rows], "glass")
    g.finish("canopy", smooth=60)

    g = Geo()
    # The frame the canopy ends in behind the seat, and its sill along each side.
    y, w, zt = table_at(CANOPY, 4.02)
    tube(g, canopy_arc(y, w, zt, grow=0.010, n=40), 0.026, "frame", n=16, ref=V((0, 1, 0)))
    for s in (1, -1):
        rows = spline_rows(CANOPY, 0, len(CANOPY) - 1, 4)
        tube(g, [V((s * (w + 0.004), y, sill_z(y, w) + 0.012)) for y, w, _ in rows], 0.026, "frame", n=16)
        inner = [[V((s * (w - 0.03), y, sill_z(y, w) + 0.02)), V((s * (w - 0.03), y, sill_z(y, w) - 0.10))] for y, w, _ in spline_rows(CANOPY, 0, len(CANOPY) - 1, 2)]
        g.strip(inner, "cockpit", toward=V((0, 5.5, 0.8)))
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


def surface2(g, sect, u0, u1, a, b, t, m="skin", nu=6):
    """As `surface`, between chord fractions that change along the span: a(u), b(u)."""
    rings = []
    for k in range(nu):
        u = lerp(u0, u1, k / (nu - 1))
        rings.append(foil_ring(sect(u), t, a(u), b(u)))
    g.loft(rings, m)


# Wing (kept in step with wing_sect in src/aircraft/f22/mod.rs): a clipped
# diamond. Span stations of its root (inside the fuselage) and tip; the
# leading edge on the centreline and its sweep (m back per m out: 42 degrees);
# the chord on the centreline and how much it loses per m out (the trailing
# edge is swept forward 17 degrees); height on the centreline, anhedral, thickness.
W_X0, W_X1, W_LE0, W_SWEEP, W_C0, W_TAPER, W_Z0, W_ANH, W_T = 1.2, 6.78, 3.6, 0.9004, 9.85, 1.2061, 0.28, 0.0568, 0.045
NOTCH = (2.3, 0.90)  # inboard of this station the wing ends at this chord fraction: the stabilator's leading edge tucks in behind it
LEF = (2.3, 6.4)   # leading-edge flap
FLAP = (2.3, 4.3)  # flaperon
AIL = (4.3, 6.4)


def wing_chord(x):
    return W_C0 - W_TAPER * x


def lef_frac(x):
    """Chord fraction of the leading-edge flap's hinge."""
    return lerp(0.60, 0.35, (x - LEF[0]) / (LEF[1] - LEF[0])) / wing_chord(x)


def tef_frac(x):
    """Chord fraction of the flaperon's and the aileron's hinge."""
    return 1.0 - lerp(1.25, 0.50, (x - FLAP[0]) / (AIL[1] - FLAP[0])) / wing_chord(x)


def wing_sect(s):
    def f(x):
        le = V((x * s, W_LE0 - W_SWEEP * x, W_Z0 - W_ANH * x))
        return le, wing_chord(x), V((0, -1, 0)), V((W_ANH * s, 0, 1)).normalized()
    return f


def wick(g, p, aft=V((0, -1, 0)), up=V((0, 0, 1))):
    """A static discharger trailing from point p: a fine rod on a little base."""
    g.box(p + aft * 0.02, (0.006, 0.025, 0.004), "skin", (up.cross(aft).normalized(), aft, up))
    tube(g, [p + aft * 0.03, p + aft * 0.14], 0.0035, "rubber", n=5)


def pair_axis(right_axis, s):
    """The axis a mirrored piece turns about so equal angles move both sides alike (as Builder::pair)."""
    return right_axis if s > 0 else -M(right_axis, -1)


def wings():
    one = lambda x: 1.0
    zero = lambda x: 0.0
    for s, side in ((1, "right"), (-1, "left")):
        ws = wing_sect(s)
        g = Geo()
        surface(g, ws, W_X0, NOTCH[0], 0.0, NOTCH[1], W_T, nu=3)
        surface2(g, ws, LEF[0], AIL[1], lef_frac, tef_frac, W_T, nu=12)
        surface(g, ws, AIL[1], W_X1, 0.0, 1.0, W_T, nu=3)
        # Formation light strip along the tip.
        le, c, cdir, up = ws(W_X1)
        g.box(le + cdir * (c * 0.5) + V((s * 0.002, 0, 0)), (0.004, 0.30, 0.025), "slime")
        g.finish(side + " wing")

        for name, (x0, x1), a, b in (("le flap", LEF, zero, lef_frac), ("flap", FLAP, tef_frac, one), ("aileron", AIL, tef_frac, one)):
            g = Geo()
            surface2(g, ws, x0, x1, a, b, W_T, nu=8)
            if name == "aileron":
                for xw in (5.2, 6.1):
                    wick(g, hinge(ws, xw, 1.0))
            xc = lef_frac if name == "le flap" else tef_frac
            p0, p1 = hinge(wing_sect(1), x0, xc(x0)), hinge(wing_sect(1), x1, xc(x1))
            g.finish(side + " " + name, pivot=M(p0, s), axis=pair_axis((p1 - p0).normalized(), s))

        g = Geo()
        le, c, cdir, up = ws(W_X1 - 0.04)
        g.box(le + cdir * (c * 0.12), (0.035, 0.05, 0.022), "nav_green" if s > 0 else "nav_red")
        g.finish(side + " nav light", smooth=10)


# The stabilators: the same sweeps as the wing, their leading edges tucked into
# the wing's notch, their inboard corners the aftmost points of the aircraft.
S_X0, S_X1, S_LE0, S_C0, S_Z, S_T = 1.75, 4.42, -5.05, 3.97, 0.05, 0.035
S_PIVOT = V((S_X0, -7.30, S_Z))


def stab_sect(s):
    def f(x):
        le = V((x * s, S_LE0 - W_SWEEP * (x - S_X0), S_Z))
        return le, S_C0 - W_TAPER * (x - S_X0), V((0, -1, 0)), V((0, 0, 1))
    return f


# The fins: canted out 28 degrees, leading edge swept back and trailing edge
# forward by the same 22.9 degrees.
FIN_CANT = math.radians(28.0)
FIN_BASE = V((1.45, 0.0, 0.42))
FIN_LEN = 3.01
FIN_LE0, FIN_SWEEP, FIN_C0, FIN_C1, FIN_T = -2.30, 1.27, 4.30, 1.76, 0.04
RUDDER = (0.03, 0.70, 0.72)  # from, to (fractions of the fin's height), hinge (chord fraction)


def fin_sect(s):
    ca, sa = math.cos(FIN_CANT), math.sin(FIN_CANT)
    def f(u):
        p = FIN_BASE + V((sa, 0, ca)) * (FIN_LEN * u)
        le = V((p.x * s, FIN_LE0 - FIN_SWEEP * u, p.z))
        return le, lerp(FIN_C0, FIN_C1, u), V((0, -1, 0)), V((ca * s, 0, -sa))
    return f


def tail():
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        surface(g, stab_sect(s), S_X0, S_X1, 0.0, 1.0, S_T, nu=8)
        for xw in (3.6, 4.3):
            wick(g, hinge(stab_sect(s), xw, 1.0))
        g.finish(side + " stabilator", pivot=M(S_PIVOT, s), axis=pair_axis(V((1, 0, 0)), s))

        fs = fin_sect(s)
        u0, u1, xh = RUDDER
        g = Geo()
        # Thickened toward its root ahead of the rudder: a fillet into the fuselage.
        root = lambda u, x: 1.0 + 1.5 * smoothstep(0.14, -0.04, u) ** 2 * (1.0 - smoothstep(0.25, 0.68, x))
        surface(g, fs, -0.08, u0, 0.0, 1.0, FIN_T, nu=3, tk=root)
        surface(g, fs, u0, u1, 0.0, xh, FIN_T, nu=10, tk=root)
        surface(g, fs, u1, 1.0, 0.0, 1.0, FIN_T, nu=5)
        up = V((math.sin(FIN_CANT) * s, 0, math.cos(FIN_CANT)))
        le, c, cdir, out = fs(1.0)
        wick(g, le + cdir * (c * 1.0) - up * 0.05, up=up)
        # Formation light strip, on both faces.
        le, c, cdir, out = fs(0.80)
        mid = le + cdir * (c * 0.45)
        for side_k in (1, -1):
            g.box(mid + out * (side_k * (c * naca_t(0.45, FIN_T) + 0.002)), (0.004, 0.07, 0.26), "slime", (out, cdir, up))
        g.finish(side + " fin")

        g = Geo()
        surface(g, fs, u0, u1, xh, 1.0, FIN_T, nu=6)
        p0, p1 = hinge(fs, u0, xh), hinge(fs, u1, xh)
        # Both rudders swing the same way for the same command: no pair flip here.
        g.finish(side + " rudder", pivot=p0, axis=(p1 - p0).normalized())

    g = Geo()
    g.box(V((0, -6.02, fuse_z(-6.0, 0.0) - 0.06)), (0.04, 0.03, 0.025), "tail_white")
    g.finish("tail light", smooth=10)


# ---------------------------------------------------------------- landing gear (modelled down and locked)
# The nose leg swings forward into its well under the cockpit. Each main leg
# swings forward and a little inboard, its wheel staying upright, into the
# well in the fuselage's side under the wing. Everything stays inside the
# wells, which the doors close over (they open first and shut last: see
# `anim` in Rust).
NOSE_PIVOT = V((0, 5.15, -0.45))
NOSE_AXLE = V((0, 5.00, -1.70))
NOSE_R = 0.30
NOSE_STOWED_Z = -0.42  # height of the nose axle with the gear up
MAIN_PIVOT = V((1.22, -0.95, -0.35))
MAIN_KNEE = V((1.38, -0.95, -1.50))  # bottom of the strut
MAIN_AXLE = V((1.615, -0.95, -1.64))
MAIN_R = 0.36
MAIN_STOW_AT = V((1.22, 0.37, -0.38))  # about where the main wheels stow


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
    """How a (right) main leg stows: the axis and angle of the one swing that
    takes its wheel to MAIN_STOW_AT (or as near as the leg's length allows)."""
    d = MAIN_AXLE - MAIN_PIVOT
    return stow(MAIN_PIVOT, MAIN_AXLE, MAIN_PIVOT + (MAIN_STOW_AT - MAIN_PIVOT).normalized() * d.length)


def gear():
    # Nose: swings forward into the well under the cockpit.
    l = (NOSE_AXLE - NOSE_PIVOT).length
    dz = NOSE_STOWED_Z - NOSE_PIVOT.z
    ax, ang = stow(NOSE_PIVOT, NOSE_AXLE, NOSE_PIVOT + V((0, math.sqrt(l * l - dz * dz), dz)))
    along = (NOSE_PIVOT - NOSE_AXLE).normalized()
    low = NOSE_AXLE + along * 0.42
    g = Geo()
    # The trunnion across the top, the strut, the steering collar.
    tube(g, [NOSE_PIVOT + V((-0.16, 0, 0)), NOSE_PIVOT + V((0.16, 0, 0))], 0.045, "strut", n=10)
    tube(g, [NOSE_PIVOT, low], 0.062, "strut", n=14)
    lathe(g, NOSE_PIVOT - along * 0.55, -along, [(0.0, 0.0), (0.088, 0.0), (0.088, 0.12), (0.0, 0.12)], "strut", n=14)
    fwd = (V((0, 1, 0)) - along * along.y).normalized()
    # Landing and taxi lights on a bracket ahead of the strut.
    g.box(NOSE_PIVOT - along * 0.70 + fwd * 0.10, (0.055, 0.03, 0.04), "strut", (V((1, 0, 0)), fwd, along))
    g.box(NOSE_PIVOT - along * 0.70 + fwd * 0.132, (0.045, 0.004, 0.032), "landing", (V((1, 0, 0)), fwd, along))
    # Hydraulic lines down the back of the strut.
    for x in (-0.03, 0.03):
        tube(g, [NOSE_PIVOT + V((x, -0.07, 0)) - along * 0.05, low + V((x, -0.068, 0))], 0.008, "steel", n=5)
    g.finish("nose leg", pivot=NOSE_PIVOT, axis=ax, angle=ang)

    down = (NOSE_AXLE - NOSE_PIVOT).normalized()
    g = Geo()
    # The oleo and the single-sided fork that carries the wheel.
    tube(g, [low + along * 0.10, NOSE_AXLE + along * 0.36], 0.048, "chrome", n=12)
    g.box(NOSE_AXLE + along * 0.36, (0.07, 0.05, 0.05), "strut")
    tube(g, [NOSE_AXLE + along * 0.36, NOSE_AXLE + along * 0.34 + V((0.13, 0, 0)), NOSE_AXLE + V((0.13, 0, 0))], 0.032, "strut", n=10)
    tube(g, [NOSE_AXLE + V((-0.10, 0, 0)), NOSE_AXLE + V((0.15, 0, 0))], 0.028, "steel")
    scissors(g, low + V((0, -0.06, 0)), NOSE_AXLE + along * 0.38 + V((0, -0.05, 0)), V((0, -0.12, 0)))
    g.finish("nose fork", pivot=NOSE_AXLE, axis=down, parent="nose leg")

    g = Geo()
    wheel(g, NOSE_AXLE, NOSE_R, 0.085)
    g.finish("nose wheel", pivot=NOSE_AXLE, axis=V((1, 0, 0)), parent="nose fork")

    # Mains.
    ax, sw = main_stow()
    s_dir = (MAIN_KNEE - MAIN_PIVOT).normalized()
    mid = MAIN_PIVOT + (MAIN_KNEE - MAIN_PIVOT) * 0.50
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        # The trunnion, the strut and its oleo, the axle's arm, the brake.
        tube(g, [M(MAIN_PIVOT + V((0, 0.15, 0)), s), M(MAIN_PIVOT + V((0, -0.15, 0)), s)], 0.05, "strut", n=10)
        tube(g, [M(MAIN_PIVOT, s), M(mid, s)], 0.082, "strut", n=14)
        lathe(g, M(mid, s), M(-s_dir, s), [(0.0, 0.0), (0.098, 0.0), (0.098, 0.07), (0.0, 0.07)], "strut", n=14)
        tube(g, [M(mid - s_dir * 0.05, s), M(MAIN_KNEE, s)], 0.058, "chrome", n=12)
        sd = M(s_dir, s)
        bx = (V((1, 0, 0)) - sd * sd.x).normalized()
        g.box(M(MAIN_KNEE, s), (0.06, 0.06, 0.07), "strut", (bx, sd.cross(bx), sd))
        tube(g, [M(MAIN_KNEE, s), M(V((MAIN_KNEE.x + 0.03, MAIN_AXLE.y, MAIN_AXLE.z)), s), M(MAIN_AXLE, s)], 0.045, "strut", n=10)
        lathe(g, M(MAIN_AXLE + V((-0.20, 0, 0)), s), V((s, 0, 0)), [(0.0, 0.0), (0.16, 0.0), (0.16, 0.05), (0.0, 0.05)], "steel", n=16)
        scissors(g, M(mid + s_dir * 0.08 + V((0, 0.07, 0)), s), M(MAIN_KNEE + V((0, 0.07, 0.04)), s), M(V((0.02, 0.14, 0)), s))
        for x in (-0.03, 0.03):
            tube(g, [M(MAIN_PIVOT + V((x, -0.085, 0)), s), M(mid + V((x, -0.09, 0)), s)], 0.009, "steel", n=5)
        g.finish(side + " main leg", pivot=M(MAIN_PIVOT, s), axis=pair_axis(ax, s), angle=sw)

        g = Geo()
        wheel(g, M(MAIN_AXLE, s), MAIN_R, 0.14)
        g.finish(side + " main wheel", pivot=M(MAIN_AXLE, s), axis=V((1, 0, 0)), parent=side + " main leg")
    r = Matrix.Rotation(sw, 3, ax)
    a1 = MAIN_PIVOT + r @ (MAIN_AXLE - MAIN_PIVOT)
    print(f"main gear: swing {math.degrees(sw):.0f} deg, wheel stowed at {tuple(round(c, 3) for c in a1)}, axle {tuple(round(c, 2) for c in r @ V((1, 0, 0)))}")


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
    xi, my0, my1, _ = MAIN_WELL
    gap = 0.005
    for s, side in ((1, "right"), (-1, "left")):
        # Nose: a pair meeting on the centreline, hinged at the well's sides.
        skin_door(side + " nose door", s, gap, hw - gap, ny1 - gap, ny0 + gap, hw, math.radians(86))
        # Mains, hinged along the outboard edge so that they hang outside the
        # wheels: the long door ahead of the leg that shuts again once the leg
        # is down (Rust `anim`), and a short one beside the strut that stays open.
        xo = lambda y: main_well_out(y) - gap
        hinge_x = min(main_well_out(my0), main_well_out(my1)) - gap
        skin_door(side + " main door", s, xi + gap, xo, my0 - gap, MAIN_DOOR_SPLIT + gap, hinge_x, math.radians(100), nx=14)
        skin_door(side + " strut door", s, xi + gap, xo, MAIN_DOOR_SPLIT - gap, my1 + gap, hinge_x, math.radians(100), nx=14, ny=5)


def check_gear():
    """Sweep the gear from up to down (as Rust `anim` moves it: the legs over
    0.15..0.85) and report any gear vertex inside the fuselage's skin but
    outside the wells, or, stowed, not tucked in behind the doors. Run on
    every export: a hand edit that makes the gear poke through the skin shows
    up here."""
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
        if not FUSE[-1][0] < p.y < FUSE[0][0]:
            return -1.0
        return min(p.z - fuse_z(p.y, p.x, False), fuse_z(p.y, p.x, True) - p.z, fuse_hw(p.y, p.z) - abs(p.x))

    def in_well(p):
        hw, y0, y1, zt = NOSE_WELL
        if abs(p.x) < hw and y0 < p.y < y1 and p.z < zt:
            return True
        xi, y0, y1, zt = MAIN_WELL
        return xi < abs(p.x) < main_well_x(p.y, p.z) and y1 < p.y < y0 and p.z < zt

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
# Every piece the game looks for (PIECES in src/aircraft/f22/mod.rs); other objects are ignored.
EXPECTED = ["radome", "nose section", "fuselage", "tail cone", "canopy", "canopy frame", "tail light", "nose leg", "nose fork", "nose wheel", "afterburner"] + [
    f"{s} {n}" for s in ("left", "right") for n in ("intake", "nozzle", "wing", "le flap", "flap", "aileron", "nav light", "stabilator", "fin", "rudder", "main leg", "main wheel", "nose door", "strut door", "main door")]


def game_mat(m):
    """Which game material a Blender material is: its "game" property, its f22_ name, or else the skin."""
    if m is None:
        return "skin"
    name = m.get("game") or (m.name[4:].split(".")[0] if m.name.startswith("f22_") else None)
    return name if name in MATS else "skin"


def export(path):
    out = bytearray(b"FFAC")
    obs = [o for o in COLL.all_objects if o.type == "MESH"]
    names = {o.name for o in obs}
    missing = [n for n in EXPECTED if n not in names]
    if missing:
        raise SystemExit(f"f22 export: the game needs these pieces, missing from the '{COLL.name}' collection: {', '.join(missing)}")
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
                    if ob.name != "afterburner":
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
    target = V((0, 0, 0.3))
    views = (("three_quarter", V((16, 17, 8)), 50, target), ("side", V((30, 0.01, 0.6)), 50, target), ("top", V((0.01, 0.3, 52)), 50, target), ("front", V((0.01, 30, 1.0)), 50, target), ("rear_low", V((-11, -16, -2.5)), 50, target),
             ("rear", V((4.5, -17, 3.0)), 60, V((0, -6, 0.2))), ("nozzles", V((2.6, -11.5, 1.0)), 70, V((0, -7.0, 0.0))), ("cockpit", V((3.0, 8.6, 2.6)), 75, V((0, 5.6, 0.8))), ("under", V((8, 10, -6)), 50, V((0, 0.5, -0.3))))
    for name, eye, lens, look in views:
        cam.location = eye
        cam.rotation_euler = (look - eye).to_track_quat("-Z", "Y").to_euler()
        cam_data.lens = lens
        sc.render.filepath = os.path.join(outdir, name + ".png")
        bpy.ops.render.render(write_still=True)


# The paint is projected straight down, straight up and from each side: these
# are the rectangles (m) those four pictures cover (the Raptor is wider than
# the Hornet, so they are larger than fa18.py's). plane.wgsl and the paint
# script use the same numbers.
GUIDE_SPAN = 14.4   # across the top and bottom views; up the side views
GUIDE_LEN = 21.6    # fore and aft in all four
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
    for o in hidden:
        o.hide_render = o.name == "afterburner"


def main():
    global COLL, CUTS
    if EXPORT_ONLY:
        # Export the .blend that is open, edits and all, without rebuilding anything.
        COLL = bpy.data.collections.get("F-22A")
        if COLL is None:
            raise SystemExit("f22 export: no 'F-22A' collection in this file (open assets/blender/f22.blend)")
        export(os.path.join(ASSETS, "f22.bin.gz"))
        check_gear()
    else:
        for ob in list(bpy.data.objects):
            bpy.data.objects.remove(ob, do_unlink=True)
        for c in list(bpy.data.collections):
            bpy.data.collections.remove(c)
        COLL = bpy.data.collections.new("F-22A")
        bpy.context.scene.collection.children.link(COLL)
        CUTS = bpy.data.collections.new("F-22A well cutters")
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
        bpy.context.preferences.filepaths.save_version = 0  # no f22.blend1 beside it
        export(os.path.join(ASSETS, "f22.bin.gz"))
        check_gear()
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ASSETS, "blender", "f22.blend"))
    if RENDER:
        render_previews(RENDER)
    if GUIDES:
        render_guides(GUIDES)


main()
