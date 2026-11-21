//! Mixed editing on an imported asset with original face-corner UVs.
use super::*;
use crate::sculpt::texture::{Channel, Texture};

fn prepare_asset(dir: &std::path::Path, subdivisions: usize) -> Geometry {
    let mut mesh =
        crate::persistence::load_obj(dir.join("WaterBottle.obj").to_str().unwrap()).unwrap();
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in &mesh.vertices {
        for (i, v) in [p.0, p.1, p.2].into_iter().enumerate() {
            lo[i] = lo[i].min(v);
            hi[i] = hi[i].max(v);
        }
    }
    let centre = Vec3(
        (hi[0] + lo[0]) / 2.,
        (hi[1] + lo[1]) / 2.,
        (hi[2] + lo[2]) / 2.,
    );
    let extent = (0..3).map(|i| hi[i] - lo[i]).fold(0_f32, f32::max);
    for p in &mut mesh.vertices {
        *p = (*p - centre) * (1.8 / extent);
    }
    let mut geo = Geometry::new(mesh);
    for _ in 0..subdivisions {
        // The edit operation intentionally clears stale UVs. For this
        // triangle-only benchmark asset, interpolate the source face's own
        // corners along the same four child triangles, keeping seams split.
        assert!(geo.mesh.quads.is_empty());
        assert_eq!(geo.mesh.uv.corners.len(), geo.mesh.triangles.len());
        let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let corners = geo
            .mesh
            .uv
            .corners
            .iter()
            .flat_map(|&[a, b, c]| {
                let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
                [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]
            })
            .collect::<Vec<_>>();
        geo.subdivide().unwrap();
        assert_eq!(corners.len(), geo.mesh.triangles.len());
        geo.mesh.uv.topology = geo.mesh.triangles.clone();
        geo.mesh.uv.corners = corners;
    }
    let mesh = &mut geo.mesh;
    let back = mesh
        .vertices
        .iter()
        .map(|p| p.2)
        .fold(f32::INFINITY, f32::min)
        * 0.75;
    mesh.hidden_faces = mesh
        .triangles
        .iter()
        .map(|t| t.iter().all(|&i| mesh.vertices[i].2 < back))
        .collect();
    mesh.mask = mesh
        .vertices
        .iter()
        .map(|v| if v.0 < -0.14 { 1. } else { 0. })
        .collect();
    let image = image::open(dir.join("WaterBottle_baseColor.png"))
        .unwrap()
        .thumbnail_exact(128, 128)
        .to_rgba8();
    let mut tex = Texture::new(Channel::Color, 128, &Material::default()).unwrap();
    tex.id = 0x5741544552;
    for (i, p) in image.pixels().enumerate() {
        tex.pixels.set_pixel(i, p.0.map(|v| v as f32 / 255.));
    }
    mesh.uv.textures.push(tex);
    assert!(!mesh.uv.corners.is_empty() && mesh.has_hidden_faces());
    geo
}

fn surface_dab(doc: &Document, step: usize) -> Dab {
    let y = 0.18 * ((step % 11) as f32 / 5. - 1.);
    let centre_x = if doc.active_model_index() == 0 {
        0.
    } else {
        0.65
    };
    let target = doc
        .mesh()
        .vertices
        .iter()
        .zip(&doc.mesh().mask)
        .filter(|(_, m)| **m < 0.99)
        .max_by(|(a, _), (b, _)| {
            let score = |v: &Vec3| v.2 - 2. * (v.0 - centre_x).abs() - 1.5 * (v.1 - y).abs();
            score(a).total_cmp(&score(b))
        })
        .unwrap()
        .0;
    Dab {
        center: *target,
        normal: Vec3(0., 0., 1.),
        radius: if step.is_multiple_of(9) { 0.32 } else { 0.12 },
        ..dab(step)
    }
}

fn inventory(doc: &Document) -> serde_json::Value {
    (0..doc.model_count()).map(|i|{
        let m=doc.model(i).unwrap();let mesh=m.mesh;
        serde_json::json!({"model":i,"vertices":mesh.vertices.len(),"polygons":mesh.polygons().len(),"source_triangles":mesh.triangles.len(),"visible_triangles":(0..mesh.triangles.len()).filter(|&f|mesh.face_visible(f)).count(),"retained_levels":m.settings.lower.iter().chain(&m.settings.upper).map(|l|[l.vertices.len(),l.triangles.len()]).collect::<Vec<_>>(),"mask_values":mesh.mask.len(),"vertex_materials":mesh.materials.len(),"corner_uv_faces":mesh.uv.corners.len(),"textures":mesh.uv.textures.iter().map(|t|[t.width as u64,t.height as u64,t.pixels.byte_len() as u64]).collect::<Vec<_>>()})
    }).collect::<Vec<_>>().into()
}

#[test]
#[ignore = "manual guarded mixed-session validation with a public CC0 model"]
fn gpu_load_profile_mixed_session() {
    let level = env_limit("CLAY_REGION_LEVEL", 3);
    crate::sculpt::capacity::initialize(benchmark_capacity(level, 2_000_000, 4_000_000).unwrap())
        .unwrap();
    let out = std::path::PathBuf::from(std::env::var_os("CLAY_REGION_OUTPUT").unwrap());
    let dir = std::path::PathBuf::from(std::env::var_os("CLAY_REGION_ASSET").unwrap());
    let total = env_limit("CLAY_SESSION_STROKES", 96);
    let mut timing = Measurements::default();
    let geometry = timing.time("cold_import_and_subdivision", || {
        prepare_asset(&dir, if level >= 4 { 3 } else { 1 })
    });
    let mut doc = document(geometry);
    doc.duplicate_model(false).unwrap();
    doc.apply_transform(
        crate::sculpt::Transform {
            translation: Vec3(0.65, 0., -0.18),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    doc.select_submodel(0).unwrap();
    doc.set_symmetry(Symmetry {
        enabled: true,
        ..Default::default()
    })
    .unwrap();
    let first_inventory = inventory(&doc);
    let headless = Headless::new();
    let mut renderer = Renderer::new(&headless.state).unwrap();
    let mut rows = Vec::new();
    let path = out.with_file_name("mixed-session.clay");
    let mut changes = [0; 4];
    for step in 0..total {
        if step > 0 && step.is_multiple_of(32) {
            doc.select_submodel(1 - doc.active_model_index()).unwrap();
        }
        let mode = step % 4;
        let kind = [Kind::Standard, Kind::Paint, Kind::Mask, Kind::Paint][mode];
        doc.texture_paint = (mode == 3).then_some(Channel::Color);
        let mut brush = brush(kind);
        brush.set_paint_colour(
            false,
            if (step / 4) % 2 == 0 {
                [0.85, 0.12, 0.23]
            } else {
                [0.15, 0.7, 0.35]
            },
        );
        let mut touch = surface_dab(&doc, step);
        touch.invert = kind == Kind::Mask && (step / 4) % 2 == 1;
        let before = if step % 8 == 3 {
            Some(doc.geometry_snapshot())
        } else {
            None
        };
        let start = Instant::now();
        let initial_revision = doc.display_revision();
        for i in 0..2 {
            touch.center.0 += 0.008 * i as f32;
            timing.time(&format!("stroke_{mode}.dab"), || {
                doc.apply_dab(&brush, touch)
            });
            let display = timing.time("live_display", || renderer.geometry(&doc));
            timing.time("live_upload_and_completion", || {
                renderer.gpu.lock().unwrap().upload(&display);
                headless.state.queue.submit([]);
                headless.wait();
            });
        }
        // Mask dabs update the document while returning false for deformation.
        let changed = doc.display_revision() != initial_revision;
        let cancel = step % 16 == 3;
        if cancel {
            assert!(doc.cancel_paint_stroke());
        } else {
            doc.end_stroke();
        }
        let display = timing.time("release_display", || renderer.geometry(&doc));
        let mut gpu = renderer.gpu.lock().unwrap();
        timing.time("release_upload_and_completion", || {
            gpu.upload(&display);
            gpu.queue.submit([]);
            headless.wait();
        });
        let request_ms = start.elapsed().as_secs_f64() * 1000.;
        let gpu_buffers: u64 = gpu
            .meshes
            .iter()
            .map(|m| m.vertices.size() + m.indices.size() + m.colors.size() + m.uv.size())
            .sum();
        drop(gpu);
        if let Some(before) = before {
            if cancel {
                assert_eq!(before, doc.geometry_snapshot());
            } else if changed {
                let after = doc.geometry_snapshot();
                timing.time("undo", || doc.undo());
                assert_eq!(before, doc.geometry_snapshot());
                timing.time("redo", || doc.redo());
                assert_eq!(after, doc.geometry_snapshot());
                assert_eq!(
                    doc.model(0).unwrap().mesh.uv.textures[0].pixels,
                    doc.model(1).unwrap().mesh.uv.textures[0].pixels
                );
            }
        }
        changes[mode] += usize::from(changed);
        if step == total / 2 {
            let mut cancelled = doc.geometry_snapshot();
            let polls = std::cell::Cell::new(0usize);
            assert!(
                cancelled
                    .subdivide_cancellable(&|| {
                        polls.set(polls.get() + 1);
                        polls.get() >= 3
                    })
                    .is_err()
            );
            assert!(polls.get() >= 3);
            assert_eq!(cancelled, doc.geometry_snapshot());
            // Rebuild a subdivision level from the original model to exercise
            // topology replacement and its undo without multiplying the largest level.
            let mut replacement = doc.geometry_snapshot();
            if let Some(lower) = replacement.settings.lower.pop() {
                replacement.mesh = lower;
                doc.replace_geometry(replacement).unwrap();
                doc.undo();
                assert_eq!(doc.mesh(), &cancelled.mesh);
            }
        }
        if step == total / 2 || step + 1 == total {
            let p = doc.project();
            timing.time("save", || {
                crate::persistence::save_project(path.to_str().unwrap(), &p).unwrap()
            });
            let reopened = timing.time("reopen", || {
                crate::persistence::load_project(path.to_str().unwrap()).unwrap()
            });
            assert!(
                p == reopened,
                "complete persisted project differs after reload"
            );
            let mut recovered = Document::default();
            recovered
                .replace_project(reopened, path.to_str().unwrap())
                .unwrap();
            assert_eq!(p.mesh, *recovered.mesh());
            assert_eq!(recovered.model_count(), doc.model_count());
        }
        let history = doc.diagnostic_manifest(Default::default());
        rows.push(serde_json::json!({"active_model":doc.active_model_index(),"estimated_document_geometry_bytes":doc.estimated_geometry_bytes(),"evicted_edits":history.evicted_edits,"stroke":step,"mode":mode,"changed":changed,"cancelled":cancel,"input_through_release_upload_ms":request_ms,"history":doc.history_counts(),"estimated_history_bytes":history.retained_estimated_bytes,"display_payload_bytes":display.level_cache_bytes(),"gpu_mesh_buffer_bytes":gpu_buffers,"rendered_triangles":display.mesh.triangles.len(),"memory":memory()}));
    }
    assert!(
        changes.iter().all(|&n| n > 0),
        "all four stroke modes must alter the asset"
    );
    let final_inventory = inventory(&doc);
    let before_clear = memory();
    renderer.destroy();
    drop(renderer);
    drop(doc);
    std::fs::write(out,serde_json::to_vec(&serde_json::json!({"source":"Khronos WaterBottle CC0","initial":first_inventory,"final":final_inventory,"changes":changes,"timings":timing.summary(),"samples":rows,"pre_clear_memory":before_clear,"post_clear_memory":memory(),"driver":format!("{:?}",headless.state.adapter.get_info())})).unwrap()).unwrap();
}

#[test]
#[ignore = "manual guarded single-process 9.6m triangle boundary probe"]
fn gpu_load_profile_upper_boundary() {
    use crate::sculpt::capacity::{MeshCapacity, initialize};
    initialize(MeshCapacity {
        vertices: 5_500_000,
        triangles: 11_000_000,
    })
    .unwrap();
    let out = std::path::PathBuf::from(std::env::var_os("CLAY_REGION_OUTPUT").unwrap());
    let mut rows = Vec::new();
    let mut tick = Instant::now();
    let mut checkpoint = |stage: &str, vertices: usize, triangles: usize, retained: usize| {
        rows.push(serde_json::json!({"stage":stage,"source_vertices":vertices,"source_triangles":triangles,"retained_level_triangles":retained,"elapsed_since_previous_ms":tick.elapsed().as_secs_f64()*1000.,"memory":memory()}));
        let tmp = out.with_extension("tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec(&serde_json::json!({"stage":stage,"rows":rows})).unwrap(),
        )
        .unwrap();
        std::fs::rename(tmp, &out).unwrap();
        tick = Instant::now();
    };
    let mut geometry = Geometry::new(Mesh::starting_sphere());
    for _ in 0..5 {
        checkpoint(
            "about_to_subdivide",
            geometry.mesh.vertices.len(),
            geometry.mesh.triangles.len(),
            geometry
                .settings
                .lower
                .iter()
                .map(|m| m.triangles.len())
                .sum(),
        );
        if let Err(reason) = geometry.subdivide() {
            checkpoint(
                &format!("capacity_stopped: {reason}"),
                geometry.mesh.vertices.len(),
                geometry.mesh.triangles.len(),
                geometry
                    .settings
                    .lower
                    .iter()
                    .map(|m| m.triangles.len())
                    .sum(),
            );
            return;
        }
    }
    let (v, t) = (geometry.mesh.vertices.len(), geometry.mesh.triangles.len());
    let retained = geometry
        .settings
        .lower
        .iter()
        .map(|m| m.triangles.len())
        .sum();
    checkpoint("subdivision_complete", v, t, retained);
    let mut doc = document(geometry);
    checkpoint("document_validated", v, t, retained);
    let before = fingerprint(doc.mesh());
    assert!(doc.apply_dab(&brush(Kind::Standard), dab(0)));
    doc.end_stroke();
    checkpoint("first_stroke_released", v, t, retained);
    let after = fingerprint(doc.mesh());
    assert_ne!(before, after);
    doc.undo();
    assert_eq!(fingerprint(doc.mesh()), before);
    doc.redo();
    assert_eq!(fingerprint(doc.mesh()), after);
    checkpoint("exact_undo_redo", v, t, retained);
    let headless = Headless::new();
    let mut renderer = Renderer::new(&headless.state).unwrap();
    let display = renderer.geometry(&doc);
    checkpoint("cpu_display_built", v, t, retained);
    let mut gpu = renderer.gpu.lock().unwrap();
    gpu.upload(&display);
    gpu.queue.submit([]);
    headless.wait();
    checkpoint("gpu_upload_completed", v, t, retained);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
    let (r, u, f) = Camera::default().basis();
    let params = Params::new(rect, r, u, f, 210., rect.center(), display.depth_extent);
    gpu.paint(
        &params,
        [800, 600],
        false,
        WireMode::Visible,
        false,
        &Preview::default(),
    );
    headless.wait();
    assert!(
        gpu.read()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3] != params.background)
    );
    checkpoint("offscreen_draw_completed", v, t, retained);
}
