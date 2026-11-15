//! Photo-scanned ground materials from Poly Haven (CC0), downloaded once into
//! the tile cache and swapped into the ground material arrays in place of the
//! procedural layers from detail.wgsl as they arrive. Offline, or when a
//! download fails, the procedural layer simply stays.
//!
//! The scans are turned into detail around the satellite colour rather than
//! colour of their own: each texel is divided by a blurred copy of the scan
//! (per channel, in linear light), so only structure finer than a satellite
//! pixel is left. That keeps the photo's colour as the average, and removes the
//! large blotches that make a repeating texture recognisable from afar.
use crate::fetch::Fetcher;
use anyhow::{anyhow, Result};
use crossbeam_channel::{unbounded, Receiver};
use image::imageops::FilterType;

/// Texels per side (the arrays' size, DET_N in render.rs).
pub const N: u32 = 1024;
/// Stored colour is ratio * this, so ratios up to ~2.9 fit in 8 bits.
const RATIO_SCALE: f32 = 0.35;

pub struct Spec {
    /// Layer in the ground arrays (see L_* in terrain.wgsl).
    pub layer: u32,
    /// Poly Haven asset id.
    pub id: &'static str,
    /// Metres per repeat (the scan's real-world size).
    pub size: f32,
    /// Metres of relief between height 0 and 1.
    pub range: f32,
    /// Normal map strength.
    pub bump: f32,
    /// How much of the scan's variation to keep (0..1).
    pub contrast: f32,
    /// How much of its colour shifts (beyond brightness) to keep (0..1).
    pub chroma: f32,
    /// Structure larger than about this (m, Gaussian sigma) is taken out and
    /// left to the satellite photo (and, for close-up scans, to the mid-scale
    /// layer). Mid-scale scans lose their large-scale height too.
    pub high_pass: f32,
}

/// Close-up layers 0-7 (cereal, layer 6, has no fitting scan and stays
/// procedural) and mid-scale layers 8-12 from aerial scans, which add the
/// tussocks, bare patches, stones and fractures between a few centimetres and
/// the ~1 m satellite pixel.
pub const SPECS: [Spec; 12] = [
    Spec { layer: 0, id: "leafy_grass", size: 2.0, range: 0.05, bump: 0.55, contrast: 0.9, chroma: 0.5, high_pass: 0.45 },
    Spec { layer: 1, id: "brown_mud_dry", size: 1.3, range: 0.05, bump: 0.8, contrast: 0.9, chroma: 0.5, high_pass: 0.45 },
    Spec { layer: 2, id: "rock_face_03", size: 2.7, range: 0.3, bump: 1.0, contrast: 0.9, chroma: 0.5, high_pass: 0.45 },
    Spec { layer: 3, id: "sand_01", size: 1.5, range: 0.03, bump: 0.8, contrast: 0.8, chroma: 0.5, high_pass: 0.45 },
    Spec { layer: 4, id: "asphalt_02", size: 3.0, range: 0.008, bump: 0.6, contrast: 0.8, chroma: 0.5, high_pass: 0.45 },
    Spec { layer: 5, id: "forest_leaves_02", size: 3.0, range: 0.05, bump: 0.6, contrast: 0.8, chroma: 0.5, high_pass: 0.45 },
    Spec { layer: 7, id: "brown_mud_leaves_01", size: 1.3, range: 0.04, bump: 0.6, contrast: 0.9, chroma: 0.5, high_pass: 0.45 },
    Spec { layer: 8, id: "aerial_grass_rock", size: 15.0, range: 0.15, bump: 0.7, contrast: 0.55, chroma: 0.3, high_pass: 1.0 },
    Spec { layer: 9, id: "aerial_ground_rock", size: 20.0, range: 0.15, bump: 0.7, contrast: 0.55, chroma: 0.3, high_pass: 1.0 },
    Spec { layer: 10, id: "aerial_rocks_02", size: 50.0, range: 1.2, bump: 1.0, contrast: 0.55, chroma: 0.3, high_pass: 1.5 },
    Spec { layer: 11, id: "coast_sand_01", size: 15.0, range: 0.1, bump: 0.8, contrast: 0.55, chroma: 0.3, high_pass: 1.0 },
    Spec { layer: 12, id: "aerial_asphalt_01", size: 30.0, range: 0.01, bump: 0.6, contrast: 0.55, chroma: 0.3, high_pass: 1.0 },
];

/// One finished layer: N x N RGBA8 for each array.
pub struct Layer {
    pub layer: u32,
    /// size, range, chroma, contrast: see G.det in common.wgsl.
    pub params: [f32; 4],
    /// rgb = colour ratio * RATIO_SCALE (linear), a = height 0..1.
    pub alb: Vec<u8>,
    /// rg = normal xy (texture space), b = cavity, a = height.
    pub nrm: Vec<u8>,
}

/// Start loading every scan on worker threads; layers arrive on the channel.
pub fn spawn(fetch: Fetcher) -> Receiver<Layer> {
    let (tx, rx) = unbounded();
    let fetch = std::sync::Arc::new(fetch);
    for s in &SPECS {
        let (tx, fetch) = (tx.clone(), fetch.clone());
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            match load(&fetch, s) {
                Ok(l) => {
                    log::info!("ground scan {} ready in {:.0} ms", s.id, t0.elapsed().as_secs_f64() * 1000.0);
                    let _ = tx.send(l);
                }
                Err(e) => log::warn!("ground scan {}: {e} (keeping the procedural layer)", s.id),
            }
        });
    }
    rx
}

fn load(fetch: &Fetcher, s: &Spec) -> Result<Layer> {
    let id = s.id;
    let files = fetch.get_cached(&format!("ph/{id}/files.json"), &format!("https://api.polyhaven.com/files/{id}"))?;
    let j: serde_json::Value = serde_json::from_slice(&files)?;
    let get = |map: &str| -> Result<image::DynamicImage> {
        let url = j[map]["1k"]["jpg"]["url"].as_str().ok_or_else(|| anyhow!("no 1k {map}"))?;
        // The name comes from the server's JSON: keep only plain file-name characters so a
        // hostile reply (`..\`, `C:`, ...) can't point the cache write outside its folder.
        let name: String = url.rsplit(['/', '\\']).next().unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')).collect();
        let name = if name.is_empty() || name.starts_with('.') { format!("{map}.jpg") } else { name };
        let data = fetch.get_cached(&format!("ph/{id}/{name}"), url)?;
        let img = image::load_from_memory(&data)?;
        Ok(if img.width() == N && img.height() == N { img } else { img.resize_exact(N, N, FilterType::Triangle) })
    };
    let diff = get("Diffuse")?.to_rgb8();
    let nor = get("nor_gl")?.to_rgb8();
    let disp = get("Displacement")?.to_luma16();
    let n = N as usize;

    // Colour: linear light, divided by its blurred self.
    let lut: Vec<f32> = (0..256).map(|i| srgb_to_linear(i as f32 / 255.0)).collect();
    let mut ch: [Vec<f32>; 3] = std::array::from_fn(|c| diff.pixels().map(|p| lut[p[c] as usize]).collect());
    let sigma = s.high_pass / s.size * N as f32;
    let mut alb = vec![0u8; n * n * 4];
    for (c, plane) in ch.iter_mut().enumerate() {
        let mut low = plane.clone();
        gauss_wrap(&mut low, n, sigma);
        for i in 0..n * n {
            let r = plane[i] / low[i].max(1e-4);
            alb[i * 4 + c] = ((r * RATIO_SCALE).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }

    // Height: (mid-scale scans: high-passed, then) stretched to 0..1 between
    // its 1st and 99th percentiles.
    let mut h: Vec<f32> = disp.pixels().map(|p| p[0] as f32 / 65535.0).collect();
    if s.layer >= 8 {
        let mut low = h.clone();
        gauss_wrap(&mut low, n, sigma * 2.0);
        for (v, l) in h.iter_mut().zip(&low) {
            *v -= l;
        }
    }
    let mut sorted = h.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let (lo, hi) = (sorted[n * n / 100], sorted[n * n * 99 / 100]);
    let span = (hi - lo).max(1e-4);
    for v in h.iter_mut() {
        *v = ((*v - lo) / span).clamp(0.0, 1.0);
    }
    // Cavity: how far below its surroundings (a few mm around) a texel sits.
    let mut near = h.clone();
    gauss_wrap(&mut near, n, 3.0);

    let mut nrm = vec![0u8; n * n * 4];
    for i in 0..n * n {
        let p = nor.as_raw();
        // OpenGL normal maps have +y toward the top of the image; texture v
        // runs down the image (and north on the ground).
        let x = (p[i * 3] as f32 / 255.0 * 2.0 - 1.0) * s.bump;
        let y = -(p[i * 3 + 1] as f32 / 255.0 * 2.0 - 1.0) * s.bump;
        let z = (p[i * 3 + 2] as f32 / 255.0 * 2.0 - 1.0).max(0.05);
        let l = (x * x + y * y + z * z).sqrt();
        let cav = (1.0 - (near[i] - h[i]) * 2.2).clamp(0.0, 1.0);
        let hb = (h[i] * 255.0 + 0.5) as u8;
        nrm[i * 4] = ((x / l * 0.5 + 0.5) * 255.0 + 0.5) as u8;
        nrm[i * 4 + 1] = ((y / l * 0.5 + 0.5) * 255.0 + 0.5) as u8;
        nrm[i * 4 + 2] = (cav * 255.0 + 0.5) as u8;
        nrm[i * 4 + 3] = hb;
        alb[i * 4 + 3] = hb;
    }
    Ok(Layer { layer: s.layer, params: [s.size, s.range, s.chroma, s.contrast], alb, nrm })
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Approximate Gaussian blur of a periodic n x n plane: three box passes each way.
fn gauss_wrap(v: &mut [f32], n: usize, sigma: f32) {
    // Box width w for three passes: sigma^2 = 3 (w^2 - 1) / 12.
    let r = ((((4.0 * sigma * sigma + 1.0).sqrt() - 1.0) * 0.5).round() as usize).clamp(1, n / 2 - 1);
    let mut tmp = vec![0f32; n];
    let mut line = vec![0f32; n];
    for _ in 0..3 {
        for pass in 0..2 {
            for k in 0..n {
                let idx = |i: usize| if pass == 0 { k * n + i } else { i * n + k };
                for i in 0..n {
                    line[i] = v[idx(i)];
                }
                let w = (2 * r + 1) as f32;
                let mut s: f32 = (0..=2 * r).map(|j| line[(j + n - r) % n]).sum();
                for i in 0..n {
                    tmp[i] = s / w;
                    s += line[(i + r + 1) % n] - line[(i + n - r) % n];
                }
                for i in 0..n {
                    v[idx(i)] = tmp[i];
                }
            }
        }
    }
}
