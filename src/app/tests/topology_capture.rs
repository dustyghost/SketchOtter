use super::*;

#[test]
#[ignore = "requires Vulkan; exercises manual construction selection, preview, cancellation and Apply"]
fn manual_topology_offscreen_review() {
    use crate::{
        sculpt::{Geometry, TopologyEdit},
        viewport::Component,
    };
    let directory =
        std::env::var("CLAY_GUIDE_PREVIEW").unwrap_or_else(|_| "/tmp/clay-topology-review".into());
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    let mut g = Guide::new(directory, [1100, 1000]);
    g.interactive = true;
    g.app.ui.wire = true;
    g.panel(Tool::Mesh, Dock::Right);
    let mut primitive = Primitive::for_kind(PrimitiveKind::Cube);
    primitive.divisions = [1; 3];
    g.app
        .document
        .replace_geometry(Geometry::new(primitive.mesh().unwrap()))
        .unwrap();
    g.app.camera.yaw = 0.;
    g.app.camera.pitch = 0.;
    g.app.camera.zoom = 0.65;
    g.frame(vec![]);
    click_guide_modeling_checkbox(&mut g);
    let rect = g.frame(vec![]).0;
    let center = rect.center();
    g.frame(vec![
        egui::Event::PointerMoved(center),
        primary_event(center, true),
    ]);
    g.frame(vec![primary_event(center, false)]);
    let face = g.app.viewport.modeling.selected_faces()[0];
    let vertices = g.app.document.mesh().polygons()[face].vertices.clone();
    let corners: Vec<_> = vertices
        .iter()
        .map(|&v| g.app.document.mesh().vertices[v])
        .collect();
    reach_sidebar_control(&mut g, "Topology");
    click_guide_label(&mut g, "Topology");
    reach_sidebar_control(&mut g, "Delete faces");
    click_guide_label(&mut g, "Delete faces");
    let open = g.app.document.geometry_snapshot();
    g.app.camera.yaw = 0.3;
    g.app.camera.pitch = -0.15;
    reach_sidebar_control(&mut g, "Vertex");
    click_guide_label(&mut g, "Vertex");
    click_guide_label(&mut g, "Add");
    for p in &corners {
        // Orbit to expose each adjoining wall: back-facing surfaces do not
        // provide a visible pick target through the open front of the cube.
        g.app.camera.yaw = p.0.signum() * 0.3;
        g.app.camera.pitch = -p.1.signum() * 0.15;
        let rect = g.frame(vec![]).0;
        let scale = rect.width().min(rect.height()) * 0.35 * g.app.camera.zoom;
        let (right, up, _) = g.app.camera.basis();
        let at = rect.center() + Vec2::new(p.dot(right), -p.dot(up)) * scale;
        g.frame(vec![egui::Event::PointerMoved(at), primary_event(at, true)]);
        g.frame(vec![primary_event(at, false)]);
    }
    g.app.camera.yaw = 0.3;
    g.app.camera.pitch = -0.15;
    g.save("boundary-vertices-selected");
    assert_eq!(
        g.app.viewport.modeling.selected.len(),
        4,
        "{:?}",
        g.app.viewport.modeling.selected
    );
    reach_sidebar_control(&mut g, "Create face");
    click_guide_label(&mut g, "Create face");
    assert_eq!(g.app.document.geometry_snapshot(), open);
    let preview = g
        .app
        .viewport
        .modeling
        .preview()
        .expect(&g.app.ui.status)
        .clone();
    g.app.camera.yaw = 0.3;
    g.app.camera.pitch = -0.15;
    reach_sidebar_control(&mut g, "Cancel topology preview");
    g.save("create-face-preview");
    click_guide_label(&mut g, "Cancel topology preview");
    assert_eq!(g.app.document.geometry_snapshot(), open);
    reach_sidebar_control(&mut g, "Create face");
    click_guide_label(&mut g, "Create face");
    reach_sidebar_control(&mut g, "Apply topology");
    click_guide_label(&mut g, "Apply topology");
    assert_eq!(g.app.document.mesh(), &preview);
    g.save("create-face-applied");
    click_guide_label(&mut g, "Undo");
    assert_eq!(g.app.document.geometry_snapshot(), open);
    click_guide_label(&mut g, "Redo");
    assert_eq!(g.app.document.mesh(), &preview);
    // Select the new cap's opposite corners and inspect the explicit diagonal.
    let q = *g.app.document.mesh().quads.last().unwrap();
    g.app.viewport.modeling.sync(&g.app.document);
    g.app.viewport.modeling.selected = [Component::Vertex(q[0]), Component::Vertex(q[2])].into();
    reach_sidebar_control(&mut g, "Connect vertices");
    click_guide_label(&mut g, "Connect vertices");
    assert_eq!(
        g.app.viewport.modeling.topology_preview_label(),
        Some(TopologyEdit::Connect.label())
    );
    reach_sidebar_control(&mut g, "Apply topology");
    g.save("connect-vertices-preview");
    click_guide_label(&mut g, "Apply topology");
    assert_eq!(g.app.document.mesh().quads.len(), preview.quads.len() - 1);
    click_guide_label(&mut g, "Undo");
    assert_eq!(g.app.document.mesh(), &preview);
    let saved = serde_json::to_vec(&g.app.document.project()).unwrap();
    let restored: crate::sculpt::Project = serde_json::from_slice(&saved).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.mesh, preview);
}

#[test]
#[ignore = "requires Vulkan; exercises chain bridging and face-region controls"]
fn topology_regions_offscreen_review() {
    use crate::app::tests::regions::fixture;
    use crate::sculpt::Geometry;
    let directory =
        std::env::var("CLAY_GUIDE_PREVIEW").unwrap_or_else(|_| "/tmp/clay-regions-review".into());
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    let mut g = Guide::new(directory, [1100, 1000]);
    g.interactive = true;
    g.app.ui.wire = true;
    g.panel(Tool::Mesh, Dock::Right);
    g.app
        .document
        .replace_geometry(Geometry::new(fixture(true)))
        .unwrap();
    g.app.camera.yaw = 0.;
    g.app.camera.pitch = 0.;
    g.app.camera.zoom = 0.55;
    g.frame(vec![]);
    click_guide_modeling_checkbox(&mut g);
    reach_sidebar_control(&mut g, "Edge");
    click_guide_label(&mut g, "Edge");
    click_guide_label(&mut g, "Add");
    // Select the two inner border edges with actual viewport clicks.
    for x in [-1., 1.] {
        let rect = g.frame(vec![]).0;
        let scale = rect.width().min(rect.height()) * 0.35 * g.app.camera.zoom;
        let point = rect.center() + Vec2::new(x * scale, 0.);
        g.frame(vec![
            egui::Event::PointerMoved(point),
            primary_event(point, true),
        ]);
        g.frame(vec![primary_event(point, false), egui::Event::PointerGone]);
    }
    assert_eq!(g.app.viewport.modeling.selected.len(), 2);
    reach_sidebar_control(&mut g, "Topology");
    click_guide_label(&mut g, "Topology");
    g.app.ui.modeling.bridge_segments = 3;
    reach_sidebar_control(&mut g, "Bridge partial chains");
    click_guide_label(&mut g, "Bridge partial chains");
    let original = g.app.document.geometry_snapshot();
    let bridge = g
        .app
        .viewport
        .modeling
        .preview()
        .expect(&g.app.ui.status)
        .clone();
    assert_eq!(bridge.quads.len(), 5);
    reach_sidebar_control(&mut g, "Cancel topology preview");
    g.save("partial-chain-bridge-preview");
    click_guide_label(&mut g, "Cancel topology preview");
    assert_eq!(g.app.document.geometry_snapshot(), original);
    reach_sidebar_control(&mut g, "Bridge partial chains");
    click_guide_label(&mut g, "Bridge partial chains");
    reach_sidebar_control(&mut g, "Apply topology");
    click_guide_label(&mut g, "Apply topology");
    assert_eq!(g.app.document.mesh(), &bridge);
    click_guide_label(&mut g, "Undo");
    assert_eq!(g.app.document.geometry_snapshot(), original);
    click_guide_label(&mut g, "Redo");
    assert_eq!(g.app.document.mesh(), &bridge);
    // A flat triangle fan becomes one quad, with face selection via the pen path.
    g.app
        .document
        .replace_geometry(Geometry::new(fixture(false)))
        .unwrap();
    reach_sidebar_control(&mut g, "Face");
    click_guide_label(&mut g, "Face");
    click_guide_label(&mut g, "Add");
    for (x, y) in [(0., -0.6), (0.6, 0.), (0., 0.6), (-0.6, 0.)] {
        let rect = g.frame(vec![]).0;
        let scale = rect.width().min(rect.height()) * 0.35 * g.app.camera.zoom;
        let point = rect.center() + Vec2::new(x * scale, -y * scale);
        g.frame(vec![
            egui::Event::PointerMoved(point),
            primary_event(point, true),
        ]);
        g.frame(vec![primary_event(point, false), egui::Event::PointerGone]);
    }
    assert_eq!(g.app.viewport.modeling.selected.len(), 4);
    reach_sidebar_control(&mut g, "Dissolve face region");
    g.save("face-region-selected");
    click_guide_label(&mut g, "Dissolve face region");
    let dissolved = g
        .app
        .viewport
        .modeling
        .preview()
        .expect(&g.app.ui.status)
        .clone();
    assert_eq!(dissolved.quads.len(), 1);
    reach_sidebar_control(&mut g, "Apply topology");
    g.save("face-region-dissolve-preview");
    click_guide_label(&mut g, "Apply topology");
    assert_eq!(g.app.document.mesh(), &dissolved);
    click_guide_label(&mut g, "Undo");
    // Re-select the connected fan, then reverse its complete orientation.
    g.app.viewport.modeling.sync(&g.app.document);
    g.app.viewport.modeling.selected = (0..4).map(crate::viewport::Component::Face).collect();
    reach_sidebar_control(&mut g, "Flip face orientation");
    click_guide_label(&mut g, "Flip face orientation");
    let flipped = g
        .app
        .viewport
        .modeling
        .preview()
        .expect(&g.app.ui.status)
        .clone();
    reach_sidebar_control(&mut g, "Apply topology");
    click_guide_label(&mut g, "Apply topology");
    assert_eq!(g.app.document.mesh(), &flipped);
    g.app.camera.yaw = std::f32::consts::PI;
    g.save("face-region-flipped-back-view");
    click_guide_label(&mut g, "Undo");
    assert_eq!(g.app.document.mesh(), &fixture(false));
}
