#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use clay_form::App;
use eframe::egui;
fn main() -> eframe::Result {
    let mut args = std::env::args_os().skip(1);
    let project = match (args.next(), args.next()) {
        (None, None) => None,
        (Some(path), None) if path == "--help" || path == "-h" => {
            println!("Usage: sketchotter [PROJECT.clay]");
            return Ok(());
        }
        (Some(path), None) => match path.into_string() {
            Ok(path) => Some(path),
            Err(_) => {
                eprintln!("The project path must be valid UTF-8");
                std::process::exit(2);
            }
        },
        _ => {
            eprintln!("Usage: sketchotter [PROJECT.clay]");
            std::process::exit(2);
        }
    };
    let title = format!("SketchOtter {}", env!("CARGO_PKG_VERSION"));
    let mut options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_app_id("sketchotter")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../assets/branding/sketchotter.png"
                ))
                .expect("invalid embedded SketchOtter icon"),
            )
            .with_inner_size([1280., 850.]),
        ..Default::default()
    };
    clay_form::configure_native_input(&mut options);
    eframe::run_native(
        "SketchOtter",
        options,
        Box::new(move |cc| {
            let state = cc
                .wgpu_render_state
                .as_ref()
                .ok_or("wgpu renderer is required")?;
            let mut app = App::with_wgpu(state)?;
            if let Some(path) = project {
                app.open_startup_project(path, &cc.egui_ctx);
            }
            Ok(Box::new(app))
        }),
    )
}
