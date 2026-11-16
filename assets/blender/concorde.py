# The Aérospatiale/BAC Concorde for FableFlight, built procedurally in Blender.
#
#   blender -b --factory-startup --python assets/blender/concorde.py -- <repo>/assets [--render <dir>] [--guides <dir>]
#
# Built the way the Hornet is (assets/blender/fa18.py, whose mesh toolkit this
# file carries a copy of): the airframe from lofted sections, one object per
# piece that moves or breaks off on its own, saved as
# assets/blender/concorde.blend and exported to assets/concorde.bin.gz, which
# src/aircraft/concorde loads. That rebuild starts from scratch, so it throws
# away anything edited by hand in the .blend.
#
# To keep hand edits, export the .blend as it stands instead (save it first):
#
#   blender -b assets/blender/concorde.blend --python assets/blender/concorde.py -- <repo>/assets --export-only
#
# Edit freely: reshape meshes, add modifiers (they are applied on export),
# move or turn pieces (each carries its hinge along), assign the concorde_
# materials. Keep the piece names and the "Concorde" collection: the game
# finds pieces by name (see EXPECTED below); objects with other names are
# ignored. How each piece moves is set in Rust by its name; each object
# records where its hinge is in its own frame (custom properties "pivot",
# "axis", "angle").
#
# The nose is modelled up, as it flies supersonic, with the visor raised over
# the windscreen. "nose" droops about its hinge (its lower aft edge) by its
# `angle` (12.5 degrees, the landing position); "visor" and "visor frame"
# hang from it and slide by their `angle`, which for these two is a distance
# (m) along their `axis`, forward and down into the nose.
#
# The gear wells (the nose well and, each side, the wheel bay in the belly and
# the trench the leg lies in under the wing) are cut with boolean modifiers
# (cutters in the "Concorde well cutters" collection, not exported) and the
# doors are patches of the skin itself. Every export ends with a gear check:
# a sweep of the nose leg from up to down that names any part passing through
# the skin, and any part of the stowed main gear left outside it.
#
# --render writes preview pictures. --guides writes the four orthographic
# views (top, bottom, left side, right side) the paint is projected from, each
# with its silhouette mask and a geometry pass, <view>_geo.npz: what point of
# which piece each pixel shows (as assets/blender/f4.py's, for a paint script
# that paints by where on the aircraft a pixel is).
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
# aircraft's paint pictures; the vertex colour shows only where no picture
# faces the surface.
# Material 11 (exhaust) extra: 0 the nozzle's buckets outside, 1 inside the
# nozzle, 2 the flame holder; material 17 (the reheat plume, drawn in the
# transparent pass) extra: 0 heat haze, 1 the flame, 2 its core, 3 discs across it.
MATS = {
    "skin": (15, 17, (0.80, 0.80, 0.78), 0),  # white all over
    "inlet": (10, 0, (0.02, 0.02, 0.022), 0),
    "ramp": (3, 0, (0.42, 0.42, 0.43), 0),  # the intakes' ramps and the nacelles' bare steel
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
    "strobe": (8, 0, (1.0, 0.10, 0.06), 4),
    "haze": (17, 0, (1.0, 1.0, 1.0), 0),
    "flame": (17, 0, (1.0, 0.6, 0.3), 1),
    "core": (17, 0, (0.7, 0.8, 1.0), 2),
    "disc": (17, 0, (1.0, 0.8, 0.6), 3),
}

MAT = {}


def make_materials():
    for name, (gid, _pat, col, _x) in MATS.items():
        m = bpy.data.materials.get("concorde_" + name) or bpy.data.materials.new("concorde_" + name)
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


def join(ob, other, smooth=38.0):
    """Take another object's mesh into ob's, and remove it."""
    names = [m.name for m in ob.data.materials]
    for m in other.data.materials:
        if m.name not in names:
            ob.data.materials.append(m)
            names.append(m.name)
    remap = [names.index(m.name) for m in other.data.materials]
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    n0 = len(bm.faces)
    bm.from_mesh(other.data)
    bm.faces.ensure_lookup_table()
    for f in bm.faces[n0:]:
        f.material_index = remap[f.material_index]
    bm.to_mesh(ob.data)
    bm.free()
    bpy.data.objects.remove(other, do_unlink=True)
    for p in ob.data.polygons:
        p.use_smooth = True
    ob.data.set_sharp_from_angle(angle=math.radians(smooth))


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


def hermite(table, x, col):
    """Column `col` of a station table at x (its first column, rising or
    falling): a cubic through the stations with finite-difference slopes."""
    n = len(table)
    lo, hi = min(table[0][0], table[-1][0]), max(table[0][0], table[-1][0])
    x = max(lo, min(hi, x))
    for i in range(n - 1):
        x0, x1 = table[i][0], table[i + 1][0]
        if min(x0, x1) <= x <= max(x0, x1):
            v0, v1 = table[i][col], table[i + 1][col]
            a, d = table[max(i - 1, 0)], table[min(i + 2, n - 1)]
            m0 = (v1 - a[col]) / (x1 - a[0])
            m1 = (d[col] - v0) / (d[0] - x0)
            h = x1 - x0
            t = (x - x0) / h
            t2, t3 = t * t, t * t * t
            return (2 * t3 - 3 * t2 + 1) * v0 + (t3 - 2 * t2 + t) * h * m0 + (-2 * t3 + 3 * t2) * v1 + (t3 - t2) * h * m1
    return table[-1][col]


def aim(g, n0, toward):
    """Make the faces added since n0 look toward `toward(centre)`: for open
    sheets and funnels, which recalc_face_normals may turn inside out."""
    g.bm.faces.ensure_lookup_table()
    for f in g.bm.faces[n0:]:
        g.facing.append((f, toward(f.calc_center_median())))


def stations(x0, x1, step):
    n = max(1, round(abs(x1 - x0) / step))
    return [lerp(x0, x1, k / n) for k in range(n + 1)]


# ---------------------------------------------------------------- fuselage
# station y, half width, top, height of the widest point, bottom, superellipse exponent
# (kept in step with FUSE in src/aircraft/concorde/mod.rs). The nose tip is
# 33 m ahead of the centre of gravity and points a little down; the cabin is
# 2.88 m wide and 3.32 m deep; the tail tapers upward to a point. The skin is
# a smooth curve (`fuse_row`) through these stations.
FUSE = [
    (33.00, 0.00, -0.80, -0.80, -0.80, 2.0),
    (32.60, 0.06, -0.74, -0.80, -0.87, 2.0),
    (32.00, 0.14, -0.65, -0.80, -0.95, 2.0),
    (31.00, 0.27, -0.50, -0.79, -1.06, 2.0),
    (30.00, 0.39, -0.35, -0.77, -1.15, 2.0),
    (29.00, 0.51, -0.19, -0.74, -1.23, 2.0),
    (28.00, 0.62, -0.03, -0.70, -1.30, 2.0),
    (27.00, 0.73, 0.14, -0.65, -1.37, 2.0),
    (26.00, 0.84, 0.32, -0.59, -1.43, 2.0),
    (25.00, 0.94, 0.52, -0.52, -1.48, 2.0),
    (24.00, 1.04, 0.76, -0.44, -1.53, 2.0),
    (23.00, 1.13, 1.02, -0.35, -1.57, 2.0),
    (22.00, 1.22, 1.27, -0.25, -1.60, 2.0),
    (21.00, 1.30, 1.46, -0.15, -1.63, 2.0),
    (20.00, 1.36, 1.58, -0.07, -1.65, 2.0),
    (18.00, 1.44, 1.66, 0.00, -1.66, 2.0),
    (16.00, 1.44, 1.66, 0.00, -1.66, 2.0),
    (-9.00, 1.44, 1.66, 0.00, -1.66, 2.0),
    (-11.00, 1.44, 1.66, 0.00, -1.66, 2.0),
    (-13.00, 1.42, 1.66, 0.04, -1.60, 2.0),
    (-15.00, 1.36, 1.65, 0.14, -1.46, 2.0),
    (-17.00, 1.26, 1.63, 0.28, -1.24, 2.0),
    (-19.00, 1.12, 1.60, 0.44, -0.96, 2.0),
    (-21.00, 0.95, 1.55, 0.60, -0.64, 2.0),
    (-23.00, 0.76, 1.48, 0.76, -0.28, 2.0),
    (-25.00, 0.55, 1.39, 0.90, 0.12, 2.0),
    (-27.00, 0.30, 1.26, 1.00, 0.56, 2.0),
    (-28.00, 0.14, 1.16, 1.03, 0.82, 2.0),
    (-28.66, 0.00, 1.05, 1.05, 1.05, 2.0),
]
RING_N = 64
NOSE_HINGE_Y = 25.3  # where the drooping nose ends: the foot of the windscreen
NOSE_DROOP = math.radians(12.5)


def fuse_row(y):
    return (y,) + tuple(hermite(FUSE, y, k) for k in range(1, 6))


def fuse_ring(y, hw, top, mid, bot, p, n=RING_N):
    hw = max(hw, 0.0)
    pts = []
    for k in range(n):
        a = 2 * math.pi * k / n
        c, s = math.cos(a), math.sin(a)
        f = abs(s) ** (2 / p)
        pts.append(V((hw * math.copysign(abs(c) ** (2 / p), c), y, mid + (top - mid) * f if s >= 0 else mid - (mid - bot) * f)))
    return pts


def ring_at(y):
    return fuse_ring(*fuse_row(y))


def fuse_top(y):
    return fuse_row(y)[2]


def fuse_z(y, x, upper=True):
    """Height of the fuselage skin at station y, x out from the centreline."""
    _, hw, top, mid, bot, p = fuse_row(y)
    f = max(0.0, 1.0 - (min(abs(x), hw) / max(hw, 1e-6)) ** p) ** (1 / p)
    return mid + (top - mid) * f if upper else mid - (mid - bot) * f


def fuse_hw(y, z):
    """Half width of the fuselage at station y, height z."""
    _, hw, top, mid, bot, p = fuse_row(y)
    f = min(1.0, abs(z - mid) / max((top - mid) if z >= mid else (mid - bot), 1e-6))
    return hw * max(0.0, 1.0 - f ** p) ** (1 / p)


def fuse_depth(p):
    """How far inside the fuselage's skin p is (negative: outside)."""
    if not FUSE[-1][0] < p.y < FUSE[0][0]:
        return -1.0
    return min(p.z - fuse_z(p.y, p.x, False), fuse_z(p.y, p.x, True) - p.z, fuse_hw(p.y, p.z) - abs(p.x))


def fuse_out(c):
    """A point well outside the fuselage's skin from c on it."""
    r = V((c.x, 0, c.z - fuse_row(c.y)[3]))
    return c + (r.normalized() if r.length > 1e-4 else V((0, 0, 1))) * 20.0


# The flight deck's glazing (everything inside it is built in Rust:
# src/aircraft/concorde/pit.rs). Rows of the nose section: the windscreen's
# opening runs over the top between WS_Y, from ring vertex KW to its mirror
# image; two side windows each side behind it, between ring vertices KS.
NOSE_ROWS = [25.3, 24.9, 24.5, 24.1, 23.7, 23.45, 23.02, 22.94, 22.5, 22.2, 21.6, 20.8, 20.0]
WS_Y = (25.3, 23.7)
WS_BASE = 24.75  # where the windscreen's glass stands on the glare shield's deck
KW = 8
SIDE_WINDOWS = ((23.45, 23.02), (22.94, 22.5))
KS = (4, 9)
# The visor: a glazed fairing over the windscreen, from the drooping nose back
# onto the flight deck's roof. It slides forward and down into the nose.
VISOR_Y = (26.7, 23.6)
VISOR_SLIDE = V((0, 1.0, -0.42)).normalized()
VISOR_TRAVEL = 1.85


def fuselage():
    half = RING_N // 2
    # The drooping nose.
    g = Geo()
    ys = [33.0, 32.85, 32.6, 32.3, 32.0] + stations(31.5, 26.0, 0.5) + [25.65, NOSE_HINGE_Y]
    g.loft([ring_at(y) for y in ys], "skin")
    for s in (1, -1):
        # Pitot heads low on the sides, an ice detector ahead of them.
        y = 27.6
        x = fuse_hw(y, -0.95)
        tube(g, [V((s * (x - 0.02), y, -0.95)), V((s * (x + 0.10), y + 0.03, -0.98)), V((s * (x + 0.11), y + 0.40, -0.98))], lambda t: 0.018 - 0.008 * t, "steel", n=8)
    bot = fuse_row(NOSE_HINGE_Y)[4]
    g.finish("nose", pivot=V((0, NOSE_HINGE_Y, bot)), axis=V((-1, 0, 0)), angle=NOSE_DROOP)

    # The flight deck: the skin with the windscreen's and side windows' openings.
    rows = NOSE_ROWS
    rings = [ring_at(y) for y in rows]
    ws = (rows.index(WS_Y[0]), rows.index(WS_Y[1]))
    side = [rows.index(a) for a, _ in SIDE_WINDOWS]

    def skip(row, i):
        if ws[0] <= row < ws[1] and KW <= i < half - KW:
            return True
        return row in side and (KS[0] <= i < KS[1] or half - KS[1] <= i < half - KS[0])

    g = Geo()
    g.loft(rings, "skin", False, False, skip=skip)
    aim(g, 0, fuse_out)
    g.poly(rings[0], "skin", toward=V((0, 60, 0)))
    g.poly(rings[-1], "skin", toward=V((0, -60, 0)))
    g.finish("nose section")

    # The windscreen: the glare shield's deck from the opening's front edge
    # back to the foot of the glass, the glass leaning back from there to the
    # roof; the side windows flush in their openings.
    arc = range(KW, half - KW + 1)
    front, rear, base_ring = ring_at(WS_Y[0]), ring_at(WS_Y[1]), ring_at(WS_BASE)
    xe, ze = base_ring[KW].x, base_ring[KW].z
    base = []
    for k in arc:
        u = base_ring[k].x / xe
        base.append(V((base_ring[k].x, WS_BASE + 0.30 * (1 - u * u), ze + 0.06 * (1 - u * u))))
    g = Geo()
    g.strip([base, [rear[k] for k in arc]], "glass", toward=V((0, 60, 40)))
    for ya, yb in SIDE_WINDOWS:
        ra, rb = ring_at(ya), ring_at(yb)
        for i in list(range(KS[0], KS[1])) + list(range(half - KS[1], half - KS[0])):
            q = [ra[i], ra[i + 1], rb[i + 1], rb[i]]
            g.poly(q, "glass", toward=fuse_out((q[0] + q[2]) / 2))
    g.finish("canopy", smooth=60)

    g = Geo()
    g.strip([[front[k] for k in arc], base], "cockpit", toward=V((0, 24, 60)))
    edge_y = [y for y in rows if WS_Y[0] >= y >= WS_Y[1]]
    for k in (KW, half - KW):
        tube(g, [ring_at(y)[k] for y in edge_y], 0.03, "frame", n=8)
    tube(g, [rear[k] for k in arc], 0.03, "frame", n=8, ref=V((0, 1, 0)))
    tube(g, base, 0.022, "frame", n=8, ref=V((0, 1, 0)))
    for k in (half // 2, half // 2 - 4, half // 2 + 4):
        tube(g, [base[k - KW], rear[k]], 0.024, "frame", n=8)
    g.finish("canopy frame", smooth=80)

    # The visor, raised: glass standing a little proud of the skin, in a frame.
    vy = stations(VISOR_Y[0], VISOR_Y[1], 0.39)
    shell = []
    for y in vy:
        r = ring_at(y)
        shell.append([p + (fuse_out(p) - p).normalized() * 0.02 for p in (r[k] for k in arc)])
    g = Geo()
    g.strip(shell, "glass", toward=V((0, 25, 60)))
    pivot = V((0, (VISOR_Y[0] + VISOR_Y[1]) / 2, fuse_top((VISOR_Y[0] + VISOR_Y[1]) / 2) + 0.02))
    g.finish("visor", smooth=60, pivot=pivot, axis=VISOR_SLIDE, angle=VISOR_TRAVEL, parent="nose")
    g = Geo()
    n = len(arc)
    for row in (shell[0], shell[len(shell) // 2], shell[-1]):
        tube(g, row, 0.016, "frame", n=6, ref=V((0, 1, 0)))
    for j in (0, n // 4, n // 2, 3 * n // 4, n - 1):
        tube(g, [row[j] for row in shell], 0.016, "frame", n=6)
    g.finish("visor frame", smooth=80, pivot=pivot, axis=VISOR_SLIDE, angle=VISOR_TRAVEL, parent="nose")
    # Stowed, it must be out of sight inside the nose.
    worst = min(fuse_depth(p + VISOR_SLIDE * VISOR_TRAVEL) for row in shell for p in row)
    print(f"visor: lowered {VISOR_TRAVEL} m along {tuple(round(c, 3) for c in VISOR_SLIDE)}, at least {worst * 100:.0f} cm inside the nose")

    # The cabin.
    g = Geo()
    ys = [20.0, 19.0, 18.0, 16.0] + stations(13.0, -8.0, 3.0) + [-9.0, -11.0, -12.0, -13.0]
    g.loft([ring_at(y) for y in ys], "skin")
    # Blade antennas, the anti-collision lights.
    blade(g, V((0, 9.0, fuse_top(9.0) - 0.01)), 0.45, 0.32, 0.012)
    blade(g, V((0, 12.0, fuse_z(12.0, 0, False) + 0.01)), 0.45, 0.30, 0.012, down=True)
    g.box(V((0, 2.0, fuse_top(2.0) + 0.02)), (0.05, 0.09, 0.035), "strobe")
    g.box(V((0, 6.0, fuse_z(6.0, 0, False) - 0.02)), (0.05, 0.09, 0.035), "strobe")
    ob = g.finish("fuselage")
    carve(ob, [nose_well(), wheel_bay(1), wheel_bay(-1)])

    # The tail cone, with the bumper wheel under it.
    g = Geo()
    ys = stations(-13.0, -28.0, 1.0) + [-28.4, -28.66]
    g.loft([ring_at(y) for y in ys], "skin")
    yb = -21.0
    zb = fuse_z(yb, 0, False)
    drop = 1.05  # a long leg: it meets the runway as the nozzles would, at 11.5 degrees nose up
    tube(g, [V((0, yb + 0.45, zb + 0.05)), V((0, yb, zb - drop))], 0.05, "strut", n=10)
    tube(g, [V((-0.16, yb, zb - drop)), V((0.16, yb, zb - drop))], 0.025, "steel", n=8)
    for x in (-0.11, 0.11):
        wheel(g, V((x, yb, zb - drop)), 0.17, 0.05)
    g.finish("tail cone")

    g = Geo()
    g.box(V((0, -28.60, 1.05)), (0.05, 0.05, 0.04), "tail_white")
    g.finish("tail light", smooth=10)


# The gear wells. The nose well under the forward cabin (half width, aft end,
# front end, roof); each side a bay in the belly for the four wheels, lying
# flat (inboard edge, outboard edge, front, back, roof), and the trench under
# the wing the leg lies in, from the bay out to its pivot.
NOSE_WELL = (0.40, 14.95, 19.80, -0.55)
WHEEL_BAY = (0.04, 1.37, -1.06, -4.14, -0.20)
LEG_TRENCH = (1.30, 4.02, -2.28, -2.92)


_CUTTERS = {}


def once(name, rings):
    """The cutter of this name (two pieces may be cut with the same one)."""
    if name not in _CUTTERS:
        _CUTTERS[name] = cutter(name, rings)
    return _CUTTERS[name]


def nose_well():
    hw, y0, y1, zt = NOSE_WELL
    return once("nose well", [[V((-hw, y, -2.4)), V((hw, y, -2.4)), V((hw, y, zt)), V((-hw, y, zt))] for y in (y1, y0)])


def wheel_bay(s):
    x0, x1, y0, y1, zt = WHEEL_BAY
    return once(("right" if s > 0 else "left") + " wheel bay", [[V((x0 * s, y, -2.4)), V((x1 * s, y, -2.4)), V((x1 * s, y, zt)), V((x0 * s, y, zt))] for y in (y0, y1)])


def leg_trench(s):
    x0, x1, y0, y1 = LEG_TRENCH
    rings = []
    for x in stations(x0, x1, 0.3):
        zt = wing_z(x, (y0 + y1) / 2, True) - 0.08
        rings.append([V((x * s, y0, -2.4)), V((x * s, y1, -2.4)), V((x * s, y1, zt)), V((x * s, y0, zt))])
    return once(("right" if s > 0 else "left") + " leg trench", rings)


# ---------------------------------------------------------------- wing
# (kept in step with wing_sect in src/aircraft/concorde/mod.rs)
# The ogee: the leading edge's station at each span station (a smooth curve
# through these), the trailing edge a straight line swept a little forward.
W_LE = [(0.9, 19.8), (1.44, 15.0), (2.0, 10.0), (3.0, 3.5), (4.0, -0.5), (5.5, -4.3), (7.0, -7.2), (9.0, -10.5), (11.0, -13.6), (12.2, -15.6), (12.8, -16.8)]
W_X0, W_X1 = 0.9, 12.8  # root (inside the fuselage) and tip
W_TE0, W_TE_K = -19.0, 0.11  # the trailing edge at the centreline, and how far forward it runs per metre out
W_Z0, W_ANH, W_ANH_X = -1.05, 0.03, 1.44  # height of the chord line, anhedral (drop per metre) outboard of the fuselage's side
W_T0, W_T1 = 0.030, 0.0215  # thickness ratio at the centreline and the tip
# The elevons: name, span stations.
ELEVONS = [("inner", 1.55, 4.05), ("middle", 6.75, 9.60), ("outer", 9.65, 12.30)]


def wing_le(x):
    return hermite(W_LE, x, 1)


def wing_te(x):
    return W_TE0 + W_TE_K * x


def wing_c(x):
    return wing_le(x) - wing_te(x)


def wing_zm(x):
    return W_Z0 - W_ANH * max(x - W_ANH_X, 0.0)


def wing_t(x):
    return lerp(W_T0, W_T1, x / W_X1)


def wing_camber(c, xc):
    """The leading edge's droop: how far below the chord line the section's middle is."""
    return -min(0.016 * c, 0.0085 * c + 0.09) * max(0.0, 1.0 - xc / 0.25) ** 2


def elevon_chord(x):
    if x < 5.0:
        return 3.0
    if x < 9.62:
        return lerp(2.6, 2.0, (x - 6.75) / 2.85)
    return lerp(2.0, 0.8, (x - 9.65) / 2.65)


def hinge_frac(x):
    return 1.0 - elevon_chord(x) / wing_c(x)


def wing_ring(s, x, a, b, m=20):
    """The (closed) section at span station x from chord fraction a to b."""
    yl, c, zm, t = wing_le(x), wing_c(x), wing_zm(x), wing_t(x)
    xs = [a + (b - a) * (1 - math.cos(math.pi * k / m)) / 2 for k in range(m + 1)]

    def pt(xc, sgn):
        return V((x * s, yl - c * xc, zm + wing_camber(c, xc) + sgn * max(c * naca_t(xc, t), 0.004)))

    return [pt(xc, 1) for xc in xs] + [pt(xc, -1) for xc in reversed(xs)]


def wing_z(x, y, top):
    """Height of the wing's upper or lower surface at (x, y)."""
    x = min(max(abs(x), W_X0), W_X1)
    c = wing_c(x)
    xc = min(max((wing_le(x) - y) / c, 0.0), 1.0)
    return wing_zm(x) + wing_camber(c, xc) + (1 if top else -1) * max(c * naca_t(xc, wing_t(x)), 0.004)


def in_wing(p, tol=0.0):
    x = abs(p.x)
    return W_X0 <= x <= W_X1 and wing_te(x) < p.y < wing_le(x) and wing_z(x, p.y, False) + tol < p.z < wing_z(x, p.y, True) - tol


def belly_z(x, y):
    """Height of the aircraft's underside at (x, y): the fuselage's or the wing's, whichever is lower."""
    x = abs(x)
    zs = []
    if x < fuse_row(y)[1]:
        zs.append(fuse_z(y, x, False))
    if W_X0 <= x <= W_X1 and wing_te(x) < y < wing_le(x):
        zs.append(wing_z(x, y, False))
    return min(zs) if zs else fuse_z(y, x, False)


def wing_hinge(x):
    c = wing_c(x)
    h = hinge_frac(x)
    return V((x, wing_le(x) - c * h, wing_zm(x) + wing_camber(c, h)))


def wings():
    cuts = [x for _, a, b in ELEVONS for x in (a, b)]
    bounds = [W_X0] + cuts + [W_X1]
    area = 0.0
    xs = stations(0.0, W_X1, 0.05)
    for a, b in zip(xs, xs[1:]):
        area += (wing_c(max(a, 1.44)) + wing_c(max(b, 1.44))) * (b - a)
    print(f"wing: {area:.0f} m2 (the fuselage's width between the wings counted in), root chord at the fuselage's side {wing_c(1.44):.1f} m")
    for s, side in ((1, "right"), (-1, "left")):
        # The inboard stretch ahead of the inner elevon's hinge line, which the
        # wells are cut into: one closed loft, so the cut comes out clean.
        g = Geo()
        g.loft([wing_ring(s, x, 0.0, hinge_frac(x)) for x in stations(W_X0, bounds[2], 0.28)], "skin")
        root = g.finish(side + " wing root", smooth=30)
        carve(root, [wheel_bay(s), leg_trench(s)], smooth=30)
        # The rest: the strip behind it inboard of the elevon, and the stretches outboard.
        g = Geo()
        g.loft([wing_ring(s, x, hinge_frac(x), 1.0, m=8) for x in stations(W_X0, bounds[1], 0.28)], "skin")
        for k, (x0, x1) in enumerate(zip(bounds[2:], bounds[3:])):
            cut = k % 2 == 1  # an elevon behind this stretch
            g.loft([wing_ring(s, x, 0.0, hinge_frac(x) if cut else 1.0) for x in stations(x0, x1, 0.28)], "skin")
        join(g.finish(side + " wing", smooth=30), root, smooth=30)

        for name, x0, x1 in ELEVONS:
            g = Geo()
            g.loft([wing_ring(s, x, hinge_frac(x) + 0.02 / wing_c(x), 1.0, m=8) for x in stations(x0 + 0.012, x1 - 0.012, 0.3)], "skin")
            for xw in (lerp(x0, x1, 0.3), lerp(x0, x1, 0.8)):
                wick(g, V((xw * s, wing_te(xw), wing_zm(xw))))
            p0, p1 = wing_hinge(x0), wing_hinge(x1)
            g.finish(f"{side} {name} elevon", smooth=30, pivot=M(p0, s), axis=pair_axis((p1 - p0).normalized(), s))

        g = Geo()
        g.box(V(((W_X1 - 0.03) * s, wing_le(W_X1) - 0.25, wing_zm(W_X1))), (0.05, 0.10, 0.03), "nav_green" if s > 0 else "nav_red")
        g.finish(side + " nav light", smooth=10)


# ---------------------------------------------------------------- engines
# Two nacelles, each with a pair of Olympus 593s side by side: a box under the
# wing from the raked intake lips to where the secondary nozzles' buckets
# take over. Span stations of its sides, its top (in the wing) and bottom,
# the lip's station (at its bottom; the top is RAKE further forward), its end.
NAC_X = (4.08, 6.78)
NAC_Z = (-1.22, -2.62)
NAC_LIP = -7.6
NAC_RAKE = 0.55
NAC_END = -17.5
ENGINE_X = (4.755, 6.105)  # the engines' centrelines
ENGINE_Z = -1.92
NOZZLE_EXIT = -18.9
NOZZLE_RISE = 0.10  # how far the buckets' exits sit above the engines' centrelines: the underside sweeps up to them


def nacelles():
    xa, xb = NAC_X
    zt, zb = NAC_Z
    wall, split = 0.04, 0.08
    xm = (xa + xb) / 2
    cz = (zt + zb) / 2
    cells = ((xa + wall, xm - split / 2), (xm + split / 2, xb - wall))

    def rect(s, x0, x1, z0, z1, y, rake=0.0):
        return [V((x * s, y + rake * (z - zb) / (zt - zb), z)) for x, z in ((x0, z0), (x1, z0), (x1, z1), (x0, z1))]

    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        # The box.
        rings = [rect(s, xa, xb, zt, zb, NAC_LIP, NAC_RAKE), rect(s, xa, xb, zt, zb, -9.6), rect(s, xa, xb, zt, zb, -14.0), rect(s, xa + 0.02, xb - 0.02, zt - 0.02, zb + 0.08, -16.2), rect(s, xa + 0.06, xb - 0.06, zt - 0.05, zb + 0.20, NAC_END)]
        g.loft(rings, "skin", False, False)
        aim(g, 0, lambda c: c + V((c.x - xm * s, 0, c.z - cz)).normalized() * 20)
        g.poly(rings[-1], "skin", toward=V((xm * s, -60, cz)))
        # The lips round the two intakes.
        n0 = len(g.bm.faces)
        bars = [(xa, xb, zt, zt - wall), (xa, xb, zb + wall, zb), (xa, xa + wall, zt - wall, zb + wall), (xb - wall, xb, zt - wall, zb + wall), (xm - split / 2, xm + split / 2, zt - wall, zb + wall)]
        for x0, x1, z0, z1 in bars:
            g.poly(rect(s, x0, x1, z0, z1, NAC_LIP, NAC_RAKE), "skin")
        aim(g, n0, lambda c: c + V((0, 20, 0)))
        for (x0, x1), ex in zip(cells, ENGINE_X):
            # The duct running back into the dark toward the engine's face.
            n0 = len(g.bm.faces)
            duct = [rect(s, x0, x1, zt - wall, zb + wall, NAC_LIP, NAC_RAKE), rect(s, x0, x1, zt - wall, zb + wall, -9.4), rect(s, ex - 0.52, ex + 0.52, ENGINE_Z + 0.52, ENGINE_Z - 0.52, -11.8)]
            g.loft(duct, "inlet", False, False)
            aim(g, n0, lambda c, ex=ex: V((ex * s, c.y, ENGINE_Z)))
            g.poly(duct[-1], "inlet", toward=V((ex * s, 60, ENGINE_Z)))
            # The ramp hanging from the intake's roof.
            y0 = NAC_LIP + NAC_RAKE - 0.05
            g.poly([V((x0 * s, y0, zt - wall - 0.01)), V((x1 * s, y0, zt - wall - 0.01)), V((x1 * s, y0 - 2.0, zt - 0.50)), V((x0 * s, y0 - 2.0, zt - 0.50))], "ramp", toward=V((ex * s, 10, -60)))
        # The splitter between the two, standing ahead of the lips.
        yt = NAC_LIP + NAC_RAKE
        g.plate([V((xm * s, yt + 0.55, zt)), V((xm * s, NAC_LIP + 0.02, zb)), V((xm * s, NAC_LIP - 0.5, zb)), V((xm * s, yt - 0.5, zt))], V((1, 0, 0)), split / 2, "skin")
        g.finish(side + " intake", smooth=30)

        # The secondary nozzles: a pair of clamshell buckets round each jet pipe.
        def oval(ex, hw, hh, y, n=28):
            zc = ENGINE_Z + NOZZLE_RISE * smoothstep(NAC_END, NOZZLE_EXIT, y)
            out = []
            for k in range(n):
                a = 2 * math.pi * k / n
                c, sn = math.cos(a), math.sin(a)
                out.append(V((ex * s + hw * math.copysign(abs(c) ** 0.55, c), y, zc + hh * math.copysign(abs(sn) ** 0.55, sn))))
            return out

        g = Geo()
        for ex in ENGINE_X:
            axis = lambda c, ex=ex: V((ex * s, c.y, ENGINE_Z))
            n0 = len(g.bm.faces)
            g.loft([oval(ex, 0.64, 0.56, NAC_END + 0.05), oval(ex, 0.63, 0.55, NOZZLE_EXIT + 0.8), oval(ex, 0.56, 0.47, NOZZLE_EXIT)], "exhaust", False, False)
            aim(g, n0, lambda c, axis=axis: c + (c - axis(c)).normalized() * 20)
            n0 = len(g.bm.faces)
            g.loft([oval(ex, 0.56, 0.47, NOZZLE_EXIT), oval(ex, 0.52, 0.43, NOZZLE_EXIT)], "exhaust", False, False)
            aim(g, n0, lambda c: c + V((0, -20, 0)))
            n0 = len(g.bm.faces)
            g.loft([oval(ex, 0.52, 0.43, NOZZLE_EXIT), oval(ex, 0.50, 0.48, NOZZLE_EXIT + 0.6), oval(ex, 0.48, 0.48, NOZZLE_EXIT + 1.2)], "nozzle_in", False, False)
            aim(g, n0, axis)
            # The reheat's flame holder and the turbine's exit cone, deep inside.
            n0 = len(g.bm.faces)
            lathe(g, V((ex * s, NOZZLE_EXIT + 1.2, ENGINE_Z)), V((0, -1, 0)), [(0.50, 0.0), (0.40, 0.0), (0.39, 0.06), (0.34, 0.06), (0.33, 0.0), (0.22, 0.0), (0.21, 0.05), (0.16, 0.05), (0.13, 0.12), (0.0, 0.36)], "burner", n=24, caps=(False, False))
            aim(g, n0, lambda c: c + V((0, -20, 0)))
            # The split between the upper and lower bucket.
            for k in (-1, 1):
                g.box(V((ex * s + k * 0.60, NOZZLE_EXIT + 0.65, ENGINE_Z + NOZZLE_RISE / 2)), (0.012, 0.60, 0.022), "inlet")
        g.finish(side + " nozzle", smooth=30)


def plume():
    """The reheat, drawn in the game's transparent pass: shells and discs whose
    brightness plane.wgsl works out from the engines (material 17, `plume`).
    Haze first, then the flames, their cores and the discs: they are drawn in
    this order. One set behind each of the four engines, the Hornet's 1.4
    times the size."""
    g = Geo()
    aft = V((0, -1, 0))
    k = 1.4
    centres = [V((ex * s, NOZZLE_EXIT, ENGINE_Z + NOZZLE_RISE)) for s in (1, -1) for ex in ENGINE_X]

    def shell(prof, m, n, sub):
        rows = spline_rows(prof, 0, len(prof) - 1, sub)
        for c in centres:
            lathe(g, c, aft, [(max(r, 0.0) * k, a * k) for a, r in rows], m, n=n, caps=(False, False))

    # distance aft of the nozzle exit, radius
    shell([(-0.05, 0.36), (1.0, 0.52), (3.0, 0.66), (6.0, 0.74), (9.0, 0.64), (11.0, 0.34), (12.0, 0.0)], "haze", 16, 2)
    shell([(-0.14, 0.325), (0.4, 0.37), (1.2, 0.36), (2.2, 0.32), (3.4, 0.26), (4.8, 0.18), (6.0, 0.09), (6.9, 0.0)], "flame", 20, 3)
    shell([(-0.14, 0.26), (0.5, 0.25), (1.3, 0.21), (2.3, 0.155), (3.4, 0.09), (4.5, 0.0)], "core", 20, 3)
    # Discs across the jet: what is seen of the flame looking up it from behind, where the shells are edge on.
    for c in centres:
        for j in range(12):
            a = 0.12 + 5.2 * (j / 11) ** 1.3
            lathe(g, c, aft, [(0.0, a * k), (0.36 * (1.0 - 0.75 * a / 5.6) * k, a * k)], "disc", n=20, caps=(False, False))
    ob = g.finish("afterburner", smooth=80, recalc=False)
    ob.hide_render = True
    ob.display_type = "WIRE"


# ---------------------------------------------------------------- fin
# u from 0 at the root (in the fuselage's top) to 1 at the tip.
FIN_LE = (-12.5, -22.4)  # the leading edge's station at the root and the tip
FIN_C = (10.5, 2.2)  # chords
FIN_Z = (1.45, 6.6)  # heights
FIN_T = 0.03
# The rudder's two sections: from, to (fractions of the fin's height); the hinge line (of the chord).
RUDDERS = (("lower rudder", 0.05, 0.50), ("upper rudder", 0.52, 0.94))
RUDDER_HINGE = 0.78


def fin_sect(u):
    le = V((0, lerp(FIN_LE[0], FIN_LE[1], u), lerp(FIN_Z[0], FIN_Z[1], u)))
    return le, lerp(FIN_C[0], FIN_C[1], u), V((0, -1, 0)), V((1, 0, 0))


def fin():
    g = Geo()
    cuts = [-0.03] + [u for _, a, b in RUDDERS for u in (a, b)] + [1.0]
    for k, (u0, u1) in enumerate(zip(cuts, cuts[1:])):
        surface(g, fin_sect, u0, u1, 0.0, RUDDER_HINGE if k % 2 == 1 else 1.0, FIN_T, nu=max(2, round((u1 - u0) * 16)))
    g.finish("fin")
    for name, u0, u1 in RUDDERS:
        g = Geo()
        surface(g, fin_sect, u0 + 0.004, u1 - 0.004, RUDDER_HINGE + 0.004, 1.0, FIN_T, nu=6)
        p0, p1 = hinge(fin_sect, u0, RUDDER_HINGE), hinge(fin_sect, u1, RUDDER_HINGE)
        # (Both sections swing the same way for the same command.)
        g.finish(name, pivot=p0, axis=(p1 - p0).normalized())


# ---------------------------------------------------------------- landing gear (modelled down and locked)
# The nose leg swings forward into its well under the forward cabin. Each
# main leg hangs from the wing beside the nacelle and swings inward, its
# lower part (the "main strut") sliding up into it on the way as the real
# one's does (the legs are long, to keep the nozzles off the runway at 11.5
# degrees nose up, and the bays no wider than the fuselage), its
# four-wheel bogie ending up lying flat in the belly; the leg lies in a trench
# under the wing, behind a door it carries, and the bay has a door of its own.
GROUND = FIN_Z[1] - 12.2  # the tyres' contact, fully extended: the fin's tip is 12.2 m above it
NOSE_R = 0.40
NOSE_PIVOT = V((0, 15.3, -1.30))
NOSE_AXLE = V((0, 15.3, GROUND + NOSE_R))
NOSE_STOWED_Z = -1.10  # height of the nose axle with the gear up
MAIN_R = 0.60
MAIN_HW = 0.17
MAIN_PIVOT = V((3.86, -2.6, -1.18))
MAIN_BOGIE = V((3.86, -2.6, GROUND + MAIN_R))  # the middle of the bogie's beam
BOGIE_HALF = 0.83  # the axles, fore and aft of it
WHEEL_X = 0.30  # the wheels, either side of it
MAIN_SWING = math.radians(97.0)
MAIN_SHORTEN = 0.60  # how far the leg's lower part slides up into it as it retracts, as the real one's does: the bays are no wider than the fuselage
LEG_DOOR = (1.42, 3.78, -2.32, -2.88)


def main_stowed(p):
    """Where point p of the (right) main gear's lower part ends up with the gear up."""
    return MAIN_PIVOT + Matrix.Rotation(MAIN_SWING, 3, V((0, 1, 0))) @ (p + V((0, 0, MAIN_SHORTEN)) - MAIN_PIVOT)


def skin_patch(g, s, x0, x1, y0, y1, zf, drop, t, nx, ny):
    """A patch of the underside's skin (x0..x1 out from the centreline on side
    s, stations y0 back to y1, at height zf(x, y) less `drop`), t thick: a
    door, shut. Returns its vertices."""
    n0 = len(g.bm.faces)
    xe = x1 if callable(x1) else (lambda y: x1)
    ys = [lerp(y0, y1, k / ny) for k in range(ny + 1)]
    lo = [[g.bm.verts.new(V((s * x, y, zf(x, y) - drop))) for x in (lerp(x0, xe(y), (1 - math.cos(math.pi * k / nx)) / 2) for k in range(nx + 1))] for y in ys]
    hi = [[g.bm.verts.new(v.co + V((0, 0, t))) for v in row] for row in lo]
    for j in range(ny):
        for i in range(nx):
            g.face([lo[j][i], lo[j][i + 1], lo[j + 1][i + 1], lo[j + 1][i]], "skin")
            g.face([hi[j][i], hi[j + 1][i], hi[j + 1][i + 1], hi[j][i + 1]], "door_in")
    rim = [(j, 0) for j in range(ny)] + [(ny, i) for i in range(nx)] + [(j, nx) for j in range(ny, 0, -1)] + [(0, i) for i in range(nx, 0, -1)]
    for (ja, ia), (jb, ib) in zip(rim, rim[1:] + rim[:1]):
        g.face([lo[ja][ia], lo[jb][ib], hi[jb][ib], hi[ja][ia]], "door_in")
    if s < 0:
        g.bm.faces.ensure_lookup_table()
        bmesh.ops.reverse_faces(g.bm, faces=g.bm.faces[n0:])  # mirrored: wound the other way
    return [v for row in lo + hi for v in row]


def patch_door(name, s, x0, x1, y0, y1, hinge_x, open_angle, zf, nx=10, ny=12, t=0.02):
    """A gear door cut from the skin itself, so that shut it lies flush in its
    opening; hinged along the line x = hinge_x. Modelled open: it turns by
    open_angle about the hinge, its free edge going down."""
    g = Geo()
    vs = skin_patch(g, s, x0, x1, y0, y1, zf, 0.002, t, nx, ny)
    h0 = V((s * hinge_x, y0, zf(hinge_x, y0)))
    h1 = V((s * hinge_x, y1, zf(hinge_x, y1)))
    axis = (h1 - h0).normalized()
    xf = x0 + x1 - hinge_x
    free = V((s * xf, (y0 + y1) / 2, zf(xf, (y0 + y1) / 2)))
    rot = Matrix.Translation(h0) @ Matrix.Rotation(open_angle, 4, axis) @ Matrix.Translation(-h0)
    if (rot @ free).z > free.z:
        axis = -axis
        rot = Matrix.Translation(h0) @ Matrix.Rotation(open_angle, 4, axis) @ Matrix.Translation(-h0)
    for v in vs:
        v.co = rot @ v.co
    # A hinge bracket or two along the hinge line, inside.
    for f in (0.2, 0.8):
        g.box(h0.lerp(h1, f) + V((0, 0, 0.04)), (0.03, 0.05, 0.04), "well")
    g.finish(name, smooth=30, pivot=h0, axis=axis, angle=-open_angle, recalc=False)


def gear():
    # Nose: swings forward into the well under the forward cabin.
    l = (NOSE_AXLE - NOSE_PIVOT).length
    dz = NOSE_STOWED_Z - NOSE_PIVOT.z
    ax, ang = stow(NOSE_PIVOT, NOSE_AXLE, NOSE_PIVOT + V((0, math.sqrt(l * l - dz * dz), dz)))
    along = (NOSE_PIVOT - NOSE_AXLE).normalized()
    low = NOSE_AXLE + along * 0.95
    g = Geo()
    # The trunnion across the top, the strut, the steering collar.
    tube(g, [NOSE_PIVOT + V((-0.30, 0, 0)), NOSE_PIVOT + V((0.30, 0, 0))], 0.07, "strut", n=10)
    tube(g, [NOSE_PIVOT, low], 0.10, "strut", n=14)
    lathe(g, low + along * 0.12, -along, [(0.0, 0.0), (0.135, 0.0), (0.135, 0.16), (0.0, 0.16)], "strut", n=14)
    # Landing and taxi lights on a bracket ahead of the strut.
    g.box(NOSE_PIVOT - along * 1.30 + V((0, 0.14, 0)), (0.16, 0.04, 0.06), "strut")
    for x in (-0.09, 0.09):
        g.box(NOSE_PIVOT - along * 1.30 + V((x, 0.185, 0)), (0.055, 0.005, 0.045), "landing")
    # Hydraulic lines down the back of the strut.
    for x in (-0.04, 0.04):
        tube(g, [NOSE_PIVOT + V((x, -0.105, 0)) - along * 0.05, low + V((x, -0.11, 0))], 0.011, "steel", n=5)
    g.finish("nose leg", pivot=NOSE_PIVOT, axis=ax, angle=ang)

    down = (NOSE_AXLE - NOSE_PIVOT).normalized()
    g = Geo()
    tube(g, [low + along * 0.10, NOSE_AXLE], 0.075, "chrome", n=12)
    g.box(NOSE_AXLE + along * 0.05, (0.08, 0.07, 0.08), "strut")
    tube(g, [NOSE_AXLE + V((-0.34, 0, 0)), NOSE_AXLE + V((0.34, 0, 0))], 0.045, "strut")
    scissors(g, low + V((0, -0.09, 0)), NOSE_AXLE + along * 0.08 + V((0, -0.07, 0)), V((0, -0.22, 0)))
    g.finish("nose fork", pivot=NOSE_AXLE, axis=down, parent="nose leg")

    g = Geo()
    for x in (-0.22, 0.22):
        wheel(g, NOSE_AXLE + V((x, 0, 0)), NOSE_R, 0.115)
    g.finish("nose wheel", pivot=NOSE_AXLE, axis=V((1, 0, 0)), parent="nose fork")

    # Mains.
    knee = MAIN_BOGIE + V((0, 0, 0.14))
    mid = MAIN_PIVOT.lerp(knee, 0.50)
    unstow = Matrix.Translation(MAIN_PIVOT) @ Matrix.Rotation(-MAIN_SWING, 4, V((0, 1, 0))) @ Matrix.Translation(-MAIN_PIVOT)
    axles = (MAIN_BOGIE + V((0, BOGIE_HALF, 0)), MAIN_BOGIE - V((0, BOGIE_HALF, 0)))
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        # The trunnion it swings on, the strut's outer cylinder and its oleo.
        tube(g, [M(MAIN_PIVOT + V((0, 0.30, 0)), s), M(MAIN_PIVOT + V((0, -0.30, 0)), s)], 0.075, "strut", n=10)
        tube(g, [M(MAIN_PIVOT, s), M(mid, s)], 0.115, "strut", n=14)
        lathe(g, M(mid, s), V((0, 0, -1)), [(0.0, 0.0), (0.145, 0.0), (0.145, 0.10), (0.0, 0.10)], "strut", n=14)
        for y in (-0.05, 0.05):
            tube(g, [M(MAIN_PIVOT + V((-0.12, y, -0.1)), s), M(mid + V((-0.12, y, 0.1)), s)], 0.011, "steel", n=5)
        # The leg's door: a patch of the wing's lower skin, laid out shut and
        # swung down with the leg.
        x0, x1, y0, y1 = LEG_DOOR
        for v in skin_patch(g, 1, x0, x1, y0, y1, lambda x, y: wing_z(x, y, False), 0.004, 0.02, 12, 3):
            v.co = M(unstow @ v.co, s)
        g.finish(side + " main leg", pivot=M(MAIN_PIVOT, s), axis=pair_axis(V((0, 1, 0)), s), angle=MAIN_SWING)

        # The leg's lower part: the oleo's piston, which the bogie hangs from.
        # It slides up into the leg as the leg swings up (`angle`: how far, m;
        # `axis`: which way).
        g = Geo()
        tube(g, [M(mid + V((0, 0, 0.65)), s), M(knee, s)], 0.085, "chrome", n=12)
        scissors(g, M(mid + V((0, 0.12, -0.15)), s), M(knee + V((0, 0.10, 0.05)), s), M(V((0, 0.22, 0)), s))
        g.finish(side + " main strut", pivot=M(MAIN_PIVOT, s), axis=V((0, 0, 1)), angle=MAIN_SHORTEN, parent=side + " main leg")

        # The bogie: a beam with an axle at each end, pivoted under the leg.
        g = Geo()
        g.box(M(knee - V((0, 0, 0.06)), s), (0.10, 0.12, 0.10), "strut")
        tube(g, [M(axles[0], s), M(axles[1], s)], 0.08, "strut", n=12)
        for a in axles:
            tube(g, [M(a + V((-WHEEL_X - 0.05, 0, 0)), s), M(a + V((WHEEL_X + 0.05, 0, 0)), s)], 0.055, "steel", n=10)
            for k in (-1, 1):
                lathe(g, M(a + V((k * (WHEEL_X - MAIN_HW - 0.035), 0, 0)), s), V((k * s, 0, 0)), [(0.0, 0.0), (0.26, 0.0), (0.26, 0.05), (0.0, 0.05)], "steel", n=16)
        g.finish(side + " main bogie", pivot=M(MAIN_BOGIE, s), axis=V((1, 0, 0)), parent=side + " main strut")

        for a, name in zip(axles, ("main wheel", "rear main wheel")):
            g = Geo()
            for k in (-1, 1):
                wheel(g, M(a + V((k * WHEEL_X, 0, 0)), s), MAIN_R, MAIN_HW)
            g.finish(f"{side} {name}", pivot=M(a, s), axis=V((1, 0, 0)), parent=side + " main bogie")
    print(f"main gear: swing {math.degrees(MAIN_SWING):.0f} deg, bogie stowed at {tuple(round(c, 3) for c in main_stowed(MAIN_BOGIE))}; ground at z = {GROUND:.2f}")


def doors():
    hw, ny0, ny1, _ = NOSE_WELL
    bx0, bx1, by0, by1, _ = WHEEL_BAY
    gap = 0.008
    for s, side in ((1, "right"), (-1, "left")):
        # Nose: a pair meeting on the centreline, hinged at the well's sides.
        patch_door(side + " nose door", s, gap, hw - gap, ny1 - gap, ny0 + gap, hw, math.radians(86), lambda x, y: fuse_z(y, x, False), ny=14)
        # Main: over the wheel bay, hinged along its inboard edge.
        patch_door(side + " main door", s, bx0 + gap, bx1 - gap, by0 - gap, by1 + gap, bx0, math.radians(88), belly_z, nx=14, ny=12)


def check_gear():
    """Sweep the nose gear from up to down (as Rust `anim` moves it: the leg
    over 0.15..0.85) and report any vertex inside the fuselage's skin but
    outside its well, or, stowed, not tucked in behind the doors; and, stowed,
    any vertex of the main gear outside the wing and the fuselage. Run on
    every export: a hand edit that makes the gear poke through the skin shows
    up here."""
    obs = {o.name: o for o in COLL.all_objects}

    def local(n, g):
        o = obs[n]
        a = o.get("angle", 0.0)
        if n.endswith("main strut"):
            return Matrix.Translation(V(o["axis"]) * a * (1 - smoothstep(0.15, 0.85, g)))
        if abs(a) < 1e-6 or not n.endswith(" leg"):
            return Matrix.Identity(4)
        a *= 1 - smoothstep(0.15, 0.85, g)
        p = V(o["pivot"])
        return Matrix.Translation(p) @ Matrix.Rotation(a, 4, V(o["axis"])) @ Matrix.Translation(-p)

    def world(n, g):
        par = obs[n].get("parent")
        return (world(par, g) if par else Matrix.Identity(4)) @ local(n, g)

    def in_well(p):
        hw, y0, y1, zt = NOSE_WELL
        return abs(p.x) < hw and y0 < p.y < y1 and p.z < zt

    gear = [n for n in obs if "door" not in n and any(k in n for k in ("leg", "strut", "wheel", "fork", "bogie"))]
    bad = 0
    for k in range(21):
        g = k / 20
        for n in gear:
            m = world(n, g) @ obs[n].matrix_world
            worst, at = 0.0, None
            for v in obs[n].data.vertices:
                p = m @ v.co
                d = fuse_depth(p)
                e = 0.0
                if "main" in n:
                    if g == 0.0 and d < -0.01 and not in_wing(p, -0.01):
                        e = 0.02 if d < -0.5 else -d  # stowed: inside the fuselage or the wing (the leg's door is the wing's skin)
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
# Every piece the game looks for (PIECES in src/aircraft/concorde/mod.rs); other objects are ignored.
EXPECTED = ["nose", "visor", "visor frame", "nose section", "fuselage", "tail cone", "canopy", "canopy frame", "fin", "lower rudder", "upper rudder", "tail light", "nose leg", "nose fork", "nose wheel", "afterburner"] + [
    f"{s} {n}" for s in ("left", "right") for n in ("wing", "inner elevon", "middle elevon", "outer elevon", "intake", "nozzle", "nav light", "main leg", "main strut", "main bogie", "main wheel", "rear main wheel", "nose door", "main door")]


def game_mat(m):
    """Which game material a Blender material is: its "game" property, its concorde_ name, or else the skin."""
    if m is None:
        return "skin"
    name = m.get("game") or (m.name[9:].split(".")[0] if m.name.startswith("concorde_") else None)
    return name if name in MATS else "skin"


def export(path):
    out = bytearray(b"FFAC")
    obs = [o for o in COLL.all_objects if o.type == "MESH"]
    names = {o.name for o in obs}
    missing = [n for n in EXPECTED if n not in names]
    if missing:
        raise SystemExit(f"concorde export: the game needs these pieces, missing from the '{COLL.name}' collection: {', '.join(missing)}")
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
        volume = 0.0
        for tri in me.loop_triangles:
            mat = game_mat(me.materials[tri.material_index] if tri.material_index < len(me.materials) else None)
            gid, pat, col, extra = MATS[mat]
            a, b, c = (mw @ me.vertices[i].co for i in tri.vertices)
            volume += a.dot(b.cross(c)) / 6
            for k in range(3):
                vi = tri.vertices[k]
                n = (nm @ V(tri.split_normals[k])).normalized()
                key = (vi, round(n.x, 3), round(n.y, 3), round(n.z, 3), mat)
                if key not in idx:
                    p = mw @ me.vertices[vi].co
                    idx[key] = len(verts)
                    verts.append((*game(p), *game(n), *col, float(gid), float(pat), 0.3, float(extra)))
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
        # (The signed volume: negative for a closed piece wound inside out.)
        print(f"  {ob.name}: {len(verts)} vertices, {len(index) // 3} triangles, volume {volume:.3f}")
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
    cam_data.clip_end = 1000.0
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
    target = V((0, 2, 0.5))
    views = (("three_quarter", V((48, 62, 24)), 50, target), ("side", V((98, 2.0, 1.0)), 50, target), ("top", V((0.01, 2.2, 150)), 50, target), ("front", V((0.01, 84, 2.0)), 85, V((0, 0, 0))), ("rear_low", V((-34, -52, -6)), 50, target),
             ("rear", V((14, -50, 8)), 60, V((0, -16, 0.5))), ("nozzles", V((10, -29, -0.5)), 70, V((4.5, -19, -1.6))), ("cockpit", V((7.5, 36, 4.5)), 70, V((0, 25, 0.2))), ("under", V((26, 30, -14)), 50, V((0, 0, -1.0))),
             ("gear", V((16, 14, -3.2)), 50, V((1.5, 0, -2.6))))

    def shoot(name, eye, lens, look):
        cam.location = eye
        cam.rotation_euler = (look - eye).to_track_quat("-Z", "Y").to_euler()
        cam_data.lens = lens
        sc.render.filepath = os.path.join(outdir, name + ".png")
        bpy.ops.render.render(write_still=True)

    for v in views:
        shoot(*v)
    # The nose drooped for landing, the visor lowered into it.
    obs = {o.name: o for o in COLL.all_objects}
    nose = obs["nose"]
    p = V(nose["pivot"])
    droop = Matrix.Translation(p) @ Matrix.Rotation(nose["angle"], 4, V(nose["axis"])) @ Matrix.Translation(-p)
    nose.matrix_world = droop
    for n in ("visor", "visor frame"):
        obs[n].matrix_world = droop @ Matrix.Translation(V(obs[n]["axis"]) * obs[n]["angle"])
    shoot("drooped_side", V((30, 26, 2.0)), 50, V((0, 24, -0.2)))
    shoot("drooped_cockpit", V((7.5, 36, 4.5)), 70, V((0, 25, 0.2)))
    for n in ("nose", "visor", "visor frame"):
        obs[n].matrix_world = Matrix.Identity(4)


# The paint is projected straight down, straight up and from each side: these
# are the rectangles (m) those pictures cover (2:3, as the other painted
# aircraft's, so the pictures are 1024 x 1536). plane.wgsl must be given the same numbers.
GUIDE_SPAN = 44.0   # across the top and bottom views; up the side views
GUIDE_LEN = 66.0    # fore and aft in all of them
GUIDE_Y = 2.17      # station at the middle of the pictures
GUIDE_Z = 1.0       # height at the middle of the side views
GUIDE_PX = 1536     # pixels fore and aft
GUIDE_EYE = 120.0   # how far off the cameras stand


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
    hidden = [o for o in COLL.all_objects if any(k in o.name for k in ("leg", "wheel", "fork", "bogie", "door", "afterburner"))]
    for o in hidden:
        o.hide_render = True
    cam, cam_data = camera()
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = GUIDE_LEN
    short = round(GUIDE_PX * GUIDE_SPAN / GUIDE_LEN)
    # (The left side is seen nose to the left, the right side nose to the right.)
    views = (("top", V((0, GUIDE_Y, GUIDE_EYE)), V((0, 0, -1)), V((0, 1, 0)), (short, GUIDE_PX)),
             ("bottom", V((0, GUIDE_Y, -GUIDE_EYE)), V((0, 0, 1)), V((0, 1, 0)), (short, GUIDE_PX)),
             ("side", V((-GUIDE_EYE, GUIDE_Y, GUIDE_Z)), V((1, 0, 0)), V((0, 0, 1)), (GUIDE_PX, short)),
             ("right", V((GUIDE_EYE, GUIDE_Y, GUIDE_Z)), V((-1, 0, 0)), V((0, 0, 1)), (GUIDE_PX, short)))
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
    """What each pixel of a guide is a picture of, for a paint script that
    paints by where on the aircraft a pixel is (as assets/blender/f4_paint.py):
    <view>_geo.npz with, for each pixel, the point of the model seen there
    (`depth`: its height seen from above or below, its x seen from a side, NaN
    off the model; the picture's own grid gives the rest), the surface's normal
    (`nrm`), and which piece and material it is (`piece`, `mat`: indices into
    `pieces` and `mats`). Rays are cast through the pixel centres of the renders above."""
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
        pos = np.full((h, w, 3), np.nan, np.float32)
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
        np.savez_compressed(os.path.join(outdir, name + "_geo.npz"), depth=pos[..., 2 if abs(look.z) > 0.5 else 0].astype(np.float16), nrm=nrm, piece=piece, mat=mat, pieces=np.array([o.name for o in obs]), mats=np.array(mats))
        print(f"  {name}: geometry pass, {int((piece < 255).sum())} pixels on the model")


def main():
    global COLL, CUTS
    if EXPORT_ONLY:
        # Export the .blend that is open, edits and all, without rebuilding anything.
        COLL = bpy.data.collections.get("Concorde")
        if COLL is None:
            raise SystemExit("concorde export: no 'Concorde' collection in this file (open assets/blender/concorde.blend)")
        export(os.path.join(ASSETS, "concorde.bin.gz"))
        check_gear()
    else:
        for ob in list(bpy.data.objects):
            bpy.data.objects.remove(ob, do_unlink=True)
        for c in list(bpy.data.collections):
            bpy.data.collections.remove(c)
        COLL = bpy.data.collections.new("Concorde")
        bpy.context.scene.collection.children.link(COLL)
        CUTS = bpy.data.collections.new("Concorde well cutters")
        bpy.context.scene.collection.children.link(CUTS)
        make_materials()
        fuselage()
        wings()
        nacelles()
        fin()
        gear()
        doors()
        plume()
        os.makedirs(os.path.join(ASSETS, "blender"), exist_ok=True)
        bpy.context.preferences.filepaths.save_version = 0  # no concorde.blend1 beside it
        export(os.path.join(ASSETS, "concorde.bin.gz"))
        check_gear()
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ASSETS, "blender", "concorde.blend"))
    if RENDER:
        render_previews(RENDER)
    if GUIDES:
        render_guides(GUIDES)


main()
