//! Vertex groups and partial face visibility.
use super::*;

impl Document {
    pub(super) fn edit_group_meshes(
        &mut self,
        label: &'static str,
        edit: impl FnMut(&mut Mesh) -> Result<(), String>,
    ) -> Result<(), String> {
        self.edit_group_registry(label, edit, false).map(|_| ())
    }
    fn edit_group_registry(
        &mut self,
        label: &'static str,
        mut edit: impl FnMut(&mut Mesh) -> Result<(), String>,
        compact: bool,
    ) -> Result<Vec<Option<usize>>, String> {
        self.require_sculpt_mesh()?;
        self.end_edit();
        self.end_stroke();
        let mut geometry = self.geometry.clone();
        for mesh in std::iter::once(&mut geometry.mesh)
            .chain(geometry.settings.lower.iter_mut())
            .chain(geometry.settings.upper.iter_mut())
        {
            edit(mesh)?;
        }
        let mapping = if compact {
            geometry.compact_vertex_groups()
        } else {
            (0..geometry.mesh.vertex_groups.len()).map(Some).collect()
        };
        geometry.mesh.validate()?;
        if geometry != self.geometry {
            self.replace_geometry_named(geometry, label)?;
        }
        Ok(mapping)
    }
    pub fn create_vertex_group(&mut self, name: String, all: bool) -> Result<usize, String> {
        if all && let Some(reason) = self.group_all_unavailable() {
            return Err(reason.into());
        }
        let threshold = self.geometry.scene.active_metadata.mask_threshold;
        if self.mesh().vertices.is_empty()
            || (!all
                && !self
                    .mesh()
                    .mask
                    .iter()
                    .any(|&m| crate::sculpt::mask_selected(m, threshold)))
        {
            return Err("Mask vertices above the threshold first, or use Group all".into());
        }
        let name = if name.trim().is_empty() {
            (1..)
                .map(|n| format!("Group {n}"))
                .find(|name| !self.mesh().vertex_groups.iter().any(|g| &g.name == name))
                .unwrap()
        } else {
            name
        };
        let name = name.trim().to_owned();
        if name.is_empty() || name.len() > 128 {
            return Err("Choose a group name up to 128 bytes".into());
        }
        let index = self.mesh().vertex_groups.len();
        let mapping = self.edit_group_registry(
            if all {
                "Group visible vertices"
            } else {
                "Create vertex group"
            },
            |mesh| {
                if all {
                    mesh.group_visible_vertices(name.clone());
                    return Ok(());
                }
                let previous = mesh.vertex_groups.clone();
                let previous_labels = mesh.polygroups.clone();
                let index = mesh.create_replacement_group(name.clone(), threshold, all)?;
                mesh.replace_group_memberships(
                    index,
                    threshold,
                    false,
                    &previous,
                    &previous_labels,
                );
                Ok(())
            },
            true,
        )?;
        Ok(mapping[index].expect("A newly assigned group has members"))
    }
    pub(crate) fn group_all_unavailable(&self) -> Option<&'static str> {
        if !self.active_visible() {
            Some("Show the active model before grouping its visible faces")
        } else if self.hidden_face_count() == self.mesh().triangles.len() {
            Some("Show some faces before using Group all")
        } else {
            None
        }
    }
    pub fn assign_mask_to_group(
        &mut self,
        index: usize,
        remove: bool,
    ) -> Result<Option<usize>, String> {
        let threshold = self.geometry.scene.active_metadata.mask_threshold;
        self.edit_group_registry(
            if remove {
                "Remove masked vertices from group"
            } else {
                "Assign masked vertices to group"
            },
            |mesh| {
                let previous = mesh.vertex_groups.clone();
                let previous_labels = mesh.polygroups.clone();
                mesh.assign_mask_to_group(index, threshold, remove)?;
                mesh.replace_group_memberships(
                    index,
                    threshold,
                    remove,
                    &previous,
                    &previous_labels,
                );
                Ok(())
            },
            true,
        )
        .map(|mapping| mapping[index])
    }
    pub fn mask_from_group(&mut self, index: usize) -> Result<(), String> {
        self.edit_group_meshes("Mask vertex group", |mesh| mesh.mask_from_group(index))
    }
    pub fn remove_vertex_group(&mut self, index: usize) -> Result<(), String> {
        self.edit_group_meshes("Delete vertex group", |mesh| {
            mesh.remove_vertex_group(index)
        })
    }
    pub fn set_vertex_group_visible(&mut self, index: usize, visible: bool) -> Result<(), String> {
        self.edit_group_meshes("Vertex group visibility", |mesh| {
            mesh.require_selection_group(index)?;
            mesh.vertex_groups
                .get_mut(index)
                .ok_or("Vertex group no longer exists")?
                .visible = visible;
            if visible && !mesh.hidden_faces.is_empty() {
                for (i, hidden) in mesh.hidden_faces.iter_mut().enumerate() {
                    if mesh.polygroups.get(i).copied().unwrap_or(0) == index + 1 {
                        *hidden = false;
                    }
                }
                if mesh.hidden_faces.iter().all(|&h| !h) {
                    mesh.hidden_faces.clear();
                }
            }
            Ok(())
        })
    }
    // Normalize group visibility consistently across all levels, but preserve
    // history/redo when the operation leaves every effective face set unchanged.
    pub(super) fn edit_visibility_meshes(
        &mut self,
        label: &'static str,
        mut edit: impl FnMut(&mut Mesh) -> Result<(), String>,
    ) -> Result<(), String> {
        self.require_sculpt_mesh()?;
        self.end_edit();
        self.end_stroke();
        let mut geometry = self.geometry.clone();
        let old = std::iter::once(&self.geometry.mesh)
            .chain(self.geometry.settings.lower.iter())
            .chain(self.geometry.settings.upper.iter());
        let new = std::iter::once(&mut geometry.mesh)
            .chain(geometry.settings.lower.iter_mut())
            .chain(geometry.settings.upper.iter_mut());
        let mut changed = false;
        for (before, after) in old.zip(new) {
            edit(after)?;
            changed |= (0..before.triangles.len())
                .any(|i| before.face_visible(i) != after.face_visible(i));
        }
        if changed {
            self.replace_geometry_named(geometry, label)?;
        }
        Ok(())
    }
    pub fn show_all_faces(&mut self) -> Result<(), String> {
        self.edit_group_meshes("Show all faces", |mesh| {
            mesh.show_all_faces();
            Ok(())
        })
    }
    pub fn invert_visibility(&mut self) -> Result<(), String> {
        self.edit_visibility_meshes("Invert visibility", |mesh| {
            mesh.invert_visibility();
            Ok(())
        })
    }
    pub fn select_polygroup(&mut self, id: usize) -> Result<(), String> {
        let other_visible_group = (0..self.mesh().triangles.len()).any(|i| {
            self.mesh().face_visible(i) && self.mesh().polygroups.get(i).copied().unwrap_or(0) != id
        });
        // Hide the clicked group. At the last visible group, swap the visible
        // and hidden face sets so that the clicked group still disappears.
        if !other_visible_group {
            return self.invert_visibility();
        }
        self.edit_visibility_meshes("Hide vertex group faces", |mesh| {
            let visible = (0..mesh.triangles.len())
                .map(|i| mesh.face_visible(i) && mesh.polygroups.get(i).copied().unwrap_or(0) != id)
                .collect();
            mesh.set_face_visibility(visible);
            Ok(())
        })
    }
    /// The same view-space region is applied to every stored subdivision level.
    /// Quads are selected as a whole, using their center, through all depths.
    pub fn select_face_region(
        &mut self,
        contains: impl Fn(crate::sculpt::Vec3) -> bool,
        hide: bool,
    ) -> Result<(), String> {
        self.edit_visibility_meshes(
            if hide {
                "Hide selection"
            } else {
                "Isolate selection"
            },
            |mesh| {
                let selected = mesh.face_region_selection(&contains);
                let visible = selected
                    .iter()
                    .enumerate()
                    .map(|(i, &selected)| {
                        mesh.face_visible(i) && if hide { !selected } else { selected }
                    })
                    .collect();
                mesh.set_face_visibility(visible);
                Ok(())
            },
        )
    }
    pub fn split_hidden(&mut self) -> Result<(), String> {
        self.require_sculpt_mesh()?;
        if let Some(reason) = self.hidden_faces_unavailable(true) {
            return Err(reason.into());
        }
        let visible_mesh = self.mesh().visibility_part(true)?;
        let hidden_mesh = self.mesh().visibility_part(false)?;
        let name = (1..)
            .map(|i| format!("Hidden {i}"))
            .find(|name| !self.submodel_names().any(|n| n == name))
            .unwrap();
        let mut models = self.ordered_models();
        let active = self.active_model_index();
        models[active].mesh = visible_mesh;
        models[active].mesh_settings = MeshSettings::default();
        let mut metadata = models[active].metadata.clone();
        metadata.model_id = 0;
        metadata.visible = true;
        models.insert(
            active + 1,
            crate::sculpt::Submodel {
                name,
                metadata,
                mesh: hidden_mesh,
                mesh_settings: MeshSettings::default(),
            },
        );
        self.end_edit();
        self.end_stroke();
        self.record_edit("Split hidden");
        self.install_models(models, active);
        Ok(())
    }
    /// Shared prerequisites for menus, search and document operations. Visibility
    /// means face/group hiding inside the active model, never camera occlusion.
    pub(crate) fn hidden_faces_unavailable(&self, split: bool) -> Option<&'static str> {
        if self.primitive().is_some() {
            Some("Convert the primitive to an editable mesh first")
        } else if self.require_sculpt_mesh().is_err() {
            Some("Choose Edit source or Make editable for the active array first")
        } else if !self.active_visible() {
            Some("Show the active model in Submodels first")
        } else if self.geometry.scene.active_metadata.temporary_thickness > 0. {
            Some("Apply or remove the Thickness modifier first")
        } else if split && self.model_count() >= 64 {
            Some("Split hidden needs a free submodel slot (maximum 64)")
        } else if self.hidden_face_count() == 0 {
            Some("Hide some faces inside the active model first")
        } else if self.hidden_face_count() == self.mesh().triangles.len() {
            Some("Keep some faces visible; this operation needs a remaining surface")
        } else if !self.mesh().loose_edges.is_empty() || !self.mesh().loose_vertices.is_empty() {
            Some(
                "This operation cannot preserve loose edges or isolated vertices; remove them first",
            )
        } else if !self.mesh().uv.corners.is_empty() && !self.mesh().uv.current(self.mesh()) {
            Some("The UV layout is stale; restore or replace it before editing hidden faces")
        } else {
            None
        }
    }
    pub fn delete_hidden(&mut self) -> Result<(), String> {
        self.require_sculpt_mesh()?;
        // A genuine no-op must not end gestures, mark dirty or consume Redo.
        if self.hidden_face_count() == 0 {
            return Ok(());
        }
        if let Some(reason) = self.hidden_faces_unavailable(false) {
            return Err(reason.into());
        }
        let mesh = self.mesh().visibility_part(true)?;
        let active = self.active_model_index();
        let mut models = self.ordered_models();
        models[active].mesh = mesh;
        models[active].mesh_settings = MeshSettings::default();
        // Extraction deliberately removes attributes belonging only to deleted
        // faces. Install it like Split hidden, without the general geometry
        // commit's fallback that preserves an old UV map when the new one is empty.
        self.end_edit();
        self.end_stroke();
        self.record_edit("Delete hidden");
        self.install_models(models, active);
        Ok(())
    }
    pub fn rename_vertex_group(&mut self, index: usize, name: String) -> Result<(), String> {
        self.edit_group_meshes("Rename vertex group", |mesh| {
            mesh.rename_vertex_group(index, name.clone())
        })
    }
}
