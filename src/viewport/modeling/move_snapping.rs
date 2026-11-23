//! Proximity welds for free Move drags. The stationary mesh supplies all targets.
use super::*;
use edit::EditOutcome;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum MoveSnap {
    #[default]
    Off,
    Vertex,
    Edge,
}

impl ModelingState {
    pub(super) fn advance_move_snap(
        &mut self,
        document: &Document,
        scene: &Scene,
        options: ModelingOptions,
        free: bool,
    ) {
        if options.tool != ModelTool::Move || free || options.move_snap == MoveSnap::Off {
            return;
        }
        let Some(drag) = &mut self.edit_drag else {
            return;
        };
        let Some(direct) = &mut drag.direct else {
            return;
        };
        // Axis/plane handles retain their constraint instead of jumping off it.
        if !drag.moved || direct.handle != transform::Handle::Free {
            return;
        }
        let mesh = document.mesh();
        let Ok(symmetry) = ModelingSymmetry::new(mesh, document.symmetry()) else {
            return;
        };
        let delta = direct.direction * drag.amount;
        let source = drag
            .components
            .iter()
            .min_by(|a, b| {
                let distance = |c: &&Component| match **c {
                    Component::Vertex(v) => (mesh.vertices[v] - drag.anchor).len(),
                    Component::Edge([a, b]) => {
                        ((mesh.vertices[a] + mesh.vertices[b]) * 0.5 - drag.anchor).len()
                    }
                    Component::Face(_) => f32::INFINITY,
                };
                distance(a).total_cmp(&distance(b))
            })
            .copied();
        let Some(source) = source else {
            return;
        };
        let result = match source {
            Component::Vertex(v) => {
                let copies: BTreeSet<_> = symmetry.vertex_copies(v).into_iter().collect();
                if !drag
                    .components
                    .iter()
                    .all(|c| matches!(c,Component::Vertex(v) if copies.contains(v)))
                {
                    return;
                }
                vertex_target(mesh, scene, v, &copies, delta, options, &symmetry)
            }
            Component::Edge(e) if options.move_snap == MoveSnap::Edge => {
                let copies: BTreeSet<_> = symmetry.edges(&[e]).into_iter().collect();
                if !drag
                    .components
                    .iter()
                    .all(|c| matches!(c,Component::Edge(e) if copies.contains(e)))
                {
                    return;
                }
                edge_target(mesh, scene, e, &copies, delta, options, &symmetry)
            }
            _ => None,
        };
        if let Some((result, displacement)) = result {
            direct.direction = displacement.norm();
            drag.amount = displacement.len();
            drag.result = Some(Ok(result));
        }
    }
}

fn vertex_target(
    mesh: &Mesh,
    scene: &Scene,
    source: usize,
    excluded: &BTreeSet<usize>,
    delta: Vec3,
    options: ModelingOptions,
    symmetry: &ModelingSymmetry,
) -> Option<(EditOutcome, Vec3)> {
    let pointer = scene.project(mesh.vertices[source] + delta);
    let visible: BTreeSet<_> = mesh
        .polygons()
        .into_iter()
        .filter(|p| p.visible)
        .flat_map(|p| p.vertices)
        .chain(mesh.visible_loose_edges().into_iter().flatten())
        .chain(mesh.loose_vertices.iter().copied())
        .collect();
    let mut candidates: Vec<_> = visible
        .into_iter()
        .filter(|v| !excluded.contains(v))
        .filter_map(|v| {
            let d = scene.project(mesh.vertices[v]).distance(pointer);
            (d <= options.snap_radius && scene.point_unoccluded(mesh.vertices[v])).then_some((d, v))
        })
        .collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    candidates.into_iter().take(8).find_map(|(_, target)| {
        let (out, indices) = mesh
            .weld_moved_vertices(&[[source, target]], options.protect_mask, symmetry)
            .ok()?;
        let delta = mesh.vertices[target] - mesh.vertices[source];
        let selection = indices.values().copied().map(Component::Vertex).collect();
        Some((
            EditOutcome::move_weld(out, selection, Component::Vertex(target), delta.len()),
            delta,
        ))
    })
}

fn edge_target(
    mesh: &Mesh,
    scene: &Scene,
    source: [usize; 2],
    copies: &BTreeSet<[usize; 2]>,
    delta: Vec3,
    options: ModelingOptions,
    symmetry: &ModelingSymmetry,
) -> Option<(EditOutcome, Vec3)> {
    let excluded: BTreeSet<_> = copies.iter().flatten().copied().collect();
    let visible: BTreeSet<_> = mesh
        .polygons()
        .into_iter()
        .filter(|p| p.visible)
        .flat_map(|p| {
            p.edges()
                .map(|(a, b)| [a.min(b), a.max(b)])
                .collect::<Vec<_>>()
        })
        .chain(mesh.visible_loose_edges())
        .collect();
    let projected = source.map(|v| scene.project(mesh.vertices[v] + delta));
    let direction = (mesh.vertices[source[1]] - mesh.vertices[source[0]]).norm();
    let mut candidates = Vec::new();
    for target in visible {
        if target.iter().any(|v| excluded.contains(v)) {
            continue;
        }
        let [a, b] = target.map(|v| mesh.vertices[v]);
        if direction.dot((b - a).norm()).abs() < 0.985
            || !scene.point_unoccluded(a)
            || !scene.point_unoccluded(b)
        {
            continue;
        }
        for pair in [target, [target[1], target[0]]] {
            let distances = std::array::from_fn::<_, 2, _>(|i| {
                projected[i].distance(scene.project(mesh.vertices[pair[i]]))
            });
            if distances.iter().all(|d| *d <= options.snap_radius) {
                candidates.push((distances[0] + distances[1], pair, target));
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    candidates
        .into_iter()
        .take(8)
        .find_map(|(_, pair, target)| {
            let (out, indices) = mesh
                .weld_moved_vertices(
                    &[[source[0], pair[0]], [source[1], pair[1]]],
                    options.protect_mask,
                    symmetry,
                )
                .ok()?;
            let selection = symmetry
                .edges(&[target])
                .into_iter()
                .map(|e| {
                    let [a, b] = e.map(|v| indices[&v]);
                    Component::Edge([a.min(b), a.max(b)])
                })
                .collect();
            let delta = (mesh.vertices[pair[0]] + mesh.vertices[pair[1]]
                - mesh.vertices[source[0]]
                - mesh.vertices[source[1]])
                * 0.5;
            Some((
                EditOutcome::move_weld(out, selection, Component::Edge(target), delta.len()),
                delta,
            ))
        })
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;
    #[test]
    fn move_snap_edge_needs_parallel_edges_and_both_nearby_endpoints() {
        for perspective in [false, true] {
            for (height, angle, expected) in [(1., 0., true), (0.5, 0., false), (1., 0.4, false)] {
                let direction = Vec3(-(angle as f32).sin(), (angle as f32).cos(), 0.);
                let mesh = Mesh {
                    vertices: vec![
                        Vec3(-0.6, -0.5, 0.),
                        Vec3(-0.6, 0.5, 0.),
                        Vec3(0.3, 0., 0.) - direction * (height * 0.5),
                        Vec3(0.3, 0., 0.) + direction * (height * 0.5),
                    ],
                    mask: vec![0.; 4],
                    loose_edges: vec![[0, 1], [2, 3]],
                    ..Default::default()
                };
                let mut document = Document::default();
                document
                    .replace_geometry(crate::sculpt::Geometry::new(mesh.clone()))
                    .unwrap();
                let camera = crate::camera::Camera {
                    yaw: 0.,
                    pitch: 0.,
                    perspective,
                    ..Default::default()
                };
                let scene = Scene::from_geometry(
                    egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(1000., 800.)),
                    &camera,
                    std::sync::Arc::new(super::super::super::gpu::DisplayMesh::new(&document)),
                    false,
                );
                let symmetry = ModelingSymmetry::new(&mesh, &Default::default()).unwrap();
                let mut options = crate::ui::UiState::default().modeling.options();
                options.move_snap = MoveSnap::Edge;
                options.snap_radius = 40.;
                let result = edge_target(
                    &mesh,
                    &scene,
                    [0, 1],
                    &[[0, 1]].into(),
                    Vec3(0.9, 0., 0.),
                    options,
                    &symmetry,
                );
                assert_eq!(result.is_some(), expected, "height {height} angle {angle}");
                if let Some((result, _)) = result {
                    assert_eq!(result.mesh.vertices.len(), 2);
                    assert_eq!(result.mesh.loose_edges, vec![[0, 1]]);
                }
            }
        }
    }
}
