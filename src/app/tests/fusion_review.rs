//! Release-mode checks through the application worker, document, viewport and renderer.
use super::*;
use crate::sculpt::{
    Geometry, Mesh, Submodel,
    fusion::tests::{fluted_open_parts, relief_parts},
};
use std::time::{Duration, Instant};

fn rendered_join(intersection: bool) {
    let mode = if intersection {
        "intersection"
    } else {
        "open-ends"
    };
    let directory = std::env::var_os("CLAY_GUIDE_PREVIEW")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(mode);
    std::fs::create_dir_all(&directory).unwrap();
    let mut g = Guide::new(&directory, [1100, 780]);
    g.interactive = true;
    if let Ok(path) = std::env::var("CLAY_REVIEW_PROJECT") {
        let project = crate::persistence::load_project(&path).unwrap();
        g.app.document.replace_project(project, &path).unwrap();
    } else {
        let [a, b] = if intersection {
            relief_parts(96)
        } else {
            fluted_open_parts()
        };
        let mut geometry = Geometry::new(a);
        geometry.submodels.push(Submodel {
            name: "Second part".into(),
            mesh: b,
            mesh_settings: Default::default(),
            metadata: Default::default(),
        });
        g.app.document.replace_geometry(geometry).unwrap();
    }
    let before = g.app.document.geometry_snapshot();
    g.app.camera.pitch = if intersection { 0.12 } else { -0.32 };
    g.app.camera.yaw = if intersection { 0.12 } else { 1.04 };
    g.app.camera.zoom = 0.82;
    g.app.camera.pan = Vec2::new(170., 0.);
    g.app.dispatch(Command::OpenFusion, &g.ctx);
    super::super::fusion::finish_fusion(&mut g.app);
    g.app.ui.fusion.intersections = intersection;
    if !intersection {
        // Select the opposing ends, independent of vertex numbering.
        for side in 0..2 {
            let rims = &g.app.ui.fusion.loops[side];
            g.app.ui.fusion.selected[side] = rims
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.center.2.abs().total_cmp(&b.center.2.abs()))
                .map(|(i, _)| i);
        }
    }
    let selected = g.app.ui.fusion.selected;
    g.app.ui.fusion.collar = 2;
    g.app.ui.fusion.options.segments = 5;
    g.app.ui.fusion.options.curvature = 0.7;
    if !intersection {
        g.app.ui.fusion.options.twist = 3;
    }
    g.save("before");
    let start = Instant::now();
    g.app.dispatch(Command::PreviewFusion, &g.ctx);
    assert!(g.app.ui.fusion.busy, "{}", g.app.ui.fusion.notice);
    let mut frames = 0;
    while g.app.ui.fusion.busy {
        g.frame(vec![]);
        frames += 1;
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "{}",
            g.app.ui.fusion.notice
        );
    }
    assert!(g.app.ui.fusion.ready, "{}", g.app.ui.fusion.notice);
    if !intersection {
        assert!(
            g.app
                .ui
                .fusion
                .notice
                .contains("5 segments · alignment offset 3 · curvature 0.70")
        );
    }
    g.frame(vec![]);
    let preview_time = start.elapsed();
    let memory = || {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status
                    .lines()
                    .find(|line| line.starts_with("VmHWM:"))
                    .map(str::to_owned)
            })
    };
    let preview_peak = memory();
    assert!(g.app.document.geometry_matches(&before));
    g.app.ui.fusion.show_patch = false;
    g.app.ui.fusion.show_removed = false;
    g.save("preview");
    assert!(g.app.viewport.session.join_rims.is_empty());
    g.app.ui.wire = true;
    g.save("preview-wire");
    let preview: Mesh = g.app.fusion.preview_mesh().clone();
    let start = Instant::now();
    g.app.dispatch(Command::ApplyFusion, &g.ctx);
    g.frame(vec![]);
    let apply_time = start.elapsed();
    let apply_peak = memory();
    assert!(
        g.app.document.mesh() == &preview,
        "Apply changed the preview mesh or its attributes"
    );
    g.save("applied-wire");
    g.app.ui.wire = false;
    g.save("applied");
    let triangles = before.mesh.triangles.len()
        + before
            .submodels
            .iter()
            .map(|m| m.mesh.triangles.len())
            .sum::<usize>();
    eprintln!(
        "{mode}: {triangles} source triangles; Preview incl. display {preview_time:?}, {frames} frames, {preview_peak:?}; Apply incl. display {apply_time:?}, {apply_peak:?}"
    );
    let joined = g.app.document.geometry_snapshot();
    let saved = directory.join("joined-roundtrip.clay");
    let saved = saved.to_str().unwrap();
    crate::persistence::save_project(saved, &g.app.document.project()).unwrap();
    let mut reopened = Document::default();
    reopened
        .replace_project(crate::persistence::load_project(saved).unwrap(), saved)
        .unwrap();
    assert!(reopened.geometry_matches(&joined));
    std::fs::remove_file(saved).unwrap();
    g.app.dispatch(Command::Undo, &g.ctx);
    assert!(g.app.document.geometry_matches(&before));
    g.app.dispatch(Command::Redo, &g.ctx);
    assert!(g.app.document.geometry_matches(&joined));
    if !intersection {
        g.app.dispatch(Command::Undo, &g.ctx);
        g.app.dispatch(Command::OpenFusion, &g.ctx);
        super::super::fusion::finish_fusion(&mut g.app);
        g.app.ui.fusion.selected = selected;
        g.app.dispatch(Command::PreviewFusion, &g.ctx);
        // Let the app paint while the dense worker is running before cancelling.
        g.frame(vec![]);
        assert!(
            g.app.ui.fusion.busy,
            "preview finished before the cancellation sample"
        );
        let start = Instant::now();
        g.app.dispatch(Command::CancelFusion, &g.ctx);
        let dispatch = start.elapsed();
        g.frame(vec![]);
        eprintln!(
            "Cancel dispatch {dispatch:?}, next rendered frame {:?}",
            start.elapsed()
        );
        assert!(!g.app.ui.fusion.open && !g.app.ui.fusion.busy);
        assert!(g.app.document.geometry_matches(&before));
    }
}

#[test]
#[ignore = "manual rendered release profile; requires Vulkan"]
fn fusion_rendered_relief_intersection() {
    rendered_join(true);
}

#[test]
#[ignore = "manual rendered release profile; requires Vulkan"]
fn fusion_rendered_fluted_open_ends() {
    rendered_join(false);
}
