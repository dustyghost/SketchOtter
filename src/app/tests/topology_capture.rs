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
