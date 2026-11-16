# SpaceX Starship for FableFlight, built procedurally in Blender: the whole
# stack, the Super Heavy booster with the ship on top.
#
#   blender -b --factory-startup --python assets/blender/starship.py -- <repo>/assets [--render <dir>]
#
# Builds both stages from lathed sections, one object per piece that moves or
# comes away on its own, saves it as assets/blender/starship.blend and exports
# assets/starship.bin.gz (the same file format as the Hornet's, see fa18.py).
# That rebuild starts from scratch, so it throws away anything edited by hand
# in the .blend.
#
# To keep hand edits, export the .blend as it stands instead (save it first):
#
#   blender -b assets/blender/starship.blend --python assets/blender/starship.py -- <repo>/assets --export-only
#
# Keep the piece names and the "Starship" collection: the game finds pieces by
# name (see EXPECTED below). Every piece of the booster is called "booster
# ...", every piece of the ship "ship ...", so the game can let the booster go
# at staging. Each object records where its hinge is in its own frame (custom
# properties "pivot", "axis", "angle").
#
# There is no paint picture: the finish is the materials below (bare stainless
# steel in rings of slightly different tone, black heat-shield tiles on the
# ship's windward half), carried as vertex colours.
#
# --render writes preview pictures.
#
# Blender frame here: +Y up the stack (toward the nose), +X right, +Z the
# ship's leeward (bare steel) side, so its heat shield faces -Z; the origin is
# on the centreline 48 m above the booster's engines' exit plane, metres. The
# game's body frame is +X right, +Y up, -Z forward, so a point (x, y, z) here
# is (x, z, -y) there: the nose is forward, the heat shield is the belly.
import bpy, bmesh, math, struct, gzip, sys, os
from mathutils import Vector as V, Matrix

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
ASSETS = os.path.abspath(argv[0]) if argv else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RENDER = argv[argv.index("--render") + 1] if "--render" in argv else None
EXPORT_ONLY = "--export-only" in argv


def lerp(a, b, t):
    return a + (b - a) * t


def smoothstep(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3.0 - 2.0 * t)


def hash01(k):
    """A number in 0..1 from an integer: the same every build."""
    x = math.sin(k * 127.1 + 311.7) * 43758.5453
    return x - math.floor(x)


def game(v):
    return (v.x, v.z, -v.y)


# ---------------------------------------------------------------- materials
# name: (game material id, surface pattern, linear colour, extra); ids as airframe::mat.
# The skin is bare stainless steel: game material 2 (bare metal, whose
# roughness and tone the shader varies finely, as brushed sheet does), each
# ring of the barrel a slightly different tone, the welds between them darker.
# Material 11 (exhaust) extra: 0 the engine bells outside, 1 inside them, 2 the
# injector face; material 17 (the exhaust plume, drawn in the transparent pass)
# extra: 0 heat haze, 1 the flame, 2 its core, 3 discs across it.
MATS = {
    "steel_a": (2, 0, (0.60, 0.61, 0.62), 0),
    "steel_b": (2, 0, (0.54, 0.55, 0.57), 0),
    "steel_c": (2, 0, (0.66, 0.66, 0.66), 0),
    "steel_d": (2, 0, (0.49, 0.50, 0.52), 0),  # the payload door, the raceways
    "weld": (2, 0, (0.30, 0.30, 0.31), 0),
    "heat": (3, 0, (0.33, 0.30, 0.27), 2),  # heat-tinted: the hot-staging ring's shield, the engine bays
    "grid": (3, 0, (0.40, 0.40, 0.41), 0),  # the grid fins' lattice
    "hardware": (3, 0, (0.22, 0.22, 0.23), 0),  # the engines' powerheads
    "tile": (10, 0, (0.020, 0.020, 0.022), 0),  # heat-shield tiles
    "tile_b": (10, 0, (0.030, 0.030, 0.032), 0),
    "dark": (10, 0, (0.03, 0.03, 0.03), 0),
    "bell": (11, 0, (0.30, 0.28, 0.26), 0),
    "bell_in": (11, 0, (0.06, 0.055, 0.05), 1),
    "injector": (11, 0, (0.10, 0.09, 0.08), 2),
    "haze": (17, 0, (1.0, 1.0, 1.0), 0),
    "flame": (17, 0, (1.0, 0.6, 0.3), 1),
    "core": (17, 0, (0.7, 0.8, 1.0), 2),
    "disc": (17, 0, (1.0, 0.8, 0.6), 3),
}

MAT = {}


def make_materials():
    for name, (gid, _pat, col, _x) in MATS.items():
        m = bpy.data.materials.get("starship_" + name) or bpy.data.materials.new("starship_" + name)
        srgb = tuple(max(c, 0.0) ** (1 / 2.2) for c in col)
        m.diffuse_color = (*srgb, 0.3 if gid == 17 else 1.0)
        m["game"] = name
        MAT[name] = m


# ---------------------------------------------------------------- mesh building
class Geo:
    def __init__(self):
        self.bm = bmesh.new()
        self.mats = []
        self.facing = []  # (face, point it must face): surfaces whose outside cannot be worked out

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

    def loft(self, rings, m, cap0=True, cap1=True, cap_mat=None):
        """Join closed rings of equal length; a ring that is a single point closes to it."""
        vr = []
        for r in rings:
            if max((p - r[0]).length for p in r) < 1e-5:
                vr.append([self.bm.verts.new(r[0])])
            else:
                vr.append([self.bm.verts.new(p) for p in r])
        n = len(rings[0])
        for a, b in zip(vr, vr[1:]):
            for i in range(n):
                j = (i + 1) % n
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

    def box(self, c, h, m, basis=None):
        ax, ay, az = basis or (V((1, 0, 0)), V((0, 1, 0)), V((0, 0, 1)))
        vs = [self.bm.verts.new(c + ax * (h[0] * sx) + ay * (h[1] * sy) + az * (h[2] * sz)) for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)]
        for q in ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)):
            self.face([vs[k] for k in q], m)

    def plate(self, outline, normal, t, m, m_back=None, m_edge=None):
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
            self.face([a[i], b[i], b[j], a[j]], m_edge or m)

    def finish(self, name, smooth=38.0, pivot=None, axis=None, angle=0.0, recalc=True, parent=None):
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
        if parent:
            ob["parent"] = parent
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


def rev(g, c, axis, prof, m, n=24, u=None, w=None):
    """Revolve the profile (radius, distance along the axis) about the axis
    through c. Which way the surface faces follows from the way the profile
    runs: outward when it goes up the axis, so trace an outside from the near
    end to the far end and an inside the other way; a step in radius faces the
    far end when it runs inward. `m`: one finish, or a function of (segment of
    the profile, segment round the ring, the angle there, the distance along
    the axis there)."""
    axis = axis.normalized()
    if u is None:
        u, w = frame_at(axis)
    rows = []
    for r, a in prof:
        if r < 1e-6:
            rows.append([g.bm.verts.new(c + axis * a)])
        else:
            rows.append([g.bm.verts.new(c + axis * a + (u * math.cos(t) + w * math.sin(t)) * r) for t in (2 * math.pi * k / n for k in range(n))])
    for s, ((r0, a0), (r1, a1)) in enumerate(zip(prof, prof[1:])):
        ra, rb = rows[s], rows[s + 1]
        if len(ra) == 1 and len(rb) == 1:
            continue
        dr, da = r1 - r0, a1 - a0
        l = math.hypot(dr, da) or 1.0
        nr, na = da / l, -dr / l
        for i in range(n):
            j = (i + 1) % n
            t = 2 * math.pi * (i + 0.5) / n
            rad = u * math.cos(t) + w * math.sin(t)
            mid = c + axis * ((a0 + a1) / 2) + rad * ((r0 + r1) / 2)
            mm = m(s, i, t, (a0 + a1) / 2) if callable(m) else m
            toward = mid + (rad * nr + axis * na) * 50.0
            if len(ra) == 1:
                g.face([ra[0], rb[i], rb[j]], mm, toward)
            elif len(rb) == 1:
                g.face([ra[i], ra[j], rb[0]], mm, toward)
            else:
                g.face([ra[i], ra[j], rb[j], rb[i]], mm, toward)


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


# ---------------------------------------------------------------- the stack
UP = V((0, 1, 0))
X, Z = V((1, 0, 0)), V((0, 0, 1))
R = 4.5            # both stages are 9 m across
N = 96             # sides round the barrel
RING = 1.8         # height of one ring of the barrel, weld to weld
WELD = 0.03        # half width of a weld line

# Stations up the stack (y).
B_EXIT = -48.0     # the booster's engines' exit plane
B_SKIRT = -47.2    # bottom of its skirt
B_PLATE = -45.4    # the thrust structure the engines hang from
B_TOP = 21.2       # top of the booster's tanks: the hot-staging ring stands on it
HSR_TOP = 23.0     # top of the hot-staging ring: where the ship sits
S_EXIT = 23.2      # the ship's engines' exit plane (inside the ring until staging)
S_SKIRT = 23.0     # bottom of the ship's skirt
S_PLATE = 26.8     # the ship's thrust structure
S_NOSE = 55.0      # where the ship's barrel becomes the nose
S_TIP = 73.0       # the nose's tip
OGIVE_L = S_TIP - S_NOSE
OGIVE_RHO = (R * R + OGIVE_L * OGIVE_L) / (2 * R)
TILE_EDGE = 0.10   # the heat shield covers the ship where sin(angle from +X) is below this: a little over half
DOOR = (50.0, 57.2, math.radians(62), math.radians(118))  # the payload door: stations, angles round the leeward side


def ship_r(y):
    """Radius of the ship at station y."""
    if y <= S_NOSE:
        return R
    x = min(y - S_NOSE, OGIVE_L)
    return max(math.sqrt(OGIVE_RHO * OGIVE_RHO - x * x) - (OGIVE_RHO - R), 0.0)


def barrel(y0, y1, radius=lambda y: R, step=RING):
    """The profile of a barrel from y0 to y1: (radius, y) rows, a narrow pair
    at every weld; and which rows start a weld line."""
    prof, welds = [(radius(y0), y0)], set()
    y = y0 + step
    while y < y1 - 0.3:
        welds.add(len(prof))
        prof += [(radius(y - WELD), y - WELD), (radius(y + WELD), y + WELD)]
        y += step
    prof.append((radius(y1), y1))
    return prof, welds


def steel(k):
    """The tone of ring k of a barrel."""
    return ("steel_a", "steel_b", "steel_c", "steel_a", "steel_b")[int(hash01(k) * 5) % 5]


# ---------------------------------------------------------------- engines
# Raptors: a powerhead, the throat, the bell. Sea-level ones on both stages,
# three vacuum ones with their big bells round the ship's.
RAPTOR_R = 0.60    # a sea-level bell's exit radius
RAPTOR_L = B_PLATE - B_EXIT
RVAC_R = 1.20
RVAC_L = S_PLATE - S_EXIT
B_RINGS = ((3, 0.78, 0.0), (10, 2.20, 0.0), (20, 3.88, 0.0))  # the booster's 33: how many, on what radius, turned by
S_SEA = (3, 0.85, math.radians(90))                              # the ship's three sea-level engines
S_VAC = (3, 2.95, math.radians(30))                              # and three vacuum ones between them


def raptor(g, c, r_exit, length, n=14):
    """An engine hanging from c, firing down the stack."""
    down = -UP
    k = r_exit / RAPTOR_R
    s = min(k, 1.25)
    L = length
    rev(g, c, down, [(0.0, 0.0), (0.40 * s, 0.0), (0.40 * s, 0.26 * L), (0.17 * s, 0.34 * L)], "hardware", n)
    bell = [(0.28, 0.34), (0.56, 0.45), (0.78, 0.60), (0.92, 0.78), (1.0, 1.0)]  # of the exit radius, of the length
    rev(g, c, down, [(0.17 * s, 0.34 * L)] + [(r_exit * r, a * L) for r, a in bell[1:]], "bell", n)
    rev(g, c, down, [(r_exit * r * 0.97, a * L) for r, a in bell[:0:-1]] + [(0.13 * s, 0.36 * L), (0.0, 0.36 * L)],
        lambda seg, *_: "injector" if seg == len(bell) - 1 else "bell_in", n)


def ring_of(count, radius, turn, y):
    return [V((radius * math.cos(turn + 2 * math.pi * k / count), y, radius * math.sin(turn + 2 * math.pi * k / count))) for k in range(count)]


# ---------------------------------------------------------------- Super Heavy
GRID_Y = 19.0                     # the grid fins' station
GRID_AT = (60.0, 120.0, 240.0, 300.0)  # round the booster (degrees from +X toward +Z): two pairs, either side of the catch pins
GRID_LEN = 4.3                    # out from the skin
GRID_HW = (1.6, 1.15)             # half width at the root, at the tip
GRID_D = 0.24                     # half depth of the lattice
CHINE_AT = (45.0, 135.0, 225.0, 315.0)
CHINE_Y = (-46.9, -31.0)
PIN_Y = 17.4                      # the catch pins the tower's arms take it by


def booster():
    g = Geo()
    prof, welds = barrel(B_SKIRT, B_TOP)
    prof = [(r, y - B_SKIRT) for r, y in prof]
    rev(g, V((0, B_SKIRT, 0)), UP, prof, lambda s, i, t, a: "weld" if s in welds else steel(s // 2), N, X, Z)
    # Inside the skirt, up to the thrust structure the engines hang from.
    rev(g, V((0, B_SKIRT, 0)), UP, [(0.0, B_PLATE - B_SKIRT), (R - 0.08, B_PLATE - B_SKIRT), (R - 0.08, 0.0), (R, 0.0)],
        lambda s, *_: "dark" if s == 0 else "heat", N, X, Z)
    # The forward dome, under the hot-staging ring.
    rev(g, V((0, B_TOP, 0)), UP, [(R, 0.0), (3.9, 0.35), (2.6, 0.75), (1.2, 0.95), (0.0, 1.0)], "heat", N // 2, X, Z)
    # Stringers stiffening the engine section and the top of the tanks.
    for y0, y1, count in ((B_SKIRT + 0.1, -41.0, 72), (13.6, B_TOP - 0.1, 72)):
        for k in range(count):
            a = 2 * math.pi * (k + 0.5) / count
            d = V((math.cos(a), 0, math.sin(a)))
            g.box(V((0, (y0 + y1) / 2, 0)) + d * (R + 0.025), (0.035, (y1 - y0) / 2, 0.035), "steel_b", (d.cross(UP), UP, d))
    # The chines down the engine section's sides, over the pressure vessels.
    for deg in CHINE_AT:
        a = math.radians(deg)
        d = V((math.cos(a), 0, math.sin(a)))
        t = d.cross(UP)
        rings = []
        for y, h, hw in ((CHINE_Y[0], 0.50, 0.62), (-44.0, 0.62, 0.66), (-34.0, 0.62, 0.66), (-32.2, 0.42, 0.55), (CHINE_Y[1], 0.02, 0.30)):
            o = V((0, y, 0))
            rings.append([o + d * (R - 0.15) - t * hw, o + d * (R + h * 0.75) - t * hw, o + d * (R + h) - t * (hw * 0.6),
                          o + d * (R + h) + t * (hw * 0.6), o + d * (R + h * 0.75) + t * hw, o + d * (R - 0.15) + t * hw])
        g.loft(rings, "steel_d")
    # The raceway carrying lines and cables up the leeward side.
    g.box(V((0, (-41.0 + 13.6) / 2, R + 0.09)), (0.28, (13.6 + 41.0) / 2, 0.12), "steel_d")
    # The catch pins, between each pair of grid fins.
    for s in (1, -1):
        tube(g, [V((s * (R - 0.2), PIN_Y, 0)), V((s * (R + 0.75), PIN_Y, 0))], 0.20, "grid", n=12)
        g.box(V((s * (R + 0.15), PIN_Y - 0.35, 0)), (0.20, 0.30, 0.45), "steel_d")
    g.finish("booster body")

    # The engines: three in the middle and ten round them that gimbal, twenty fixed round the rim.
    for name, rings in (("booster centre engines", B_RINGS[:2]), ("booster outer engines", B_RINGS[2:])):
        g = Geo()
        for count, radius, turn in rings:
            for c in ring_of(count, radius, turn, B_PLATE):
                raptor(g, c, RAPTOR_R, RAPTOR_L)
        g.finish(name, smooth=30, pivot=V((0, B_PLATE, 0)), axis=X)

    # Grid fins: a lattice of thin plates in a frame, standing straight out.
    for k, deg in enumerate(GRID_AT):
        a = math.radians(deg)
        d = V((math.cos(a), 0, math.sin(a)))
        t = d.cross(UP)
        root = V((0, GRID_Y, 0)) + d * R
        g = Geo()
        s0, s1 = 0.35, GRID_LEN
        quad = [(s0, -GRID_HW[0]), (s1, -GRID_HW[1]), (s1, GRID_HW[1]), (s0, GRID_HW[0])]  # (out, across)
        at = lambda s, w: root + d * s + t * w

        def bar(p, q, th, depth=GRID_D):
            a3, b3 = at(*p), at(*q)
            l = (b3 - a3).length
            if l < 0.05:
                return
            ax = (b3 - a3) / l
            g.box((a3 + b3) / 2, (l / 2, depth, th), "grid", (ax, UP, ax.cross(UP)))

        for p, q in zip(quad, quad[1:] + quad[:1]):
            bar(p, q, 0.05)
        # Two families of plates crossing at right angles, cut off at the frame.
        for sign in (1, -1):
            c0 = -3.0
            while c0 < GRID_LEN + 3.0:
                # the line s + sign * w = c0, clipped to the frame
                pts = []
                for (sa, wa), (sb, wb) in zip(quad, quad[1:] + quad[:1]):
                    fa, fb = sa + sign * wa - c0, sb + sign * wb - c0
                    if (fa < 0) != (fb < 0):
                        u = fa / (fa - fb)
                        pts.append((lerp(sa, sb, u), lerp(wa, wb, u)))
                if len(pts) == 2:
                    bar(pts[0], pts[1], 0.014, GRID_D * 0.92)
                c0 += 0.52
        # The root: the shaft it turns on and its fairing on the skin.
        tube(g, [root - d * 0.3, root + d * (s0 + 0.05)], 0.34, "grid", n=14)
        g.box(root + d * 0.12, (0.9, 0.55, 0.16), "steel_d", (t, UP, d))
        g.finish(f"booster grid fin {k + 1}", smooth=30, pivot=root, axis=d)

    # The hot-staging ring: a vented lattice the ship's engines fire through
    # as it leaves, and the shield over the booster's dome.
    g = Geo()
    c = V((0, B_TOP, 0))
    h = HSR_TOP - B_TOP
    for a0, a1 in ((0.0, 0.30), (h - 0.28, h)):
        rev(g, c, UP, [(R - 0.10, a0), (R, a0), (R, a1), (R - 0.10, a1), (R - 0.10, a0)], "steel_b", N, X, Z)
    posts = 40
    for k in range(posts):
        a = 2 * math.pi * (k + 0.5) / posts
        d = V((math.cos(a), 0, math.sin(a)))
        t = d.cross(UP)
        lean = 0.22 if k % 2 else -0.22
        ax = (UP + t * lean).normalized()
        g.box(c + UP * (h / 2) + d * (R - 0.05), (0.10, (h - 0.5) / 2 / ax.y, 0.05), "steel_a", (ax.cross(d), ax, d))
    rev(g, c, UP, [(R - 0.10, 0.25), (3.6, 0.62), (2.2, 1.0), (0.9, 1.18), (0.0, 1.22)], "heat", N // 2, X, Z)
    g.finish("booster hot stage ring", smooth=30)


# ---------------------------------------------------------------- Starship
# The flaps: (out from the centreline, station) corners of the right one, the
# first two along the hinge. They lie in the plane between the heat shield and
# the bare steel; a positive angle folds them toward the leeward side.
FWD_FLAP = [(4.15, 59.6), (2.35, 67.6), (7.30, 63.5), (7.30, 60.2)]
AFT_FLAP = [(4.45, 23.6), (4.45, 36.0), (9.00, 30.0), (9.00, 24.2)]
FLAP_T = 0.20      # half thickness


def ship_skin(s, i, t, a, welds, y0):
    y = y0 + a
    if s in welds:
        return "tile" if math.sin(t) < TILE_EDGE else "weld"
    if math.sin(t) < TILE_EDGE:
        # (Tiles replaced since the last flight are a shade lighter.)
        return "tile_b" if hash01(i * 7 + s * 131) > 0.86 else "tile"
    if DOOR[0] < y < DOOR[1] and DOOR[2] < t < DOOR[3]:
        return "steel_d"
    return steel(s // 2 + 50)


def ship():
    g = Geo()
    prof, welds = barrel(S_SKIRT, S_NOSE)
    # The nose: an ogive, its rings closer together toward the tip.
    nose, nose_welds = barrel(S_NOSE, S_TIP - 0.6, ship_r, 1.5)
    welds |= {len(prof) - 1 + k for k in nose_welds}
    prof += nose[1:] + [(ship_r(S_TIP - 0.25) * 0.9, S_TIP - 0.25), (0.0, S_TIP - 0.05)]
    prof = [(r, y - S_SKIRT) for r, y in prof]
    rev(g, V((0, S_SKIRT, 0)), UP, prof, lambda s, i, t, a: ship_skin(s, i, t, a, welds, S_SKIRT), N, X, Z)
    # Inside the skirt, up to the thrust structure.
    rev(g, V((0, S_SKIRT, 0)), UP, [(0.0, S_PLATE - S_SKIRT), (R - 0.08, S_PLATE - S_SKIRT), (R - 0.08, 0.0), (R, 0.0)],
        lambda s, *_: "dark" if s == 0 else "heat", N, X, Z)
    # The raceway up the leeward side, and the payload door's frame.
    g.box(V((0, (27.0 + 49.0) / 2, R + 0.08)), (0.26, (49.0 - 27.0) / 2, 0.11), "steel_d")
    for y in (DOOR[0], DOOR[1]):
        r = ship_r(y)
        pts = [V((r * math.cos(a), y, r * math.sin(a))) * 1.0 for a in (lerp(DOOR[2], DOOR[3], k / 10) for k in range(11))]
        tube(g, [V((p.x * (1 + 0.012), p.y, p.z * (1 + 0.012))) for p in pts], 0.035, "weld", n=6)
    for a in (DOOR[2], DOOR[3]):
        tube(g, [V((ship_r(y) * 1.012 * math.cos(a), y, ship_r(y) * 1.012 * math.sin(a))) for y in (lerp(DOOR[0], DOOR[1], k / 6) for k in range(7))], 0.035, "weld", n=6)
    # The fairings the flaps hinge in.
    for outline in (FWD_FLAP, AFT_FLAP):
        (x0, y0), (x1, y1) = outline[0], outline[1]
        for s in (1, -1):
            tube(g, [V((s * (lerp(x0, x1, k / 6) - 0.10), lerp(y0, y1, k / 6), 0.0)) for k in range(7)], lambda u: 0.40 * (1.0 - (2 * u - 1) ** 6) + 0.04, "steel_b", n=12)
    g.finish("ship body")

    for name, outline in (("forward flap", FWD_FLAP), ("aft flap", AFT_FLAP)):
        for s, side in ((1, "right"), (-1, "left")):
            g = Geo()
            pts = [V((s * x, y, 0.0)) for x, y in outline]
            # Bare steel on the leeward face, tiles on the windward face and round the edge.
            g.plate(pts, Z, FLAP_T, "steel_a", "tile", "tile")
            # A rib or two across the steel face.
            (x0, y0), (x1, y1), (x2, y2), (x3, y3) = outline
            for f in (0.3, 0.7):
                a = V((s * lerp(x0, x1, f), lerp(y0, y1, f), FLAP_T + 0.02))
                b = V((s * lerp(x3, x2, f), lerp(y3, y2, f), FLAP_T + 0.02))
                ax = (b - a).normalized()
                g.box((a + b) / 2, ((b - a).length / 2 - 0.1, 0.06, 0.03), "steel_b", (ax, Z.cross(ax), Z))
            p0, p1 = V((x0, y0, 0.0)), V((x1, y1, 0.0))
            hinge = (p1 - p0).normalized()
            # Round the hinge a positive angle takes the tip toward +Z: the two sides turn opposite ways.
            axis = -hinge if s > 0 else V((-hinge.x, hinge.y, hinge.z))
            g.finish(f"ship {side} {name}", smooth=30, pivot=V((s * x0, y0, 0.0)), axis=axis)

    g = Geo()
    for c in ring_of(S_SEA[0], S_SEA[1], S_SEA[2], S_PLATE):
        raptor(g, c + V((0, S_EXIT + RAPTOR_L - S_PLATE, 0)), RAPTOR_R, RAPTOR_L)
        tube(g, [c, c + V((0, S_EXIT + RAPTOR_L - S_PLATE, 0))], 0.25, "hardware", n=8)
    g.finish("ship sea level engines", smooth=30, pivot=V((0, S_EXIT + RAPTOR_L, 0)), axis=X)
    g = Geo()
    for c in ring_of(S_VAC[0], S_VAC[1], S_VAC[2], S_PLATE):
        raptor(g, c, RVAC_R, RVAC_L, n=20)
    g.finish("ship vacuum engines", smooth=30, pivot=V((0, S_PLATE, 0)), axis=X)


# ---------------------------------------------------------------- exhaust
# One plume for each stage on the centreline, drawn in the game's transparent
# pass: shells and discs whose brightness the shader works out from the
# engines (material 17). Haze first, then the flame, its core and the discs:
# they are drawn in this order. Distance down from the exit plane, radius.
B_PLUME = (
    [(-0.3, 4.3), (5.0, 5.2), (20.0, 6.5), (45.0, 7.0), (70.0, 5.6), (85.0, 2.6), (90.0, 0.0)],
    [(-0.5, 4.2), (4.0, 4.5), (15.0, 4.2), (30.0, 3.4), (45.0, 2.3), (58.0, 1.0), (64.0, 0.0)],
    [(-0.5, 3.2), (5.0, 3.0), (15.0, 2.3), (28.0, 1.2), (36.0, 0.0)],
    (4.0, 50.0, 56.0),  # discs: the widest one's radius, how far down they go, where the cone they fill would end
)
S_PLUME = (
    [(-0.2, 3.9), (3.0, 4.6), (10.0, 5.4), (22.0, 5.2), (32.0, 3.2), (38.0, 0.0)],
    [(-0.3, 3.7), (2.0, 3.9), (8.0, 3.5), (15.0, 2.6), (22.0, 1.3), (28.0, 0.0)],
    [(-0.3, 2.6), (3.0, 2.4), (8.0, 1.6), (13.0, 0.7), (16.0, 0.0)],
    (3.4, 22.0, 26.0),
)


def plume(name, y_exit, shells):
    g = Geo()
    c = V((0, y_exit, 0))
    haze, flame, core, (disc_r, disc_len, cone) = shells
    for prof, m, n, sub in ((haze, "haze", 24, 2), (flame, "flame", 28, 3), (core, "core", 24, 3)):
        rows = spline_rows(prof, 0, len(prof) - 1, sub)
        rev(g, c, -UP, [(max(r, 0.0), a) for a, r in rows], m, n, X, Z)
    for k in range(14):
        a = 0.6 + (disc_len - 0.6) * (k / 13) ** 1.3
        rev(g, c, -UP, [(0.0, a), (disc_r * (1.0 - a / cone), a)], "disc", 24, X, Z)
    ob = g.finish(name, smooth=80, recalc=False)
    ob.hide_render = True
    ob.display_type = "WIRE"


# ---------------------------------------------------------------- export
# Every piece the game looks for; other objects are ignored.
EXPECTED = ["booster body", "booster centre engines", "booster outer engines", "booster hot stage ring", "booster plume"] + [f"booster grid fin {k}" for k in (1, 2, 3, 4)] + [
    "ship body", "ship sea level engines", "ship vacuum engines", "ship plume"] + [f"ship {s} {n} flap" for s in ("left", "right") for n in ("forward", "aft")]


def game_mat(m):
    """Which game material a Blender material is: its "game" property, its starship_ name, or else steel."""
    if m is None:
        return "steel_a"
    name = m.get("game") or (m.name[9:].split(".")[0] if m.name.startswith("starship_") else None)
    return name if name in MATS else "steel_a"


def export(path):
    out = bytearray(b"FFAC")
    obs = [o for o in COLL.all_objects if o.type == "MESH"]
    names = {o.name for o in obs}
    missing = [n for n in EXPECTED if n not in names]
    if missing:
        raise SystemExit(f"starship export: the game needs these pieces, missing from the '{COLL.name}' collection: {', '.join(missing)}")
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
                    if "plume" not in ob.name:
                        lo, hi = V(map(min, lo, p)), V(map(max, hi, p))
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
        print(f"  {ob.name}: {len(verts)} vertices, {len(index) // 3} triangles, pivot {tuple(round(c, 3) for c in pivot)}, axis {tuple(round(c, 3) for c in axis)}")
    with gzip.open(path, "wb", compresslevel=9) as f:
        f.write(bytes(out))
    print("wrote", path, len(out), "bytes raw,", total, "triangles")
    print("bounds without the plumes (Blender frame):", tuple(round(c, 2) for c in lo), "to", tuple(round(c, 2) for c in hi))


# ---------------------------------------------------------------- pictures
def render_previews(outdir):
    os.makedirs(outdir, exist_ok=True)
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_WORKBENCH"
    sc.display.shading.light = "STUDIO"
    sc.display.shading.color_type = "MATERIAL"
    sc.display.shading.show_cavity = True
    sc.render.film_transparent = False
    cam_data = bpy.data.cameras.new("cam")
    cam = bpy.data.objects.new("cam", cam_data)
    sc.collection.objects.link(cam)
    sc.camera = cam
    cam_data.clip_start, cam_data.clip_end = 0.5, 3000.0
    mid = V((0, (B_EXIT + S_TIP) / 2, 0))
    ship_mid = V((0, (S_SKIRT + S_TIP) / 2, 0))
    tall, wide = (800, 1280), (1280, 800)
    # name, eye, lens, what it looks at, picture size, which stage to leave out
    views = (
        ("stack_side", V((235, 12.5, 0)), 50, mid, tall, None),
        ("stack_windward", V((0, 12.5, -235)), 50, mid, tall, None),
        ("stack_three_quarter", V((150, 45, -150)), 50, mid, tall, None),
        ("stack_leeward", V((-120, 60, 170)), 50, mid, tall, None),
        ("booster_engines", V((9, -66, -9)), 35, V((0, -47, 0)), wide, None),
        ("grid_fins", V((26, 31, -18)), 40, V((0, 19.5, 0)), wide, None),
        ("ship", V((75, 55, -75)), 50, ship_mid, tall, "booster"),
        ("ship_leeward", V((-60, 60, 85)), 50, ship_mid, tall, "booster"),
        ("ship_engines", V((7, 9, -7)), 35, V((0, 24.5, 0)), wide, "booster"),
    )
    obs = list(COLL.all_objects)
    for name, eye, lens, look, (w, h), leave_out in views:
        for o in obs:
            o.hide_render = "plume" in o.name or (leave_out is not None and o.name.startswith(leave_out))
        fwd = (look - eye).normalized()
        up = UP if abs(fwd.y) < 0.95 else Z
        right = fwd.cross(up).normalized()
        cam.location = eye
        cam.rotation_euler = Matrix((right, right.cross(fwd), -fwd)).transposed().to_euler()
        cam_data.lens = lens
        sc.render.resolution_x, sc.render.resolution_y = w, h
        sc.render.filepath = os.path.join(outdir, name + ".png")
        bpy.ops.render.render(write_still=True)
    for o in obs:
        o.hide_render = "plume" in o.name


def main():
    global COLL
    if EXPORT_ONLY:
        # Export the .blend that is open, edits and all, without rebuilding anything.
        COLL = bpy.data.collections.get("Starship")
        if COLL is None:
            raise SystemExit("starship export: no 'Starship' collection in this file (open assets/blender/starship.blend)")
        export(os.path.join(ASSETS, "starship.bin.gz"))
    else:
        for ob in list(bpy.data.objects):
            bpy.data.objects.remove(ob, do_unlink=True)
        for c in list(bpy.data.collections):
            bpy.data.collections.remove(c)
        COLL = bpy.data.collections.new("Starship")
        bpy.context.scene.collection.children.link(COLL)
        make_materials()
        booster()
        plume("booster plume", B_EXIT, B_PLUME)
        ship()
        plume("ship plume", S_EXIT, S_PLUME)
        os.makedirs(os.path.join(ASSETS, "blender"), exist_ok=True)
        bpy.context.preferences.filepaths.save_version = 0  # no starship.blend1 beside it
        export(os.path.join(ASSETS, "starship.bin.gz"))
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ASSETS, "blender", "starship.blend"))
    if RENDER:
        render_previews(RENDER)


main()
