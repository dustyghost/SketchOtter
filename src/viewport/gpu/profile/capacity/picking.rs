//! Orbit-pointer sequences with the original screen index and reusable world bounds.
use super::*;
use crate::viewport::scene::cache::Cache;

#[test]
#[ignore = "manual performance measurement"]
fn gpu_load_profile_world_picking() {
    let level = env_limit("CLAY_REGION_LEVEL", 3);
    crate::sculpt::capacity::initialize(benchmark_capacity(level, 2_000_000, 4_000_000).unwrap())
        .unwrap();
    let output = std::env::var("CLAY_REGION_OUTPUT").unwrap();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
    let context = egui::Context::default();
    let mut doc = document(fixture("hidden", level));
    let mut rows = Vec::new();
    let brush = brush(Kind::Standard);
    for revision in 0..2 {
        if revision == 1 {
            assert!(doc.apply_dab(&brush, dab(0)));
            doc.end_stroke();
        }
        let display = Arc::new(DisplayMesh::new(&doc));
        for perspective in [false, true] {
            let mut oracle = None;
            for reference in if perspective == (revision == 0) {
                [true, false]
            } else {
                [false, true]
            } {
                let mut cache = Cache::for_picking_comparison(reference);
                let mut times = Measurements::default();
                let mut hits = Vec::new();
                for frame in 0..10 {
                    let start = Instant::now();
                    let camera = Camera {
                        perspective,
                        yaw: frame as f32 * 0.065,
                        pitch: 0.12,
                        ..Default::default()
                    };
                    let scene = times.time("prepare", || {
                        cache.get(rect, &camera, display.clone(), false)
                    });
                    let point = rect.center() + egui::vec2(3., 2.);
                    let far = point + egui::vec2(1400., 1400.);
                    let (a, b) = if frame % 2 == 0 {
                        (point, far)
                    } else {
                        (far, point)
                    };
                    let first = times.time(
                        if frame % 2 == 0 {
                            "first_hit"
                        } else {
                            "first_miss"
                        },
                        || scene.pick_model(a),
                    );
                    let second = times.time("second_distinct", || scene.pick_model(b));
                    let warm = times.time("repeated", || scene.pick_model(b));
                    assert_eq!(second, warm);
                    hits.push((first, second, scene.pick_face(point)));
                    let mut painted = false;
                    let _ = times.time("footprint", || {
                        context.run(
                            egui::RawInput {
                                screen_rect: Some(rect),
                                ..Default::default()
                            },
                            |ctx| {
                                painted = scene.brush_cursor_with_footprint(
                                    &ctx.layer_painter(egui::LayerId::background()),
                                    point,
                                    50.,
                                    &brush,
                                    egui::Color32::WHITE,
                                    true,
                                );
                            },
                        )
                    });
                    assert!(painted);
                    times
                        .values
                        .entry("orbit_pointer".into())
                        .or_default()
                        .push(start.elapsed().as_secs_f64() * 1000.);
                }
                if let Some(expected) = &oracle {
                    assert_eq!(expected, &hits);
                } else {
                    oracle = Some(hits);
                }
                rows.push(serde_json::json!({"reference":reference,"perspective":perspective,"revision":revision,"timings":times.summary()}));
            }
        }
    }
    std::fs::write(output, serde_json::to_vec(&serde_json::json!({"triangles":doc.mesh().triangles.len(),"rows":rows,"memory":memory()})).unwrap()).unwrap();
}
