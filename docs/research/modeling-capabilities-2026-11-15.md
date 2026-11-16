# Modelling capability audit — 15 November 2026

Scope: #127, with the manual-construction delivery in #126. This inventory uses the current command catalog, geometry routines, controller tests and user guide. It distinguishes available workflows from limitations; a command name alone is not evidence of general topology support.

PRs #146 (interaction), #147 (cameras) and #150 (arrays) have merged. #146 supplies the Apply/Discard/Keep editing brush-switch decision reused by this batch. #150 adds face-based instance placement, not polygon construction. Neither implements the missing modelling operations below.

Integration update: this branch now includes `main` through `b00f68e`, including PRs #152, #162, #163 and #164. The table records the original audit baseline. Its selection-growth (#153) and general-snapping (#154) gaps have since been delivered by #164 and are available alongside manual construction in this branch.

| Capability | State before this batch | Evidence and limits | Delivery decision |
| --- | --- | --- | --- |
| Face/edge/vertex selection, paint Add/Remove, connected islands, flat islands, groups | Existing | `viewport/modeling/selection.rs`, `sculpt/modeling/selection.rs`; depth-tested targets, exact symmetry maps and explicit mode conversion | Reuse unchanged |
| Grow/shrink component selection | Missing | The component action catalog has no growth/shrink commands; sculpt mask growth is a different workflow | [#153](https://github.com/dustyghost/clay-form/issues/153) |
| Component move/rotate/scale, numeric amounts, axes and gizmos | Existing | `viewport/modeling/transform.rs`, `sculpt/modeling/transform.rs`; includes free dragging and one-step history | Reuse unchanged |
| General vertex/edge/surface snapping during transforms | Missing | `contact.rs` provides contact snapping for Extrude and fuse, not general transform snapping | [#154](https://github.com/dustyghost/clay-form/issues/154) |
| Region/individual extrusion and inset | Existing with limits | `construction.rs`: planar connected inset, or individual faces; rejection of folded corners and excessive width | Document limits; no duplicate command |
| Boundary bevel and creases | Partial | Bevel handles compatible exterior region corners; creases and subdivision are already available in `edges.rs` | [#155](https://github.com/dustyghost/clay-form/issues/155) scopes interior-edge and vertex bevels |
| Insert/slide/dissolve loops | Existing with limits | `loops.rs`, `topology.rs`, `dissolve.rs`: exact quad strips and complete unambiguous loops; stops/rejects at unsupported poles, triangles and branches | Retain existing regressions |
| Split edges, extrude edges, split/extrude points, insert face centres, split faces | Existing | `components.rs`; visible side/endpoint choices, supported closed fans and exact symmetry | Reuse unchanged |
| Connect opposite vertices of a quad | Partial | Existing command commits immediately and rebuilds topology | #126 keeps immediate Undo/Redo and adds UV-preserving construction and explicit unsupported-edge feedback |
| Create a face from selected boundary vertices | Missing | No vertex-selection face-creation action | #126 adds Create face for a complete existing opening; vertex order comes from the directed boundary |
| Fill a selected opening | Partial | Existing Fill boundary requires a planar rim and commits immediately | #126 adds safe concave/mildly nonplanar filling with immediate Undo/Redo; convex four-sided caps remain quads |
| Advanced quad-oriented/adaptive caps | Missing | Basic ear-clipped patches do not choose edge flow, redistribute density or optimise valence | Existing #125 owns this work |
| Equal-count, same-model boundary bridging | Existing | `topology.rs`: complete loops, automatic correspondence and a quad strip | Preserve; not replaced by Create face |
| Unequal loops and separate models with twist/segments/local blending | Existing foundation; acceptance still in progress | `fusion.rs`, `app/fusion.rs`, #117 and #121; #121 also owns enclosed-shell cleanup and remaining validation | Continue under #121; no duplicate full-boundary join ticket |
| Partial-boundary/edge-chain bridge and face-region bridge | Missing | The existing bridge/join workflows require complete loops; edge extrusion extends a border but does not pair two chains | [#156](https://github.com/dustyghost/clay-form/issues/156) scopes partial chains; face-region bridging needs separate investigation |
| Delete faces and transform face regions | Existing | Delete in `topology.rs`, transforms in `transform.rs`; deletion opens the surface | Retain |
| Flip a selected face region / dissolve selected faces | Missing | Loop dissolution is not arbitrary face-region dissolution; no face-orientation action exists | [#157](https://github.com/dustyghost/clay-form/issues/157) |
| Directly drawn topology on a source surface | Missing | Guided solver output and Path Brush are different operations | Existing #151 owns drawing/intersections/automatic face filling |
| Topology validity, attributes and protected geometry | Partial foundation | Shared manifold/symmetry/protection checks exist. They are not a general self-intersection proof for every legacy operation | #126 validates new patch intersections and preserves untouched face-corner UV data; broader auditing remains operation-specific |

## First delivery

Complete #126 as one reviewable workflow: select vertices or boundary edges, run the action immediately, then use Undo/Redo as needed. Each successful construction or deletion is one history step; failed actions leave both mesh and history unchanged. Keep source vertex indices, masks, materials, groups, creases and unaffected UV corners unchanged. New cap faces receive no group and are explicitly unmapped until unwrapped. New triangles are checked against the existing surface; unsupported or ambiguous input leaves the scene unchanged.

Create face obtains its vertex order from a complete open boundary, including symmetry copies. The mesh engine stores triangle/quad surfaces, so this delivery does not introduce loose edges, loose vertices or stored n-gons. Concave four-sided and larger caps are tessellated with visible triangle edges. Winding follows neighbouring faces; reversing an attached cap would break edge orientation. Basic filling is bounded to 1,024 vertices per loop and a rim depth of at most 5% of its span. More demanding patch generation belongs to #125.

The user-facing guide and focused geometry/controller/rendered-interaction checks accompany the implementation. Synthetic pointer tests exercise mouse/pen gesture paths; physical tablet feel remains a hands-on validation item.

## Following work

1. #125: advanced caps, reusing boundary validation and the history contract.
2. #121: reconcile remaining open-end joining acceptance without duplicating its implemented connector.
3. The audit's follow-up tickets #153–#157 are tracked in the development project; #153 and #154 are now delivered through #164. Extend only a separately scoped remaining operation from the matrix after its interaction and validation cases have been agreed. Bevels, partial bridges and face-region editing are independent changes and are not completion criteria for this first implementation batch.
