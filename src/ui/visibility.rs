//! Review the target and topology consequences before partitioning a surface.
use super::*;

pub(crate) struct SurfaceEditWarning {
    pub action: Action,
    pub revision: u64,
    pub model: usize,
    name: String,
    faces: usize,
    all_hidden: bool,
    threshold: Option<f32>,
    levels: usize,
}

impl SurfaceEditWarning {
    pub fn new(document: &Document, action: Action) -> Self {
        let masked = action == Action::SplitMasked;
        Self {
            action,
            revision: document.source_revision(),
            model: document.active_model_index(),
            name: document
                .model(document.active_model_index())
                .unwrap()
                .name
                .into(),
            faces: if masked {
                document.masked_face_count()
            } else {
                document.hidden_face_count()
            },
            all_hidden: document.hidden_face_count() == document.mesh().triangles.len(),
            threshold: masked.then(|| {
                document
                    .model(document.active_model_index())
                    .unwrap()
                    .metadata
                    .mask_threshold
            }),
            levels: document.mesh_settings().levels(),
        }
    }
}

impl UiState {
    pub(super) fn surface_edit_confirmation(
        &self,
        ctx: &egui::Context,
    ) -> (Vec<Command>, Option<egui::Rect>) {
        let warning = self.surface_edit_warning.as_ref().unwrap();
        let split = warning.action != Action::DeleteHidden;
        let mut commands = Vec::new();
        let response = egui::Modal::new(egui::Id::new("surface_edit_confirmation")).show(ctx, |ui| {
            ui.set_max_width(360.);
            ui.heading(match warning.action {
                Action::SplitMasked => "Split masked faces?",
                Action::SplitHidden => "Split hidden faces?",
                _ => "Delete hidden faces?",
            });
            if let Some(threshold) = warning.threshold {
                ui.label(format!("{} triangles inside “{}” meet the {:.0}% mask threshold at every corner.", warning.faces, warning.name, threshold * 100.));
                ui.label("Move these faces to a new visible, unmasked, unselected submodel; keep the other faces and their mask/visibility in the original. Includes masked hidden faces. Both pieces stay in place.");
            } else {
                ui.label(format!("{} hidden triangles inside “{}”.", warning.faces, warning.name));
                ui.label(if split {
                 "Move these faces into a new visible, unmasked, unselected submodel. Keep the visible faces and their mask/visibility in the original model. Both pieces stay in place."
            } else if warning.all_hidden {
                "Every face is hidden. Delete the entire surface and keep an empty editable model in the scene."
            } else {
                "Remove these faces from the active model. Keep its visible faces in place."
            });
            }
            ui.label(if split {
                "Other submodels are unchanged. Cut boundaries stay open. Groups/weights, paint, UVs and surviving seams are preserved; the new part's working mask and face hiding are cleared."
            } else {
                "Other submodels are unchanged. Cut boundaries stay open. Masks, groups, paint, UVs and surviving seams are preserved."
            });
            if warning.levels > 1 {
                ui.label(format!("Use the current subdivision level; discard the other {} stored levels on the edited model.", warning.levels - 1));
            }
            ui.label("Reset subdivision, Adaptive and Dynamic Mesh settings on the resulting mesh. One Undo restores the complete original state.");
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() { commands.push(Command::ConfirmSurfaceEdit(false)); }
                if ui.button(if split { "Apply split" } else { "Delete hidden faces" }).clicked() { commands.push(Command::ConfirmSurfaceEdit(true)); }
            });
        });
        if response.should_close() && commands.is_empty() {
            commands.push(Command::ConfirmSurfaceEdit(false));
        }
        (commands, Some(response.response.rect))
    }
}
