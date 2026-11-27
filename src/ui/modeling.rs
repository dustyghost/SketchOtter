use super::*;
use crate::viewport::{ComponentMode, ModelTool, SelectionChange, SelectionSpan};
#[derive(Default)]
pub(crate) struct ModelingControls {
    pub active: bool,
    pub mode: ComponentMode,
    pub tool: ModelTool,
    pub change: SelectionChange,
    pub span: SelectionSpan,
    pub flat_angle: f32,
    pub erase_radius: f32,
    pub count: usize,
    pub symmetry_error: Option<String>,
    pub selection_undo: bool,
    pub selection_redo: bool,
    pub loop_count: u32,
    pub bridge_segments: u32,
    pub join: bool,
    pub axis: crate::viewport::ModelAxis,
    pub interaction: crate::viewport::ModelInteraction,
    pub transform_direction: crate::sculpt::Vec3,
    pub directions: std::collections::BTreeMap<ModelTool, crate::sculpt::Vec3>,
    pub amount: f32,
    pub preview_amount: Option<f32>,
    pub protect_mask: bool,
    pub individual: bool,
    pub snap: f32,
    pub snap_target: crate::viewport::SnapTarget,
    pub snap_radius: Option<f32>,
    pub extrude_snap: Option<bool>,
    pub move_snap: crate::viewport::MoveSnap,
    pub new_group: bool,
    pub delete_keep_vertices: bool,
    pub bevel_segments: Option<u32>,
    pub bevel_follow_edge_loop: Option<bool>,
    pub snap_active: bool,
    pub snap_ready: bool,
    pub snap_hint: Option<String>,
    pub construction: crate::viewport::ModelConstruction,
    pub tap_repeat: bool,
    pub smooth_preview: bool,
    pub preset_name: String,
    pub preset: Option<u64>,
    pub can_repeat: bool,
    pub repeat_label: String,
    pub amounts: std::collections::BTreeMap<ModelTool, f32>,
    pub group_name: String,
    pub group: usize,
}
impl ModelingControls {
    pub fn set_tool(&mut self, tool: ModelTool) {
        if tool == self.tool {
            return;
        }
        self.amounts.insert(self.tool, self.amount);
        self.directions.insert(self.tool, self.transform_direction);
        self.transform_direction = self.directions.get(&tool).copied().unwrap_or_default();
        self.tool = tool;
        self.amount = self.amounts.get(&tool).copied().unwrap_or(match tool {
            ModelTool::EdgeSplit | ModelTool::EdgeCollapse => 0.5,
            ModelTool::Scale => 1.,
            ModelTool::Rotate => 15.,
            _ => 0.1,
        });
    }

    pub fn load_options(&mut self, o: crate::viewport::ModelingOptions) {
        self.amounts.insert(self.tool, self.amount);
        self.amounts.insert(o.tool, o.amount);
        self.directions.insert(self.tool, self.transform_direction);
        self.directions.insert(o.tool, o.transform_direction);
        self.interaction = o.interaction;
        self.transform_direction = o.transform_direction;
        self.mode = o.mode;
        self.tool = o.tool;
        self.change = o.change;
        self.span = o.span;
        self.flat_angle = o.flat_angle;
        self.erase_radius = o.erase_radius;
        self.join = o.join;
        self.loop_count = o.loop_count;
        self.bridge_segments = o.bridge_segments;
        self.axis = o.axis;
        self.amount = o.amount;
        self.protect_mask = o.protect_mask;
        self.individual = o.individual;
        self.snap = o.snap;
        self.snap_target = o.snap_target;
        self.snap_radius = Some(o.snap_radius);
        self.extrude_snap = Some(o.extrude_snap);
        self.move_snap = o.move_snap;
        self.new_group = o.new_group;
        self.delete_keep_vertices = o.delete_keep_vertices;
        self.bevel_segments = Some(o.bevel_segments);
        self.bevel_follow_edge_loop = Some(o.bevel_follow_edge_loop);
        self.construction = o.construction;
        self.tap_repeat = o.tap_repeat;
        self.smooth_preview = o.smooth_preview;
    }

    pub fn options(&self) -> crate::viewport::ModelingOptions {
        crate::viewport::ModelingOptions {
            mode: self.mode,
            tool: self.tool,
            change: self.change,
            span: self.span,
            flat_angle: self.flat_angle,
            erase_radius: self.erase_radius,
            join: self.join,
            loop_count: self.loop_count,
            bridge_segments: self.bridge_segments,
            axis: self.axis,
            interaction: self.interaction,
            // Slide's visible numeric controls use Axis. A drag latches its
            // own view-relative direction privately; Repeat retains that edit,
            // but loading it must not override a later numeric axis choice.
            transform_direction: if self.tool == ModelTool::Slide {
                crate::sculpt::Vec3::default()
            } else {
                self.transform_direction
            },
            amount: self.amount,
            protect_mask: self.protect_mask,
            individual: self.individual,
            snap: self.snap,
            snap_target: self.snap_target,
            snap_radius: self.snap_radius.unwrap_or(12.),
            extrude_snap: self.extrude_snap.unwrap_or(true),
            move_snap: self.move_snap,
            new_group: self.new_group,
            delete_keep_vertices: self.delete_keep_vertices,
            bevel_segments: self.bevel_segments.unwrap_or(3),
            bevel_follow_edge_loop: self.bevel_follow_edge_loop.unwrap_or(true),
            construction: self.construction,
            tap_repeat: self.tap_repeat,
            smooth_preview: self.smooth_preview,
        }
    }
}
impl UiState {
    pub(super) fn bevel_controls(&mut self, ui: &mut egui::Ui) -> bool {
        ui.separator();
        ui.strong(self.modeling.tool.label());
        if let Some(width) = self.modeling.preview_amount {
            ui.label(format!("Width {width:.6} (preview)"));
        } else {
            search::setting_response(
                ui.add(
                    egui::DragValue::new(&mut self.modeling.amount)
                        .prefix("Width ")
                        .range(0.000001..=1000.)
                        .speed(0.001)
                        .max_decimals(6),
                ),
                "Modeling amount",
            );
        }
        if self.modeling.tool.rounded_bevel() {
            self.bevel_segments_control(ui);
        }
        if self.modeling.mode != ComponentMode::Face {
            search::setting_response(
                design::checkbox(
                    ui,
                    self.modeling.bevel_follow_edge_loop.get_or_insert(true),
                    "Follow edge loop",
                ),
                "Follow edge loop",
            );
        }
        ui.weak("Drag up to widen · release applies · Esc cancels");
        ui.weak("Closed surface corners. Reduce width if cuts overlap. Edited faces need unwrapping for texture painting.");
        ui.add_enabled(self.modeling.count > 0, egui::Button::new("Apply amount"))
            .customize(Action::ModelApply)
            .clicked()
    }
    pub(super) fn bevel_segments_control(&mut self, ui: &mut egui::Ui) {
        search::setting_response(
            ui.add(
                egui::Slider::new(self.modeling.bevel_segments.get_or_insert(3), 2..=8)
                    .text("Segments"),
            ),
            "Bevel segments",
        );
    }
    pub(super) fn modeling_general_options(&mut self, ui: &mut egui::Ui) {
        if self.modeling.tool == ModelTool::Delete {
            ui.checkbox(
                &mut self.modeling.delete_keep_vertices,
                "Keep unused vertices",
            )
            .on_hover_text("Deleting an edge also removes its attached faces; protected borders stay intact. Keep the selected vertices after deleting.");
        }
        search::setting_response(
            ui.checkbox(
                &mut self.modeling.new_group,
                "New vertex group after each edit",
            ),
            "Modeling new vertex group",
        )
        .on_hover_text("Put created or changed geometry in a named group. Undo removes the edit and group together. Selection alone creates no group.");
    }
    pub(super) fn point_split_controls(&mut self, ui: &mut egui::Ui) -> bool {
        ui.separator();
        ui.strong("Split point settings");
        if let Some(amount) = self.modeling.preview_amount {
            ui.label(format!("Radius {amount:.6} (preview)"));
        } else {
            search::setting_response(
                ui.add(
                    egui::DragValue::new(&mut self.modeling.amount)
                        .prefix("Radius ")
                        .range(0.000001..=1000.)
                        .speed(0.001)
                        .max_decimals(6),
                ),
                "Modeling amount",
            );
        }
        search::setting_response(
            design::checkbox(
                ui,
                &mut self.modeling.construction.double_ring,
                "Double ring sides (4 to 8)",
            ),
            "Double ring sides",
        );
        search::setting_response(
            design::checkbox(
                ui,
                &mut self.modeling.construction.round,
                "Circularize ring",
            ),
            "Circularize ring",
        );
        search::setting_response(
            design::checkbox(ui, &mut self.modeling.construction.crease, "Crease ring"),
            "Crease ring",
        );
        search::setting_response(
            design::checkbox(
                ui,
                &mut self.modeling.construction.triangulate,
                "Triangulate center",
            ),
            "Triangulate centre",
        );
        design::checkbox(ui, &mut self.modeling.tap_repeat, "Tap to repeat")
            .customize(Action::ModelTapRepeat);
        ui.weak("Radius uses model units from the original vertex. Press to preview; drag up/down to resize. Release applies; Esc cancels.");
        ui.weak("Closed point fan · radius below half the shortest edge");
        ui.add_enabled(self.modeling.count > 0, egui::Button::new("Apply amount"))
            .customize(Action::ModelApply)
            .clicked()
    }

    pub(super) fn modeling_presets(&mut self, ui: &mut egui::Ui, commands: &mut Vec<Command>) {
        egui::CollapsingHeader::new("Tool presets")
            .open(search::requested_section(ui.ctx(), "model_presets"))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.modeling.preset_name)
                        .hint_text("Preset name"),
                );
                if ui
                    .button("Save tool preset")
                    .customize(Action::ModelSavePreset)
                    .clicked()
                {
                    commands.push(Command::Action(Action::ModelSavePreset));
                }
                for preset in &self.preferences.modeling_presets {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.modeling.preset == Some(preset.id),
                                format!("Use {}", preset.name),
                            )
                            .customize(Action::ModelPreset(preset.id))
                            .clicked()
                        {
                            commands.push(Command::Action(Action::ModelPreset(preset.id)));
                        }
                        if ui.small_button("Delete").clicked() {
                            commands.push(Command::Action(Action::ModelDeletePreset(preset.id)));
                        }
                    });
                }
                ui.weak(
                    "Saves action, target and options. Right-click a preset to add it to a shelf.",
                );
            });
    }

    pub(super) fn modeling_tool_options(
        &mut self,
        ui: &mut egui::Ui,
        document: &Document,
        commands: &mut Vec<Command>,
    ) {
        if document.symmetry().enabled
            && let Some(error) = &self.modeling.symmetry_error
        {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Symmetry cannot match this mesh",
            );
            ui.label(error);
            ui.label("Select still works. Check the mirror axes, radial settings and centre in Symmetry before editing.");
            if ui.button("Turn symmetry off").clicked() {
                commands.push(Command::Action(Action::Symmetry));
            }
        }
        design::checkbox(ui, &mut self.modeling.protect_mask, "Protect mask")
            .customize(Action::ModelProtectMask);
        ui.weak("Hidden faces are protected · topology edits replace stored levels");
        match self.modeling.tool {
            ModelTool::Delete => {}
            ModelTool::SurfaceSlide => {
                ui.weak("Drag individual faces, edges or vertices along the original surface. Stops at the next boundary; masked and hidden vertices stay fixed.");
                ui.add(
                    egui::DragValue::new(&mut self.modeling.amount)
                        .speed(0.01)
                        .prefix("Distance "),
                );
                ui.horizontal(|ui| {
                    for (axis, name) in [
                        (crate::viewport::ModelAxis::X, "X"),
                        (crate::viewport::ModelAxis::Y, "Y"),
                        (crate::viewport::ModelAxis::Z, "Z"),
                    ] {
                        ui.selectable_value(&mut self.modeling.axis, axis, name);
                    }
                });
                if ui
                    .add_enabled(self.modeling.count > 0, egui::Button::new("Apply amount"))
                    .clicked()
                {
                    commands.push(Command::ModelApply);
                }
            }
            ModelTool::EdgeChamfer
            | ModelTool::EdgeBevel
            | ModelTool::VertexChamfer
            | ModelTool::VertexBevel
            | ModelTool::FaceChamfer => {
                if self.bevel_controls(ui) {
                    commands.push(Command::ModelApply);
                }
            }
            ModelTool::PointSplit => {
                if self.point_split_controls(ui) {
                    commands.push(Command::ModelApply);
                }
            }
            ModelTool::EdgeExtrude
            | ModelTool::EdgeSplit
            | ModelTool::EdgeCollapse
            | ModelTool::PointExtrude
            | ModelTool::FacePoint
            | ModelTool::FaceSplit => {
                let tool = self.modeling.tool;
                if matches!(tool, ModelTool::EdgeSplit | ModelTool::EdgeCollapse) {
                    ui.label("Position from marked endpoint (0–1)");
                    let range = if tool == ModelTool::EdgeSplit {
                        0.001..=0.999
                    } else {
                        0.0..=1.0
                    };
                    search::setting_response(
                        ui.add(
                            egui::DragValue::new(&mut self.modeling.amount)
                                .range(range)
                                .speed(0.01),
                        ),
                        "Modeling amount",
                    );
                    if tool == ModelTool::EdgeCollapse {
                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(&mut self.modeling.amount, 0., "Marked end");
                            ui.selectable_value(&mut self.modeling.amount, 0.5, "Midpoint");
                            ui.selectable_value(&mut self.modeling.amount, 1., "Other end");
                        });
                    }
                    ui.weak("Drag along the edge to place the point · Esc cancels");
                } else if !tool.instant() {
                    ui.label(if tool == ModelTool::PointSplit {
                        "Radius"
                    } else {
                        "Distance"
                    });
                    search::setting_response(
                        ui.add(egui::DragValue::new(&mut self.modeling.amount).speed(0.01)),
                        "Modeling amount",
                    );
                    ui.weak(if tool == ModelTool::PointExtrude {
                        "Drag in the screen plane · release applies · Esc cancels"
                    } else {
                        "Drag up/down to adjust · release applies · Esc cancels"
                    });
                }
                if tool == ModelTool::EdgeExtrude {
                    design::checkbox(
                        ui,
                        &mut self.modeling.construction.other_side,
                        "Other adjacent side",
                    );
                    ui.weak("Border: extend outward · surface: extrude the highlighted side");
                }
                if tool == ModelTool::PointExtrude {
                    ui.weak("Creates one vertex joined to the original by an edge. The new endpoint follows your drag in the screen plane and stays selected.");
                    ui.weak("Drag first to set the direction for Apply amount and tap to repeat.");
                    ui.weak("Hold Ctrl during a drag for a detached vertex copy; release Ctrl to restore its connecting edge.");
                }
                if matches!(tool, ModelTool::PointExtrude | ModelTool::EdgeExtrude) {
                    search::setting_response(
                        ui.checkbox(
                            self.modeling.extrude_snap.get_or_insert(true),
                            "Snap and join",
                        ),
                        "Extrusion snap and join",
                    );
                    search::setting_response(
                        ui.add(
                            egui::DragValue::new(self.modeling.snap_radius.get_or_insert(12.))
                                .range(4. ..=40.)
                                .prefix("Snap radius ")
                                .suffix(" px"),
                        ),
                        "Snap pick radius",
                    );
                    ui.weak("Nearby visible targets on this mesh · hold Shift for free extrusion");
                    ui.weak(if tool == ModelTool::PointExtrude { "One point (plus symmetry copies) connects to an existing vertex. Gold marks the join before release." } else { "One open edge (plus symmetry copies) joins another open edge. Gold marks the join before release." });
                }
                if tool == ModelTool::PointSplit {
                    search::setting_response(
                        design::checkbox(
                            ui,
                            &mut self.modeling.construction.round,
                            "Circularize ring",
                        ),
                        "Circularize ring",
                    );
                    search::setting_response(
                        design::checkbox(ui, &mut self.modeling.construction.crease, "Crease ring"),
                        "Crease ring",
                    );
                    ui.weak("Closed point fan · radius below half the shortest edge");
                }
                if tool.instant() {
                    ui.weak(if tool == ModelTool::FacePoint {
                        "Tap: connect a center point to each corner"
                    } else {
                        "Tap: split into quads and connect neighboring edges"
                    });
                }
                if ui
                    .add_enabled(
                        self.modeling.count > 0
                            && (tool != ModelTool::PointExtrude
                                || self.modeling.transform_direction.len() > 1e-6),
                        egui::Button::new(if tool.instant() {
                            "Apply to selection"
                        } else {
                            "Apply amount"
                        }),
                    )
                    .customize(Action::ModelApply)
                    .clicked()
                {
                    commands.push(Command::ModelApply);
                }
            }

            ModelTool::Select => {
                ui.horizontal_wrapped(|ui| {
                    for (change, action) in [
                        (SelectionChange::Replace, Action::ModelReplace),
                        (SelectionChange::Add, Action::ModelAdd),
                        (SelectionChange::Remove, Action::ModelRemove),
                    ] {
                        if ui
                            .selectable_label(
                                self.modeling.erase_radius == 0. && self.modeling.change == change,
                                format!("{change:?}"),
                            )
                            .customize(action.clone())
                            .clicked()
                        {
                            commands.push(Command::Action(action));
                        }
                    }
                });
                let mut erase = self.modeling.erase_radius > 0.;
                if search::setting_response(
                    ui.checkbox(&mut erase, "Paint away selection"),
                    "Paint away selection",
                )
                .changed()
                {
                    self.modeling.erase_radius = if erase { 32. } else { 0. };
                }
                if erase {
                    search::setting_response(
                        ui.add(
                            egui::Slider::new(&mut self.modeling.erase_radius, 4. ..=160.)
                                .text("Eraser radius (px)"),
                        ),
                        "Eraser radius",
                    );
                    ui.weak("Brush visible selected components · Esc restores the selection");
                } else {
                    ui.weak("Tap or drag to select · Shift adds · Ctrl removes");
                }
                match (self.modeling.mode, self.modeling.span) {
                    (ComponentMode::Face, SelectionSpan::Loop) => {
                        ui.weak("Hover near a face edge to choose the loop direction.");
                    }
                    (ComponentMode::Edge, SelectionSpan::Loop) => {
                        ui.weak("Follows connected edges along the loop.");
                    }
                    (ComponentMode::Edge, SelectionSpan::Ring) => {
                        ui.weak("Follows opposite edges across a quad strip.");
                    }
                    _ => {}
                }
                if !erase {
                    ui.weak("Replace clears once per stroke · Esc cancels");
                }
            }
            ModelTool::CutCell => {
                ui.weak(if self.modeling.mode == ComponentMode::Face {
                    "Press a rectangular grid face to preview a one-cell recess."
                } else {
                    "Press a right-angle ridge edge to preview a one-cell notch."
                });
                ui.weak("Release to cut. Esc cancels the preview.");
                if ui
                    .add_enabled(
                        self.modeling.count > 0,
                        egui::Button::new("Apply to selection"),
                    )
                    .customize(Action::ModelApply)
                    .clicked()
                {
                    commands.push(Command::ModelApply);
                }
            }
            ModelTool::Extrude | ModelTool::Fuse => {
                if self.modeling.tool == ModelTool::Fuse {
                    ui.label("Snap range (fraction of local edge)");
                    search::setting_response(
                        ui.add(
                            egui::DragValue::new(&mut self.modeling.snap)
                                .range(0.0..=0.25)
                                .speed(0.005),
                        ),
                        "Snap range",
                    );
                    ui.weak("Each separate flat region snaps to its own compatible loop or cap.");
                }
                if self.modeling.tool == ModelTool::Extrude {
                    search::setting_response(
                        design::checkbox(ui, &mut self.modeling.individual, "Individual faces"),
                        "Individual faces",
                    );
                }
                search::setting_response(
                    ui.add(
                        egui::DragValue::new(&mut self.modeling.amount)
                            .prefix("Distance ")
                            .speed(0.01),
                    ),
                    "Modeling amount",
                );
                ui.horizontal_wrapped(|ui| {
                    if self.modeling.tool == ModelTool::Fuse
                        && ui
                            .add_enabled(self.modeling.count > 0, egui::Button::new("Preview"))
                            .clicked()
                    {
                        commands.push(Command::ModelPreviewFuse);
                    }
                    if ui
                        .add_enabled(self.modeling.count > 0, egui::Button::new("Apply amount"))
                        .customize(Action::ModelApply)
                        .clicked()
                    {
                        commands.push(Command::ModelApply);
                    }
                });

                design::help(
                    design::checkbox(ui, &mut self.modeling.join, "Join matching faces")
                        .customize(Action::ModelJoin),
                    "Snap to an aligned opposite cap with matching face layout and remove both caps to open a tunnel.",
                );
                ui.weak("Drag up: out · down: in · release: apply · Esc: cancel");
                if self.modeling.tool == ModelTool::Fuse {
                    ui.weak("Gold marks the target face or loop. Coral walls will be removed.");
                } else {
                    ui.weak("Connected regions share walls; Individual faces extrude separately.");
                    ui.weak("Hold Ctrl during a drag for detached face copies. Release Ctrl to restore attached extrusion.");
                }
            }
            ModelTool::Inset | ModelTool::Bevel => {
                search::setting_response(
                    design::checkbox(ui, &mut self.modeling.individual, "Individual faces"),
                    "Individual faces",
                );
                search::setting_response(
                    ui.add(
                        egui::DragValue::new(&mut self.modeling.amount)
                            .prefix("Width ")
                            .range(0.0..=1000.)
                            .speed(0.01),
                    ),
                    "Modeling amount",
                );
                if self.modeling.tool == ModelTool::Bevel
                    && matches!(
                        self.modeling.span,
                        SelectionSpan::Flat | SelectionSpan::Group
                    )
                {
                    self.bevel_segments_control(ui);
                }
                if ui
                    .add_enabled(self.modeling.count > 0, egui::Button::new("Apply amount"))
                    .customize(Action::ModelApply)
                    .clicked()
                {
                    commands.push(Command::ModelApply);
                }
                ui.weak("Drag up to increase width · release applies · Esc cancels");
                if self.modeling.tool == ModelTool::Bevel {
                    ui.weak("Flat region with one exterior edge at each corner.");
                }
            }
            ModelTool::Move | ModelTool::Rotate | ModelTool::Scale | ModelTool::Slide => {
                use crate::viewport::ModelInteraction;
                let direct = self.modeling.tool.is_transform();
                if direct {
                    ui.horizontal_wrapped(|ui| {
                        for mode in [
                            ModelInteraction::Free,
                            ModelInteraction::Gizmo,
                            ModelInteraction::Axis,
                        ] {
                            if ui
                                .selectable_label(self.modeling.interaction == mode, mode.label())
                                .customize(Action::ModelInteraction(mode))
                                .clicked()
                            {
                                commands.push(Command::Action(Action::ModelInteraction(mode)));
                            }
                        }
                    });
                }
                if !direct || self.modeling.interaction == ModelInteraction::Axis {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Axis");
                        for axis in crate::viewport::ModelAxis::ALL {
                            if axis == crate::viewport::ModelAxis::Uniform
                                && self.modeling.tool != ModelTool::Scale
                            {
                                continue;
                            }
                            ui.selectable_value(&mut self.modeling.axis, axis, format!("{axis:?}"));
                        }
                    });
                } else {
                    ui.weak(if self.modeling.interaction==ModelInteraction::Gizmo {"Select components, then drag the handles. Center: free / uniform. Colors: X, Y, Z."}else{match self.modeling.tool {ModelTool::Move=>"Drag a target freely in the view plane",ModelTool::Rotate=>"Drag a target to rotate with the trackball",_=>"Drag up/down to scale uniformly"}});
                    let d = self.modeling.transform_direction;
                    if d.len() < 1e-6 {
                        ui.weak(if self.modeling.tool == ModelTool::Scale {
                            "Uniform scale"
                        } else {
                            "Drag first to set the direction for Apply amount"
                        });
                    } else {
                        ui.weak(format!(
                            "Direction: X {:.2}, Y {:.2}, Z {:.2}",
                            d.0, d.1, d.2
                        ));
                    }
                }
                ui.label(match self.modeling.tool {
                    ModelTool::Slide => "Slide fraction (-1 to 1)",
                    ModelTool::Rotate => "Angle (degrees)",
                    ModelTool::Scale => "Scale factor",
                    _ => "Distance",
                });
                search::setting_response(
                    ui.add(egui::DragValue::new(&mut self.modeling.amount).speed(0.01)),
                    "Modeling amount",
                );
                let has_direction = !direct
                    || self.modeling.interaction == ModelInteraction::Axis
                    || self.modeling.tool == ModelTool::Scale
                    || self.modeling.transform_direction.len() > 1e-6;
                if ui
                    .add_enabled(
                        self.modeling.count > 0 && has_direction,
                        egui::Button::new("Apply amount"),
                    )
                    .customize(Action::ModelApply)
                    .clicked()
                {
                    commands.push(Command::ModelApply);
                }
                ui.weak("Release applies · Esc cancels · one Undo per drag");
                if self.modeling.tool == ModelTool::Slide {
                    ui.weak("Drag along surface rails · Axis sets the numeric slide direction");
                }
                if self.modeling.tool == ModelTool::Move {
                    ui.strong("Snap and weld on move");
                    ui.horizontal_wrapped(|ui| {
                        use crate::viewport::MoveSnap;
                        for (option, label) in [
                            (MoveSnap::Off, "Off"),
                            (MoveSnap::Vertex, "Vertex"),
                            (MoveSnap::Edge, "Edge"),
                        ] {
                            search::setting_response(
                                ui.selectable_value(&mut self.modeling.move_snap, option, label),
                                "Move snap and weld",
                            );
                        }
                    });
                    ui.weak("Vertex welds a moved point to an existing vertex. Edge also welds both ends of a parallel edge when both are nearby.");
                    search::setting_response(
                        ui.add(
                            egui::DragValue::new(self.modeling.snap_radius.get_or_insert(12.))
                                .range(4. ..=40.)
                                .prefix("Snap radius ")
                                .suffix(" px"),
                        ),
                        "Snap pick radius",
                    );
                    ui.weak("Free drag or gizmo center · hold Shift for free movement · release to weld");
                }

                ui.weak("Rotate and scale around the selection center");
                if direct {
                    ui.separator();
                    ui.strong("Align without welding");
                    ui.horizontal_wrapped(|ui| {
                        use crate::viewport::SnapTarget;
                        for target in [SnapTarget::Vertex, SnapTarget::Edge, SnapTarget::Surface] {
                            search::setting_response(
                                ui.selectable_value(
                                    &mut self.modeling.snap_target,
                                    target,
                                    target.label(),
                                ),
                                "Snap target type",
                            );
                        }
                    });
                    search::setting_response(
                        ui.add(
                            egui::DragValue::new(self.modeling.snap_radius.get_or_insert(12.))
                                .range(4. ..=40.)
                                .prefix("Pick radius ")
                                .suffix(" px"),
                        ),
                        "Snap pick radius",
                    );
                    ui.weak("Active mesh · visible, unselected targets").on_hover_text("Selected vertices, symmetry copies and incident target geometry are excluded. Free/Gizmo snap freely; Axis / amount uses the chosen axis.");
                    ui.weak(match self.modeling.tool {
                            ModelTool::Move => "Move aligns the source to the target; an axis projects that movement.",
                            ModelTool::Rotate => "Rotate aims the source direction at the target around the selection center; distance stays fixed.",
                            _ => "Scale matches distance from the selection center, or its chosen axis component; direction stays fixed.",
                        });
                    if let Some(hint) = &self.modeling.snap_hint {
                        ui.label(hint);
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add_enabled(
                                self.modeling.count > 0,
                                egui::Button::new("Pick snap source"),
                            )
                            .customize(Action::ModelSnap)
                            .clicked()
                        {
                            commands.push(Command::ModelSnap);
                        }
                        if self.modeling.snap_active {
                            if ui.button("Pick another target").clicked() {
                                commands.push(Command::ModelSnapRetarget);
                            }
                            if ui
                                .add_enabled(
                                    self.modeling.snap_ready,
                                    egui::Button::new("Apply snap"),
                                )
                                .clicked()
                            {
                                commands.push(Command::ModelSnapApply);
                            }
                            if ui.button("Cancel snap").clicked() {
                                commands.push(Command::ModelSnapCancel);
                            }
                        }
                    });
                    ui.weak("Click source, then target. Orbit/zoom to inspect.").on_hover_text("Apply amount uses the numeric value without snapping and discards this preview.");
                }
            }
            ModelTool::LoopCut => {
                use crate::sculpt::LoopCut;
                self.modeling.loop_count = self.modeling.loop_count.clamp(1, LoopCut::MAX_COUNT);
                ui.horizontal(|ui| {
                    ui.label("Loops");
                    if design::help(
                        ui.button("−").customize(Action::ModelFewerLoops),
                        "Fewer loop cuts (minimum 1)",
                    )
                    .clicked()
                    {
                        self.modeling.loop_count =
                            self.modeling.loop_count.saturating_sub(1).max(1);
                    }
                    ui.add(
                        egui::DragValue::new(&mut self.modeling.loop_count)
                            .range(1..=LoopCut::MAX_COUNT),
                    );
                    if design::help(
                        ui.button("+").customize(Action::ModelMoreLoops),
                        "More loop cuts (maximum 16)",
                    )
                    .clicked()
                    {
                        self.modeling.loop_count =
                            (self.modeling.loop_count + 1).min(LoopCut::MAX_COUNT);
                    }
                });
                ui.weak("Tap: centered · drag along edge: slide · release: apply");
                ui.weak("Wheel while dragging: loop count · Esc: cancel");
                ui.weak("Follows visible quads. Edits replace stored levels.");
            }
        }
        if ((self.modeling.tool.is_edit() && !self.modeling.tool.instant())
            || self.modeling.tool == ModelTool::Extrude)
            && !(self.modeling.tool.is_transform()
                && self.modeling.interaction == crate::viewport::ModelInteraction::Gizmo)
        {
            design::checkbox(ui, &mut self.modeling.tap_repeat,"Tap to repeat").customize(Action::ModelTapRepeat)
                    .on_hover_text("Tap a target to apply the displayed amount and options. Drag to set a new amount. Select and centered loop cuts are unchanged.");
            if self.modeling.tap_repeat {
                ui.weak(format!(
                    "Tap applies {:.3} · drag sets a new amount",
                    self.modeling.amount
                ));
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Clear selection")
                .customize(Action::ModelClearSelection)
                .clicked()
            {
                commands.push(Command::ClearModelSelection);
            }
            if self.modeling.mode == ComponentMode::Edge
                && ui
                    .button("Select loop")
                    .customize(Action::ModelSelectLoop)
                    .clicked()
            {
                commands.push(Command::ModelSelectLoop);
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (label, action) in [
                ("Grow selection", Action::ModelGrowSelection),
                ("Shrink selection", Action::ModelShrinkSelection),
            ] {
                if ui
                    .add_enabled(self.modeling.count > 0, egui::Button::new(label))
                    .customize(action.clone())
                    .clicked()
                {
                    commands.push(Command::Action(action));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (label, action, enabled) in [
                (
                    "Undo selection change",
                    Action::ModelUndoSelection,
                    self.modeling.selection_undo,
                ),
                (
                    "Redo selection change",
                    Action::ModelRedoSelection,
                    self.modeling.selection_redo,
                ),
            ] {
                if ui
                    .add_enabled(enabled, egui::Button::new(label))
                    .customize(action.clone())
                    .clicked()
                {
                    commands.push(Command::Action(action));
                }
            }
        });
        ui.weak("One polygon step. Hidden/protected geometry blocks growth. Selection Undo/Redo is separate from geometry history.");
        ui.horizontal_wrapped(|ui| {
            for action in [Action::SplitSelectedFaces, Action::SplitDisconnectedIslands] {
                if ui
                    .add_enabled(
                        self.action_enabled(&action, document),
                        egui::Button::new(action.label()),
                    )
                    .on_hover_text(action.help().unwrap())
                    .on_disabled_hover_text(
                        self.action_unavailable(&action, document)
                            .unwrap_or("Choose an editable surface"),
                    )
                    .customize(action.clone())
                    .clicked()
                {
                    commands.push(Command::Action(action));
                }
            }
        });
        ui.weak("Split uses the current surface; subdivision and adaptive/voxel settings reset. Undo restores all parts.");
        if self.modeling.mode == ComponentMode::Face
            && ui
                .button("Select region boundary")
                .customize(Action::ModelBoundary)
                .clicked()
        {
            commands.push(Command::ModelBoundary);
        }
        if self.modeling.mode == ComponentMode::Edge {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(self.modeling.count > 0, egui::Button::new("Mark UV seams"))
                    .clicked()
                {
                    commands.push(Command::UvSeams(true));
                }
                if ui
                    .add_enabled(self.modeling.count > 0, egui::Button::new("Clear UV seams"))
                    .clicked()
                {
                    commands.push(Command::UvSeams(false));
                }
            });
            ui.weak("Select edges, then mark or clear seams. Unwrap uses the marked seams.");
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.modeling.can_repeat && self.modeling.count > 0,
                    egui::Button::new("Repeat last edit"),
                )
                .customize(Action::ModelRepeat)
                .on_hover_text(&self.modeling.repeat_label)
                .clicked()
            {
                commands.push(Command::ModelRepeat);
            }
            design::checkbox(ui, &mut self.modeling.smooth_preview, "Smooth preview")
                .customize(Action::ModelSmoothPreview);
        });
        if self.modeling.smooth_preview {
            ui.weak("One smooth level · cage stays editable · preview pauses during a drag");
        }
        self.modeling_presets(ui, commands);
        egui::CollapsingHeader::new("Selection to vertex group").open(search::requested_section(ui.ctx(), "model_groups")).show(ui, |ui| {
                ui.text_edit_singleline(&mut self.modeling.group_name);
                let enabled = self.modeling.count > 0 && document.mesh_settings().levels() == 1;
                let help = "Use a single subdivision level and select components first. Face colors use only selected faces; vertex weights include their corners. Edges include both ends. Creating or adding replaces previous membership; partial regions remain, fully replaced groups disappear. Masks stay unchanged.";
                if design::help(
                    ui.add_enabled(enabled, egui::Button::new("Create group from selection")).customize(Action::ModelCreateGroup), help,
                ).clicked() {
                    commands.push(Command::GroupModelSelection(None, false));
                }
                let groups = &document.mesh().vertex_groups;
                self.modeling.group = self.modeling.group.min(groups.len().saturating_sub(1));
                if let Some(first) = groups.iter().position(|g| g.purpose.is_selection()) {
                    if !groups[self.modeling.group].purpose.is_selection() { self.modeling.group = first; }
                    egui::ComboBox::from_id_salt("modeling_group")
                        .selected_text(&groups[self.modeling.group].name)
                        .show_ui(ui, |ui| {
                            for (i, group) in groups.iter().enumerate().filter(|(_, g)| g.purpose.is_selection()) {
                                ui.selectable_value(&mut self.modeling.group, i, &group.name);
                            }
                        });
                    ui.horizontal_wrapped(|ui| {
                        for (label, remove, action) in [
                            ("Add to group", false, Action::ModelAddToGroup),
                            ("Remove from group", true, Action::ModelRemoveFromGroup),
                        ] {
                            if design::help(ui.add_enabled(enabled, egui::Button::new(label)).customize(action), help).clicked() {
                                commands.push(Command::GroupModelSelection(Some(self.modeling.group), remove));
                            }
                        }
                    });
                }
            });
    }

    pub(super) fn modeling_controls(
        &mut self,
        ui: &mut egui::Ui,
        document: &Document,
        commands: &mut Vec<Command>,
    ) {
        search::mark_destination(ui, "section:modeling_options");
        if self.modeling.active && self.modeling.mode == ComponentMode::Face {
            if let Some(set) = document.instances().sets.iter().find(|s| document.surface_guide_editing(s.id)) {
                if ui.add_enabled(self.modeling.count > 0, egui::Button::new("Use selected faces for surface array")).on_hover_text("Keeps edits on retained faces while connectivity is unchanged. Removed faces lose their copy edits; topology changes reset all copy edits. Preview before applying.").clicked() {
                    commands.push(Command::Instances(crate::command::InstanceCommand::SurfaceSelection(set.id)));
                }
                if ui.button("Show surface array result").clicked() { commands.push(Command::Instances(crate::command::InstanceCommand::SurfaceGuide(set.id, false))); }
            } else if ui.add_enabled(self.modeling.count > 0, egui::Button::new("Surface array from selected faces…")).customize(Action::CreateSurfaceArray).on_hover_text("Place one captured Object brush part per polygon. Capture a part in Brush settings first.").clicked() {
                commands.push(Command::Instances(crate::command::InstanceCommand::NewSurfaceArray));
            }
        }
        let unavailable = self.edge_unavailable(&Action::SelectEdges, document);
        let mut active = self.modeling.active;
        let response = ui
            .horizontal(|ui| {
                let response = ui
                    .add_enabled(
                        unavailable.is_none(),
                        egui::Checkbox::new(&mut active, "Modeling"),
                    )
                    .customize(Action::PenModeling);
                if ui
                    .small_button("New mesh…")
                    .customize(Action::ModelNewMesh)
                    .on_hover_text(
                        "Start with a low-resolution cube. Review the replacement warning first.",
                    )
                    .clicked()
                {
                    commands.push(Command::RequestModelingPrimitive);
                }
                response
            })
            .inner;
        if self
            .edge_help(response, &Action::PenModeling, document)
            .changed()
        {
            commands.push(Command::Action(Action::PenModeling));
        }
        if let Some(reason) = unavailable {
            ui.label(reason);
        }
        if search::requested_section(ui.ctx(), "modeling_options").is_some() {
            ui.label("Tool options follow the active Modeling tool and selection target. Enable Modeling, then choose the relevant tool to adjust its settings.");
        }
        if !self.modeling.active {
            ui.weak("Enable Modeling to select faces, edges or vertices. Your component selection is kept when switching tools.");
            return;
        }
        ui.label(format!(
            "{:?} · {} · {} selected",
            self.modeling.mode,
            self.modeling.tool.label(),
            self.modeling.count
        ));
        if self.modeling.preview_amount.is_some() {
            ui.label("Preview: release to apply, or Escape to cancel and keep the selection.");
        } else if self.modeling.count == 0 {
            ui.label(match self.modeling.mode {
                ComponentMode::Face => "Select a face in the viewport to use component actions.",
                ComponentMode::Edge => "Select an edge in the viewport to use component actions.",
                ComponentMode::Vertex => {
                    "Select a vertex in the viewport to use component actions."
                }
            });
        }
        if document.symmetry().enabled {
            ui.weak("Symmetry on · matching geometry and topology required");
        }
        ui.add_enabled_ui(unavailable.is_none(), |ui| {
            let entries = self.viewport_menu_entries(
                crate::ui::context_menu::ViewportMenuContext::Modeling,
                document,
            );
            let flat_angle = (self.modeling.tool != ModelTool::CutCell
                && self.modeling.span == SelectionSpan::Flat)
                .then_some(&mut self.modeling.flat_angle);
            if let Some((_, action)) = crate::ui::context_menu::modeling_choices(
                ui,
                &entries,
                None,
                false,
                flat_angle,
                (self.modeling.mode == ComponentMode::Edge)
                    .then_some(&mut self.modeling.bridge_segments),
            ) {
                commands.push(Command::Action(action));
            }
            self.modeling_general_options(ui);
            self.modeling_tool_options(ui, document, commands);
        });
    }
}
