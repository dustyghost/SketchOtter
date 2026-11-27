use super::*;
use crate::sculpt::{GroupPurpose, Primitive, PrimitiveKind, VertexGroup};

fn plane(group_hidden: bool) -> Document {
    let mut primitive = Primitive::for_kind(PrimitiveKind::Plane);
    primitive.divisions = [2, 1, 1];
    let mut mesh = primitive.mesh().unwrap();
    mesh.mask = mesh
        .vertices
        .iter()
        .map(|p| if p.0 >= 0. { 0.8 } else { 0.2 })
        .collect();
    mesh.create_vertex_group("Left".into(), 0.5, true).unwrap();
    mesh.create_vertex_group("Right".into(), 0.5, false)
        .unwrap();
    mesh.vertex_groups.push(VertexGroup {
        purpose: GroupPurpose::RetopologyMore,
        visible: true,
        name: "Density".into(),
        weights: mesh
            .vertices
            .iter()
            .map(|p| if p.0 > 0. { 0.6 } else { 0. })
            .collect(),
    });
    let mut d = Document::default();
    d.replace_geometry(Geometry::new(mesh)).unwrap();
    if group_hidden {
        d.set_vertex_group_visible(1, false).unwrap();
    } else {
        d.select_face_region(|p| p.0 > 0., true).unwrap();
    }
    d
}

fn roundtrip(d: &mut Document, before: &Geometry) {
    let after = d.geometry_snapshot();
    d.project().validate().unwrap();
    let saved = serde_json::to_string(&d.project()).unwrap();
    let mut loaded = Document::default();
    loaded
        .replace_project(serde_json::from_str(&saved).unwrap(), "visibility.clay")
        .unwrap();
    assert_eq!(loaded.geometry_snapshot(), after);
    d.undo();
    assert_eq!(&d.geometry_snapshot(), before);
    d.redo();
    assert_eq!(d.geometry_snapshot(), after);
}

#[test]
fn visibility_group_all_preserves_hidden_memberships_boundaries_density_and_levels() {
    for group_hidden in [false, true] {
        let mut d = plane(group_hidden);
        // Build stored levels without changing the visibility/group contract.
        let mut geometry = d.geometry_snapshot();
        geometry.subdivide().unwrap();
        geometry.subdivide().unwrap();
        d.replace_geometry(geometry).unwrap();
        d.set_mesh_level(2).unwrap();
        d.duplicate_model(false).unwrap();
        let before = d.geometry_snapshot();
        let count = d.history_counts().0;
        let new_group = d.create_vertex_group("Visible".into(), true).unwrap();
        assert_eq!(d.history_counts().0, count + 1);
        let after = d.geometry_snapshot();
        assert_eq!(after.submodels, before.submodels);
        for (old, new) in std::iter::once(&before.mesh)
            .chain(&before.settings.lower)
            .chain(&before.settings.upper)
            .zip(
                std::iter::once(&after.mesh)
                    .chain(&after.settings.lower)
                    .chain(&after.settings.upper),
            )
        {
            assert_eq!(old.vertices, new.vertices);
            assert_eq!(old.triangles, new.triangles);
            assert_eq!(old.quads, new.quads);
            assert_eq!(old.mask, new.mask);
            assert_eq!(old.hidden_faces, new.hidden_faces);
            assert_eq!(
                old.vertex_groups.iter().find(|g| g.name == "Density"),
                new.vertex_groups.iter().find(|g| g.name == "Density")
            );
            let mut visible = vec![false; old.vertices.len()];
            let mut hidden = vec![false; old.vertices.len()];
            for (f, t) in old.triangles.iter().enumerate() {
                assert_eq!(old.face_visible(f), new.face_visible(f));
                if old.face_visible(f) {
                    assert_eq!(new.polygroups[f], new_group + 1);
                    for &v in t {
                        visible[v] = true;
                    }
                } else {
                    let old_group = &old.vertex_groups[old.polygroups[f] - 1];
                    let retained = &new.vertex_groups[new.polygroups[f] - 1];
                    assert_eq!(old_group.name, retained.name);
                    assert_eq!(old_group.visible, retained.visible);
                    for &v in t {
                        hidden[v] = true;
                    }
                }
            }
            assert!(
                visible.iter().zip(&hidden).any(|(&v, &h)| v && h),
                "exercise shared boundaries"
            );
            for v in 0..old.vertices.len() {
                assert_eq!(
                    new.vertex_groups[new_group].weights[v],
                    if visible[v] { 1. } else { 0. }
                );
                for group in &old.vertex_groups {
                    let weight = new
                        .vertex_groups
                        .iter()
                        .find(|g| g.name == group.name)
                        .map_or(0., |g| g.weights[v]);
                    if hidden[v] || !group.purpose.is_selection() {
                        assert_eq!(weight, group.weights[v]);
                    } else if visible[v] {
                        assert_eq!(weight, 0.);
                    }
                }
            }
        }
        roundtrip(&mut d, &before);
        d.set_mesh_level(1).unwrap();
        assert_eq!(d.hidden_face_count(), 2);
        d.set_mesh_level(3).unwrap();
        assert!(d.hidden_face_count() > 0);
    }
}

#[test]
fn visibility_group_all_hidden_or_hidden_name_collision_is_atomic() {
    for whole_model in [false, true] {
        let mut d = plane(true);
        if whole_model {
            d.geometry.scene.active_metadata.visible = false;
        } else {
            d.invert_visibility().unwrap();
            d.show_all_faces().unwrap();
            d.select_face_region(|_| true, true).unwrap();
        }
        d.invert_mask().unwrap();
        d.undo();
        let before = d.geometry_snapshot();
        let history = d.history_counts();
        assert!(d.create_vertex_group("Visible".into(), true).is_err());
        assert_eq!(d.geometry_snapshot(), before);
        assert_eq!(d.history_counts(), history);
    }
    let mut d = plane(true);
    d.invert_mask().unwrap();
    d.undo();
    let before = d.geometry_snapshot();
    let history = d.history_counts();
    assert!(
        d.create_vertex_group("Right".into(), true).is_err(),
        "cannot reuse a hidden region's name"
    );
    assert_eq!(d.geometry_snapshot(), before);
    assert_eq!(d.history_counts(), history);
}

#[test]
fn visibility_split_and_delete_preserve_survivors_and_exact_history() {
    for operation in 0..3 {
        let split = operation != 0;
        let masked = operation == 2;
        for group_hidden in [false, true] {
            let mut d = plane(group_hidden);
            // Nonuniform paint and seam-aware UVs exercise the real extraction.
            let mesh = &mut d.geometry.mesh;
            mesh.materials = mesh
                .vertices
                .iter()
                .enumerate()
                .map(|(i, _)| crate::sculpt::Material {
                    roughness: 0.1 + i as f32 * 0.05,
                    ..Default::default()
                })
                .collect();
            mesh.uv.corners = mesh
                .triangles
                .iter()
                .map(|_| [[0., 0.], [1., 0.], [0., 1.]])
                .collect();
            mesh.uv.topology = mesh.triangles.clone();
            mesh.uv.masks = mesh.triangles.iter().map(|_| [0.1, 0.4, 0.9]).collect();
            mesh.uv.removed = (0..mesh.triangles.len()).map(|f| f == 1).collect();
            mesh.uv.seams = mesh.polygon_edges();
            mesh.creases = mesh.polygon_edges();
            d.duplicate_model(false).unwrap();
            let before = d.geometry_snapshot();
            let history = d.history_counts().0;
            if masked {
                d.split_masked().unwrap();
            } else if split {
                d.split_hidden().unwrap();
            } else {
                d.delete_hidden().unwrap();
            }
            assert_eq!(d.history_counts().0, history + 1);
            assert_eq!(d.model_count(), if split { 3 } else { 2 });
            let active = d.active_model_index();
            for i in active..=active + usize::from(split) {
                let mesh = d.model(i).unwrap().mesh;
                assert_eq!(mesh.triangles.len(), before.mesh.triangles.len() / 2);
                assert_eq!(mesh.quads.len() * 2, mesh.triangles.len());
                assert!(!mesh.has_hidden_faces());
                assert!(mesh.uv.current(mesh));
                assert_eq!(mesh.creases.len(), mesh.polygon_edges().len());
                assert_eq!(mesh.uv.seams.len(), mesh.polygon_edges().len());
                for (v, p) in mesh.vertices.iter().enumerate() {
                    let old = before.mesh.vertices.iter().position(|q| q == p).unwrap();
                    assert_eq!(
                        mesh.mask[v],
                        if split && i != active {
                            0.
                        } else {
                            before.mesh.mask[old]
                        }
                    );
                    assert_eq!(mesh.materials[v], before.mesh.materials[old]);
                    for (group, source) in mesh.vertex_groups.iter().zip(&before.mesh.vertex_groups)
                    {
                        assert_eq!(group.weights[v], source.weights[old]);
                    }
                }
                for (f, t) in mesh.triangles.iter().enumerate() {
                    let positions = t.map(|v| mesh.vertices[v]);
                    let old = before
                        .mesh
                        .triangles
                        .iter()
                        .position(|t| t.map(|v| before.mesh.vertices[v]) == positions)
                        .unwrap();
                    assert_eq!(before.mesh.face_visible(old), i == active);
                    assert!(mesh.face_visible(f));
                    assert_eq!(mesh.uv.corners[f], before.mesh.uv.corners[old]);
                    assert_eq!(mesh.uv.masks[f], before.mesh.uv.masks[old]);
                    assert_eq!(mesh.uv.removed[f], before.mesh.uv.removed[old]);
                    assert_eq!(mesh.polygroups[f], before.mesh.polygroups[old]);
                }
            }
            assert_eq!(
                d.model(if split { 2 } else { 1 }).unwrap().mesh,
                &before.submodels[0].mesh
            );
            roundtrip(&mut d, &before);
        }
    }
}

#[test]
fn visibility_split_masked_uses_blurred_threshold_and_invalidates_cached_counts() {
    let mut d = plane(false);
    d.show_all_faces().unwrap();
    assert_eq!(d.masked_face_count(), 2);
    d.blur_mask().unwrap();
    assert!(d.mesh().mask.iter().all(|&v| v > 0. && v < 1.));
    assert_eq!(d.masked_face_count(), 2);
    d.set_mask_threshold(0.8).unwrap();
    assert_eq!(d.masked_face_count(), 0);
    assert!(d.split_masked_unavailable().unwrap().contains("threshold"));
    d.end_edit();
    let before = d.geometry_snapshot();
    let history = d.history_counts();
    assert!(d.split_masked().is_err());
    assert_eq!(d.geometry_snapshot(), before);
    assert_eq!(d.history_counts(), history);
    d.undo();
    assert_eq!(d.masked_face_count(), 2);
    d.redo();
    assert_eq!(d.masked_face_count(), 0);
    d.set_mask_threshold(0.6).unwrap();
    let mut geometry = d.geometry_snapshot();
    geometry.subdivide().unwrap();
    d.replace_geometry(geometry).unwrap();
    let expected = (0..d.mesh().triangles.len())
        .filter(|&f| d.mesh().face_mask_selected(f, 0.6))
        .count();
    assert_eq!(d.masked_face_count(), expected);
    let before = d.geometry_snapshot();
    let history = d.history_counts();
    d.split_masked().unwrap();
    assert_eq!(d.history_counts().0, history.0 + 1);
    assert_eq!(d.mesh_settings().levels(), 1);
    assert_eq!(d.model(1).unwrap().settings.levels(), 1);
    assert_eq!(d.model(1).unwrap().name, "Masked 1");
    assert_eq!(d.model(1).unwrap().mesh.triangles.len(), expected);
    assert_eq!(d.masked_face_count(), 0);
    d.select_submodel(1).unwrap();
    assert_eq!(d.masked_face_count(), 0);
    d.undo(); // Restore the active model before checking the split's history.
    roundtrip(&mut d, &before);

    // At 0%, clear vertices remain excluded; every corner must qualify.
    let mut d = plane(false);
    for (i, p) in d.geometry.mesh.vertices.iter().enumerate() {
        d.geometry.mesh.mask[i] = if p.0 >= 0. { 0.4 } else { 0. };
    }
    d.set_mask_threshold(0.).unwrap();
    assert_eq!(d.masked_face_count(), 2);
    let before = d.geometry_snapshot();
    d.split_masked().unwrap();
    assert_eq!(d.mesh().triangles.len(), 2);
    assert_eq!(d.model(1).unwrap().mesh.triangles.len(), 2);
    roundtrip(&mut d, &before);
}

#[test]
fn visibility_split_masked_rejects_unsupported_inputs_without_consuming_redo() {
    for mode in 0..8 {
        let mut d = plane(false);
        match mode {
            0 => d.geometry.mesh.mask.fill(0.),
            1 => d.geometry.mesh.mask.fill(1.),
            2 => d.geometry.scene.active_metadata.visible = false,
            3 => d.geometry.scene.active_metadata.temporary_thickness = 0.1,
            4 => {
                let template = d.ordered_models()[0].clone();
                d.geometry.submodels = vec![template; 63];
            }
            5 => {
                let v = d.geometry.mesh.vertices.len();
                d.geometry
                    .mesh
                    .vertices
                    .push(crate::sculpt::Vec3(5., 5., 5.));
                d.geometry.mesh.mask.push(0.);
                for g in &mut d.geometry.mesh.vertex_groups {
                    g.weights.push(0.);
                }
                d.geometry.mesh.loose_vertices.push(v);
            }
            6 => {
                let mesh = &mut d.geometry.mesh;
                mesh.uv.corners = vec![[[0., 0.]; 3]];
                mesh.uv.topology = vec![mesh.triangles[0]];
            }
            _ => {
                d.initialize_primitive().unwrap();
            }
        }
        if mode != 7 {
            d.invert_mask().unwrap();
            d.undo();
        }
        let before = d.geometry_snapshot();
        let history = d.history_counts();
        let dirty = d.is_dirty();
        assert!(d.split_masked_unavailable().is_some());
        assert!(d.split_masked().is_err());
        assert_eq!(d.geometry_snapshot(), before);
        assert_eq!(d.history_counts(), history);
        assert_eq!(d.is_dirty(), dirty);
    }
}

#[test]
fn visibility_delete_noop_and_unsupported_inputs_preserve_redo() {
    for mode in 0..5 {
        let mut d = plane(false);
        match mode {
            0 => d.show_all_faces().unwrap(),
            1 => d.geometry.scene.active_metadata.visible = false,
            2 => d.geometry.scene.active_metadata.temporary_thickness = 0.1,
            3 => {
                let v = d.geometry.mesh.vertices.len();
                d.geometry
                    .mesh
                    .vertices
                    .push(crate::sculpt::Vec3(5., 5., 5.));
                d.geometry.mesh.mask.push(0.);
                for g in &mut d.geometry.mesh.vertex_groups {
                    g.weights.push(0.);
                }
                d.geometry.mesh.loose_vertices.push(v);
            }
            _ => {
                let mesh = &mut d.geometry.mesh;
                mesh.uv.corners = vec![[[0., 0.]; 3]];
                mesh.uv.topology = vec![mesh.triangles[0]];
            }
        }
        d.invert_mask().unwrap();
        d.undo();
        let before = d.geometry_snapshot();
        let history = d.history_counts();
        let dirty = d.is_dirty();
        let result = d.delete_hidden();
        assert_eq!(result.is_ok(), mode == 0);
        assert_eq!(d.geometry_snapshot(), before);
        assert_eq!(d.history_counts(), history);
        assert_eq!(d.is_dirty(), dirty);
        assert!(d.split_hidden().is_err());
        assert_eq!(d.geometry_snapshot(), before);
        assert_eq!(d.history_counts(), history);
    }
}

#[test]
fn visibility_delete_all_hidden_keeps_empty_model_and_roundtrips() {
    for group_hidden in [false, true] {
        let mut d = plane(group_hidden);
        d.duplicate_model(false).unwrap();
        if group_hidden {
            d.set_vertex_group_visible(0, false).unwrap();
        } else {
            d.select_face_region(|_| true, true).unwrap();
        }
        let before = d.geometry_snapshot();
        let history = d.history_counts();
        let active = d.active_model_index();
        let name = d.model(active).unwrap().name.to_string();
        assert_eq!(d.hidden_face_count(), d.mesh().triangles.len());
        assert!(d.split_hidden().is_err());
        assert_eq!(d.history_counts(), history);
        assert_eq!(d.geometry_snapshot(), before);
        d.delete_hidden().unwrap();
        assert!(d.mesh().vertices.is_empty());
        assert!(d.mesh().triangles.is_empty());
        assert!(d.mesh().quads.is_empty());
        assert_eq!(d.model_count(), 2);
        assert_eq!(d.active_model_index(), active);
        assert_eq!(d.model(active).unwrap().name, name);
        assert_eq!(d.geometry_snapshot().submodels, before.submodels);
        assert_eq!(d.history_counts().0, history.0 + 1);
        roundtrip(&mut d, &before);
        let history = d.history_counts();
        d.delete_hidden().unwrap();
        assert_eq!(d.history_counts(), history);
    }
}

#[test]
fn visibility_split_limit_and_delete_stack_reset() {
    let mut d = plane(false);
    let template = d.ordered_models()[0].clone();
    d.geometry.submodels = vec![template; 63];
    assert!(d.hidden_faces_unavailable(true).unwrap().contains("64"));
    assert!(d.hidden_faces_unavailable(false).is_none());
    let before = d.geometry_snapshot();
    assert!(d.split_hidden().is_err());
    assert_eq!(d.geometry_snapshot(), before);
    let mut d = plane(false);
    let mut g = d.geometry_snapshot();
    g.subdivide().unwrap();
    d.replace_geometry(g).unwrap();
    let before = d.geometry_snapshot();
    d.delete_hidden().unwrap();
    assert_eq!(d.mesh_settings().levels(), 1);
    roundtrip(&mut d, &before);
}

#[test]
fn visibility_disconnected_parts_keep_hidden_groups_and_split_without_moving() {
    for split in [true, false] {
        let mut left = Primitive::for_kind(PrimitiveKind::Cube).mesh().unwrap();
        let mut right = left.clone();
        for p in &mut left.vertices {
            p.0 -= 3.;
        }
        for p in &mut right.vertices {
            p.0 += 3.;
        }
        left.create_vertex_group("Left".into(), 0.5, true).unwrap();
        right
            .create_vertex_group("Right".into(), 0.5, true)
            .unwrap();
        let mesh = left.merged_with(&right).unwrap();
        let mut d = Document::default();
        d.replace_geometry(Geometry::new(mesh)).unwrap();
        d.set_vertex_group_visible(1, false).unwrap();
        let before_group = d.geometry_snapshot();
        d.create_vertex_group("Visible".into(), true).unwrap();
        let retained = d
            .mesh()
            .vertex_groups
            .iter()
            .find(|g| g.name == "Right")
            .unwrap();
        assert_eq!(retained, &before_group.mesh.vertex_groups[1]);
        let before = d.geometry_snapshot();
        if split {
            d.split_hidden().unwrap();
        } else {
            d.delete_hidden().unwrap();
        }
        assert_eq!(d.mesh().vertices.len(), left.vertices.len());
        assert!(d.mesh().vertices.iter().all(|p| left.vertices.contains(p)));
        if split {
            assert_eq!(
                d.model(1).unwrap().mesh.vertices.len(),
                right.vertices.len()
            );
            assert!(
                d.model(1)
                    .unwrap()
                    .mesh
                    .vertices
                    .iter()
                    .all(|p| right.vertices.contains(p))
            );
        }
        roundtrip(&mut d, &before);
    }
}

#[test]
fn visibility_delete_does_not_restore_uv_marks_from_deleted_faces() {
    let mut d = plane(false);
    let mesh = &mut d.geometry.mesh;
    mesh.uv.seams = mesh
        .polygon_edges()
        .into_iter()
        .filter(|e| e.iter().all(|&v| mesh.vertices[v].0 > 0.))
        .collect();
    assert!(!mesh.uv.seams.is_empty());
    let before = d.geometry_snapshot();
    d.delete_hidden().unwrap();
    assert!(
        d.mesh().uv.is_empty(),
        "deleted-only UV marks must not be restored by a generic geometry commit"
    );
    roundtrip(&mut d, &before);
}
