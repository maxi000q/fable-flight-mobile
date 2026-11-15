//! Distance to the shore, for the breakers (composite.wgsl `water`).
//!
//! Our water seen from above (terrain.wgsl `fs_top`, the same pass that marks
//! it in the 3D tiles' coverage mask) is drawn into a square around the camera,
//! together with where any ground was drawn at all. A jump flood
//! (Rong & Tan 2006) then finds each water texel's nearest land texel; the
//! shaders read the distance and the seaward direction.
//!
//! The work is spread over frames: one frame draws the mask and seeds the
//! flood, one jump a frame after that, then one frame turns the result into
//! the field the shaders read, which only then switches to the new square.
//! Squares share one texel lattice (snapped on fixed axes), so a rebuild
//! moves nothing that has not changed.
//!
//! How tall the breakers come in depends on how exposed the shore is: swell
//! only reaches water open to the sea, and the wind's own waves grow with the
//! fetch. A much wider, coarser mask of the same kind is drawn alongside, and
//! rays across it (over the jump frames, a band a frame) find, around the
//! square, the share of directions with no land within `REACH` and the fetch
//! upwind; the resolve turns them into a breaker height per texel. So a bay
//! or a harbour basin in light air gets at most a ripple at its edge, and an
//! open coast its surf.

use glam::DVec3;

/// Texels across the square.
pub const N: u32 = 2048;
/// Metres per texel.
pub const TEXEL: f64 = 4.0;
/// Half the square's side (m).
pub const HALF: f64 = N as f64 * TEXEL * 0.5;
/// Distances are kept up to this far out (m); beyond is "open water".
pub const MAX_D: f32 = 480.0;
/// Jump lengths (texels): they reach 127 texels (more than `MAX_D`), plus
/// one more pass of 1 to mend the flood's few mistakes.
const JUMPS: [u32; 8] = [64, 32, 16, 8, 4, 2, 1, 1];
/// Mip levels of the field: the breakers read it smoothed over a span that
/// grows with the distance out (as refraction straightens the crests), so
/// small steps and corners of the shoreline do not carry out to sea.
pub const MIPS: u32 = 7;
/// Vertical half range of the projection (m).
const DEPTH: f64 = 12000.0;
/// Texels across the wide mask (for the exposure), and metres per texel.
pub const WIDE_N: u32 = 1024;
pub const WIDE_TEXEL: f64 = 96.0;
pub const WIDE_HALF: f64 = WIDE_N as f64 * WIDE_TEXEL * 0.5;
/// Exposure grid: texels across (the middle of the wide mask; it covers the
/// field's square with some to spare).
const EXPOSE_N: u32 = 128;
/// Farthest land looked for (m): water open that far counts as open sea.
const REACH: f64 = 40_000.0;

/// What the renderer does for the field this frame.
#[derive(Clone, Copy, Debug)]
pub enum Step {
    /// draw the mask (from `build_vp`) and seed the flood
    Raster,
    /// one jump of the flood
    Jump(usize),
    /// the field the shaders read, from the finished flood
    Resolve,
}

#[derive(Clone, Copy)]
struct Square {
    c: DVec3,
    e: DVec3,
    n: DVec3,
    u: DVec3,
}

impl Square {
    /// Camera-relative position to the clip space (x east, y north) of the
    /// square of half side `half` around the same centre.
    fn vp(&self, cam: DVec3, half: f64) -> [[f32; 4]; 4] {
        let o = cam - self.c;
        let r0 = (self.e / half, o.dot(self.e) / half);
        let r1 = (self.n / half, o.dot(self.n) / half);
        let r2 = (self.u / (2.0 * DEPTH), 0.5 + o.dot(self.u) / (2.0 * DEPTH));
        let col = |a: f64, b: f64, c: f64, d: f64| [a as f32, b as f32, c as f32, d as f32];
        [col(r0.0.x, r1.0.x, r2.0.x, 0.0), col(r0.0.y, r1.0.y, r2.0.y, 0.0), col(r0.0.z, r1.0.z, r2.0.z, 0.0), col(r0.1, r1.1, r2.1, 1.0)]
    }
}

/// This frame's part of the field (main.rs hands it to the renderer).
pub struct ShoreFrame {
    pub step: Option<Step>,
    /// for `Step::Raster`: the projection to draw the mask with, and the new
    /// square's centre relative to the camera
    pub build_vp: [[f32; 4]; 4],
    /// the same for the wide mask
    pub build_vp_wide: [[f32; 4]; 4],
    pub build_c: DVec3,
    /// the projection of the field the shaders read, and whether there is one
    pub vp: [[f32; 4]; 4],
    pub on: bool,
}

#[derive(Default)]
pub struct Shore {
    /// fixed axes (and their origin) the squares snap to
    axes: Option<Square>,
    shown: Option<Square>,
    building: Option<(Square, usize)>,
    idle: u32,
}

impl Shore {
    pub fn frame(&mut self, cam: DVec3) -> ShoreFrame {
        // New axes when far from the old ones (the squares' "up" must stay up).
        if self.axes.is_none_or(|a| (cam - a.c).length() > 40_000.0) {
            let (e, n, u) = crate::geo::enu(cam);
            self.axes = Some(Square { c: cam, e, n, u });
        }
        let a = self.axes.unwrap();
        let snap = TEXEL * 16.0;
        let o = cam - a.c;
        let c = a.c + a.e * ((o.dot(a.e) / snap).round() * snap) + a.n * ((o.dot(a.n) / snap).round() * snap);
        let target = Square { c, ..a };
        if self.building.is_none() {
            // Rebuild once the camera has moved on, or now and then for tiles
            // that have streamed in since.
            let moved = self.shown.is_none_or(|s| {
                let d = cam - s.c;
                let up = d.dot(a.u);
                (d - a.u * up).length() > 320.0
            });
            self.idle += 1;
            if moved || self.idle > 40 {
                self.building = Some((target, 0));
                self.idle = 0;
            }
        }
        let mut step = None;
        let mut build = target;
        if let Some((sq, stage)) = self.building {
            build = sq;
            step = Some(match stage {
                0 => Step::Raster,
                s if s <= JUMPS.len() => Step::Jump(s - 1),
                _ => Step::Resolve,
            });
            if stage > JUMPS.len() {
                self.shown = Some(sq);
                self.building = None;
            } else {
                self.building = Some((sq, stage + 1));
            }
        }
        let shown = self.shown.unwrap_or(target);
        ShoreFrame { step, build_vp: build.vp(cam, HALF), build_vp_wide: build.vp(cam, WIDE_HALF), build_c: build.c - cam, vp: shown.vp(cam, HALF), on: self.shown.is_some() }
    }
}

// ------------------------------------------------------------------ GPU

pub struct ShoreGpu {
    /// our water seen from above: r = ground drawn here, g = water height
    /// (far below everything where there is none); render target
    pub mask: wgpu::TextureView,
    /// the same over the wide square (`WIDE_N`), for the exposure
    pub wide: wgpu::TextureView,
    /// the sea for the exposure (shore.wgsl `Sea`)
    sea: wgpu::Buffer,
    /// what the shaders read (common.wgsl `t_shore`): distance to the
    /// shore (m), seaward direction (east, north), 1; with mips
    pub view: wgpu::TextureView,
    p_seed: wgpu::ComputePipeline,
    p_jump: wgpu::ComputePipeline,
    p_resolve: wgpu::ComputePipeline,
    p_mip: wgpu::ComputePipeline,
    p_expose: wgpu::ComputePipeline,
    bg_expose: Vec<wgpu::BindGroup>,
    bg_seed: wgpu::BindGroup,
    bg_jump: Vec<wgpu::BindGroup>,
    bg_resolve: wgpu::BindGroup,
    bg_mips: Vec<wgpu::BindGroup>,
}

impl ShoreGpu {
    pub fn new(d: &wgpu::Device, q: &wgpu::Queue, mask_fmt: wgpu::TextureFormat) -> Self {
        let m = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("shore"), source: wgpu::ShaderSource::Wgsl(include_str!("shaders/shore.wgsl").into()) });
        let pipe = |e: &str| d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(e), layout: None, module: &m, entry_point: Some(e), compilation_options: Default::default(), cache: None });
        let (p_seed, p_jump, p_resolve, p_mip, p_expose) = (pipe("seed"), pipe("jump"), pipe("resolve"), pipe("mip"), pipe("expose"));
        let tex_n = |label: &str, n: u32, fmt: wgpu::TextureFormat, usage: wgpu::TextureUsages, mips: u32| {
            d.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: fmt,
                usage,
                view_formats: &[],
            })
        };
        let tex = |label: &str, fmt, usage, mips| tex_n(label, N, fmt, usage, mips);
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let wide = tex_n("shore wide mask", WIDE_N, mask_fmt, rt, 1).create_view(&Default::default());
        let expose = tex_n("shore exposure", EXPOSE_N, wgpu::TextureFormat::Rgba16Float, wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, 1).create_view(&Default::default());
        let sea = d.create_buffer(&wgpu::BufferDescriptor { label: Some("shore sea"), size: 32, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mask = tex("shore mask", mask_fmt, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, 1).create_view(&Default::default());
        let field = tex("shore field", wgpu::TextureFormat::Rgba16Float, wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING, MIPS);
        let view = field.create_view(&Default::default());
        let level = |m: u32| field.create_view(&wgpu::TextureViewDescriptor { base_mip_level: m, mip_level_count: Some(1), ..Default::default() });
        let seeds = |l: &str| d.create_buffer(&wgpu::BufferDescriptor { label: Some(l), size: (N * N * 4) as u64, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let bufs = [seeds("shore seeds a"), seeds("shore seeds b")];
        // shore.wgsl `U`: jump, size, metres per texel, farthest distance kept.
        let ubuf = |step: u32| {
            let b = d.create_buffer(&wgpu::BufferDescriptor { label: Some("shore u"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let mut bytes = [0u8; 16];
            bytes[0..4].copy_from_slice(&step.to_le_bytes());
            bytes[4..8].copy_from_slice(&N.to_le_bytes());
            bytes[8..12].copy_from_slice(&(TEXEL as f32).to_le_bytes());
            bytes[12..16].copy_from_slice(&MAX_D.to_le_bytes());
            q.write_buffer(&b, 0, &bytes);
            b
        };
        fn ent(b: u32, r: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry { binding: b, resource: r }
        }
        let u0 = ubuf(0);
        let bg_seed = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shore seed"),
            layout: &p_seed.get_bind_group_layout(0),
            entries: &[ent(0, u0.as_entire_binding()), ent(1, wgpu::BindingResource::TextureView(&mask)), ent(3, bufs[0].as_entire_binding())],
        });
        let bg_jump = JUMPS
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let u = ubuf(s);
                d.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shore jump"),
                    layout: &p_jump.get_bind_group_layout(0),
                    entries: &[ent(0, u.as_entire_binding()), ent(2, bufs[i % 2].as_entire_binding()), ent(3, bufs[1 - i % 2].as_entire_binding())],
                })
            })
            .collect();
        let bg_resolve = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shore resolve"),
            layout: &p_resolve.get_bind_group_layout(0),
            entries: &[
                ent(0, u0.as_entire_binding()),
                ent(2, bufs[JUMPS.len() % 2].as_entire_binding()),
                ent(4, wgpu::BindingResource::TextureView(&level(0))),
                ent(6, sea.as_entire_binding()),
                ent(9, wgpu::BindingResource::TextureView(&expose)),
            ],
        });
        // One band of the exposure grid in each jump frame.
        let bg_expose = (0..JUMPS.len() as u32)
            .map(|b| {
                let u = ubuf(b);
                d.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shore expose"),
                    layout: &p_expose.get_bind_group_layout(0),
                    entries: &[
                        ent(0, u.as_entire_binding()),
                        ent(6, sea.as_entire_binding()),
                        ent(7, wgpu::BindingResource::TextureView(&wide)),
                        ent(8, wgpu::BindingResource::TextureView(&expose)),
                    ],
                })
            })
            .collect();
        let bg_mips = (1..MIPS)
            .map(|m| {
                d.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shore mip"),
                    layout: &p_mip.get_bind_group_layout(0),
                    entries: &[ent(4, wgpu::BindingResource::TextureView(&level(m))), ent(5, wgpu::BindingResource::TextureView(&level(m - 1)))],
                })
            })
            .collect();
        Self { mask, wide, sea, view, p_seed, p_jump, p_resolve, p_mip, p_expose, bg_expose, bg_seed, bg_jump, bg_resolve, bg_mips }
    }

    /// This frame's compute part of `step` (after the masks are drawn, for
    /// `Raster`). `ocean`: Globals `ocean` rows 0 and 1 (wind direction;
    /// wind, significant wave height), taken when a rebuild starts.
    pub fn compute(&self, q: &wgpu::Queue, enc: &mut wgpu::CommandEncoder, step: Step, ocean: [[f32; 4]; 2]) {
        if let Step::Raster = step {
            // Breakers on an open coast: swell always runs in, more with the sea.
            let hopen = (0.45 + 0.6 * ocean[1][1]).min(3.0);
            let mut b = [0u8; 32];
            let f = [ocean[0][2], ocean[0][3], ocean[1][0], hopen, WIDE_TEXEL as f32];
            for (i, v) in f.iter().enumerate() {
                b[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            b[20..24].copy_from_slice(&EXPOSE_N.to_le_bytes());
            b[24..28].copy_from_slice(&WIDE_N.to_le_bytes());
            b[28..32].copy_from_slice(&((REACH / WIDE_TEXEL) as f32).to_le_bytes());
            q.write_buffer(&self.sea, 0, &b);
        }
        let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("shore"), timestamp_writes: None });
        let (p, bg) = match step {
            Step::Raster => (&self.p_seed, &self.bg_seed),
            Step::Jump(i) => (&self.p_jump, &self.bg_jump[i]),
            Step::Resolve => (&self.p_resolve, &self.bg_resolve),
        };
        cp.set_pipeline(p);
        cp.set_bind_group(0, bg, &[]);
        cp.dispatch_workgroups(N / 8, N / 8, 1);
        if let Step::Jump(i) = step {
            cp.set_pipeline(&self.p_expose);
            cp.set_bind_group(0, &self.bg_expose[i], &[]);
            cp.dispatch_workgroups(EXPOSE_N.div_ceil(8), (EXPOSE_N / JUMPS.len() as u32).div_ceil(8), 1);
        }
        if let Step::Resolve = step {
            cp.set_pipeline(&self.p_mip);
            for (i, bg) in self.bg_mips.iter().enumerate() {
                let s = (N >> (i + 1)).max(1);
                cp.set_bind_group(0, bg, &[]);
                cp.dispatch_workgroups(s.div_ceil(8), s.div_ceil(8), 1);
            }
        }
    }
}
