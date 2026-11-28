//! One set of transform controls reused by the sidebar and floating toolbar.
use super::design;
use super::{UiState, customization::ActionResponse};
use crate::preferences::BuiltinMenu;
use crate::{
    command::Command,
    preferences::Action,
    viewport::{TransformMode as Mode, TransformOperation as Operation},
};
use eframe::egui::{self, Vec2};

/// A compact target selector beside the viewport gizmo, using the app's icons.
pub(crate) fn gizmo_target_toggle(
    ui: &mut egui::Ui,
    edit_gizmo: bool,
    enabled: bool,
) -> Option<bool> {
    let mut requested = None;
    egui::Frame::popup(ui.style())
        .corner_radius(12.)
        .inner_margin(4.)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.;
            ui.spacing_mut().button_padding = Vec2::splat(6.);
            ui.horizontal(|ui| {
                for (edit, glyph, label, help) in [
                    (
                        false,
                        design::Glyph::Primitive,
                        "Affect model",
                        "Affect model — move, rotate and scale geometry using the gizmo",
                    ),
                    (
                        true,
                        design::Glyph::Transform,
                        "Edit gizmo",
                        "Edit gizmo — move the pivot and rotate its axes without changing geometry",
                    ),
                ] {
                    let selected = edit_gizmo == edit;
                    let icon = egui::Image::new((design::icon(ui.ctx(), glyph), Vec2::splat(20.)))
                        .tint(ui.visuals().text_color());
                    let response = ui
                        .add_enabled(
                            enabled,
                            egui::Button::image(icon)
                                .min_size(Vec2::splat(32.))
                                .corner_radius(8.)
                                .selected(selected),
                        )
                        .on_hover_text(help);
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            enabled,
                            selected,
                            label,
                        )
                    });
                    #[cfg(test)]
                    if !selected {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                egui::Id::new("gizmo_affect_model_toggle_response"),
                                response.rect,
                            )
                        });
                    }
                    if response.clicked() && !selected {
                        requested = Some(edit);
                    }
                }
            });
        });
    requested
}

impl UiState {
    fn gizmo_edit_controls(&self, ui: &mut egui::Ui, commands: &mut Vec<Command>) {
        if self.transform_mode != Mode::World || self.transform_operation == Operation::Cage {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for (edit, label) in [(false, "Affect model"), (true, "Edit gizmo")] {
                if ui.selectable_label(self.edit_gizmo == edit, label)
                    .on_hover_text("Edit gizmo moves the pivot and rotates axes without changing the model. Esc restores the current gizmo drag.")
                    .clicked() {
                    commands.push(Command::EditGizmo(edit));
                }
            }
        });
        if self.edit_gizmo {
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("Place pivot").clicked() {
                    commands.push(Command::TransformOperation(Operation::PlacePivot));
                }
                if ui.small_button("Move / rotate").clicked() {
                    commands.push(Command::TransformOperation(Operation::Universal));
                }
            });
        }
    }
    fn gizmo_reset_controls(&self, ui: &mut egui::Ui, commands: &mut Vec<Command>) {
        ui.menu_button("Reset…", |ui| {
            if ui
                .button("Reset pivot")
                .on_hover_text(
                    "Return to the active model's centre; preserve axes and Action Line direction",
                )
                .customize(Action::ResetPivot)
                .clicked()
            {
                commands.push(Command::ResetTransformPivot);
                ui.close();
            }
            if self.transform_mode == Mode::World {
                for (label, command, hint) in [
                    (
                        "Reset axes",
                        Command::ResetTransformAxes,
                        "Restore the selected space's axes; keep the pivot",
                    ),
                    (
                        "Reset gizmo",
                        Command::ResetTransformGizmo,
                        "Restore the model centre and the selected space's axes",
                    ),
                ] {
                    if ui.button(label).on_hover_text(hint).clicked() {
                        commands.push(command);
                        ui.close();
                    }
                }
            }
        });
    }
    fn transform_space_controls(&self, ui: &mut egui::Ui, commands: &mut Vec<Command>) {
        use crate::viewport::TransformSpace;
        if self.transform_mode != Mode::World || self.transform_operation == Operation::Cage {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Space");
            for (space, action) in [
                (TransformSpace::World, Action::TransformWorldSpace),
                (TransformSpace::Screen, Action::TransformScreenSpace),
            ] {
                if ui
                    .selectable_label(self.transform_space == space, action.label())
                    .on_hover_text(action.help().unwrap())
                    .customize(action)
                    .clicked()
                {
                    commands.push(Command::TransformSpace(space));
                }
            }
        });
    }
    fn transform_operations(&mut self, ui: &mut egui::Ui, commands: &mut Vec<Command>) {
        self.gizmo_edit_controls(ui, commands);
        self.transform_space_controls(ui, commands);
        ui.horizontal_wrapped(|ui| {
            for operation in Operation::ALL.into_iter().filter(|op| {
                op.available(self.transform_mode)
                    && (!self.edit_gizmo || !matches!(op, Operation::Scale | Operation::Cage))
            }) {
                if ui
                    .selectable_label(self.transform_operation == operation, operation.label())
                    .on_hover_text(operation.hint())
                    .customize(operation.into())
                    .clicked()
                {
                    commands.push(Command::TransformOperation(operation));
                }
            }
            if self.transform_operation != Operation::Cage {
                self.gizmo_reset_controls(ui, commands);
            }
        });
        if self.transform_operation == Operation::Cage {
            self.cage_options(ui);
        }
        if !self.edit_gizmo {
            self.transform_mask_control(ui, commands);
        }
    }
    fn cage_options(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Bend / twist axis");
            for axis in crate::sculpt::Axis::ALL {
                ui.selectable_value(&mut self.cage_axis, axis, format!("{axis:?}"));
            }
        });
        ui.weak("All subdivision levels and hidden faces follow.");
    }
    fn transform_mask_control(&self, ui: &mut egui::Ui, commands: &mut Vec<Command>) {
        let mut respect = self.respect_transform_mask;
        if design::checkbox(ui, &mut respect, "Respect mask").customize(Action::RespectMask)
            .on_hover_text("Protect masked vertices; soft masks reduce the rotation angle. Turn off to transform the whole model").changed() {
            commands.push(Command::RespectTransformMask(respect));
        }
    }
    pub(crate) fn transform_toolbar(
        &mut self,
        ctx: &egui::Context,
        viewport: egui::Rect,
        enabled: bool,
    ) -> (Vec<Command>, Option<egui::Rect>) {
        if self.transform_mode == Mode::Sculpt
            || (self.boolean_enabled && self.transform_operation == Operation::Cage)
            || self.preferences.menu_placed(BuiltinMenu::Transform)
        {
            return (Vec::new(), None);
        }
        let mut commands = Vec::new();
        let response = egui::Area::new(egui::Id::new("viewport_transform_toolbar"))
            .order(egui::Order::Foreground)
            .fixed_pos(viewport.left_top() + Vec2::splat(12.))
            .constrain_to(viewport)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let width = if self.transform_operation == Operation::Universal {
                        200.
                    } else {
                        300.
                    };
                    ui.set_max_width((viewport.width() - 32.).clamp(160., width));
                    ui.add_enabled_ui(enabled, |ui| {
                        let compact = self.transform_operation == Operation::Universal
                            && viewport.height() < 400.;
                        if !compact {
                            self.menu_placement_controls(ui, BuiltinMenu::Transform, false);
                        }
                        if self.transform_operation == Operation::Cage {
                            ui.horizontal(|ui| {
                                ui.strong("Deformation frame");
                                if ui.small_button("Done").clicked() {
                                    commands.push(Command::TransformMode(Mode::Sculpt));
                                }
                            });
                            ui.label("Drag corners to reshape. Drag B to bend or T to twist.");
                            self.cage_options(ui);
                            self.transform_mask_control(ui, &mut commands);
                            return;
                        }
                        if self.transform_operation == Operation::Universal {
                            self.gizmo_edit_controls(ui, &mut commands);
                            self.transform_space_controls(ui, &mut commands);
                            ui.horizontal_wrapped(|ui| {
                                // Keep the compact toolbar clear of the centre pivot
                                // in a short, narrow viewport.
                                if !compact || viewport.width() >= 500. {
                                    ui.strong("Universal")
                                        .on_hover_text(Operation::Universal.hint());
                                }
                                self.gizmo_reset_controls(ui, &mut commands);
                                if ui.small_button("Done").clicked() {
                                    commands.push(Command::TransformMode(Mode::Sculpt));
                                }
                            });
                            if compact {
                                return;
                            }
                            ui.weak(if self.edit_gizmo {
                                "Editing gizmo only · drag arrows or rings"
                            } else {
                                "Drag centre diamond to move pivot"
                            });
                            let key = self
                                .preferences
                                .shortcut(&Action::UniversalTransform)
                                .map_or("Unbound".into(), |s| s.label());
                            let menu = self
                                .preferences
                                .shortcut(&Action::TransformMenu)
                                .map_or("Unbound".into(), |s| s.label());
                            ui.weak(format!("{key}: sculpt · {menu}: transform options"));
                            return;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.strong("Transform");
                            for (mode, label) in [
                                (Mode::World, "World axes"),
                                (Mode::ActionLine, "Action Line"),
                            ] {
                                if ui
                                    .selectable_label(self.transform_mode == mode, label)
                                    .customize(mode.into())
                                    .clicked()
                                {
                                    commands.push(Command::TransformMode(mode));
                                }
                            }
                            if ui
                                .small_button("Done")
                                .on_hover_text("Return to sculpting")
                                .clicked()
                            {
                                commands.push(Command::TransformMode(Mode::Sculpt));
                            }
                        });
                        ui.separator();
                        self.transform_operations(ui, &mut commands);
                    });
                });
            });
        (commands, Some(response.response.rect))
    }
    pub(super) fn transform_controls(&mut self, ui: &mut egui::Ui, commands: &mut Vec<Command>) {
        ui.add_enabled_ui(!self.mesh_busy, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (mode, label) in [(Mode::Sculpt, "Sculpt"), (Mode::World, "World axes"), (Mode::ActionLine, "Action Line")] {
                    if ui.selectable_label(self.transform_mode == mode, label).customize(mode.into()).clicked() {
                        commands.push(Command::TransformMode(mode));
                    }
                }
            });
            if self.transform_mode != Mode::Sculpt {
                self.transform_operations(ui, commands);
                ui.label(if self.edit_gizmo { "Drag arrows to move the pivot or rings to rotate the axes. Esc cancels the current gizmo drag." } else { self.transform_operation.hint() });
                ui.label("Pivot and line placement only affect the controls. Each geometry drag is one undo step; all subdivision levels follow.");
            }
        });
    }
}
