//! Compare release kernels with one reference topology over accumulated edits.
use super::*;
use crate::sculpt::SurfaceShadingTopology;

#[test]
#[ignore = "manual guarded release fan measurements"]
fn gpu_load_profile_release_fans() {
    let level = env_limit("CLAY_REGION_LEVEL", 3);
    crate::sculpt::capacity::initialize(benchmark_capacity(level, 2_000_000, 4_000_000).unwrap())
        .unwrap();
    let mut rows = Vec::new();
    for fixture_name in ["sphere", "organic", "hidden", "sharpness"] {
        let mut geo = fixture(
            if fixture_name == "sharpness" {
                "sphere"
            } else {
                fixture_name
            },
            level,
        );
        let mesh = &mut geo.mesh;
        let mut setup = Measurements::default();
        let topo = setup.time("topology_and_reference_angles", || {
            SurfaceShadingTopology::new(mesh)
        });
        let original = mesh.vertices.clone();
        for edit in 0..5 {
            // Includes consecutive local edits, a sharpness split/merge cycle
            // and a broad deformation while keeping the original angle cache.
            for (i, v) in mesh.vertices.iter_mut().enumerate() {
                let p = original[i];
                if edit == 4 {
                    v.0 = p.0 + 0.025 * (6. * p.1).sin();
                } else if fixture_name == "sharpness" {
                    if p.2 > 0.91 && p.0.abs() < 0.08 {
                        v.2 = p.2 + if edit % 2 == 0 { 0.035 } else { 0. };
                    }
                } else if p.2 > 0.9 && (p.0 - 0.025 * edit as f32).abs() < 0.12 && mesh.mask[i] < 1.
                {
                    v.2 += 0.003 * (p.1 * 25.).cos();
                }
            }
            let mut metrics = Measurements::default();
            let mut first = None;
            let mut fan_count = 0;
            for candidate in if edit % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let start = Instant::now();
                let (shading, stages) = if candidate {
                    let mut stages = Vec::new();
                    let mut clock = Instant::now();
                    let shading = topo.compact_shade(mesh, |name| {
                        stages.push((name, clock.elapsed().as_secs_f64() * 1000.));
                        clock = Instant::now();
                    });
                    (shading, stages)
                } else {
                    topo.profile_shade(mesh, true)
                };
                let arm = if candidate { "compact" } else { "baseline" };
                metrics
                    .values
                    .entry(format!("{arm}.total"))
                    .or_default()
                    .push(start.elapsed().as_secs_f64() * 1000.);
                for (name, ms) in stages {
                    metrics
                        .values
                        .entry(format!("{arm}.{name}"))
                        .or_default()
                        .push(ms);
                }
                if let Some(previous) = first.take() {
                    let prev: crate::sculpt::SurfaceShading = previous;
                    assert_eq!(prev.triangles, shading.triangles);
                    assert_eq!(prev.vertices.len(), shading.vertices.len());
                    for ((a, v), (b, w)) in prev.vertices.iter().zip(&shading.vertices) {
                        assert_eq!(a, b);
                        assert_eq!(
                            [v.0, v.1, v.2].map(f32::to_bits),
                            [w.0, w.1, w.2].map(f32::to_bits)
                        );
                    }
                } else {
                    fan_count = shading.vertices.len();
                    first = Some(shading);
                }
            }
            let (angle_bytes, unchanged) = topo.angle_cache_stats(mesh);
            rows.push(serde_json::json!({"fixture":fixture_name,"edit":edit,"source_vertices":mesh.vertices.len(),"source_triangles":mesh.triangles.len(),"unchanged_angle_faces":unchanged,"angle_cache_bytes":angle_bytes,"fans":fan_count,"setup":setup.summary(),"times":metrics.summary(),"memory":memory()}));
        }
    }
    std::fs::write(
        std::env::var_os("CLAY_REGION_OUTPUT").unwrap(),
        serde_json::to_vec(&serde_json::json!({"rows":rows,"memory":memory()})).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "manual dense release captures compared to freshly reconstructed canonical shading"]
fn gpu_load_profile_release_visual() {
    let level = env_limit("CLAY_REGION_LEVEL", 3);
    crate::sculpt::capacity::initialize(benchmark_capacity(level, 2_000_000, 4_000_000).unwrap())
        .unwrap();
    let output = std::path::PathBuf::from(std::env::var_os("CLAY_REGION_OUTPUT").unwrap());
    let headless = Headless::new();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320., 240.));
    let mut rows = Vec::new();
    for name in ["sphere", "organic", "hidden"] {
        let mut doc = document(fixture(name, level));
        let original = DisplayMesh::new(&doc);
        let mut brush = brush(Kind::Clay);
        brush.strength = 0.85;
        for i in 0..4 {
            doc.apply_dab(&brush, dab(i));
        }
        let live = original.during_stroke(&doc).unwrap();
        doc.end_stroke();
        let released = Arc::new(live).after_stroke(&doc).unwrap();
        let mut reference = (*released).clone();
        let canonical = doc.mesh().surface_shading();
        let geometry = Arc::make_mut(&mut reference.geometry);
        geometry.shading.triangles = Arc::new(
            geometry
                .source_faces
                .iter()
                .map(|&f| canonical.triangles[f])
                .collect(),
        );
        geometry.shading.vertices = Arc::new(canonical.vertices);
        assert_eq!(released.shading.triangles, reference.shading.triangles);
        let a = released.vertices(released.vertex_stride());
        let b = reference.vertices(reference.vertex_stride());
        assert_eq!(
            bytemuck::cast_slice::<f32, u32>(&a),
            bytemuck::cast_slice::<f32, u32>(&b)
        );
        drop(a);
        drop(b);
        let scene = Scene::from_geometry(
            rect,
            &Camera {
                yaw: 0.44,
                pitch: 0.15,
                perspective: true,
                ..Default::default()
            },
            released.clone(),
            false,
        );
        let mut params = Params::new(
            rect,
            scene.right,
            scene.up,
            scene.view,
            scene.scale,
            scene.center,
            released.depth_extent,
        );
        params.set_perspective(scene.eye_distance, released.depth_extent);
        let mut gpu = Gpu::new(&headless.state.device, &headless.state.queue, COLOR);
        for split in [false, true] {
            let mut render = |surface: &Arc<DisplayMesh>| {
                gpu.upload(surface);
                if split {
                    gpu.upload_meshes(surface, 4 << 20);
                }
                gpu.paint(
                    &params,
                    [320, 240],
                    false,
                    WireMode::Visible,
                    false,
                    &Preview::default(),
                );
                gpu.read()
            };
            let pixels = render(&released);
            let other = render(&Arc::new(reference.clone()));
            assert_eq!(pixels, other);
            let count = pixels
                .chunks_exact(4)
                .filter(|p| p[..3] != params.background)
                .count();
            assert!(count > 1000);
            rows.push(serde_json::json!({"fixture":name,"split":split,"drawn_pixels":count,"different_pixels":0,"triangles":released.mesh.triangles.len()}));
            if !split {
                image::save_buffer(
                    output.with_file_name(format!("release-{name}.png")),
                    &pixels,
                    320,
                    240,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
    std::fs::write(
        output,
        serde_json::to_vec(&serde_json::json!({"rows":rows,"memory":memory()})).unwrap(),
    )
    .unwrap();
}
