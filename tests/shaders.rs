//! Parses and validates every shader without needing a GPU.
fn check(name: &str, src: &str) -> bool {
    match naga::front::wgsl::parse_str(src) {
        Err(e) => {
            eprintln!("--- {name}\n{}", e.emit_to_string(src));
            false
        }
        Ok(m) => match naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&m) {
            Err(e) => {
                eprintln!("--- {name}\n{}", e.emit_to_string(src));
                false
            }
            Ok(_) => true,
        },
    }
}

#[test]
fn shaders_validate() {
    let common = include_str!("../src/shaders/common.wgsl");
    // Modules that draw the sky get sky.wgsl too, as in render.rs.
    let sky = include_str!("../src/shaders/sky.wgsl");
    // The constants render.rs injects before every module: the multisample
    // count (per-sample loops) and the cloud-shadow map side.
    // Mirrors render.rs's cfg: phones build at one sample and a 512-side
    // cloud-shadow map.
    let (samples, cs_res, det_n) = if cfg!(target_os = "android") { (1, 512, 512) } else { (4, 1024, 1024) };
    let ov = format!("const SAMPLES = {samples}u;\nconst CS_RES = {cs_res}.0;\n");
    let ov_det = format!("{ov}const N = {det_n};\n");
    let mut ok = check("noise3d", include_str!("../src/shaders/noise3d.wgsl"));
    // Mirrors render.rs's assembly exactly: every shading pass gets
    // {ov}{common}, and ONLY resolve (self-contained) goes without common —
    // 1.0.8 shipped dead because render.rs drifted from this mirror and
    // dropped common from composite/trees. Keep the two in step.
    ok &= check("resolve", &format!("{ov}{}", include_str!("../src/shaders/resolve.wgsl")));
    ok &= check("detail", &format!("{ov_det}{}", include_str!("../src/shaders/detail.wgsl")));
    ok &= check("ocean", include_str!("../src/shaders/ocean.wgsl"));
    ok &= check("shore", include_str!("../src/shaders/shore.wgsl"));
    for (name, src) in [
        ("lut", include_str!("../src/shaders/lut.wgsl")),
        ("terrain", include_str!("../src/shaders/terrain.wgsl")),
        // The safe-mode terrain (1.0.16): same bindings and entry points as
        // the full one, no marches.
        ("terrain lite", include_str!("../src/shaders/terrain_lite.wgsl")),
        ("photo", include_str!("../src/shaders/photo.wgsl")),
        ("post", include_str!("../src/shaders/post.wgsl")),
        ("particles_sim", include_str!("../src/shaders/particles_sim.wgsl")),
        ("particles_draw", include_str!("../src/shaders/particles_draw.wgsl")),
        ("roads", include_str!("../src/shaders/roads.wgsl")),
        ("trees", include_str!("../src/shaders/trees.wgsl")),
        ("vol", include_str!("../src/shaders/vol.wgsl")),
        ("ao", include_str!("../src/shaders/ao.wgsl")),
        ("taa", include_str!("../src/shaders/taa.wgsl")),
        ("airfield", include_str!("../src/shaders/airfield.wgsl")),
        ("lights", include_str!("../src/shaders/lights.wgsl")),
    ] {
        ok &= check(name, &format!("{ov}{common}\n{src}"));
    }
    // As in render.rs, buildings and instanced solids share the facade shading.
    let facade = include_str!("../src/shaders/facade.wgsl");
    for (name, src) in [("buildings", include_str!("../src/shaders/buildings.wgsl")), ("inst", include_str!("../src/shaders/inst.wgsl"))] {
        ok &= check(name, &format!("{ov}{common}\n{facade}\n{src}"));
    }
    // The G-buffer sample copies that composite's bindings 5-8 stand for, and
    // the depth resolve's source, switch with MSAA (render.rs injects both);
    // validate every combination of them wherever the test runs, not just the
    // host's config — a phone at MSAA 1 is where these have to hold.
    for (samples, ms_ty, ms_depth_ty, ms_idx) in [
        (1u32, "texture_2d<f32>", "texture_depth_2d", "0"),
        (4, "texture_multisampled_2d<f32>", "texture_depth_multisampled_2d", "i"),
    ] {
        let ov = format!("const SAMPLES = {samples}u;\nconst CS_RES = 512.0;\n");
        let comp_ms = format!(
            "@group(1) @binding(5) var t_color_ms: {ms_ty};\n@group(1) @binding(6) var t_aux_ms: {ms_ty};\n\
             @group(1) @binding(7) var t_depth_ms: {ms_depth_ty};\n@group(1) @binding(8) var t_amb_ms: {ms_ty};\n\
             fn ms_color(px: vec2i, i: u32) -> vec4f {{ return textureLoad(t_color_ms, px, {ms_idx}); }}\n\
             fn ms_aux(px: vec2i, i: u32) -> vec4f {{ return textureLoad(t_aux_ms, px, {ms_idx}); }}\n\
             fn ms_depth(px: vec2i, i: u32) -> f32 {{ return textureLoad(t_depth_ms, px, {ms_idx}); }}\n\
             fn ms_amb(px: vec2i, i: u32) -> f32 {{ return textureLoad(t_amb_ms, px, {ms_idx}).r; }}\n"
        );
        ok &= check(&format!("composite msaa{samples}"), &format!("{ov}{common}\n{sky}\n{comp_ms}\n{}", include_str!("../src/shaders/composite.wgsl")));
        // The safe-mode composite (no sky.wgsl, no sample copies).
        ok &= check(&format!("composite lite msaa{samples}"), &format!("{ov}{common}\n{}", include_str!("../src/shaders/composite_lite.wgsl")));
        let resolve = if samples > 1 {
            include_str!("../src/shaders/resolve.wgsl").to_string()
        } else {
            // render.rs's MSAA-1 variant: a plain copy off a texture_depth_2d.
            "@group(0) @binding(0) var t_d: texture_depth_2d;\n\
             @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {\n\
               let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));\n\
               return vec4f(p * 2.0 - 1.0, 0.0, 1.0);\n\
             }\n\
             @fragment fn fs(@builtin(position) p: vec4f) -> @builtin(frag_depth) f32 {\n\
               return textureLoad(t_d, vec2i(p.xy), 0);\n\
             }\n"
                .to_string()
        };
        ok &= check(&format!("resolve msaa{samples}"), &format!("{ov}{resolve}"));
    }
    for (name, src) in [("composite", include_str!("../src/shaders/composite.wgsl")), ("clouds", include_str!("../src/shaders/clouds.wgsl"))] {
        ok &= check(name, &format!("{ov}{common}\n{sky}\n{src}"));
    }
    // plane.wgsl takes the 747 deck's string table: checked in b747::deck's tests.
    assert!(ok, "shader errors above");
}
