//! Fixed tiny surfaces and seeded, fully serialized operation settings.
use super::*;

fn mesh_fixture(fixture: Fixture) -> Mesh {
    let mut primitive = Primitive::for_kind(match fixture {
        Fixture::Sphere => PrimitiveKind::Cube,
        _ => PrimitiveKind::Grid,
    });
    primitive.divisions = match fixture {
        Fixture::Sphere => [4; 3],
        _ => [6, 6, 1],
    };
    let mut mesh = match fixture {
        Fixture::Fan | Fixture::Bicone => fan(32),
        _ => primitive.mesh().unwrap(),
    };
    if matches!(fixture, Fixture::Sphere) {
        for p in &mut mesh.vertices {
            *p = p.norm();
        }
    } else if matches!(fixture, Fixture::Fan | Fixture::Bicone) {
        // Same high-valence fan as #170, with irregular but noncrossing sampling.
        for (i, p) in mesh.vertices.iter_mut().enumerate().skip(1) {
            *p = *p * (0.85 + 0.12 * (i as f32 * 1.7).sin());
        }
    } else if matches!(fixture, Fixture::NearbySheets) {
        let n = mesh.vertices.len();
        mesh.vertices.extend(
            mesh.vertices
                .clone()
                .into_iter()
                .map(|p| p + Vec3(0., 0., 0.03)),
        );
        mesh.triangles
            .extend(mesh.triangles.clone().into_iter().map(|t| t.map(|v| v + n)));
        mesh.quads
            .extend(mesh.quads.clone().into_iter().map(|q| q.map(|v| v + n)));
        mesh.mask.resize(mesh.vertices.len(), 0.);
    }
    if matches!(fixture, Fixture::Bicone) {
        mesh.vertices[0].2 = 0.3;
        let bottom = mesh.vertices.len();
        mesh.vertices.push(Vec3(0., 0., -0.3));
        let faces = mesh.triangles.clone();
        mesh.triangles
            .extend(faces.into_iter().map(|[_, a, b]| [bottom, b, a]));
        mesh.mask.resize(mesh.vertices.len(), 0.);
        mesh = mesh.subdivide().unwrap();
    }
    // Two independent nonuniform channels make old protected vertices identifiable
    // after compaction. The third channel independently echoes sculpt-mask transfer.
    let n = mesh.vertices.len();
    mesh.mask = mesh
        .vertices
        .iter()
        .map(|p| {
            if p.0 < -0.2 {
                if p.1 > 0. { 1. } else { 0.25 }
            } else {
                0.
            }
        })
        .collect();
    let weights: Vec<_> = (0..n).map(|i| i as f32 / (n - 1) as f32).collect();
    mesh.vertex_groups = [
        weights.clone(),
        weights.iter().map(|w| w * w).collect(),
        mesh.mask.clone(),
    ]
    .into_iter()
    .zip(["Gradient", "Curve", "Mask echo"])
    .map(|(weights, name)| VertexGroup {
        name: name.into(),
        weights,
        visible: true,
        purpose: GroupPurpose::Selection,
    })
    .collect();
    mesh.materials = weights
        .iter()
        .map(|&w| Material {
            base_color: [w, 0.3, 0.2],
            metallic: w,
            roughness: 0.2 + 0.5 * w,
            ..Default::default()
        })
        .collect();
    mesh.polygroups = mesh
        .triangles
        .iter()
        .map(|t| {
            if t.iter().map(|&v| mesh.vertices[v].0).sum::<f32>() < 0. {
                1
            } else {
                2
            }
        })
        .collect();
    mesh.hidden_faces = mesh
        .triangles
        .iter()
        .map(|t| {
            if matches!(fixture, Fixture::NearbySheets) {
                t.iter().all(|&v| mesh.vertices[v].2 > 0.01)
            } else {
                t.iter().map(|&v| mesh.vertices[v].1).sum::<f32>() / 3. < -0.55
            }
        })
        .collect();
    mesh
}
fn random(seed: &mut u64) -> f32 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 40) as f32) / ((1u32 << 24) as f32)
}
pub(super) fn generate(
    seed: u64,
    fixture: Fixture,
    scale: f32,
    quads: bool,
    mirrored: bool,
) -> Replay {
    let mut mesh = mesh_fixture(fixture);
    if !quads {
        mesh.quads.clear();
    }
    for p in &mut mesh.vertices {
        *p = *p * scale;
    }
    let closed = matches!(fixture, Fixture::Sphere | Fixture::Bicone);
    let mut app = App::default();
    let mut geometry = Geometry::new(mesh);
    geometry.settings.resolution = 16;
    geometry.settings.remesh_topology = if quads {
        Topology::Quads
    } else {
        Topology::Triangles
    };
    app.document.replace_geometry(geometry).unwrap();
    app.document
        .set_symmetry(Symmetry {
            enabled: mirrored,
            ..Default::default()
        })
        .unwrap();
    let initial = app.document.project();
    let mut rng = seed;
    let x = 0.25 + 0.12 * random(&mut rng);
    let normal = if matches!(fixture, Fixture::Sphere) {
        Vec3(x, 1., 0.).norm()
    } else {
        Vec3(0., 0., 1.)
    };
    let dab = SavedDab {
        center: if matches!(fixture, Fixture::Sphere) {
            normal * scale
        } else {
            Vec3(
                x,
                0.1,
                if matches!(fixture, Fixture::Bicone) {
                    0.18
                } else {
                    0.
                },
            ) * scale
        },
        normal,
        right: Vec3(1., 0., 0.),
        up: if matches!(fixture, Fixture::Sphere) {
            Vec3(0., 0., 1.)
        } else {
            Vec3(0., 1., 0.)
        },
        radius: (0.65 + 0.1 * random(&mut rng)) * scale,
    };
    let mut brush = Brush::new(Kind::Standard);
    brush.pressure = false;
    brush.alpha = Alpha::Soft;
    brush.strength = 0.025 + 0.015 * random(&mut rng);
    let refine = AdaptiveSettings {
        enabled: true,
        topology: if quads {
            Topology::Quads
        } else {
            Topology::Triangles
        },
        unmasked_only: true,
        relative: true,
        detail: 8. + 4. * random(&mut rng),
        simplify: false,
        ..Default::default()
    };
    let disabled = AdaptiveSettings {
        enabled: false,
        ..refine.clone()
    };
    let simplify = AdaptiveSettings {
        refine: false,
        simplify: true,
        detail: 32.,
        collapse_ratio: 0.8,
        ..refine.clone()
    };
    let mut cleanup = brush.clone();
    cleanup.kind = Kind::Decimate;
    cleanup.strength = 1.;
    let mut steps = Vec::new();
    if seed & 1 != 0 {
        steps.push(Step::Stroke {
            brush: brush.clone(),
            dab: dab.clone(),
            intent: Intent::Sculpt,
        });
    }
    steps.push(Step::Adaptive(refine.clone()));
    for _ in 0..(1 + seed as usize % 2) {
        steps.push(Step::Stroke {
            brush: brush.clone(),
            dab: dab.clone(),
            intent: Intent::Refine,
        });
    }
    steps.push(Step::Adaptive(disabled.clone()));
    steps.push(Step::Stroke {
        brush: brush.clone(),
        dab: dab.clone(),
        intent: Intent::Sculpt,
    });
    steps.push(Step::NoopAndRejection {
        brush: brush.clone(),
        dab: dab.clone(),
    });
    steps.push(Step::Adaptive(simplify));
    for _ in 0..3 {
        steps.push(Step::Stroke {
            brush: cleanup.clone(),
            dab: dab.clone(),
            intent: Intent::Simplify,
        });
    }
    steps.push(Step::Adaptive(disabled));
    steps.push(Step::Dynamic);
    steps.push(Step::RejectedRemesh {
        brush: brush.clone(),
        dab: dab.clone(),
    });
    steps.push(Step::ShowAll);
    steps.push(Step::Remesh { succeeds: closed });
    steps.push(Step::SaveReopen);
    if closed {
        steps.push(Step::Adaptive(refine));
        steps.push(Step::Stroke {
            brush,
            dab,
            intent: Intent::Refine,
        });
        steps.push(Step::SaveReopen);
    }
    if !closed {
        // The document explicitly rejects adaptive mode on open surfaces. Keep
        // successful sculpt/history/save sequences and exercise that guard with
        // a live Redo branch; never install unsupported adaptive settings.
        steps.retain(|step| {
            !matches!(
                step,
                Step::Adaptive(_)
                    | Step::Stroke {
                        intent: Intent::Refine | Intent::Simplify,
                        ..
                    }
            )
        });
    }
    Replay {
        version: 1,
        seed,
        fixture,
        scale,
        initial,
        steps,
        require_remesh: closed,
    }
}
