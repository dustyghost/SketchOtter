//! Paired dense-wire costs on the actual startup sphere, including a displayed edit.
use super::*;

#[test]
#[ignore = "manual guarded default-sphere wireframe comparison"]
fn gpu_load_profile_wire_sphere() {
    let level = env_limit("CLAY_REGION_LEVEL", 3);
    assert!(matches!(level, 2..=4));
    crate::sculpt::capacity::initialize(benchmark_capacity(level, 2_000_000, 4_000_000).unwrap())
        .unwrap();
    let out = std::path::PathBuf::from(std::env::var_os("CLAY_REGION_OUTPUT").unwrap());
    let headless = Headless::new();
    let size = [800, 600];
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
    let mut rows = Vec::new();
    for arm in ["solid", "visible_edges", "visible_surface"] {
        let mut doc = document(fixture("sphere", level));
        let mut gpu = Gpu::new(&headless.state.device, &headless.state.queue, COLOR);
        let mut display = Arc::new(DisplayMesh::new(&doc));
        gpu.upload(&display);
        gpu.queue.submit([]);
        headless.wait();
        assert!(
            gpu.use_wire_surface(false, &Preview::default()),
            "expected regular subdivided quads"
        );
        gpu.ordered_wire_quads = None;
        gpu.force_wire_lines = arm == "visible_edges";
        let mut timing = Measurements::default();
        for frame in 0..9 {
            let camera = Camera {
                yaw: frame as f32 * 0.05,
                ..Default::default()
            };
            let (r, u, v) = camera.basis();
            let p = Params::new(rect, r, u, v, 240., rect.center(), display.depth_extent);
            let stage = match frame {
                0 => "first",
                8 => "sculpt_and_draw",
                _ => "orbit",
            };
            timing.time(stage, || {
                if frame == 8 {
                    assert!(doc.apply_dab(&brush(Kind::Standard), dab(0)));
                    display = Arc::new(display.during_stroke(&doc).unwrap());
                    gpu.upload(&display);
                }
                if (frame == 0 || frame == 8) && gpu.force_wire_lines {
                    // The previous renderer always built both topologies here.
                    gpu.upload_edges(false);
                    gpu.upload_edges(true);
                }
                gpu.paint(
                    &p,
                    size,
                    arm != "solid",
                    WireMode::Visible,
                    false,
                    &Preview::default(),
                );
                headless.wait();
            });
            if frame == 7 && level == 3 {
                image::save_buffer(
                    out.with_file_name(format!("{arm}.png")),
                    &gpu.read(),
                    size[0],
                    size[1],
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
        let bytes: u64 = gpu
            .edges
            .iter()
            .chain(&gpu.triangle_edges)
            .flatten()
            .map(|(b, _)| b.size())
            .sum();
        rows.push(serde_json::json!({"arm":arm,"level":level,"source_triangles":doc.mesh().triangles.len(),"quads":doc.mesh().quads.len(),"edge_buffer_bytes":bytes,"timings":timing.summary(),"memory":memory()}));
    }
    std::fs::write(out,serde_json::to_vec_pretty(&serde_json::json!({"driver": format!("{:?}",headless.state.adapter.get_info()),"size":size,"rows":rows})).unwrap()).unwrap();
}
