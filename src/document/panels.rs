//! Transactional panel output. The source (including its stack and paths) stays recoverable.
use super::*;
impl Document {
    pub(crate) fn require_panel_target(&self) -> Result<(), String> {
        self.require_sculpt_mesh()?;
        if !self.active_visible() {
            return Err("Show the source model before generating panels".into());
        }
        let metadata = &self.geometry.scene.active_metadata;
        if metadata.temporary_thickness > 0. {
            return Err("Apply or remove the Thickness modifier before generating panels".into());
        }
        if self.attached_array(self.active_model_index()).is_some() {
            return Err("Make the instance or array editable before generating panels".into());
        }
        if self.model_count() >= 64 {
            return Err("Free a submodel slot before generating panels".into());
        }
        if !self.mesh().polygroups.iter().any(|&g| g > 0)
            && !self.mesh().mask.iter().any(|&m| m > 0.)
        {
            return Err(
                "Mask a region or assign faces to a polygon group before generating panels".into(),
            );
        }
        Ok(())
    }
    pub(crate) fn apply_panels(
        &mut self,
        panels: Vec<crate::sculpt::GeneratedPanel>,
        revision: u64,
        settings: Option<crate::diagnostics::EditSettings>,
    ) -> Result<(), String> {
        self.require_panel_target()?;
        if revision != self.display_revision() {
            return Err("The scene changed; preview the panels again".into());
        }
        if panels.is_empty() || self.model_count() + panels.len() > 64 {
            return Err("Panels plus their retained source exceed the 64 submodel limit; select fewer groups".into());
        }
        let mut geometry = self.geometry_snapshot();
        let active = geometry.scene.active_index;
        let source = crate::sculpt::Submodel {
            mesh: geometry.mesh.clone(),
            mesh_settings: geometry.settings.clone(),
            name: geometry.name.clone(),
            metadata: crate::sculpt::ModelMetadata {
                visible: false,
                ..geometry.scene.active_metadata.clone()
            },
        };
        let mut used: std::collections::BTreeSet<String> =
            self.submodel_names().map(String::from).collect();
        let mut output = Vec::new();
        for panel in panels {
            panel.mesh.validate_closed()?;
            let label = panel
                .mesh
                .vertex_groups
                .get(panel.group.saturating_sub(1))
                .ok_or("Panel group no longer exists")?;
            let mut short = label.name.clone();
            while short.len() > 105 {
                short.pop();
            }
            let base = format!("Panel · {short}");
            let name = (1..)
                .map(|i| {
                    if i == 1 {
                        base.clone()
                    } else {
                        format!("{base} {i}")
                    }
                })
                .find(|n| !used.contains(n))
                .unwrap();
            used.insert(name.clone());
            output.push(crate::sculpt::Submodel {
                name,
                mesh: panel.mesh,
                mesh_settings: Default::default(),
                metadata: crate::sculpt::ModelMetadata {
                    model_id: 0,
                    visible: true,
                    boolean_role: Default::default(),
                    ..geometry.scene.active_metadata.clone()
                },
            });
        }
        let first = output.remove(0);
        geometry.mesh = first.mesh;
        geometry.settings = first.mesh_settings;
        geometry.name = first.name;
        geometry.scene.active_metadata = first.metadata;
        output.push(source);
        geometry.submodels.splice(active..active, output);
        self.replace_geometry_recorded(geometry, "Generate panels", settings)
    }
}
