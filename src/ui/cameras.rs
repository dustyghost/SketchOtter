use super::*;
use crate::command::CameraCommand as C;

#[derive(Default)]
pub(crate) struct Controls {
    pub open: bool,
    pub notice: String,
    pub active: Option<String>,
    name: String,
    rename: Option<(usize, String)>,
    delete: Option<usize>,
    update: Option<usize>,
    feedback: String,
    focus_edit: bool,
}
impl Controls {
    pub(crate) fn saved(&mut self, name: String) {
        self.name.clear();
        self.feedback = format!("Saved ‘{name}’.");
        self.active = Some(name);
    }
    pub(crate) fn updated(&mut self, name: String) {
        self.update = None;
        self.feedback = format!("Updated ‘{name}’.");
        self.active = Some(name);
    }
    pub(crate) fn recalled(&mut self, name: String) {
        self.feedback = format!("Recalled ‘{name}’.");
        self.active = Some(name);
        self.update = None;
    }
    pub(crate) fn renamed(&mut self, old: &str, new: String) {
        if self.active.as_deref() == Some(old) {
            self.active = Some(new.clone());
        }
        self.rename = None;
        self.feedback = format!("Renamed to ‘{new}’.");
    }
    pub(crate) fn deleted(&mut self, name: &str) {
        if self.active.as_deref() == Some(name) {
            self.active = None;
        }
        // Removing a row shifts every later index. Drop pending row actions.
        self.rename = None;
        self.delete = None;
        self.update = None;
        self.feedback = format!("Deleted ‘{name}’.");
    }
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
        let reason = crate::app::policy::EditorPolicy::new(self, document)
            .action(&Action::SavedCameras)
            .err();
        let response = egui::Window::new("Saved cameras")
            .open(&mut open)
            .order(egui::Order::Foreground)
            .default_pos(egui::pos2(40., 120.))
            .default_width(370.)
            .min_width(260.)
            .show(ctx, |ui| {
                if let Some(reason) = reason { ui.label(reason); }
                ui.add_enabled_ui(reason.is_none(), |ui| {
                    ui.horizontal_wrapped(|ui| {
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
                    ui.label("Save views with this project. Navigation leaves them unchanged.");
                    let cameras = document.saved_cameras();
                    let mut name_error = None;
                    ui.horizontal_wrapped(|ui| {
                        let response = ui.add(egui::TextEdit::singleline(&mut self.cameras.name).id_salt("save_camera_name").hint_text("Camera name").desired_width(190.).char_limit(128));
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("camera_save_name_rect"), response.rect));
                        let _ = response;
                        name_error = if self.cameras.name.trim().is_empty() { None } else { crate::saved_camera::validate_name(self.cameras.name.trim(), cameras, None).err() };
                        if ui.add_enabled(cameras.len() < crate::saved_camera::MAX_CAMERAS && name_error.is_none(), egui::Button::new("Save view")).on_disabled_hover_text(name_error.as_deref().unwrap_or("The project has reached its 128-camera limit. Delete an unused view first.")).clicked() {
                            let name = if self.cameras.name.trim().is_empty() {
                                (1..).map(|i| format!("Camera {i}")).find(|name| !cameras.iter().any(|c| &c.name == name)).unwrap()
                            } else { self.cameras.name.clone() };
                            commands.push(Command::Camera(C::Save(name)));
                        }
                    });
                    if let Some(error) = name_error { ui.colored_label(Color32::LIGHT_RED, error); }
                    if !self.cameras.notice.is_empty() {
                        ui.colored_label(Color32::LIGHT_RED, &self.cameras.notice);
                    } else if !self.cameras.feedback.is_empty() {
                        ui.label(&self.cameras.feedback);
                    }
                    ui.weak(format!("{} / {} views · Camera edits are separate from sculpt Undo.", cameras.len(), crate::saved_camera::MAX_CAMERAS));
                    if cameras.is_empty() { ui.weak("No saved cameras in this project yet."); }
                    egui::ScrollArea::vertical().id_salt("saved_camera_list").max_height((ctx.content_rect().height() - 350.).max(100.)).show(ui, |ui| {
                        for (index, saved) in cameras.iter().enumerate() {
                            ui.push_id(index, |ui| {
                                ui.separator();
                                let active = self.cameras.active.as_deref() == Some(saved.name.as_str());
                                if ui.add(egui::Button::new(&saved.name).selected(active).wrap()).on_hover_text("Switch to this saved view").clicked() {
                                    commands.push(Command::Camera(C::Recall(index)));
                                }
                                let projection = if saved.view.perspective { "Perspective" } else { "Orthographic" };
                                let state = if active {
                                    if current.is_some_and(|v| v.matches(&saved.view)) { " · Current view" } else { " · View modified" }
                                } else { "" };
                                ui.weak(format!("{projection}{state}"));
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("Update from view").on_hover_text("Review replacing this saved camera with the current view").clicked() {
                                        self.cameras.update = Some(index);
                                        self.cameras.focus_edit = true;
                                        self.cameras.rename = None;
                                        self.cameras.delete = None;
                                    }
                                    if ui.button("Rename").clicked() {
                                        self.cameras.rename = Some((index, saved.name.clone()));
                                        self.cameras.focus_edit = true;
                                        self.cameras.delete = None;
                                        self.cameras.update = None;
                                    }
                                    if ui.button("Delete").clicked() {
                                        self.cameras.delete = Some(index);
                                        self.cameras.focus_edit = true;
                                        self.cameras.rename = None;
                                        self.cameras.update = None;
                                    }
                                });
                                let mut cancel_rename = false;
                                if let Some((target, name)) = &mut self.cameras.rename && *target == index {
                                    let response = ui.add(egui::TextEdit::singleline(name).id_salt("rename_camera_name").desired_width(230.).char_limit(128));
                                    #[cfg(test)]
                                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("camera_rename_name_rect"), response.rect));
                                    let _ = response;
                                    let error = crate::saved_camera::validate_name(name.trim(), cameras, Some(index)).err();
                                    if let Some(error) = &error { ui.colored_label(Color32::LIGHT_RED, error); }
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.add_enabled(error.is_none(), egui::Button::new("Apply name")).clicked() {
                                            commands.push(Command::Camera(C::Rename(index, name.clone())));
                                        }
                                        cancel_rename = ui.button("Cancel rename").clicked();
                                    });
                                }
                                if cancel_rename { self.cameras.rename = None; }
                                if self.cameras.update == Some(index) {
                                    ui.label(format!("Replace ‘{}’ with the current view? The previous view cannot be restored with Undo.", saved.name));
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button("Replace saved view").clicked() { commands.push(Command::Camera(C::Update(index))); }
                                        if ui.button("Keep saved view").clicked() { self.cameras.update = None; }
                                    });
                                }
                                if self.cameras.delete == Some(index) {
                                    ui.label(format!("Delete ‘{}’? This cannot be undone.", saved.name));
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.button("Delete camera").clicked() { commands.push(Command::Camera(C::Delete(index))); }
                                        if ui.button("Keep camera").clicked() { self.cameras.delete = None; }
                                    });
                                }
                                if self.cameras.focus_edit && (self.cameras.update == Some(index) || self.cameras.delete == Some(index) || self.cameras.rename.as_ref().is_some_and(|(i, _)| *i == index)) {
                                    let row = ui.min_rect();
                                    let align = (row.height() > ui.clip_rect().height()).then_some(egui::Align::Max);
                                    ui.scroll_to_rect_animation(row, align, egui::style::ScrollAnimation::none());
                                    self.cameras.focus_edit = false;
                                }
                            });
                        }
                    });
                });
            });
        self.cameras.open = open;
        let blocks = response.is_some_and(|r| r.response.contains_pointer());
        (commands, blocks)
    }
}
