//! A version-specific development notice shown before the workspace opens.
use super::*;

impl UiState {
    pub(crate) fn initialize_startup_notice(&mut self) {
        self.startup_notice_open = !env!("CARGO_PKG_VERSION_PRE").is_empty()
            && self.preferences.dismissed_startup_version != env!("CARGO_PKG_VERSION");
        self.startup_notice_skip = false;
    }

    fn splash_art(&mut self, ui: &mut egui::Ui, max_height: f32) {
        let texture = self.startup_art.get_or_insert_with(|| {
            let image =
                image::load_from_memory(include_bytes!("../../docs/images/sketchotter-splash.jpg"))
                    .expect("invalid embedded SketchOtter splash")
                    .into_rgb8();
            let size = [image.width() as usize, image.height() as usize];
            ui.ctx().load_texture(
                "SketchOtter splash",
                egui::ColorImage::from_rgb(size, image.as_raw()),
                egui::TextureOptions::LINEAR,
            )
        });
        let size = texture.size_vec2();
        let scale = (ui.available_width() / size.x).min(max_height / size.y);
        ui.vertical_centered(|ui| {
            ui.add(
                egui::Image::new((texture.id(), size * scale))
                    .alt_text("SketchOtter, a sculpted teal clay otter"),
            );
        });
    }

    pub(crate) fn startup_notice(&mut self, ctx: &egui::Context) {
        design::appearance(ctx, &self.preferences.appearance);
        egui::CentralPanel::default().show(ctx, |_| {});
        let screen = ctx.content_rect().size();
        egui::Modal::new(egui::Id::new("development_release_notice")).show(ctx, |ui| {
            ui.set_width((screen.x - 48.).clamp(120., 460.));
            egui::ScrollArea::vertical()
                .max_height((screen.y - 48.).max(80.))
                .show(ui, |ui| {
                    self.splash_art(ui, screen.y * 0.32);
                    ui.add_space(8.);
                    ui.heading(concat!("SketchOtter ", env!("CARGO_PKG_VERSION")));
                    ui.label("Clay Form is now SketchOtter. Your projects and settings carry over.");
                    ui.add_space(6.);
                    ui.label("This is an early development release. Save often and keep backups of important work.");
                    ui.hyperlink_to("Report an issue", concat!(env!("CARGO_PKG_REPOSITORY"), "/issues"));
                    ui.separator();
                    ui.checkbox(&mut self.startup_notice_skip, "Don’t show again for this version");
                    if ui.button("Continue to SketchOtter").clicked() {
                        if self.startup_notice_skip {
                            self.preferences.dismissed_startup_version = env!("CARGO_PKG_VERSION").into();
                        }
                        self.startup_notice_open = false;
                    }
                });
        });
    }
}
