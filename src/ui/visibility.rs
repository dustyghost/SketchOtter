//! Review the target and topology consequences before editing hidden faces.
use super::*;

pub(crate) struct HiddenFacesWarning {
    pub split: bool,
    pub revision: u64,
    pub model: usize,
    name: String,
    faces: usize,
    levels: usize,
}

impl HiddenFacesWarning {
    pub fn new(document: &Document, split: bool) -> Self {
        Self {
            split,
            revision: document.source_revision(),
            model: document.active_model_index(),
            name: document
                .model(document.active_model_index())
                .unwrap()
                .name
                .into(),
            faces: document.hidden_face_count(),
            levels: document.mesh_settings().levels(),
        }
    }
}

impl UiState {
    pub(super) fn hidden_faces_confirmation(
        &self,
        ctx: &egui::Context,
    ) -> (Vec<Command>, Option<egui::Rect>) {
        let warning = self.hidden_faces_warning.as_ref().unwrap();
        let mut commands = Vec::new();
        let response = egui::Modal::new(egui::Id::new("hidden_faces_confirmation")).show(ctx, |ui| {
            ui.set_max_width(360.);
            ui.heading(if warning.split { "Split hidden faces?" } else { "Delete hidden faces?" });
            ui.label(format!("{} hidden triangles inside “{}”.", warning.faces, warning.name));
            ui.label(if warning.split {
                "Move these faces into a new visible submodel. Keep the visible faces in the original model. Both pieces stay in place."
            } else {
                "Remove these faces from the active model. Keep its visible faces in place."
            });
            ui.label("Other submodels are unchanged. Cut boundaries stay open. Masks, groups, paint, UVs and surviving seams are preserved.");
            if warning.levels > 1 {
                ui.label(format!("Use the current subdivision level; discard the other {} stored levels on the edited model.", warning.levels - 1));
            }
            ui.label("Reset subdivision, Adaptive and Dynamic Mesh settings on the resulting mesh. One Undo restores the complete original state.");
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() { commands.push(Command::ConfirmHiddenFaces(false)); }
                if ui.button(if warning.split { "Apply split" } else { "Delete hidden faces" }).clicked() { commands.push(Command::ConfirmHiddenFaces(true)); }
            });
        });
        if response.should_close() && commands.is_empty() {
            commands.push(Command::ConfirmHiddenFaces(false));
        }
        (commands, Some(response.response.rect))
    }
}
