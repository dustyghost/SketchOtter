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
