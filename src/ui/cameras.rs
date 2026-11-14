use super::*;
use crate::command::CameraCommand as C;

#[derive(Default)]
pub(crate) struct Controls {
    pub open: bool,
    pub notice: String,
    name: String,
    rename: Option<(usize, String)>,
    delete: Option<usize>,
}

impl UiState {
    pub(crate) fn camera_window(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
        camera: &Camera,
    ) -> (Vec<Command>, bool) {
        let mut commands = Vec::new();
        let mut open = self.cameras.open;
        let current = self
            .viewport_menu
            .viewport_rect
            .and_then(|r| camera.saved_view(r.size()).ok());
        let policy = crate::app::policy::EditorPolicy::new(self, document);
        let reason = policy.action(&Action::SavedCameras).err();
        let response = egui::Window::new("Saved cameras")
            .open(&mut open)
            .order(egui::Order::Foreground)
            .default_pos(egui::pos2(40., 120.))
            .default_width(370.)
            .default_height(300.)
            .min_width(260.)
            .max_height((ctx.content_rect().height() - 100.).max(160.))
            .vscroll(true)
            .show(ctx, |ui| {
                if let Some(reason) = reason { ui.label(reason); }
                ui.add_enabled_ui(reason.is_none(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Roll");
                        let mut degrees = camera.roll.to_degrees();
                        if ui.add(egui::DragValue::new(&mut degrees).speed(0.5).suffix("°")).on_hover_text("Clockwise rotation around the viewing axis. Drag or type an angle.").changed() {
                            commands.push(Command::Camera(C::Roll(degrees.to_radians())));
                        }
                        if ui.button("Reset roll").on_hover_text("Keep the current direction, pan, zoom and projection").clicked() {
                            commands.push(Command::Action(Action::ResetRoll));
                        }
                    });
                    ui.separator();
                    ui.label("Save a view to revisit while you work.");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.cameras.name).hint_text("Camera name").desired_width(190.).char_limit(128));
                        if ui.add_enabled(document.saved_cameras().len() < crate::saved_camera::MAX_CAMERAS, egui::Button::new("Save view")).clicked() {
                            let name = if self.cameras.name.trim().is_empty() {
                                (1..).map(|i| format!("Camera {i}")).find(|name| !document.saved_cameras().iter().any(|c| &c.name == name)).unwrap()
                            } else { self.cameras.name.clone() };
                            commands.push(Command::Camera(C::Save(name)));
                        }
                    });
                    if document.saved_cameras().is_empty() {
                        ui.weak("No saved cameras in this project yet.");
                    }
                    for (index, saved) in document.saved_cameras().iter().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.separator();
                            if ui.add(egui::Button::new(&saved.name).selected(current == Some(saved.view)).wrap()).on_hover_text("Switch to this saved view").clicked() {
                                commands.push(Command::Camera(C::Recall(index)));
                            }
                            ui.weak(if saved.view.perspective { "Perspective" } else { "Orthographic" });
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Update from view").on_hover_text("Replace this saved camera with the current view").clicked() {
                                    commands.push(Command::Camera(C::Update(index)));
                                }
                                if ui.button("Rename").clicked() {
                                    self.cameras.rename = Some((index, saved.name.clone()));
                                    self.cameras.delete = None;
                                }
                                if ui.button("Delete").clicked() {
                                    self.cameras.delete = Some(index);
                                    self.cameras.rename = None;
                                }
                            });
                            let mut end_rename = false;
                            if let Some((target, name)) = &mut self.cameras.rename && *target == index {
                                ui.add(egui::TextEdit::singleline(name).desired_width(230.).char_limit(128));
                                ui.horizontal(|ui| {
                                    if ui.button("Apply name").clicked() {
                                        commands.push(Command::Camera(C::Rename(index, name.clone())));
                                        end_rename = true;
                                    }
                                    end_rename |= ui.button("Cancel rename").clicked();
                                });
                            }
                            if end_rename { self.cameras.rename = None; }
                            if self.cameras.delete == Some(index) {
                                ui.label(format!("Delete ‘{}’? This cannot be undone.", saved.name));
                                ui.horizontal(|ui| {
                                    if ui.button("Delete camera").clicked() {
                                        commands.push(Command::Camera(C::Delete(index)));
                                        self.cameras.delete = None;
                                    }
                                    if ui.button("Keep camera").clicked() { self.cameras.delete = None; }
                                });
                            }
                        });
                    }
                    ui.separator();
                    ui.weak("Navigation leaves saved views unchanged. Use Update from view to replace one. Cameras save with the project; they are separate from sculpt Undo.");
                    if !self.cameras.notice.is_empty() { ui.colored_label(Color32::LIGHT_RED, &self.cameras.notice); }
                });
            });
        self.cameras.open = open;
        let blocks = response.is_some_and(|r| r.response.contains_pointer());
        (commands, blocks)
    }
}
