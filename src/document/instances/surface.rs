use super::*;
impl Document {
    pub fn surface_owner(&self, id: u64) -> Option<usize> {
        let set = self.instances().sets.iter().find(|s| s.id == id)?;
        let Layout::Surface(a) = &set.layout else {
            return None;
        };
        (0..self.model_count()).find(|&i| self.instance_model_id(i) == a.owner)
    }
    pub fn surface_guide_editing(&self, id: u64) -> bool {
        self.array_source_edit == Some(id)
            && self.surface_owner(id) == Some(self.active_model_index())
    }
    pub fn set_surface_guide_mode(&mut self, id: u64, editing: bool) -> Result<(), String> {
        self.apply_instance_preview()?;
        let owner = self
            .surface_owner(id)
            .ok_or("Surface guide no longer exists")?;
        if owner != self.active_model_index() {
            self.select_submodel(owner)?;
        }
        self.end_edit();
        self.end_stroke();
        self.array_source_edit = editing.then_some(id);
        self.selected_instance = None;
        self.instances_display_changed();
        Ok(())
    }
    pub fn preview_surface_array(
        &mut self,
        asset: Option<u64>,
        faces: Vec<usize>,
    ) -> Result<Set, String> {
        if self.attached_array(self.active_model_index()).is_some() {
            return Err("Make the existing array editable before assigning guide faces".into());
        }
        let asset = asset
            .and_then(|id| self.instances().assets.iter().find(|a| a.id == id))
            .or_else(|| self.instances().assets.first())
            .ok_or("Capture a model in Object brush settings before creating a surface array")?;
        let piece = Piece {
            source: Source::Asset(asset.id),
            pivot: asset.pivot,
            forward: asset.forward,
            up: asset.up,
        };
        let mut a = SurfaceArray::new(1, piece, self.mesh(), faces)?;
        let started = !self.instance_preview_active();
        self.begin_instance_preview()?;
        a.owner = self.instance_model_id(self.active_model_index());
        let set = Set {
            id: self.next_instance_set_id()?,
            name: "Surface array".into(),
            visible: true,
            layout: Layout::Surface(a),
            adjustments: Vec::new(),
        };
        if let Err(error) = self.preview_instance_set(set.clone()) {
            if started {
                self.cancel_instance_preview();
            }
            return Err(error);
        }
        Ok(set)
    }
    pub fn preview_surface_region(&mut self, id: u64, faces: Vec<usize>) -> Result<Set, String> {
        if self.surface_owner(id) != Some(self.active_model_index()) {
            return Err("Select the surface guide before assigning faces".into());
        }
        let mut set = self
            .instances()
            .sets
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or("Surface array no longer exists")?;
        let Layout::Surface(a) = &mut set.layout else {
            return Err("Choose a surface array".into());
        };
        a.select(self.mesh(), faces)?;
        // A new region defines new copy slots; old per-copy overrides must not jump.
        set.adjustments.clear();
        let mode = self.array_source_edit.take();
        self.instance_cache.take();
        if let Err(error) = self.preview_instance_set(set.clone()) {
            self.array_source_edit = mode;
            self.instance_cache.take();
            return Err(error);
        }
        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> (Document, u64) {
        let mut p = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        p.divisions = [1; 3];
        let mut d = Document {
            geometry: Geometry::new(p.mesh().unwrap()),
            ..Default::default()
        };
        let asset = d
            .capture_object("Part".into(), Vec3::default(), 0, 2)
            .unwrap();
        (d, asset)
    }
    #[test]
    fn surface_preview_cancel_undo_redo_and_project_roundtrip() {
        let (mut d, asset) = document();
        let before = d.project();
        let history = d.history_counts();
        let mut set = d.preview_surface_array(Some(asset), vec![0, 1]).unwrap();
        if let Layout::Surface(a) = &mut set.layout {
            a.replace = true;
            a.offset = 0.2;
        }
        d.preview_instance_set(set.clone()).unwrap();
        assert_eq!(d.instance_placements().unwrap().len(), 2);
        assert_eq!(d.instance_placements().unwrap().source_meshes.len(), 1);
        assert!(d.project() == before);
        d.cancel_instance_preview();
        assert!(d.instance_placements().unwrap().source_meshes.is_empty());
        d.preview_instance_set(set).unwrap();
        d.apply_instance_preview().unwrap();
        assert_eq!(d.history_counts().0, history.0 + 1);
        let applied = d.project();
        let reopened: crate::sculpt::Project =
            serde_json::from_str(&serde_json::to_string(&applied).unwrap()).unwrap();
        reopened.validate().unwrap();
        assert!(applied == reopened);
        d.undo();
        assert!(d.project() == before);
        d.redo();
        assert!(d.project() == applied);
    }
    #[test]
    fn surface_invalid_preview_retains_last_valid_display_and_source_sharing() {
        let (mut d, asset) = document();
        let mut set = d.preview_surface_array(Some(asset), vec![0]).unwrap();
        let valid = d.instance_placements().unwrap();
        let source = d.instances().assets[0].mesh.clone();
        if let Layout::Surface(a) = &mut set.layout {
            a.scale = 0.;
        }
        assert!(d.preview_instance_set(set).is_err());
        assert_eq!(d.instance_placements().unwrap(), valid);
        assert!(std::sync::Arc::ptr_eq(
            &source,
            &d.instances().assets[0].mesh
        ));
    }
    #[test]
    fn surface_stale_assignment_survives_save_and_explicit_reselection() {
        let (mut d, asset) = document();
        let mut set = d.preview_surface_array(Some(asset), vec![0]).unwrap();
        if let Layout::Surface(a) = &mut set.layout {
            a.replace = true;
        }
        d.preview_instance_set(set.clone()).unwrap();
        d.apply_instance_preview().unwrap();
        d.geometry.mesh.triangles.swap(0, 1);
        d.display_changed();
        let out = d.instance_placements().unwrap();
        assert!(out.is_empty());
        assert_eq!(out.notices.len(), 1);
        assert!(out.source_meshes.is_empty());
        d.project().validate().unwrap();
        d.set_surface_guide_mode(set.id, true).unwrap();
        d.preview_surface_region(set.id, vec![1, 2]).unwrap();
        assert_eq!(d.instance_placements().unwrap().len(), 2);
        d.apply_instance_preview().unwrap();
        d.remove_instance_set(set.id).unwrap();
        assert!(d.instance_placements().unwrap().source_meshes.is_empty());
    }
    #[test]
    fn surface_make_editable_matches_keep_replace_and_is_one_undo() {
        for replace in [false, true] {
            let (mut d, asset) = document();
            let mut set = d.preview_surface_array(Some(asset), vec![0, 1]).unwrap();
            if let Layout::Surface(a) = &mut set.layout {
                a.replace = replace;
            }
            d.preview_instance_set(set.clone()).unwrap();
            d.apply_instance_preview().unwrap();
            let before = d.geometry_snapshot();
            let history = d.history_counts().0;
            let mut baked = before.clone();
            baked
                .bake_instances(
                    BakeRequest {
                        scope: BakeScope::Set(set.id),
                        destination: BakeDestination::Current,
                        fuse: false,
                    },
                    &|| false,
                )
                .unwrap();
            assert_eq!(baked.mesh.triangles.len(), 36 - usize::from(replace) * 4);
            assert!(baked.scene.instances.sets.is_empty());
            d.replace_geometry_named(baked, "Make surface array editable")
                .unwrap();
            assert_eq!(d.history_counts().0, history + 1);
            d.undo();
            assert_eq!(d.geometry_snapshot(), before);
        }
    }
    #[test]
    fn surface_guide_mode_disable_and_remove_restore_original_faces() {
        let (mut d, asset) = document();
        let mut set = d.preview_surface_array(Some(asset), vec![0]).unwrap();
        if let Layout::Surface(a) = &mut set.layout {
            a.replace = true;
        }
        d.preview_instance_set(set.clone()).unwrap();
        d.apply_instance_preview().unwrap();
        d.set_surface_guide_mode(set.id, true).unwrap();
        assert!(d.instance_placements().unwrap().is_empty());
        assert!(d.instance_placements().unwrap().source_meshes.is_empty());
        d.set_surface_guide_mode(set.id, false).unwrap();
        assert_eq!(d.instance_placements().unwrap().len(), 1);
        set.visible = false;
        d.preview_instance_set(set).unwrap();
        d.apply_instance_preview().unwrap();
        assert!(d.instance_placements().unwrap().source_meshes.is_empty());
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn surface_expanded_budget_rejects_preview_without_losing_last_valid_result() {
        let mut part = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        part.divisions = [1; 3];
        let mut d = Document {
            geometry: Geometry::new(part.mesh().unwrap()),
            ..Default::default()
        };
        let asset = d
            .capture_object("Dense part".into(), Vec3::default(), 0, 2)
            .unwrap();
        let mesh = std::sync::Arc::make_mut(&mut d.geometry.scene.instances.assets[0].mesh);
        mesh.vertices.resize(100_000, Vec3::default());
        mesh.mask.resize(100_000, 0.);
        let mut guide = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Plane);
        guide.divisions = [4, 4, 1];
        d.geometry.mesh = guide.mesh().unwrap();
        d.display_changed();
        let set = d.preview_surface_array(Some(asset), vec![0]).unwrap();
        d.apply_instance_preview().unwrap();
        let before = d.instance_placements().unwrap();
        assert!(
            d.preview_surface_region(set.id, (0..16).collect())
                .unwrap_err()
                .contains("budget exceeded")
        );
        assert_eq!(before, d.instance_placements().unwrap());
        assert_eq!(d.instances().sets[0], set);
    }
    #[test]
    fn surface_ownership_and_hidden_owner_are_respected() {
        let mut p = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube);
        p.divisions = [1; 3];
        let mut d = Document {
            geometry: Geometry::new(p.mesh().unwrap()),
            ..Default::default()
        };
        let asset = d
            .capture_object("Part".into(), Vec3::default(), 0, 2)
            .unwrap();
        let set = d.preview_surface_array(Some(asset), vec![0]).unwrap();
        d.apply_instance_preview().unwrap();
        assert!(d.preview_surface_array(Some(asset), vec![1]).is_err());
        assert!(!d.instance_preview_active());
        assert!(!d.delete_submodel());
        d.geometry.scene.active_metadata.visible = false;
        d.display_changed();
        assert!(d.instance_placements().unwrap().is_empty());
        let mut geo = d.geometry_snapshot();
        assert!(
            geo.bake_instances(
                BakeRequest {
                    scope: BakeScope::Set(set.id),
                    destination: BakeDestination::New,
                    fuse: false
                },
                &|| false
            )
            .is_err()
        );
    }
}
