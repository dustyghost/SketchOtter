# Modeling selection growth and snapping — 15 November 2026

Tickets: #153 and #154. One independent PR based on main; existing lighting, topology, surface-array and camera PRs remain unmerged.

Grow/shrink follows logical polygon adjacency in all component modes. Open boundaries erode, closed fully selected islands remain selected, disconnected islands do not join, and hidden/protected vertices are barriers. Exact symmetry retains whole orbits. A bounded, revision-local selection history supplies explicit Undo/Redo selection actions without changing geometry history or tool choices.

Snapping uses an explicit selected source vertex and a visible vertex, edge or surface target on the active mesh. Move aligns positions; Rotate matches direction around the selection centre; Scale matches distance. Axis/amount constraints project the relevant displacement/direction/distance; numeric Apply remains separate. Targets exclude moving vertices, symmetry copies and incident polygons. Border picking samples neighbouring pixels with a depth check. The source document stays unchanged until Apply; navigation keeps the pinned preview stable, and stale settings, cancellation and failed targets cannot apply an earlier result.

Validation:

- Focused selection and snapping tests cover adjacency, boundaries/islands, protection, symmetry, selection history, transform constraints, picking in both projections, hidden/occluded/out-of-range targets, preset compatibility, cancellation, stale previews and geometry history.
- All 46 modeling controller tests passed, including the two new controller tests. Existing selection gestures, free/gizmo transforms, symmetry, protection, topology edits, repeat and presets remain covered.
- The existing `component_transforms_preserve_attributes_and_mirror_displacements` regression passed.
- `precision_modeling_offscreen_review` passed using Vulkan llvmpipe (Mesa 26.2.6, LLVM 23.1.4), one software rendering thread, at 1200×1000 and 760×720. It clicks Grow/Shrink and selection history controls, selects source/target through pointer input, retargets, cancels, applies and uses geometry Undo/Redo. Inspected captures show the grown region, coincident source/target marker and accessible Apply/Cancel controls. The walkthrough is included in GPU CI.
- The new pointer controller test checks orthographic/perspective picking, navigation with a stable preview, brush-switch Keep editing, one-step Undo/Redo and project serialization. No physical tablet test was performed.

Local Cargo commands used one compiler job and the shared build lock. Full tests, lint and platform builds run in PR CI. The guide and screenshot regeneration script include the two selected captures.

Scope: single active editable mesh and a single subdivision level. This does not weld geometry or snap to other submodels. Rotation and scale use the existing selection centre, so a direction/distance target does not necessarily coincide with the final source point. Selection history and unfinished snap previews are transient; applied geometry is saved normally.
