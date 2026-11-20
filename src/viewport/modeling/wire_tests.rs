use super::*;
use crate::{
    camera::Camera,
    sculpt::{Geometry, Vec3},
};

#[test]
fn point_extrude_wire_picking_and_region_depth_work_without_faces_at_the_tip() {
    for perspective in [false, true] {
        let mesh = Mesh {
            vertices: vec![
                Vec3(-1., -1., 0.),
                Vec3(1., -1., 0.),
                Vec3(1., 1., 0.),
                Vec3(-1., 1., 0.),
                Vec3(0., 0., -0.5),
                Vec3(1.4, 1.2, 0.4),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            quads: vec![[0, 1, 2, 3]],
            loose_edges: vec![[0, 4], [2, 5]],
            mask: vec![0.; 6],
            ..Default::default()
        };
        let mut document = Document::default();
        document
            .replace_geometry(Geometry::new(mesh.clone()))
            .unwrap();
        let camera = Camera {
            yaw: 0.,
            pitch: 0.,
            roll: 0.2,
            perspective,
            ..Default::default()
        };
        let scene = Scene::from_geometry(
            egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 800.)),
            &camera,
            std::sync::Arc::new(super::super::gpu::DisplayMesh::new(&document)),
            false,
        );
        assert_eq!(
            Viewport::component_at(
                scene.project(mesh.vertices[5]),
                ComponentMode::Vertex,
                &document,
                &scene
            ),
            Some(Component::Vertex(5))
        );
        assert_eq!(
            Viewport::component_at(
                scene.project(mesh.vertices[4]),
                ComponentMode::Vertex,
                &document,
                &scene
            ),
            None
        );
        let middle = (mesh.vertices[2] + mesh.vertices[5]) * 0.5;
        assert_eq!(
            Viewport::edge_at(scene.project(middle), &document, &scene),
            Some([2, 5])
        );
        let mut viewport = Viewport::default();
        let rear = scene.project(mesh.vertices[4]);
        for through in [false, true] {
            viewport
                .select_component_region(
                    &document,
                    &scene,
                    super::super::RegionTool {
                        shape: super::super::RegionShape::Rectangle,
                        through,
                        flip: false,
                        operation: super::super::RegionOperation::Components {
                            mode: ComponentMode::Vertex,
                            change: SelectionChange::Replace,
                            protect: true,
                        },
                    },
                    |p| p.distance(rear) < 5.,
                )
                .unwrap();
            assert_eq!(
                viewport.modeling.selected,
                if through {
                    [Component::Vertex(4)].into()
                } else {
                    BTreeSet::new()
                }
            );
        }
        let connected = Viewport::selection_target(
            Component::Vertex(5),
            scene.project(mesh.vertices[5]),
            SelectionSpan::Connected,
            0.,
            &mesh,
            &scene,
            None,
        )
        .unwrap();
        assert!(connected.contains(&Component::Vertex(2)));
        assert!(connected.contains(&Component::Vertex(4)));
        let display = super::super::gpu::DisplayMesh::new(&document);
        assert_eq!(display.mesh.loose_edges.as_ref(), &mesh.loose_edges);
    }
}

#[test]
fn review_wire_only_extrusion_has_no_invented_outline() {
    let mesh = Mesh {
        vertices: vec![
            Vec3(0., 0., 0.),
            Vec3(1., 0., 0.),
            Vec3(1., 1., 0.),
            Vec3(0., 1., 0.),
            Vec3(2., 2., 0.),
        ],
        loose_edges: vec![[0, 1], [0, 3], [1, 2], [2, 3], [2, 4]],
        mask: vec![0.; 5],
        ..Default::default()
    };
    let mut document = Document::default();
    document
        .replace_geometry(Geometry::new(mesh.clone()))
        .unwrap();
    let display = super::super::gpu::DisplayMesh::new(&document);
    assert!(
        display.outlines.is_empty(),
        "Explicit wires must not also be connected in index order"
    );
    assert_eq!(display.mesh.loose_edges.as_ref(), &mesh.loose_edges);
    let preview = super::super::gpu::DisplayMesh::with_active_mesh(&document, &mesh);
    assert!(preview.outlines.is_empty());
}
