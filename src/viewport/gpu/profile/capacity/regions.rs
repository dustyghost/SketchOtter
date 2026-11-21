use super::*;

fn rgba_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |a, &b| {
        (a ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

#[test]
#[ignore = "manual end-to-end display and buffer update measurements"]
fn gpu_load_profile_changed_regions() {
    let level = env_limit("CLAY_REGION_LEVEL", 3);
    crate::sculpt::capacity::initialize(benchmark_capacity(level, 2_000_000, 4_000_000).unwrap())
        .unwrap();
    let headless = Headless::new();
    let mut rows = Vec::new();
    for wide in [false, true] {
        let mut expected = None;
        for reference in if wide { [false, true] } else { [true, false] } {
            let mut doc = document(fixture("hidden", level));
            let mut gpu = Gpu::new(&headless.state.device, &headless.state.queue, COLOR);
            let mut phases = Measurements::default();
            let mut live = phases.time("initial_display", || Arc::new(DisplayMesh::new(&doc)));
            phases.time("initial_upload", || {
                gpu.upload(&live);
                gpu.queue.submit([]);
                headless.wait()
            });
            let mut results = Vec::new();
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(192., 144.));
            let (right, up, view) = Camera::default().basis();
            let params = Params::new(rect, right, up, view, 54., rect.center(), live.depth_extent);
            for (stroke, kind) in [Kind::Standard, Kind::Clay, Kind::Mask, Kind::Paint]
                .into_iter()
                .enumerate()
            {
                let brush = brush(kind);
                for frame in 0..8 {
                    let mut touch = dab(frame);
                    if wide {
                        touch.radius = 0.72;
                    }
                    phases.time(&format!("{kind:?}.edit"), || {
                        doc.apply_dab(&brush, touch);
                        if frame % 3 == 1 {
                            touch.center.0 += 0.025;
                            doc.apply_dab(&brush, touch);
                        }
                    });
                    if reference {
                        Arc::make_mut(&mut live).disable_region_input();
                    }
                    let overall = Instant::now();
                    let next = phases.time(&format!("{kind:?}.display"), || {
                        Arc::new(live.during_stroke(&doc).unwrap())
                    });
                    gpu.upload_trace.start();
                    phases.time(&format!("{kind:?}.upload"), || gpu.upload(&next));
                    let cpu_stages = gpu.upload_trace.stop();
                    phases.time(&format!("{kind:?}.submit_wait"), || {
                        gpu.queue.submit([]);
                        headless.wait()
                    });
                    let total = overall.elapsed().as_secs_f64() * 1000.;
                    phases
                        .values
                        .entry(format!("{kind:?}.display_plus_upload"))
                        .or_default()
                        .push(total);
                    for (key, v) in cpu_stages {
                        phases
                            .values
                            .entry(format!("{kind:?}.{key}"))
                            .or_default()
                            .push(v);
                    }
                    if frame == 0 || frame == 7 {
                        gpu.paint(
                            &params,
                            [192, 144],
                            false,
                            WireMode::Visible,
                            false,
                            &Preview::default(),
                        );
                        results.push((
                            stroke,
                            frame,
                            rgba_hash(&gpu.read()),
                            next.shading.vertices.len(),
                            gpu.vertex_write_bytes,
                        ));
                    }
                    live = next;
                }
                doc.end_stroke();
                live = phases.time(&format!("{kind:?}.release"), || {
                    live.after_stroke(&doc)
                        .unwrap_or_else(|| Arc::new(DisplayMesh::new(&doc)))
                });
                gpu.upload(&live);
                gpu.queue.submit([]);
                headless.wait();
            }
            let pixels: Vec<_> = results.iter().map(|r| (r.0, r.1, r.2, r.3)).collect();
            if let Some(reference) = &expected {
                assert_eq!(reference, &pixels);
            } else {
                expected = Some(pixels);
            }
            rows.push(serde_json::json!({"reference":reference,"broad":wide,"source_triangles":doc.mesh().triangles.len(),"timings":phases.summary(),"samples":results,"memory":memory(),"display_payload_bytes":live.level_cache_bytes(),"packed_bytes":gpu.packed_vertices.capacity()*4}));
        }
    }
    std::fs::write(
        std::env::var_os("CLAY_REGION_OUTPUT").unwrap(),
        serde_json::to_vec(&serde_json::json!({"rows":rows})).unwrap(),
    )
    .unwrap();
}
