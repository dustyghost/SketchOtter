use super::*;
impl App {
    pub(super) fn retopology_diagnostic_settings(&self) -> serde_json::Value {
        let options = self.ui.projection.layout;
        serde_json::json!({"backend":options.method.diagnostic_name(),"input_triangle_limit":options.input_triangle_limit,"target":options.target,"paths":self.ui.projection.flow_paths,"path_strength":options.flow,"path_reprojection_distance":self.paths.reproject_distance,"density_strength":options.density,"shape_adaptation":options.adaptive,"mirror_path_x":options.mirror_path_x,"preserve_sharp":options.preserve_sharp,"sharp_angle_degrees":options.sharp_angle_degrees,"symmetry_axis":options.symmetry_axis,"symmetry_center":options.symmetry_center,"symmetry_centreline":options.symmetry_centreline,"symmetry_source":options.symmetry_source,"target_model_id":self.document.active_model_id(),"source_vertices":self.document.mesh().vertices.len(),"source_triangles":self.document.mesh().triangles.len()})
    }
    pub(super) fn diagnostic_warning(&mut self, category: &str) {
        let warnings = &mut self.diagnostic_environment.warnings;
        if warnings.len() == 32 {
            warnings.remove(0);
        }
        warnings.push(category.into());
    }

    pub(super) fn capture_diagnostic_context(&mut self, ctx: &egui::Context) {
        let camera = &self.camera;
        let modifiers = ctx.input(|i| i.modifiers);
        self.document.set_diagnostic_context(serde_json::json!({
            "selected_brush": self.ui.brush_index_for_context(&self.document, ctx),
            "camera": { "yaw": camera.yaw, "pitch": camera.pitch, "roll": camera.roll, "zoom": camera.zoom,
                "pan": [camera.pan.x, camera.pan.y], "perspective": camera.perspective,
                "field_of_view": camera.field_of_view, "perspective_radius": camera.perspective_radius },
            "pressure_at_boundary": self.pressure,
            "modifiers": { "shift": modifiers.shift, "ctrl": modifiers.ctrl, "alt": modifiers.alt, "command": modifiers.command },
            "wireframe": self.ui.wire, "flat_shading": self.ui.flat_shading, "show_polygroups": self.ui.show_polygroups,
            "show_backfaces": self.ui.show_backfaces, "backface_opacity": self.ui.backface_opacity,
            "transform_mode": format!("{:?}", self.ui.transform_mode),
            "transform_operation": format!("{:?}", self.ui.transform_operation),
            "respect_transform_mask": self.ui.respect_transform_mask,
            "modeling_active": self.ui.modeling.active,
            "reverse_hold": format!("{:?}", self.ui.preferences.reverse),
            "smooth_hold": format!("{:?}", self.ui.preferences.smooth),
            "guided_retopology": self.retopology_diagnostic_settings(),
            "last_guided_retopology_attempt": self.projection.last_retopology_settings,
        }));
    }
    pub(super) fn show_diagnostics(&mut self, ctx: &egui::Context) {
        if !self.ui.diagnostics_open {
            return;
        }
        let mut open = true;
        let mut export = false;
        let before = self.ui.preferences.history_retention;
        egui::Window::new("Export diagnostic bundle").open(&mut open).default_width(520.).max_height((ctx.content_rect().height() - 80.).max(160.)).vscroll(true).show(ctx, |ui| {
            ui.label("A local folder containing a readable summary, versioned manifest, and current/undo/redo projects.");
            let (undo, redo) = self.document.history_counts();
            ui.label(format!("Includes {undo} undo stages, the current project, and {redo} redo stages."));
            ui.label("Includes model names, brush alphas, current embedded reference images, edit settings, view/input state and app/GPU information. Review these before sharing.");
            ui.label("No uploads, credentials or project/device paths. Earlier reference images, evicted history and full input samples are unavailable.");
            ui.separator();
            ui.label("History retention (saved automatically; lower limits can evict stages now)");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.ui.preferences.history_retention.edits).range(1..=256).suffix(" edits"));
                ui.add(egui::DragValue::new(&mut self.ui.preferences.history_retention.memory_mib).range(16..=1024).suffix(" MiB"));
            });
            ui.label("Memory estimate excludes the live project and allocator overhead. Export limit: 512 MiB total, 128 MiB per file.");
            ui.separator();
            ui.label("New destination folder (existing folders are never replaced)");
            ui.text_edit_singleline(&mut self.ui.diagnostic_path);
            let ready = self.mesh_job.is_none() && !self.document.edit_in_progress();
            if !ready { ui.label("Finish the current edit or mesh operation before exporting."); }
            export = ui.add_enabled(ready, egui::Button::new("Export locally")).clicked();
            ui.label(&self.ui.status);
        });
        self.ui.diagnostics_open = open;
        if before != self.ui.preferences.history_retention {
            let _ = self
                .document
                .set_history_retention(self.ui.preferences.history_retention);
        }
        if export {
            self.capture_diagnostic_context(ctx);
            let mut environment = self.diagnostic_environment.clone();
            // Tablet status can contain device paths and OS error text. Export categories only.
            let status = self.tablet.status();
            environment.input = Some(
                if status.starts_with("Tablet:") {
                    "Pen tablet detected"
                } else {
                    "Mouse/native touch; tablet pressure unavailable or disconnected"
                }
                .into(),
            );
            if self.recovery_error.is_some() {
                environment.warnings.push("Recovery reported an error; details omitted because they may contain personal paths".into());
            }
            match self.document.export_diagnostics(std::path::Path::new(self.ui.diagnostic_path.trim()), environment) {
                Ok(()) => self.ui.status = "Diagnostic bundle exported. Open README.txt first; use Open to inspect a stage.".into(),
                Err(error) => self.ui.status = format!("Diagnostic export failed: {error}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_keep_rejected_retopology_settings_and_current_guide_selection() {
        let mut app = App::default();
        let ctx = egui::Context::default();
        app.dispatch(Command::OpenRetopology, &ctx);
        app.ui.projection.layout.method = crate::sculpt::retopology::Method::IntegerGrid;
        app.ui.projection.layout.target = 3583;
        app.ui.projection.flow_paths = vec![2, 1];
        app.ui.projection.layout.preserve_sharp = true;
        app.ui.projection.layout.symmetry_axis = Some(crate::sculpt::Axis::X);
        let history = app.document.history_counts();
        app.dispatch(Command::PreviewProjection, &ctx);
        assert!(app.ui.status.contains("Layout symmetry"));
        assert!(app.projection.draft.is_none());
        assert_eq!(app.document.history_counts(), history);
        app.ui.projection.flow_paths = vec![1];
        app.ui.projection.layout.target = 1000;
        app.capture_diagnostic_context(&ctx);
        let manifest = app.document.diagnostic_manifest(Default::default());
        let summary = manifest.summary();
        assert!(summary.contains("target 3583, selected path IDs [2,1]"));
        assert!(summary.contains("target 1000, selected path IDs [1]"));
        let encoded = serde_json::to_vec(&manifest).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        let view = &saved["current_settings"]["view_input"];
        let attempted = &view["last_guided_retopology_attempt"];
        assert_eq!(attempted["backend"], "native_miq_integer_grid_experimental");
        assert_eq!(attempted["target"], 3583);
        assert_eq!(attempted["paths"], serde_json::json!([2, 1]));
        assert_eq!(attempted["preserve_sharp"], true);
        assert_eq!(view["guided_retopology"]["target"], 1000);
        assert_eq!(view["guided_retopology"]["paths"], serde_json::json!([1]));
    }
    #[test]
    fn diagnostics_latch_view_at_edit_and_keep_safe_bounded_warnings() {
        let mut app = App::default();
        let ctx = egui::Context::default();
        app.camera.zoom = 2.;
        app.dispatch(Command::InvertMask, &ctx);
        app.camera.zoom = 3.;
        app.capture_diagnostic_context(&ctx);
        let manifest = app.document.diagnostic_manifest(Default::default());
        assert_eq!(
            manifest.current_settings.view_input.as_ref().unwrap()["camera"]["zoom"],
            3.
        );
        assert_eq!(
            manifest.transitions[0]
                .settings_at_edit_start
                .as_ref()
                .unwrap()
                .view_input
                .as_ref()
                .unwrap()["camera"]["zoom"],
            2.
        );
        for i in 0..40 {
            app.diagnostic_warning(&format!("Warning category {i}"));
        }
        assert_eq!(app.diagnostic_environment.warnings.len(), 32);
        assert_eq!(app.diagnostic_environment.warnings[0], "Warning category 8");
    }

    #[test]
    fn background_edit_keeps_launch_settings_when_the_live_view_changes() {
        let mut app = App::default();
        let ctx = egui::Context::default();
        app.document
            .replace_project(
                serde_json::from_str(include_str!("../../tests/fixtures/v1-project.json")).unwrap(),
                "source.clay",
            )
            .unwrap();
        app.camera.zoom = 2.;
        app.capture_diagnostic_context(&ctx);
        app.start_mesh_job(Operation::Subdivide).unwrap();
        app.camera.zoom = 3.;
        app.capture_diagnostic_context(&ctx);
        let started = std::time::Instant::now();
        while app.mesh_job.is_some() {
            app.poll_mesh_job();
            assert!(started.elapsed().as_secs() < 10);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let manifest = app.document.diagnostic_manifest(Default::default());
        assert_eq!(manifest.transitions.len(), 1);
        assert_eq!(
            manifest.transitions[0]
                .settings_at_edit_start
                .as_ref()
                .unwrap()
                .view_input
                .as_ref()
                .unwrap()["camera"]["zoom"],
            2.
        );
        assert_eq!(
            manifest.current_settings.view_input.as_ref().unwrap()["camera"]["zoom"],
            3.
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a Vulkan adapter (software lavapipe supported)"]
    fn diagnostic_export_dialog_offscreen_preview() {
        let mut capture = crate::viewport::UiCapture::new([1200, 850]);
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut app = App::default();
        app.ui.diagnostics_open = true;
        app.document.invert_mask().unwrap();
        app.document.undo();
        for step in 0..3 {
            let output = ctx.run(
                egui::RawInput {
                    time: Some(step as f64),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 850.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    let _ = app.ui.toolbar(ctx, &app.document, &app.camera);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.label("Model viewport");
                    });
                    app.show_diagnostics(ctx);
                },
            );
            let rgba = capture.render(&ctx, output);
            if step == 2 {
                capture.save("/tmp/clay-diagnostic-export.png", &rgba);
            }
        }
        assert_eq!(app.document.history_counts(), (0, 1));
    }
}
