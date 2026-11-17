# Extrusion joins and selected face filling

Point extrusion attracts to nearby visible vertices and connects using the existing target index. Open-edge extrusion can join a compatible open edge with one face, reusing all four endpoints. A gold marker identifies the proposed join. Shift temporarily restores free extrusion, including without pointer movement; releasing it restores attraction. The toggle and pixel radius are saved in Modeling presets. Snapping supports one source and its exact symmetry copies on the active mesh.

Create face now accepts three or four distinct selected vertices or selected edge endpoints, including loose chains and outlines. Existing edges constrain perimeter order, adjoining faces constrain winding, and symmetry copies preserve reflected winding. A triangle or convex quad is created immediately; concave four-point shapes triangulate. Loose edges promoted into face edges are removed from wire storage. Larger selections keep the complete-boundary filling behavior. Both Vertex and Edge Q menus expose the action.

## Validation

28 focused CPU checks passed across extrusion joins, selected-face geometry, existing manual topology, point extrusion and preset compatibility. These cover vertex reuse, symmetry, masks/hidden faces, occlusion/radius, obstructed joins, UV preservation, invalid selections, history atomicity and old preset defaults.

Three Vulkan interaction walkthroughs passed:
- `extrusion_snap_join_offscreen_review`: point and edge joins in orthographic and perspective views with roll; Shift toggled mid-drag; unchanged committed geometry during drag; target reuse, one-step Undo/Redo and native project round-trip.
- `create_selected_face_offscreen_review`: actual Q clicks for three vertices, four vertices and two opposite edges on a loose outline; immediate results, no preview, consumed wire edges, one-step Undo/Redo and native project round-trip.
- `point_extrude_offscreen_review`: free screen-plane extrusion, repeated directions, picking, movement and history in both projections, with attraction disabled explicitly for pointer-accuracy checks.

The [test log](focused-tests.log) records all 31 passing checks. Captures were visually inspected: gold join markers and the connecting geometry are legible, Create face is reachable in Edge Q, and triangle/quad output matches the selected outline. Representative captures are in the [Modeling guide](../../guide/modeling.md). The two new interaction checks are registered in GPU CI and the screenshot refresh script. Full tests, lint and platform packaging remain CI checks; physical pen feel is a hands-on check.
