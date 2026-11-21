//! Paired measurements of the general and indexed pipelines on identical buffers.
use super::*;
use crate::sculpt::texture::{Channel, Texture};

fn dense_draw_fixture(layout: &str, level: usize) -> Document {
    let mut geometry = fixture("hidden", level);
    let mesh = &mut geometry.mesh;
    if layout == "extended" {
        for (material, point) in mesh.materials.iter_mut().zip(&mesh.vertices) {
            material.emission_strength = 0.2 + 0.1 * point.0;
            material.emission = [0.4, 0.3, 0.2];
            material.roughness = (0.5 + point.1 * 0.35).clamp(0., 1.);
        }
    }
    if layout == "texture" {
        mesh.uv.topology = mesh.triangles.clone();
        mesh.uv.corners = (0..mesh.triangles.len())
            .map(|i| {
                if i % 2 == 0 {
                    [[0., 0.], [1., 0.], [1., 1.]]
                } else {
                    [[0., 1.], [0., 0.], [1., 1.]]
                }
            })
            .collect();
        let mut texture = Texture::new(Channel::Color, 64, &mesh.materials[0]).unwrap();
        for pixel in 0..64 * 64 {
            if ((pixel % 64) / 8 + (pixel / 64) / 8) % 2 == 0 {
                texture.pixels.set_pixel(pixel, [0.85, 0.1, 0.45, 1.]);
            }
        }
        mesh.uv.textures.push(texture);
    }
    document(geometry)
}

#[test]
#[ignore = "manual dense material draw timings with guarded memory"]
fn gpu_load_profile_dense_materials() {
    let level = env_limit("CLAY_REGION_LEVEL", 3);
    crate::sculpt::capacity::initialize(benchmark_capacity(level, 2_000_000, 4_000_000).unwrap())
        .unwrap();
    let headless = Headless::new();
    let mut rows = Vec::new();
    let mut setups = Vec::new();
    let size = [256, 192];
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256., 192.));
    for layout in ["basic", "extended", "texture"] {
        let doc = dense_draw_fixture(layout, level);
        let mut setup = Measurements::default();
        let mut gpu = setup.time("pipeline_setup", || {
            let gpu = Gpu::new(&headless.state.device, &headless.state.queue, COLOR);
            headless.state.queue.submit([]);
            headless.wait();
            gpu
        });
        let display = setup.time("display_preparation", || Arc::new(DisplayMesh::new(&doc)));
        setup.time("cold_upload", || {
            gpu.upload(&display);
            gpu.queue.submit([]);
            headless.wait()
        });
        let mesh_bytes: u64 = gpu
            .meshes
            .iter()
            .map(|m| m.vertices.size() + m.indices.size() + m.colors.size() + m.uv.size())
            .sum();
        setups.push(serde_json::json!({"layout":layout,"source_triangles":doc.mesh().triangles.len(),"source_vertices":doc.mesh().vertices.len(),"display_triangles":display.mesh.triangles.len(),"display_vertices":display.mesh.vertices.len(),"mesh_buffers_bytes":mesh_bytes,"stride":gpu.vertex_stride,"batches":gpu.meshes.len(),"timings":setup.summary(),"memory":memory()}));
        let (right, up, view) = Camera::default().basis();
        let params = Params::new(
            rect,
            right,
            up,
            view,
            72.,
            rect.center(),
            display.depth_extent,
        );
        for case in [
            "lit",
            "groups",
            "colour",
            "roughness",
            "metallic",
            "uv",
            "wire",
            "backs",
        ] {
            let preview = Preview {
                show_polygroups: case == "groups",
                uv_checker: case == "uv",
                backface_opacity: if case == "backs" { 0.35 } else { 0. },
                inspection: match case {
                    "colour" => InspectionMode::Color,
                    "roughness" => InspectionMode::Roughness,
                    "metallic" => InspectionMode::Metallic,
                    _ => InspectionMode::Lit,
                },
                ..Default::default()
            };
            let eligible = gpu.basic_surface_eligible(&preview);
            let mut timing = [Measurements::default(), Measurements::default()];
            let mut images = [None, None];
            for repetition in 0..4 {
                for arm in if repetition % 2 == 0 { [0, 1] } else { [1, 0] } {
                    gpu.force_general_surface = arm == 0;
                    let phases = &mut timing[arm];
                    let total = Instant::now();
                    let commands = phases.time("encode", || {
                        let mut encoder = gpu.device.create_command_encoder(&Default::default());
                        gpu.encode(
                            &mut encoder,
                            &params,
                            size,
                            case == "wire",
                            WireMode::Visible,
                            false,
                            &preview,
                        );
                        encoder.finish()
                    });
                    phases.time("submit", || gpu.queue.submit([commands]));
                    phases.time("completion", || headless.wait());
                    phases
                        .values
                        .entry("total".into())
                        .or_default()
                        .push(total.elapsed().as_secs_f64() * 1000.);
                    if repetition == 0 {
                        images[arm] = Some(gpu.read());
                    }
                }
            }
            gpu.force_general_surface = false;
            let a = images[0].as_ref().unwrap();
            let b = images[1].as_ref().unwrap();
            let mut max_delta = 0;
            let mut drawn = 0;
            for (a, b) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
                assert_eq!(a[3], b[3]);
                assert_eq!(a[..3] == params.background, b[..3] == params.background);
                drawn += usize::from(a[..3] != params.background);
                for (a, b) in a.iter().zip(b) {
                    max_delta = max_delta.max(a.abs_diff(*b));
                }
            }
            assert!(drawn > 0 && max_delta <= 1, "{layout} {case} pixels");
            rows.push(serde_json::json!({"layout":layout,"case":case,"indexed":eligible,"coverage":drawn,"max_channel_delta":max_delta,"general":timing[0].summary(),"candidate":timing[1].summary()}));
        }
    }
    let output = std::env::var_os("CLAY_REGION_OUTPUT").unwrap();
    std::fs::write(output,serde_json::to_vec(&serde_json::json!({"driver":format!("{:?}",headless.state.adapter.get_info()),"setup":setups,"rows":rows,"memory":memory()})).unwrap()).unwrap();
}
