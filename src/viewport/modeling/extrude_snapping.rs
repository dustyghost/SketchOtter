//! Live attraction during extrusion; release commits the validated connection once.
use super::*;
use edit::{EditDrag, EditOutcome};

impl ModelingState {
    pub(super) fn advance_extrude_snap(
        &mut self,
        document: &Document,
        scene: &Scene,
        pointer: Option<Pos2>,
        options: ModelingOptions,
        free: bool,
    ) {
        if !matches!(
            options.tool,
            ModelTool::PointExtrude | ModelTool::EdgeExtrude
        ) {
            return;
        }
        let Some(drag) = &mut self.edit_drag else {
            return;
        };
        let Some(pointer) = pointer else { return };
        if !drag.moved {
            return;
        }
        // Point dragging refreshes its free result every frame. Edge dragging's
        // amount cache needs an explicit refresh when leaving a snap target.
        if options.tool == ModelTool::EdgeExtrude {
            drag.result = Some(Self::edit_result(
                document,
                &drag.components,
                drag.anchor,
                options,
                drag.amount,
            ));
        }
        if free || !options.extrude_snap {
            return;
        }
        let result = if options.tool == ModelTool::PointExtrude {
            point_join(document, scene, drag, options)
        } else {
            edge_join(document, scene, pointer, drag, options)
        };
        if let Some(result) = result {
            if let Some(Component::Vertex(target)) = result.snap_target
                && let Some(direct) = &mut drag.direct
            {
                let delta = document.mesh().vertices[target] - drag.anchor;
                direct.direction = delta.norm();
                drag.amount = delta.len();
            }
            drag.result = Some(Ok(result));
        }
    }
}

fn point_join(
    document: &Document,
    scene: &Scene,
    drag: &EditDrag,
    options: ModelingOptions,
) -> Option<EditOutcome> {
    let mesh = document.mesh();
    let source = drag
        .components
        .iter()
        .filter_map(|c| match *c {
            Component::Vertex(v) => Some(v),
            _ => None,
        })
        .min_by(|&a, &b| {
            (mesh.vertices[a] - drag.anchor)
                .len()
                .total_cmp(&(mesh.vertices[b] - drag.anchor).len())
        })?;
    let symmetry = ModelingSymmetry::new(mesh, document.symmetry()).ok()?;
    let copies: BTreeSet<_> = symmetry.vertex_copies(source).into_iter().collect();
    if !drag
        .components
        .iter()
        .all(|c| matches!(c, Component::Vertex(v) if copies.contains(v)))
    {
        return None;
    }
    let pointer =
        scene.project(mesh.vertices[source] + drag.direct.as_ref()?.direction * drag.amount);
    let visible: BTreeSet<_> = mesh
        .polygons()
        .into_iter()
        .filter(|p| p.visible)
        .flat_map(|p| p.vertices)
        .chain(mesh.visible_loose_edges().into_iter().flatten())
        .collect();
    let existing: BTreeSet<_> = mesh.model_edges().into_iter().collect();
    let mut targets: Vec<_> = visible
        .into_iter()
        .filter(|v| !copies.contains(v))
        .filter(|&v| !existing.contains(&[source.min(v), source.max(v)]))
        .filter_map(|v| {
            let distance = scene.project(mesh.vertices[v]).distance(pointer);
            (distance <= options.snap_radius && scene.point_unoccluded(mesh.vertices[v]))
                .then_some((distance, v))
        })
        .collect();
    targets.sort_by(|a, b| a.0.total_cmp(&b.0));
    targets.into_iter().take(8).find_map(|(_, target)| {
        let (out, tips) = mesh
            .connect_extruded_point(source, target, options.protect_mask, &symmetry)
            .ok()?;
        Some(EditOutcome::snap_join(
            out,
            tips.into_iter().map(Component::Vertex).collect(),
            Component::Vertex(target),
            (mesh.vertices[target] - mesh.vertices[source]).len(),
        ))
    })
}

fn edge_join(
    document: &Document,
    scene: &Scene,
    pointer: Pos2,
    drag: &EditDrag,
    options: ModelingOptions,
) -> Option<EditOutcome> {
    let mesh = document.mesh();
    let boundaries = mesh.open_model_edges();
    let source = drag
        .components
        .iter()
        .filter_map(|c| match *c {
            Component::Edge(e) => Some(e),
            _ => None,
        })
        .min_by(|a, b| {
            let distance = |e: &[usize; 2]| {
                ((mesh.vertices[e[0]] + mesh.vertices[e[1]]) * 0.5 - drag.anchor).len()
            };
            distance(a).total_cmp(&distance(b))
        })?;
    if !boundaries.contains_key(&source) {
        return None;
    }
    let symmetry = ModelingSymmetry::new(mesh, document.symmetry()).ok()?;
    let copies: BTreeSet<_> = symmetry.edges(&[source]).into_iter().collect();
    if !drag
        .components
        .iter()
        .all(|c| matches!(c, Component::Edge(e) if copies.contains(e)))
    {
        return None;
    }
    let excluded: BTreeSet<_> = copies.iter().flatten().copied().collect();
    let mut targets: Vec<_> = boundaries
        .keys()
        .copied()
        .filter(|e| e.iter().all(|v| !excluded.contains(v)))
        .filter_map(|e| {
            let point = snapping::nearest_edge_point(
                scene,
                pointer,
                mesh.vertices[e[0]],
                mesh.vertices[e[1]],
            );
            let distance = scene.project(point).distance(pointer);
            (distance <= options.snap_radius && scene.point_unoccluded(point))
                .then_some((distance, e))
        })
        .collect();
    targets.sort_by(|a, b| a.0.total_cmp(&b.0));
    targets.into_iter().take(8).find_map(|(_, target)| {
        let out = mesh
            .join_extruded_edge(source, target, options.protect_mask, &symmetry)
            .ok()?;
        let selection = symmetry
            .edges(&[target])
            .into_iter()
            .map(Component::Edge)
            .collect();
        Some(EditOutcome::snap_join(
            out,
            selection,
            Component::Edge(target),
            drag.amount,
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extrusion_snap_join_targets_respect_depth_radius_masks_and_toggle() {
        for perspective in [false, true] {
            let mesh = Mesh {
                vertices: vec![
                    Vec3(-1., -1., 0.),
                    Vec3(1., -1., 0.),
                    Vec3(1., 1., 0.),
                    Vec3(-1., 1., 0.),
                    Vec3(0., 0., -0.5),
                    Vec3(1.4, 1.2, 0.4),
                ],
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                quads: vec![[0, 1, 2, 3]],
                loose_edges: vec![[0, 4], [2, 5]],
                mask: vec![0.; 6],
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
            let mut ui = crate::ui::UiState::default();
            ui.modeling.set_tool(ModelTool::PointExtrude);
            ui.modeling.mode = ComponentMode::Vertex;
            let mut options = ui.modeling.options();
            let mut state = ModelingState::default();
            state.sync(&document);
            let start = scene.project(mesh.vertices[5]);
            state
                .begin_direct(
                    &document,
                    &scene,
                    options,
                    start,
                    [Component::Vertex(5)].into(),
                    mesh.vertices[5],
                    super::super::transform::Handle::Free,
                    false,
                )
                .unwrap();
            let pointer = scene.project(mesh.vertices[1]) + Vec2::new(3., 3.);
            state.advance_direct_transform(&document, &scene, Some(pointer), options);
            state.advance_extrude_snap(&document, &scene, Some(pointer), options, false);
            assert_eq!(
                state
                    .edit_drag
                    .as_ref()
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .snap_target,
                Some(Component::Vertex(1))
            );
            for (free, enabled) in [(true, true), (false, false)] {
                options.extrude_snap = enabled;
                state.advance_direct_transform(&document, &scene, Some(pointer), options);
                state.advance_extrude_snap(&document, &scene, Some(pointer), options, free);
                assert!(
                    state
                        .edit_drag
                        .as_ref()
                        .unwrap()
                        .result
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .snap_target
                        .is_none()
                );
            }
            options.extrude_snap = true;
            for pointer in [
                scene.project(mesh.vertices[4]),
                scene.project(mesh.vertices[1]) + Vec2::new(30., 30.),
            ] {
                state.advance_direct_transform(&document, &scene, Some(pointer), options);
                state.advance_extrude_snap(&document, &scene, Some(pointer), options, false);
                assert!(
                    state
                        .edit_drag
                        .as_ref()
                        .unwrap()
                        .result
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .snap_target
                        .is_none()
                );
            }
            let mut protected = document.geometry_snapshot();
            protected.mesh.mask[1] = 1.;
            document.replace_geometry(protected).unwrap();
            state.advance_direct_transform(&document, &scene, Some(pointer), options);
            state.advance_extrude_snap(&document, &scene, Some(pointer), options, false);
            assert!(
                state
                    .edit_drag
                    .as_ref()
                    .unwrap()
                    .result
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .snap_target
                    .is_none()
            );
        }
    }
}
