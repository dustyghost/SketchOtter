//! Panel generation uses the shared cancellable surface preview and explicit commit controls.
use super::*;
use crate::sculpt::{PanelOptions, PanelProfile, Symmetry};
#[derive(Clone, PartialEq)]
pub(crate) struct PanelKey {
    pub cut: bool,
    pub mask_threshold: Option<f32>,
    pub groups: Vec<usize>,
    pub options: PanelOptions,
    pub protect: bool,
    pub symmetry: Symmetry,
}
impl PanelKey {
    pub fn same_selection(&self, other: &Self) -> bool {
        self.cut == other.cut
            && self.groups == other.groups
            && self.mask_threshold == other.mask_threshold
            && self.protect == other.protect
            && self.symmetry == other.symmetry
    }
}
pub(crate) struct PanelControls {
    pub cut: bool,
    pub use_mask: bool,
    pub groups: Vec<usize>,
    pub options: PanelOptions,
    pub protect: bool,
    pub outline: bool,
}
impl Default for PanelControls {
    fn default() -> Self {
        Self {
            cut: false,
            use_mask: false,
            groups: Vec::new(),
            options: Default::default(),
            protect: true,
            outline: true,
        }
    }
}
impl PanelControls {
    pub fn key(&self, document: &Document) -> PanelKey {
        PanelKey {
            cut: self.cut,
            mask_threshold: self.use_mask.then(|| {
                document
                    .model(document.active_model_index())
                    .unwrap()
                    .metadata
                    .mask_threshold
            }),
            groups: self.groups.clone(),
            options: if self.cut {
                PanelOptions {
                    border: crate::sculpt::PanelBorder {
                        gap: 0.,
                        ..self.options.border
                    },
                    ..Default::default()
                }
            } else {
                self.options
            },
            protect: self.protect,
            symmetry: document.symmetry().clone(),
        }
    }
}
impl UiState {
    pub(super) fn panels_window(
        &mut self,
        ctx: &egui::Context,
        document: &Document,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        // File decisions own input and must stay above this foreground preview.
        if self.pending.is_some() || self.file_picker.is_some() {
            return commands;
        }
        let mut open = true;
        let mut round_outline = false;
        // Keep Apply/Cancel above the scroll area on short screens.
        egui::Window::new("Generate panels")
            .order(egui::Order::Foreground).open(&mut open).default_width(370.).default_height((ctx.content_rect().height()-60.).clamp(160.,850.)).resizable(true)
            .show(ctx, |ui| {
                let state=&mut self.projection;
                let old=state.panels.key(document);
                let previous_cut = state.panels.cut;
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut state.panels.cut, false, "Extract panels");
                    ui.selectable_value(&mut state.panels.cut, true, "Panel cut");
                });
                if state.panels.cut != previous_cut && state.panels.cut {
                    self.wire = true;
                    self.show_polygroups = true;
                    state.panels.outline = false;
                }
                ui.label(if state.panels.cut { "Cut outline/support loops into the active surface. Inspect in Wireframe. Apply is one Undo step." } else { "One editable submodel per group. Source kept hidden below the panels." });
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!state.busy && (state.panels.use_mask || !state.panels.groups.is_empty()), egui::Button::new(if state.panels.cut { "Preview cut" } else { "Preview panels" })).clicked() {
                        commands.push(Command::PreviewProjection);
                    }
                    if ui.add_enabled(!state.busy && state.report.as_ref().is_some_and(SurfaceReport::can_apply), egui::Button::new(if state.panels.cut { "Apply cut" } else { "Apply panels" })).clicked() {
                        commands.push(Command::ApplyProjection);
                    }
                    if ui.button("Cancel").clicked() { commands.push(Command::CancelProjection); }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.wire, "Wireframe");
                    ui.checkbox(&mut state.panels.outline, "Source outlines").on_hover_text("Gold lines show the original unsmoothed mask/group boundary, even when the panel is rounded. Turn off to inspect the panel silhouette.");
                });
                if state.busy { ui.horizontal(|ui| { ui.spinner(); ui.label("Building and checking panels… Cancel remains available."); }); }
                if let Some(error)=&state.error { ui.colored_label(Color32::from_rgb(240,130,110), error); }
                if let Some(report)=&state.report { ui.label(report.summary()); }
                if !state.busy && state.report.is_none() { ui.weak("Preview needs updating. The previous view stays visible; Apply is disabled."); }
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-230.).max(120.)).show(ui, |ui| {
                    ui.add_enabled_ui(!state.busy, |ui| {
                        let panel=&mut state.panels;
                        ui.checkbox(&mut panel.use_mask, "Use mask as panel selection").on_hover_text(if panel.cut { "Use complete masked faces as the cut region. The group and surface topology are committed together on Apply." } else { "Create a vertex group from complete masked faces at the selection threshold, then generate panels in the same Apply step. The source mask stays intact." });
                        if panel.use_mask {
                            ui.label(if panel.cut { "Masked faces become a Mask cut group on the active surface. Islands share that group." } else { "Masked faces become a new Mask panel group on the output. Disconnected islands share that group." });
                            ui.weak(if panel.cut { "The mask selects the cut region here. Hidden faces remain protected." } else { "The mask selects the panel here; it does not protect those faces from generation. Hidden faces remain protected." });
                        } else {
                        ui.label("Polygon groups");
                        let available: std::collections::BTreeSet<_> = document.mesh().polygroups.iter().copied().filter(|&g| g>0).collect();
                        ui.horizontal(|ui| {
                            if ui.button("All groups").clicked() { panel.groups=available.iter().copied().collect(); }
                            if ui.button("Clear groups").clicked() { panel.groups.clear(); }
                        });
                        for group in available {
                            let mut selected=panel.groups.contains(&group);
                            if ui.checkbox(&mut selected, &document.mesh().vertex_groups[group-1].name).changed() {
                                if selected { panel.groups.push(group); } else { panel.groups.retain(|&g| g!=group); }
                                panel.groups.sort_unstable();
                            }
                        }
                        ui.checkbox(&mut panel.protect, "Protect mask").on_hover_text("Reject selected regions touching masked or protected vertices. Hidden faces are always protected.");
                        }
                        ui.weak(if document.symmetry().enabled { "Symmetry includes matching whole groups. Mismatched groups are rejected." } else { "Symmetry is off. Only the checked groups are generated." });
                        ui.separator();
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Fit border to mesh").on_hover_text(if panel.cut { "Reduce support width to fit source edge spacing. Preview again to validate." } else { "Reduce gap, border width and bevel together to fit the source edge spacing. Thickness and offset stay unchanged. Preview again to validate." }).clicked() {
                                if panel.cut {
                                let cut = PanelOptions { bevel: 0., border: crate::sculpt::PanelBorder { gap: 0., ..panel.options.border }, ..Default::default() };
                                panel.options.border.width = document.mesh().fit_panel_border(cut).border.width;
                            } else { panel.options = document.mesh().fit_panel_border(panel.options); }
                            }
                            if ui.button("Round outline").on_hover_text("Round stair-step boundaries: set smoothing to 1 and preserve turns to 120°, rebuild the preview, and hide the original gold outline to show the result. Thickness, gap and bevel stay unchanged.").clicked() {
                                panel.options.border.smoothing = 1.;
                                panel.options.border.corner_angle = 120.;
                                panel.outline = false;
                                round_outline = true;
                            }
                        });
                        let cutting = panel.cut;
                        let o=&mut panel.options;
                        if cutting { outline_controls(ui,o,true); }
                        ui.add_enabled_ui(!cutting, |ui| {
                        ui.add(egui::DragValue::new(&mut o.thickness).range(0.00001..=100.).speed(0.001).prefix("Thickness "));
                        ui.add(egui::DragValue::new(&mut o.offset).range(-100. ..=100.).speed(0.001).prefix("Normal offset "));
                        ui.horizontal(|ui| {
                            if ui.button("Inward").clicked() { o.offset=-o.thickness*0.5; }
                            if ui.button("Centred").clicked() { o.offset=0.; }
                            if ui.button("Outward").clicked() { o.offset=o.thickness*0.5; }
                        });
                        ui.weak("Offset is the centre of the shell: positive along the source normal. Thickness changes independently.");
                        ui.add(egui::DragValue::new(&mut o.border.gap).range(0. ..=100.).speed(0.001).prefix("Gap ")).on_hover_text("Each outline retreats by half this distance. Adjacent panels share the total gap.");
                        ui.add(egui::DragValue::new(&mut o.bevel).range(0. ..=100.).speed(0.001).prefix("Bevel width ")).on_hover_text("Equal lateral width and depth; must be smaller than half the thickness. Zero gives a straight wall.");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut o.profile, PanelProfile::Flat, "Flat bevel");
                            ui.selectable_value(&mut o.profile, PanelProfile::Rounded, "Rounded bevel");
                        });
                        ui.add(egui::Slider::new(&mut o.segments, 1..=8).text("Bevel segments"));
                        ui.add(egui::Slider::new(&mut o.wall_loops, 0..=8).text("Wall support loops"));
                        }).response.on_disabled_hover_text("Shell controls apply to Extract panels. Panel cut edits only surface topology.");
                        if !cutting { outline_controls(ui,o,false); }
                        ui.weak(if cutting { format!("Uses current subdivision level {}. Apply resets this subdivision stack; Undo restores it.", document.mesh_settings().level()) } else { format!("Uses current subdivision level {}. Output has no subdivision stack; the source keeps its complete stack and paths.", document.mesh_settings().level()) });
                    });
                });
                if old!=state.panels.key(document) || round_outline {
                    commands.retain(|c| !matches!(c,Command::ApplyProjection|Command::PreviewProjection));
                    commands.push(if round_outline { Command::PreviewProjection } else { Command::ClearProjectionPreview });
                }
            });
        if !open {
            commands.push(Command::CancelProjection);
        }
        commands
    }
}

fn outline_controls(ui: &mut egui::Ui, o: &mut PanelOptions, cutting: bool) {
    ui.separator();
    ui.label(if cutting {
        "Surface cut loops"
    } else {
        "Surface border reconstruction"
    });
    ui.add(
        egui::DragValue::new(&mut o.border.width)
            .range(0.00001..=100.)
            .speed(0.001)
            .prefix(if cutting {
                "Support width "
            } else {
                "Border width "
            }),
    );
    ui.add(
        egui::Slider::new(&mut o.border.loops, 1..=8).text(if cutting {
            "Added loops"
        } else {
            "Surface border loops"
        }),
    );
    ui.add(egui::Slider::new(&mut o.border.spacing, 0.25..=4.).text("Loop spacing"))
        .on_hover_text(if cutting {
            "1 is uniform. Higher values concentrate added loops near the cut outline."
        } else {
            "1 is uniform. Higher values place support closer to the bevel; dimensions stay fixed."
        });
    ui.add(egui::Slider::new(&mut o.border.smoothing, 0. ..=1.).text("Outline smoothing"));
    ui.add(egui::Slider::new(&mut o.border.corner_angle, 15. ..=150.).text("Preserve turns ≥"))
                            .on_hover_text("Turns at or above this angle and crease endpoints stay pinned. Raise it deliberately to smooth staircase boundaries; inspect the gold source outline.");
    ui.weak(if cutting { "Welded surface loops. Inspect the cut in Wireframe or polygon-group colours. New support-strip UVs are unmapped." } else { "Only boundary vertices and their incident faces are reconstructed. Interior connectivity stays intact. New border and wall faces inherit the group and outline attributes; their UVs are unmapped." });
}
