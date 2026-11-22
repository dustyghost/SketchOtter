use super::*;
impl Document {
    pub(crate) fn paint_retopology_density(
        &mut self,
        brush: &Brush,
        dab: Dab,
        mode: crate::sculpt::retopology::DensityPaint,
    ) -> Result<bool, String> {
        self.require_retopology_target()?;
        for mesh in std::iter::once(&self.geometry.mesh)
            .chain(&self.geometry.settings.lower)
            .chain(&self.geometry.settings.upper)
        {
            mesh.require_density_channels()?;
        }
        self.begin_stroke();
        let dabs = self.symmetry.dabs(dab);
        let changed = self
            .geometry
            .mesh
            .paint_retopology_density(brush, &dabs, mode)?;
        if changed {
            if let Some(before) = self.stroke_before.take() {
                self.history
                    .record_owned(before, "Retopology density stroke", None);
            }
            // Use the same spatial dabs on stored levels; geometry never changes.
            for mesh in self
                .geometry
                .settings
                .lower
                .iter_mut()
                .chain(&mut self.geometry.settings.upper)
            {
                mesh.ensure_density_channels()?;
                mesh.paint_retopology_density(brush, &dabs, mode)?;
            }
            self.dirty = true;
            self.display_changed();
        }
        Ok(changed)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::sculpt::{
        Alpha, GroupPurpose, Vec3,
        retopology::{DensityPaint, tests::fixture},
    };
    #[test]
    fn retopology_density_undo_save_subdivision_and_group_all_preserve_independence() {
        let (mesh, _) = fixture(true);
        let mut geometry = Geometry::new(mesh);
        geometry.subdivide().unwrap();
        let mut document = Document::default();
        document.replace_geometry(geometry).unwrap();
        let before = document.project();
        let i = 320;
        let normal = document.mesh().normals()[i];
        let mut brush = Brush::new(Kind::Mask);
        brush.alpha = Alpha::Hard;
        brush.strength = 1.;
        brush.pressure = false;
        let dab = Dab {
            center: document.mesh().vertices[i],
            normal,
            right: Vec3(0., 1., 0.),
            up: normal.cross(Vec3(0., 1., 0.)),
            radius: 0.3,
            pressure: 1.,
            delta: Vec3::default(),
            invert: false,
        };
        document
            .paint_retopology_density(&brush, dab, DensityPaint::More)
            .unwrap();
        document.end_stroke();
        assert_eq!(document.undo_label(), Some("Retopology density stroke"));
        assert_eq!(document.mesh().mask, before.mesh.mask);
        assert_eq!(document.mesh().vertices, before.mesh.vertices);
        assert_eq!(document.mesh().retopology_detail()[i], 1.);
        document.mesh_settings().validate(document.mesh()).unwrap();
        let saved = document.project();
        let mut reopened = Document::default();
        reopened
            .replace_project(
                serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap(),
                "density.clay",
            )
            .unwrap();
        assert!(reopened.project() == saved);
        document.undo();
        assert!(document.project() == before);
        document.redo();
        assert!(document.project() == saved);
        let density = document.mesh().retopology_detail();
        document
            .create_vertex_group("New selection".into(), true)
            .unwrap();
        assert_eq!(document.mesh().retopology_detail(), density);
        assert_eq!(
            document
                .mesh()
                .vertex_groups
                .iter()
                .filter(|g| g.purpose == GroupPurpose::Selection)
                .count(),
            1
        );
    }

    use crate::sculpt::{Primitive, PrimitiveKind};
    pub(in crate::document) fn mask_faces(mesh: &mut Mesh, faces: &[usize]) {
        mesh.mask.fill(0.);
        for &face in faces {
            for v in mesh.polygons()[face].vertices.clone() {
                mesh.mask[v] = 1.;
            }
        }
    }
    #[test]
    fn groups_and_mask_intensity_are_independent_undoable_and_survive_levels() {
        let mut geometry = Geometry::new(
            crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube)
                .mesh()
                .unwrap(),
        );
        geometry.subdivide().unwrap();
        geometry.mesh.mask[geometry.settings.lower[0].vertices.len()] = 0.8;
        let mut d = Document::default();
        d.replace_geometry(geometry).unwrap();
        d.duplicate_model(false).unwrap();
        let before = d.geometry_snapshot();
        d.create_vertex_group("Fine detail".into(), false).unwrap();
        let grouped = d.geometry_snapshot();
        assert_eq!(grouped.submodels, before.submodels);
        assert!(
            grouped.settings.lower[0].vertex_groups[0]
                .weights
                .iter()
                .all(|&w| w == 0.)
        );
        assert_eq!(
            grouped.mesh.vertex_groups[0]
                .weights
                .iter()
                .filter(|&&w| w > 0.)
                .count(),
            1
        );
        d.undo();
        assert_eq!(d.geometry_snapshot(), before);
        d.redo();
        assert_eq!(d.geometry_snapshot(), grouped);
        let count = d.history_counts().0;
        d.set_mask_intensity(0.).unwrap();
        d.set_mask_intensity(0.4).unwrap();
        assert!((d.mask_peak() - 0.4).abs() < 1e-6);
        assert_eq!(d.history_counts().0, count + 1);
        assert_eq!(d.mesh().vertex_groups, grouped.mesh.vertex_groups);
        d.end_edit();
        d.mask_from_group(0).unwrap();
        assert_eq!(d.mesh().mask, grouped.mesh.mask);
        d.set_mesh_level(1).unwrap();
        d.set_mesh_level(2).unwrap();
        assert_eq!(d.mesh().vertex_groups, grouped.mesh.vertex_groups);
        d.rename_vertex_group(0, "Detail".into()).unwrap();
        let project = d.project();
        project.validate().unwrap();
        let loaded: Project =
            serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        loaded.validate().unwrap();
        assert_eq!(
            serde_json::to_value(loaded).unwrap(),
            serde_json::to_value(project).unwrap()
        );
    }

    #[test]
    fn group_all_replaces_groups_on_every_level_and_undo_restores_them() {
        let mut mesh = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Plane)
            .mesh()
            .unwrap();
        mesh.mask.fill(0.7);
        mesh.create_vertex_group("Old".into(), 0.5, true).unwrap();
        mesh.create_vertex_group("Detail".into(), 0.5, false)
            .unwrap();
        let mut geometry = Geometry::new(mesh);
        geometry.subdivide().unwrap();
        geometry.subdivide().unwrap();
        let mut doc = Document::default();
        doc.replace_geometry(geometry).unwrap();
        doc.set_mesh_level(2).unwrap();
        doc.duplicate_model(false).unwrap();
        let before = doc.geometry_snapshot();
        assert_eq!(
            doc.create_vertex_group("Detail".into(), true).unwrap(),
            0,
            "replacement may reuse an old name"
        );
        let after = doc.geometry_snapshot();
        assert_eq!(after.submodels, before.submodels);
        let levels = |g: &Geometry| {
            std::iter::once(&g.mesh)
                .chain(g.settings.lower.iter())
                .chain(g.settings.upper.iter())
                .cloned()
                .collect::<Vec<_>>()
        };
        for (new, old) in levels(&after).iter().zip(levels(&before)) {
            assert_eq!(new.vertex_groups.len(), 1);
            assert_eq!(new.vertex_groups[0].name, "Detail");
            assert!(new.vertex_groups[0].weights.iter().all(|&w| w == 1.));
            assert!(new.polygroups.iter().all(|&id| id == 1));
            assert_eq!(new.mask, old.mask);
            assert_eq!(new.vertices, old.vertices);
            assert_eq!(new.hidden_face_count(), old.hidden_face_count());
        }
        doc.project().validate().unwrap();
        doc.undo();
        assert_eq!(doc.geometry_snapshot(), before);
        doc.redo();
        assert_eq!(doc.geometry_snapshot(), after);
        assert!(doc.create_vertex_group("x".repeat(129), true).is_err());
        assert_eq!(
            doc.geometry_snapshot(),
            after,
            "invalid replacement must not clear old data"
        );
    }

    #[test]
    fn face_replacement_at_group_limit_reclaims_an_old_slot_atomically() {
        let mut p = Primitive::for_kind(PrimitiveKind::Grid);
        p.divisions = [8, 8, 1];
        let mut mesh = p.mesh().unwrap();
        for face in 0..64 {
            mask_faces(&mut mesh, &[face]);
            mesh.create_vertex_group(format!("Group {face}"), 0.5, false)
                .unwrap();
        }
        let mut doc = Document::default();
        doc.replace_geometry(Geometry::new(mesh)).unwrap();
        let before = doc.geometry_snapshot();
        let vertices = doc.mesh().polygons()[0].vertices.clone();
        assert_eq!(
            doc.group_model_selection(&vertices, Some(&[0]), "Replacement", None, false)
                .unwrap(),
            Some(63)
        );
        assert_eq!(doc.mesh().vertex_groups.len(), 64);
        assert!(!doc.mesh().vertex_groups.iter().any(|g| g.name == "Group 0"));
        assert_eq!(doc.mesh().polygons()[0].group, 64);
        doc.project().validate().unwrap();
        let replaced = doc.geometry_snapshot();
        assert!(
            doc.group_model_selection(&vertices[..1], None, "Too many", None, false)
                .is_err()
        );
        assert_eq!(doc.geometry_snapshot(), replaced);
        doc.undo();
        assert_eq!(doc.geometry_snapshot(), before);
    }
}
