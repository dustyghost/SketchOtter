# Modeling workflow review: move welding, detached copies, groups and corner tools

The focused geometry and controller checks cover stationary-target vertex/edge welding, duplicate edge cleanup, UV/material/group preservation, protection, symmetry, detached face copies, isolated vertices through import/export and subdivision, automatic group naming, one-step Undo/Redo, old preset compatibility and ordinary free movement.

Three Vulkan interaction walkthroughs use the real application controls:

- `move_snap_weld_offscreen_review`: Off/Vertex/Edge controls; gold target preview; Shift changes without pointer movement; move away and back; cancellation; target selection; Undo/Redo; native save/reopen. Both projections with camera roll.
- `detached_copy_offscreen_review`: enable automatic grouping; Ctrl-copy a connected face selection and an isolated vertex; preserve originals; keep copied points selectable and movable; group membership; Undo/Redo and native save/reopen.
- `component_bevel_offscreen_review`: choose all four tools from Q; drag live width; Escape cancellation; one-step geometry/group history; native round-trip. CPU cases additionally cover selected junctions, all cube edges/vertices, mirrored topology, unmapped edited UVs and invalid widths.

Captures were visually inspected. Documentation selections are in `docs/images`: `move-snap-*`, `detached-copy-*`, `chamfer-*`, and `bevel-*`.

The interaction review found and fixed a silhouette edge-picking problem caused by relying on an exact face hit at the projected edge. Picking now checks the point on the edge against the visible surface depth. A detached point dragged behind its source surface correctly remains occluded; the selection walkthrough uses an exposed point. The bevel width control displays the current preview amount. Transform groups contain only moved vertices; topology groups include affected surviving face corners or deletion boundaries.

Local commands used one compiler job, the shared build lock, a disk-backed target/scratch directory, and one test thread. Full tests, formatting, lint and all platform builds remain CI checks. Physical pen feel and arbitrary production meshes still need hands-on review.
