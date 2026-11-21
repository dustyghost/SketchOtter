# Sculpting capacity and regional detail

[Issue #103](https://github.com/dustyghost/clay-form/issues/103) asks how much detail an artist can work with interactively. The largest archived workload completed fixed-topology edits on **1,204,224 quad polygons**. Those quads produce **2,408,448 triangles** for rendering and picking. A fresh position-history experiment below measures how much of a sculpt actually changed during a local stroke.

## What fits

These limits apply to the source tree at `e122065`. Vertices, authored polygons, triangulated faces and bytes answer different questions.

| Resource | Limit and consequence |
| --- | --- |
| Active mesh | Defaults of 500,000 vertices and 1,000,000 triangles. The independent settings allow up to 16 million vertices and 32 million triangles after restart. A quad normally consumes two triangles. [Settings](../../src/sculpt/capacity.rs) · [Mesh validation](../../src/sculpt/mesh.rs) |
| Subdivision stack | At most eight levels per model, with the sum of active, lower and upper vertex counts capped at twice the active vertex budget. Each level also passes mesh validation. [Stack validation](../../src/sculpt/subdivision.rs) |
| Scene and project | Up to 64 sculpt models including the active one. Each model's stack is validated separately. Native JSON has a 2 GiB file limit. Loading reads the file into memory and saving serialises it before checking the byte length, so peak memory can exceed file size. OBJ import has a separate 128 MiB input limit. [Project](../../src/sculpt/project.rs) · [Native I/O](../../src/persistence.rs) · [OBJ](../../src/persistence/obj.rs) |
| History | Defaults of 24 edits and 1,024 MiB of estimated retained data across Undo and Redo. Dense edits can reach the byte limit first. Geometry snapshots include lower levels and other models. Certain shared assets, including texture pixels, are counted once. [History](../../src/history.rs) · [Stroke capture](../../src/document/stroke.rs) |
| Display and GPU | Shading can split a source vertex across hard edges. Packed vertex stride depends on the material layout. The renderer batches dense geometry at the device's storage-buffer binding limit. The source triangle budget alone cannot predict buffer sizes, device memory or frame time. [Renderer](../../src/viewport/gpu.rs) |

## What has run on the laptop

The retained October runs used the i7-7500U, 7.63 GiB RAM and Intel HD 620 at 800×600 offscreen. Their fixtures cover a sphere, an attributed organic surface and a partially hidden variant. The largest had five stored levels and 1,604,074 stored vertices. The figures below come from different, named stages of the earlier work. Each is a timing for its stated operation.

| At 2,408,448 source triangles | Recorded result |
| --- | --- |
| Native save and reload | One 187.7 MiB sphere project took 10.01 s to save and 10.91 s to reopen. The original six-case editing matrix peaked at 2,258 to 2,654 MiB process RSS across its three large fixtures. [Capacity runs and raw data](../validation/high-capacity-2026-10-30/README.md) |
| Live sculpt | On the organic fixture, shared display arrays brought sustained Standard brush + display + completed upload from 193.0 to 115.8 ms median, with a 133.5 ms p95 afterwards. Presentation and drawing were outside the timer. [Paired stroke timings](../validation/display-arrays-2026-10-30/README.md) |
| Orbit and picking | The later indexed basic-material draw completed in 16.5 ms median for orthographic organic geometry. Warm CPU scene preparation measured 17.17 ms in a separate camera experiment. A new view, first pick and lookup construction can still cost more. [Draw](../validation/indexed-surface-draw-2026-10-31/README.md) · [Camera](../validation/camera-geometry-2026-10-31/README.md) · [Picking](../validation/picking-and-scene-reuse-2026-10-31/README.md) |
| Stroke boundaries | Cold organic Paint took 724 ms for the first dab and 111–144 ms on later starts with cached visibility. A release-shading prototype cut Standard release to 59 ms on a sphere but increased it from 257 to 310 ms on masked organic geometry. It was rejected. [Paint](../validation/paint-startup-2026-10-31/README.md) · [Release](../validation/local-release-shading-2026-10-31/README.md) |

The earlier editor work addressed shared settings accounting and worker validation in [#53](https://github.com/dustyghost/clay-form/issues/53). [#60](https://github.com/dustyghost/clay-form/issues/60) addressed live stroke feedback. The timings above identify costs that persisted after those changes. Whole-frame desktop latency, long-session residency and dense adaptive remeshing still need separate measurements.

## A bounded history experiment

The [release example](../../examples/stroke_history_profile.rs) sculpts a ridged, vertex-coloured quad sphere with a fully masked strip along its centre. Three overlapping Standard strokes of twelve dabs each use X symmetry to edit both sides. Topology and vertex order stay fixed. The experiment runs the real document history alongside a prototype that retains each changed source index and its previous XYZ value. Swapping those values implements both directions. After each Undo and Redo the example checks the entire active mesh against the document, including every position bit. The same index can occur in several successive edits.

Measured on 21 November 2026 with an AC-powered i7-7500U, 7.63 GiB RAM, Linux 7.3.4, the powersave governor and Rust 1.99.0 in release mode. Three fresh processes per resolution each ran the same three strokes, for 27 strokes and 54 full active-mesh comparisons. All source triangles were visible. Timings are medians of nine stroke observations per row and RSS is the largest process high-water mark. [Raw samples](../validation/stroke-history-2026-11-21/samples.jsonl)

| Visible triangles | Active / stored vertices | Vertices changed per stroke | Full position copy / patch allocation per stroke | Copy positions / find changes (ms) | Current document stroke start (ms) | Peak RSS (MiB) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 37,632 | 18,818 / 23,524 | 276–278 | 220.5 / 8 KiB | 0.016 / 0.067 | 2.50 | 32.9 |
| 150,528 | 75,266 / 98,790 | 1,100–1,118 | 882.0 / 32 KiB | 0.089 / 0.254 | 8.13 | 96.4 |
| 602,112 | 301,058 / 399,848 | 4,412–4,420 | 3,528.0 / 128 KiB | 0.371 / 0.995 | 39.98 | 344.1 |

At 602,112 triangles the three patches retained 384 KiB, versus 10.34 MiB for three full position arrays. The application's estimated retained history for these three complete edits was 201.9 MiB. The prototype's median position-only replay cost was 0.016 ms for Undo and 0.018 ms for Redo. Document stroke start includes cloning the geometry. Capturing positions and diffing them on release are two separate costs. The results justify testing capture at the point of mutation before changing history ownership.

The new probe stops at three subdivisions under the default mesh budget. A fourth would need 1,204,226 active vertices, and the archived 3,072 MiB RSS budget plus 768 MiB reserve exceeded the memory available for this session. Larger, topology-changing, painted and textured history cases are unmeasured by this experiment.

A full position before-image still exists during capture and finding changes still scans every vertex on release. The patch handles positions only. Its retained allocation excludes the vector header, allocator overhead, edit settings and all other channels. The application history estimate covers the whole document and settings. Process RSS includes both histories, fixture construction and a reference mesh. These quantities must be budgeted separately.

## Regional HD editing and next work

In the documented **HD Geometry** workflow the artist selects an area under the cursor to edit at high detail, leaves the region to see the coarser model, and can render the complete detail. The active area's size depends on the selected level and available memory. **SpotLight** projects image detail during painting or sculpting. Blender's multiresolution modifier separately selects sculpt, viewport and render levels. These manuals describe user-visible behaviour. They do not establish how the vendors store or page geometry internally. [HD editing controls](https://help.maxon.net/zbr/en-us/Content/html/reference-guide/tool/polymesh/geometry-hd/geometry-hd.html) · [HD workflow](https://help.maxon.net/zbr/en-us/Content/html/user-guide/3d-modeling/modeling-basics/subdivision-levels/hd-geometry/hd-geometry.html) · [SpotLight](https://help.maxon.net/zbr/en-us/Content/html/user-guide/3d-modeling/painting-your-model/spotlight/spotlight.html) · [Multiresolution](https://docs.blender.org/manual/en/5.4/modeling/modifiers/generate/multiresolution.html)

| Stage | Experiment and decision |
| --- | --- |
| 1. Capture local edits | Extend [#112](https://github.com/dustyghost/clay-form/issues/112) and feed [#110](https://github.com/dustyghost/clay-form/issues/110) with a change list recorded at mutation time. Capture each old value once across dabs and symmetry copies. Compare against full snapshots for sparse, broad and cancelled edits. Keep full capture for topology changes. Measure first dab, release, retained bytes and peak RSS with the same strokes. Require exact Undo/Redo, mask and paint parity, redo branching, eviction and diagnostic export before using it in the editor. |
| 2. Reuse the same regions for display | Use the changed indices from stage 1, including the neighbouring faces needed for normals. Preserve UV seams, crease fans, material channels and hidden neighbours. Combine edits across skipped frames. Exercise fan changes in [#109](https://github.com/dustyghost/clay-form/issues/109), camera-independent spatial queries in [#111](https://github.com/dustyghost/clay-form/issues/111), and textured drawing in [#113](https://github.com/dustyghost/clay-form/issues/113). Visibility culling reduces draw work while the editable mesh and history can remain fully resident. |
| 3. Gate regional residency on evidence | In [#114](https://github.com/dustyghost/clay-form/issues/114), test an artist asset, multiple models and mixed edit sessions. If active-region memory is still the limiting cost, compare a lower-resolution overview plus one editable high-detail region with fully resident geometry. A chunked or disk-backed version must reconcile shared boundaries and mirrored edits, survive eviction and cancelled loads, and reopen and export all detail including unloaded regions. Measure page-in stalls and storage bytes before selecting a format. |

**Proposed interactive target.** On the i7-7500U / HD 620, evaluate the 2.4-million-triangle fixture at a recorded desktop resolution. Aim for p95 pointer-to-visible-dab within 50 ms and pen-up within 100 ms, across at least 100 strokes including cold starts. Report orbit separately against a 33 ms frame budget. Begin with the historical 3,072 MiB process RSS guard and a 768 MiB system reserve. Count missed targets and guard stops. A sustained 6–12 million triangle run needs a machine with headroom and the same integrity checks.

## Reproduce the position experiment

Build once from the repository root. On Linux, run one fresh process per level and repeat the loop three times. The example uses the default application capacity. The Linux preflight below requires 1,280 MiB available before each case, allowing a 512 MiB margin above the child’s 768 MiB address-space ceiling. The recorded run also stopped on a fall below 512 MiB available.

```sh
cargo test --locked --release -j 1 --example stroke_history_profile
cargo build --locked --release -j 1 --example stroke_history_profile
for trial in 1 2 3; do
  for level in 1 2 3; do
    awk '/^MemAvailable:/ { exit ($2 < 1310720) }' /proc/meminfo || exit 1
    timeout 125s prlimit --as=805306368 --cpu=120 -- \
      target/release/examples/stroke_history_profile "$level" || exit 1
  done
done
```
