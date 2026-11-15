//! Fire and smoke as a fluid: a grid simulation around a crash site, run on
//! the GPU (fluid_sim.wgsl) and drawn by ray marching (fluid_draw.wgsl).
//!
//! The crash hands it sources (a fuel cloud that ignites, burning wreckage,
//! pools of fuel, dust thrown up by the blast); the grid does the rest: the
//! fuel burns into heat, soot and expanding gas, the hot gas rises and rolls
//! into a mushroom, and the smoke flows around the ground, buildings and
//! through the trees, and drifts off with the wind.
use crate::geo;
use crate::render::{buf_res, make_tex, samp_entry, tex_entry, tex_res, ub_entry, view};
use crate::world::World;
use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Vec2, Vec3};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Simulation grid: cells east, north, up.
pub const FN: [u32; 3] = [96, 96, 128];
/// Lighting grid (half resolution).
const LN: [u32; 3] = [FN[0] / 2, FN[1] / 2, FN[2] / 2];
const CELLS: u64 = (FN[0] * FN[1] * FN[2]) as u64;
pub const MAX_SRC: usize = 128;
/// Red-black SOR sweeps per step.
const SWEEPS: u32 = 10;
/// Sources are binned per block of 8^3 cells.
const BLOCKS: [u32; 3] = [FN[0] / 8, FN[1] / 8, FN[2] / 8];
const N_BLOCKS: usize = (BLOCKS[0] * BLOCKS[1] * BLOCKS[2]) as usize;
const STAT_GROUPS: u64 = ((LN[0] / 4) * (LN[1] / 4) * (LN[2] / 4)) as u64;
const STATS_SIZE: u64 = STAT_GROUPS * 48;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Default, Debug)]
pub struct FU {
    pub a: [f32; 4],
    pub wind: [f32; 4],
    pub k1: [f32; 4],
    pub k2: [f32; 4],
    pub k3: [f32; 4],
    pub sun: [f32; 4],
    pub ext: [f32; 4],
    pub org: [f32; 4],
    pub ax_e: [f32; 4],
    pub ax_n: [f32; 4],
    pub ax_u: [f32; 4],
    pub k4: [f32; 4],
    pub coarse: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Default, Debug)]
pub struct GpuSrc {
    p: [f32; 4],
    v: [f32; 4],
    k: [f32; 4],
    m: [f32; 4],
    n: [f32; 4],
}

/// Something that feeds the fire, as the crash describes it (planet frame,
/// position relative to the crash site).
#[derive(Clone, Copy, Debug, Default)]
pub struct FluidSrc {
    pub pos: Vec3,
    pub radius: f32,
    pub vel: Vec3,
    /// outward speed at the rim (m/s)
    pub radial: f32,
    /// per second, at the centre
    pub fuel: f32,
    pub soot: f32,
    pub dust: f32,
    /// temperature it heats the gas to (thousands of kelvin above ambient)
    pub temp: f32,
    /// how firmly it imposes its velocity and its temperature (1/s)
    pub vel_k: f32,
    pub temp_k: f32,
    /// how much its fuel comes out in flickering tongues (0..1)
    pub flicker: f32,
    /// volume it adds per second, per volume (a blast pushing the air aside)
    pub expand: f32,
    /// raggedness of the velocity it imposes (share of the outward speed)
    pub turb: f32,
}

/// What the simulation does this frame.
pub struct FluidFrame {
    pub u: FU,
    pub srcs: Vec<GpuSrc>,
    /// per block of cells, which sources reach it
    pub bins: Vec<[u32; 4]>,
    pub steps: u32,
    pub reset: bool,
    pub obst: Option<(Vec<[f32; 4]>, Vec<[f32; 4]>)>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FluidStats {
    /// light given off (relative units) and where its centre is (0..1 across the domain)
    pub emit: f32,
    pub emit_c: Vec3,
    /// soot leaving through the open sides and top per second, and where (0..1 across)
    pub out: f32,
    pub out_c: Vec3,
    /// all the soot in the grid
    pub soot: f32,
}

// ------------------------------------------------------------------ domain

/// Where the grid sits and how it is fed: owned by the crash it belongs to.
pub struct Domain {
    pub site: DVec3,
    pub corner: DVec3,
    pub e: DVec3,
    pub n: DVec3,
    pub u: DVec3,
    pub h: f64,
    /// how much coarser than for a light aircraft (see `new`)
    pub sc: f32,
    /// height of the ground at the site above the domain floor
    pub ground_z: f32,
    pub time: f32,
    pub reset: bool,
    obst_t: f32,
    /// ground height per column, once every column's terrain was loaded
    ground: Option<Vec<f32>>,
    /// how far the coarse smoke volume reaches down below the usual (the
    /// impact was up the side of a building, the street is further down)
    pub vol_lift: f32,
}

impl Domain {
    /// `slide`: which way the wreck is going; `wind`: the wind (planet frame).
    /// The grid reaches further both ways: the wreck burns along its path and
    /// the smoke leans downwind.
    /// `size`: how big the aircraft is next to a light single (1); a big
    /// jet's fireball is several times wider, so the cells are coarser.
    pub fn new(site: DVec3, slide: DVec3, wind: DVec3, power: f32, size: f32) -> Self {
        let (e, n, u) = geo::enu(site);
        let sc = (size.max(1.0) as f64).powf(0.75);
        let h = (0.62 + 0.26 * power as f64).clamp(0.78, 1.2) * sc;
        let w = FN[0] as f64 * h;
        let flat = |v: DVec3| DVec3::new(v.dot(e), v.dot(n), 0.0);
        let (s, wd) = (flat(slide).normalize_or_zero(), flat(wind));
        let lean = (wd.length() / 6.0).min(1.0);
        // A heavy wreck goes on a long way, burning: more of the grid ahead.
        let ahead = if size > 2.0 { 0.2 } else { 0.1 };
        let off = (s * ahead + wd.normalize_or_zero() * 0.22 * lean) * w;
        let below = 14.0 * sc.sqrt();
        let corner = site + e * (off.x - w * 0.5) + n * (off.y - w * 0.5) - u * below;
        Self { site, corner, e, n, u, h, sc: sc as f32, ground_z: below as f32, time: 0.0, reset: true, obst_t: 0.0, ground: None, vol_lift: 0.0 }
    }

    pub fn size(&self) -> Vec3 {
        Vec3::new(FN[0] as f32, FN[1] as f32, FN[2] as f32) * self.h as f32
    }
    /// Domain metres of a planet-frame offset from the site.
    pub fn local(&self, rel_site: Vec3) -> Vec3 {
        let p = (self.site - self.corner) + rel_site.as_dvec3();
        Vec3::new(p.dot(self.e) as f32, p.dot(self.n) as f32, p.dot(self.u) as f32)
    }
    pub fn local_vec(&self, v: Vec3) -> Vec3 {
        let v = v.as_dvec3();
        Vec3::new(v.dot(self.e) as f32, v.dot(self.n) as f32, v.dot(self.u) as f32)
    }
    pub fn world(&self, l: Vec3) -> DVec3 {
        self.corner + self.e * l.x as f64 + self.n * l.y as f64 + self.u * l.z as f64
    }
    /// Whether a planet-frame offset from the site is well inside the grid.
    pub fn contains(&self, rel_site: Vec3, margin: f32) -> bool {
        let l = self.local(rel_site);
        let s = self.size();
        l.x > margin && l.y > margin && l.z > 0.0 && l.x < s.x - margin && l.y < s.y - margin && l.z < s.z - margin
    }

    /// The ground, buildings and tree crowns in each column of the grid.
    pub fn obstacles(&mut self, world: &World) -> (Vec<[f32; 4]>, Vec<[f32; 4]>) {
        let (nx, ny) = (FN[0] as usize, FN[1] as usize);
        let h = self.h;
        let base_alt = geo::pos_to_lla(self.corner).2;
        // The terrain does not change once the crater is dug: look it up once
        // (again later if some of it had not loaded yet).
        if self.ground.is_none() {
            let mut all = true;
            let mut g = vec![self.ground_z; nx * ny];
            for j in 0..ny {
                for i in 0..nx {
                    let p = self.corner + self.e * ((i as f64 + 0.5) * h) + self.n * ((j as f64 + 0.5) * h) + self.u * self.ground_z as f64;
                    let (lat, lon, _) = geo::pos_to_lla(p);
                    match world.ground(lat, lon) {
                        Some(gr) => g[j * nx + i] = (gr.h - base_alt) as f32,
                        None => all = false,
                    }
                }
            }
            if all {
                self.ground = Some(g.clone());
            }
            return self.obstacles_on(world, &g, base_alt);
        }
        let g = self.ground.clone().unwrap_or_default();
        self.obstacles_on(world, &g, base_alt)
    }

    fn obstacles_on(&self, world: &World, ground: &[f32], base_alt: f64) -> (Vec<[f32; 4]>, Vec<[f32; 4]>) {
        let (nx, ny) = (FN[0] as usize, FN[1] as usize);
        let h = self.h;
        let mut obst: Vec<[f32; 4]> = ground.iter().map(|g| [*g, 1e9, -1e9, 0.0]).collect();
        let mut trees = vec![[0.0f32, 0.0, 0.0, 0.0]; nx * ny];
        let mid = self.world(self.size() * 0.5);
        let reach = self.size().x as f64 * 0.75;
        let to_xy = |q: DVec3| {
            let d = q - self.corner;
            Vec2::new((d.dot(self.e) / h) as f32, (d.dot(self.n) / h) as f32)
        };
        for (ring, base, top, bridge) in world.footprints_near(mid, reach) {
            let pts: Vec<[f32; 2]> = ring.iter().map(|q| to_xy(*q).to_array()).collect();
            let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
            for p in &pts {
                lo = lo.min(Vec2::from(*p));
                hi = hi.max(Vec2::from(*p));
            }
            let (x0, x1) = ((lo.x.floor().max(0.0)) as usize, (hi.x.ceil().min(nx as f32)) as usize);
            let (y0, y1) = ((lo.y.floor().max(0.0)) as usize, (hi.y.ceil().min(ny as f32)) as usize);
            let (b, t) = ((base - base_alt) as f32, (top - base_alt) as f32);
            for j in y0..y1 {
                for i in x0..x1 {
                    if !crate::buildings::point_in_ring([i as f32 + 0.5, j as f32 + 0.5], &pts) {
                        continue;
                    }
                    let o = &mut obst[j * nx + i];
                    if bridge {
                        o[1] = o[1].min(b);
                        o[2] = o[2].max(t);
                    } else {
                        o[0] = o[0].max(t);
                    }
                }
            }
        }
        let mut near = Vec::new();
        world.trees_near(mid, reach, &mut near);
        for tr in &near {
            let c = to_xy(tr.pos);
            let r = (tr.crown_r / h).max(0.7) as f32;
            let z0 = (geo::pos_to_lla(tr.pos).2 - base_alt) as f32;
            let (lo, hi) = (z0 + tr.crown_base as f32, z0 + tr.height as f32);
            for j in ((c.y - r).floor().max(0.0) as usize)..((c.y + r).ceil().min(ny as f32) as usize) {
                for i in ((c.x - r).floor().max(0.0) as usize)..((c.x + r).ceil().min(nx as f32) as usize) {
                    let d2 = (Vec2::new(i as f32 + 0.5, j as f32 + 0.5) - c).length_squared() / (r * r);
                    if d2 >= 1.0 {
                        continue;
                    }
                    let t = &mut trees[j * nx + i];
                    if t[2] <= 0.0 {
                        *t = [lo, hi, 1.0 - d2, 0.0];
                    } else {
                        *t = [t[0].min(lo), t[1].max(hi), t[2].max(1.0 - d2), 0.0];
                    }
                }
            }
        }
        (obst, trees)
    }

    /// Build this frame's work. `dt` is game time; `cam` the camera; `wind`
    /// the planet-frame wind at 10 m; `sun` the planet-frame sun direction.
    pub fn frame(&mut self, dt: f64, cam: DVec3, wind: DVec3, sun: DVec3, srcs: &[FluidSrc], world: &World) -> FluidFrame {
        // One step a frame; two only when frames are very long.
        let steps = if dt <= 0.0 { 0 } else { ((dt * 20.0).ceil() as u32).clamp(1, 2) };
        let sdt = if steps > 0 { (dt / steps as f64) as f32 } else { 0.0 };
        self.time += dt as f32;
        self.obst_t -= dt as f32;
        let obst = if self.reset || self.obst_t <= 0.0 {
            self.obst_t = 1.0;
            Some(self.obstacles(world))
        } else {
            None
        };
        let gsrc: Vec<GpuSrc> = srcs
            .iter()
            .take(MAX_SRC)
            .map(|s| {
                let p = self.local(s.pos);
                let v = self.local_vec(s.vel);
                GpuSrc { p: [p.x, p.y, p.z, s.radius.max(self.h as f32 * 1.3)], v: [v.x, v.y, v.z, s.radial], k: [s.fuel, s.soot, s.dust, s.temp], m: [s.vel_k, s.temp_k, s.flicker, s.expand], n: [s.turb, (p.x * 0.137 + p.y * 0.291).fract(), 0.0, 0.0] }
            })
            .collect();
        let mut bins = vec![[0u32; 4]; N_BLOCKS];
        let bs = self.h as f32 * 8.0;
        for (k, g) in gsrc.iter().enumerate() {
            let (c, r) = (Vec3::new(g.p[0], g.p[1], g.p[2]), g.p[3]);
            let lo = ((c - r) / bs).floor().max(Vec3::ZERO);
            let hi = ((c + r) / bs).floor().min(Vec3::new(BLOCKS[0] as f32 - 1.0, BLOCKS[1] as f32 - 1.0, BLOCKS[2] as f32 - 1.0));
            if lo.x > hi.x || lo.y > hi.y || lo.z > hi.z {
                continue;
            }
            for z in lo.z as u32..=hi.z as u32 {
                for y in lo.y as u32..=hi.y as u32 {
                    for x in lo.x as u32..=hi.x as u32 {
                        bins[((z * BLOCKS[1] + y) * BLOCKS[0] + x) as usize][k / 32] |= 1 << (k % 32);
                    }
                }
            }
        }
        let w = self.local_vec(wind.as_vec3());
        let sl = self.local_vec(sun.as_vec3());
        let o = (self.corner - cam).as_vec3();
        let u = FU {
            a: [sdt, self.h as f32, self.time, gsrc.len() as f32],
            wind: [w.x, w.y, w.z, 3.5],
            // (a bigger fireball loses its heat more slowly)
            k1: [10.0, 0.5 / self.sc.sqrt(), 0.1, 0.7],
            // (soot per fuel burnt: a big fire is seen through so much more of
            // it that at a small one's yield it would be a black wall)
            k2: [0.8 / self.sc.sqrt(), 1.55, 0.35, 0.012],
            k3: [0.06, 0.5, 2.5, 0.06],
            sun: [sl.x, sl.y, sl.z, 1.7],
            ext: [1.8, 0.9, 0.1, 0.035],
            org: [o.x, o.y, o.z, 1.0],
            ax_e: [self.e.x as f32, self.e.y as f32, self.e.z as f32, self.ground_z],
            ax_n: [self.n.x as f32, self.n.y as f32, self.n.z as f32, 1.0],
            ax_u: [self.u.x as f32, self.u.y as f32, self.u.z as f32, 1.0],
            // (the turbulence's eddies grow with the cells, or they would be finer than the grid)
            k4: [25.0, 0.2 / self.sc, 1.2, 1.0],
            coarse: {
                // (as main.rs places the volume: coarser round a big fire)
                let cs = crate::render::VOL_CELL * self.sc;
                let c = self.local(Vec3::ZERO) + Vec3::new(-cs * crate::render::VOL_N[0] as f32 * 0.5, -cs * crate::render::VOL_N[1] as f32 * 0.5, crate::render::VOL_ZMIN * self.sc - self.vol_lift);
                [c.x, c.y, c.z, cs]
            },
        };
        let reset = std::mem::take(&mut self.reset);
        FluidFrame { u, srcs: gsrc, bins, steps, reset, obst }
    }

    /// Planet-frame position of a point given in 0..1 across the domain.
    pub fn at_frac(&self, f: Vec3) -> DVec3 {
        self.world(f * self.size())
    }
}

// ---------------------------------------------------------------------- GPU

/// Scene depth each half-resolution texel marched to.
const HALF_Z: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

pub struct FluidTargets {
    pub half: wgpu::TextureView,
    pub half_z: wgpu::TextureView,
    pub bg_march: wgpu::BindGroup,
    pub bg_up: wgpu::BindGroup,
}

struct Readback {
    buf: wgpu::Buffer,
    /// 0 free, 1 waiting for the map, 2 mapped
    state: Arc<AtomicU8>,
}

pub struct FluidGpu {
    ub: wgpu::Buffer,
    src_buf: wgpu::Buffer,
    bin_buf: wgpu::Buffer,
    scal: [wgpu::Texture; 3],
    obst: wgpu::Texture,
    trees: wgpu::Texture,
    p_clear: wgpu::ComputePipeline,
    p_advect: wgpu::ComputePipeline,
    p_react: wgpu::ComputePipeline,
    p_div: wgpu::ComputePipeline,
    p_press: [wgpu::ComputePipeline; 2],
    p_project: wgpu::ComputePipeline,
    p_curl: wgpu::ComputePipeline,
    p_light: wgpu::ComputePipeline,
    p_coarse: wgpu::ComputePipeline,
    bg_zero: wgpu::BindGroup,
    bg_adv: wgpu::BindGroup,
    bg_react: wgpu::BindGroup,
    bg_proj: wgpu::BindGroup,
    bg_curl: wgpu::BindGroup,
    bg_light: wgpu::BindGroup,
    pub p_draw: wgpu::RenderPipeline,
    pub p_up: wgpu::RenderPipeline,
    l_draw: wgpu::BindGroupLayout,
    scal_v: wgpu::TextureView,
    vel_v: wgpu::TextureView,
    light_v: wgpu::TextureView,
    stats: wgpu::Buffer,
    readback: Vec<Readback>,
    /// readback slot written this frame, to be mapped after the submit
    pending: Option<usize>,
    pub latest: Option<FluidStats>,
    /// the grid holds a live fire
    pub live: bool,
}

pub fn shader_prefix() -> String {
    let v = crate::render::VOL_N;
    format!("const FNX: u32 = {}u;\nconst FNY: u32 = {}u;\nconst FNZ: u32 = {}u;\nconst VOLNX: u32 = {}u;\nconst VOLNY: u32 = {}u;\nconst VOLNZ: u32 = {}u;\n{}\n", FN[0], FN[1], FN[2], v[0], v[1], v[2], include_str!("shaders/fluid_common.wgsl"))
}

impl FluidGpu {
    /// `l0`: the globals layout; `common`: the shared shader source.
    /// `coarse_acc`: the scene's coarse smoke volume accumulator (vol.wgsl).
    pub fn new(d: &wgpu::Device, l0: &wgpu::BindGroupLayout, common: &str, coarse_acc: &wgpu::Buffer) -> Self {
        let prefix = shader_prefix();
        let m_sim = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fluid sim"), source: wgpu::ShaderSource::Wgsl(format!("{prefix}\n{}", include_str!("shaders/fluid_sim.wgsl")).into()) });
        let m_draw = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fluid draw"), source: wgpu::ShaderSource::Wgsl(format!("{common}\n{prefix}\n{}", include_str!("shaders/fluid_draw.wgsl")).into()) });
        let fu_size = std::mem::size_of::<FU>() as u64;
        let ub = d.create_buffer(&wgpu::BufferDescriptor { label: Some("fluid u"), size: fu_size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let src_buf = d.create_buffer(&wgpu::BufferDescriptor { label: Some("fluid sources"), size: (MAX_SRC * std::mem::size_of::<GpuSrc>()) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let usage = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST;
        let f16 = wgpu::TextureFormat::Rgba16Float;
        let t3 = |label: &str, n: [u32; 3]| make_tex(d, label, n[0], n[1], n[2], 1, f16, usage);
        let vel = [t3("fluid vel a", FN), t3("fluid vel b", FN)];
        let scal = [t3("fluid scal a", FN), t3("fluid scal b", FN), t3("fluid scal c", FN)];
        let curl = t3("fluid curl", FN);
        let light = t3("fluid light", LN);
        let dummy_s = view(&t3("fluid dummy s", [4, 4, 4]));
        let dummy_w = view(&t3("fluid dummy w", [4, 4, 4]));
        let t2 = |label: &str| make_tex(d, label, FN[0], FN[1], 1, 1, wgpu::TextureFormat::Rgba32Float, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let obst = t2("fluid obstacles");
        let trees = t2("fluid trees");
        let sbuf = |label: &str, size: u64, extra: wgpu::BufferUsages| d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | extra, mapped_at_creation: false });
        let pres = sbuf("fluid pressure", CELLS * 4, wgpu::BufferUsages::empty());
        let rhs = sbuf("fluid rhs", CELLS * 4, wgpu::BufferUsages::empty());
        let expand = sbuf("fluid expansion", CELLS * 4, wgpu::BufferUsages::empty());
        let stats = sbuf("fluid stats", STATS_SIZE, wgpu::BufferUsages::COPY_SRC);
        let flags = sbuf("fluid flags", CELLS * 4, wgpu::BufferUsages::empty());
        let bin_buf = sbuf("fluid bins", N_BLOCKS as u64 * 16, wgpu::BufferUsages::COPY_DST);
        let readback = (0..3)
            .map(|_| Readback { buf: d.create_buffer(&wgpu::BufferDescriptor { label: Some("fluid readback"), size: STATS_SIZE, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }), state: Arc::new(AtomicU8::new(0)) })
            .collect();
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let cs = wgpu::ShaderStages::COMPUTE;
        let tex3 = |b: u32| wgpu::BindGroupLayoutEntry { binding: b, visibility: cs, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D3, multisampled: false }, count: None };
        let tex2u = |b: u32| wgpu::BindGroupLayoutEntry { binding: b, visibility: cs, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None };
        let st3 = |b: u32| wgpu::BindGroupLayoutEntry { binding: b, visibility: cs, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: f16, view_dimension: wgpu::TextureViewDimension::D3 }, count: None };
        let buf = |b: u32, ro: bool| wgpu::BindGroupLayoutEntry { binding: b, visibility: cs, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: ro }, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let l_sim = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fluid sim"),
            entries: &[ub_entry(0, false, fu_size), buf(1, true), tex3(2), tex3(3), tex3(4), st3(5), st3(6), st3(7), buf(8, false), buf(9, false), buf(10, false), tex2u(11), samp_entry(12, wgpu::SamplerBindingType::Filtering), st3(13), buf(14, false), tex2u(15), tex3(16), buf(17, false), buf(18, true), buf(19, false)],
        });
        let pl_sim = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l_sim)], immediate_size: 0 });
        let cp = |entry: &str, constants: &[(&str, f64)]| {
            d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(entry), layout: Some(&pl_sim), module: &m_sim, entry_point: Some(entry), compilation_options: wgpu::PipelineCompilationOptions { constants, ..Default::default() }, cache: None })
        };
        let v = |t: &wgpu::Texture| view(t);
        let (vel_a, vel_b, scal_a, scal_b, scal_c, curl_v, light_v) = (v(&vel[0]), v(&vel[1]), v(&scal[0]), v(&scal[1]), v(&scal[2]), v(&curl), v(&light));
        let obst_v = v(&obst);
        let trees_v = v(&trees);
        let bg = |vin: &wgpu::TextureView, sin: &wgpu::TextureView, cin: &wgpu::TextureView, vout: &wgpu::TextureView, sout: &wgpu::TextureView, cout: &wgpu::TextureView, shat: &wgpu::TextureView| {
            d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &l_sim,
                entries: &[
                    buf_res(0, &ub, 0),
                    buf_res(1, &src_buf, 0),
                    tex_res(2, vin),
                    tex_res(3, sin),
                    tex_res(4, cin),
                    tex_res(5, vout),
                    tex_res(6, sout),
                    tex_res(7, cout),
                    buf_res(8, &pres, 0),
                    buf_res(9, &rhs, 0),
                    buf_res(10, &expand, 0),
                    tex_res(11, &obst_v),
                    wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::Sampler(&sampler) },
                    tex_res(13, &light_v),
                    buf_res(14, &stats, 0),
                    tex_res(15, &trees_v),
                    tex_res(16, shat),
                    buf_res(17, &flags, 0),
                    buf_res(18, &bin_buf, 0),
                    buf_res(19, coarse_acc, 0),
                ],
            })
        };
        let bg_zero = bg(&dummy_s, &dummy_s, &dummy_s, &vel_a, &scal_a, &curl_v, &dummy_s);
        let bg_adv = bg(&vel_a, &scal_a, &curl_v, &vel_b, &scal_b, &dummy_w, &dummy_s);
        let bg_react = bg(&vel_a, &scal_a, &curl_v, &vel_b, &scal_c, &dummy_w, &scal_b);
        let bg_proj = bg(&vel_b, &scal_a, &curl_v, &vel_a, &scal_b, &dummy_w, &dummy_s);
        let bg_curl = bg(&vel_a, &scal_a, &dummy_s, &vel_b, &scal_b, &curl_v, &dummy_s);
        let bg_light = bg(&vel_a, &scal_a, &curl_v, &vel_b, &scal_b, &dummy_w, &dummy_s);

        let l_draw = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fluid draw"),
            entries: &[
                ub_entry(0, false, fu_size),
                tex_entry(1, wgpu::TextureViewDimension::D3, false),
                tex_entry(2, wgpu::TextureViewDimension::D3, false),
                tex_entry(3, wgpu::TextureViewDimension::D2, true),
                tex_entry(4, wgpu::TextureViewDimension::D3, false),
                tex_entry(5, wgpu::TextureViewDimension::D2, false),
                wgpu::BindGroupLayoutEntry { binding: 6, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            ],
        });
        let pm = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
        let keep = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        let layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(l0), Some(&l_draw)], immediate_size: 0 });
        let target = |format, blend| Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL });
        let pipe = |label: &str, fs: &str, targets: &[Option<wgpu::ColorTargetState>]| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState { module: &m_draw, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &m_draw, entry_point: Some(fs), compilation_options: Default::default(), targets }),
                multiview_mask: None,
                cache: None,
            })
        };
        let p_draw = pipe("fluid march", "fs", &[target(crate::render::HDR, None), target(HALF_Z, None)]);
        let p_up = pipe("fluid upsample", "fs_up", &[target(crate::render::HDR, Some(wgpu::BlendState { color: pm, alpha: keep }))]);

        Self {
            ub,
            src_buf,
            bin_buf,
            scal,
            obst,
            trees,
            p_clear: cp("clear", &[]),
            p_advect: cp("advect", &[]),
            p_react: cp("react", &[]),
            p_div: cp("divergence", &[]),
            p_press: [cp("pressure", &[("parity", 0.0)]), cp("pressure", &[("parity", 1.0)])],
            p_project: cp("project", &[]),
            p_curl: cp("curl", &[]),
            p_light: cp("light", &[]),
            p_coarse: cp("coarse", &[]),
            bg_zero,
            bg_adv,
            bg_react,
            bg_proj,
            bg_curl,
            bg_light,
            p_draw,
            p_up,
            l_draw,
            scal_v: scal_a,
            vel_v: vel_a,
            light_v,
            stats,
            readback,
            pending: None,
            latest: None,
            live: false,
        }
    }

    /// Targets and bind groups for drawing at a screen size (they read the
    /// scene depth, so they are remade with the other targets).
    pub fn draw_targets(&self, d: &wgpu::Device, depth: &wgpu::TextureView, w: u32, h: u32) -> FluidTargets {
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let (hw, hh) = (w.div_ceil(2), h.div_ceil(2));
        let half = view(&make_tex(d, "fluid half", hw, hh, 1, 1, crate::render::HDR, rt));
        let half_z = view(&make_tex(d, "fluid half z", hw, hh, 1, 1, HALF_Z, rt));
        let dummy = view(&make_tex(d, "fluid dummy 2d", 1, 1, 1, 1, crate::render::HDR, rt));
        let dummy_z = view(&make_tex(d, "fluid dummy z", 1, 1, 1, 1, HALF_Z, rt));
        let bg = |a: &wgpu::TextureView, b: &wgpu::TextureView| {
            d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fluid draw"),
                layout: &self.l_draw,
                entries: &[buf_res(0, &self.ub, 0), tex_res(1, &self.scal_v), tex_res(2, &self.light_v), tex_res(3, depth), tex_res(4, &self.vel_v), tex_res(5, a), tex_res(6, b)],
            })
        };
        FluidTargets { bg_march: bg(&dummy, &dummy_z), bg_up: bg(&half, &half_z), half, half_z }
    }

    /// Collect totals the GPU has finished handing back.
    pub fn poll(&mut self) {
        for r in &self.readback {
            if r.state.load(Ordering::Acquire) != 2 {
                continue;
            }
            {
                let data = r.buf.slice(..).get_mapped_range().expect("mapped");
                let v: &[[f32; 4]] = bytemuck::cast_slice(&data);
                let (mut e, mut s, mut tot) = ([0.0f64; 4], [0.0f64; 4], 0.0f64);
                for k in 0..STAT_GROUPS as usize {
                    for c in 0..4 {
                        e[c] += v[k * 3][c] as f64;
                        s[c] += v[k * 3 + 1][c] as f64;
                    }
                    tot += v[k * 3 + 2][0] as f64;
                }
                let ec = if e[0] > 1e-9 { Vec3::new((e[1] / e[0]) as f32, (e[2] / e[0]) as f32, (e[3] / e[0]) as f32) } else { Vec3::splat(0.5) };
                let oc = if s[0] > 1e-9 { Vec3::new((s[1] / s[0]) as f32, (s[2] / s[0]) as f32, (s[3] / s[0]) as f32) } else { Vec3::splat(0.5) };
                self.latest = Some(FluidStats { emit: e[0] as f32, emit_c: ec, out: s[0] as f32, out_c: oc, soot: tot as f32 });
            }
            r.buf.unmap();
            r.state.store(0, Ordering::Release);
        }
    }

    /// Record this frame's simulation work.
    pub fn compute(&mut self, q: &wgpu::Queue, enc: &mut wgpu::CommandEncoder, f: &FluidFrame) {
        self.live = true;
        q.write_buffer(&self.ub, 0, bytemuck::bytes_of(&f.u));
        if !f.srcs.is_empty() {
            q.write_buffer(&self.src_buf, 0, bytemuck::cast_slice(&f.srcs));
        }
        q.write_buffer(&self.bin_buf, 0, bytemuck::cast_slice(&f.bins));
        if let Some((o, t)) = &f.obst {
            for (tex, data) in [(&self.obst, o), (&self.trees, t)] {
                q.write_texture(
                    wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    bytemuck::cast_slice(data),
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(FN[0] * 16), rows_per_image: Some(FN[1]) },
                    wgpu::Extent3d { width: FN[0], height: FN[1], depth_or_array_layers: 1 },
                );
            }
        }
        let g = [FN[0] / 4, FN[1] / 4, FN[2] / 4];
        if f.reset {
            let mut cp = enc.begin_compute_pass(&Default::default());
            cp.set_pipeline(&self.p_clear);
            cp.set_bind_group(0, &self.bg_zero, &[]);
            cp.dispatch_workgroups(g[0], g[1], g[2]);
        }
        for _ in 0..f.steps {
            {
                let mut cp = enc.begin_compute_pass(&Default::default());
                cp.set_pipeline(&self.p_advect);
                cp.set_bind_group(0, &self.bg_adv, &[]);
                cp.dispatch_workgroups(g[0], g[1], g[2]);
                cp.set_pipeline(&self.p_react);
                cp.set_bind_group(0, &self.bg_react, &[]);
                cp.dispatch_workgroups(g[0], g[1], g[2]);
            }
            let full = wgpu::Extent3d { width: FN[0], height: FN[1], depth_or_array_layers: FN[2] };
            let ti = |t| wgpu::TexelCopyTextureInfo { texture: t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All };
            enc.copy_texture_to_texture(ti(&self.scal[2]), ti(&self.scal[0]), full);
            let mut cp = enc.begin_compute_pass(&Default::default());
            cp.set_bind_group(0, &self.bg_proj, &[]);
            cp.set_pipeline(&self.p_div);
            cp.dispatch_workgroups(g[0], g[1], g[2]);
            for _ in 0..SWEEPS {
                for p in &self.p_press {
                    cp.set_pipeline(p);
                    cp.dispatch_workgroups(FN[0] / 2 / 16, FN[1] / 4, FN[2]);
                }
            }
            cp.set_pipeline(&self.p_project);
            cp.dispatch_workgroups(g[0], g[1], g[2]);
            cp.set_pipeline(&self.p_curl);
            cp.set_bind_group(0, &self.bg_curl, &[]);
            cp.dispatch_workgroups(g[0], g[1], g[2]);
        }
        {
            let mut cp = enc.begin_compute_pass(&Default::default());
            cp.set_pipeline(&self.p_light);
            cp.set_bind_group(0, &self.bg_light, &[]);
            cp.dispatch_workgroups(LN[0] / 4, LN[1] / 4, LN[2] / 4);
        }
        if let Some(k) = self.readback.iter().position(|r| r.state.load(Ordering::Acquire) == 0) {
            enc.copy_buffer_to_buffer(&self.stats, 0, &self.readback[k].buf, 0, STATS_SIZE);
            self.pending = Some(k);
        }
    }

    /// Add the smoke to the scene's coarse smoke volume (after it is cleared, before it is smoothed).
    pub fn splat_coarse(&self, enc: &mut wgpu::CommandEncoder) {
        let v = crate::render::VOL_N;
        let mut cp = enc.begin_compute_pass(&Default::default());
        cp.set_pipeline(&self.p_coarse);
        cp.set_bind_group(0, &self.bg_light, &[]);
        cp.dispatch_workgroups(v[0] / 4, v[1] / 4, v[2] / 4);
    }

    /// After the frame is submitted: start mapping the totals copied this frame.
    pub fn after_submit(&mut self) {
        if let Some(k) = self.pending.take() {
            let r = &self.readback[k];
            r.state.store(1, Ordering::Release);
            let st = r.state.clone();
            r.buf.slice(..).map_async(wgpu::MapMode::Read, move |res| st.store(if res.is_ok() { 2 } else { 0 }, Ordering::Release));
        }
    }
}

impl FluidGpu {
    /// Before the device goes: let pending maps finish and unmap them.
    pub fn shutdown(&mut self, d: &wgpu::Device) {
        let _ = d.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        for r in &self.readback {
            if r.state.load(Ordering::Acquire) == 2 {
                r.buf.unmap();
                r.state.store(0, Ordering::Release);
            }
        }
    }
}
