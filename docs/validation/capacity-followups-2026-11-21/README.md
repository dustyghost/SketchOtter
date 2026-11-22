# Dense sculpting follow-ups

These paired comparisons use identical source data and the same release executable for each reference and candidate. They retain the current application capacity defaults. The primary Linux shading, picking and drawing comparisons used a Xeon 6975P-C with 15.33 GiB RAM. Display updates and mixed sessions used an EPYC 7763 with 15.61 GiB. Guarded probes used an 8,000 MiB process budget, 1,200 MiB system reserve and a 30-minute timeout. Raw samples for each resolution are gzip-compressed JSON. The environment records include the executable digest, commit, compiler, sampled process high-water RSS and minimum available RAM. [Environment records](environments.json)

## Canonical shading on release (#109)

The new release kernel stores a 32-bit parent per source corner and accumulates into the emitted fans. Every union selects the earliest source corner as root. By visiting corners in source order, the algorithm can resolve each parent to its final fan immediately. It evaluates all eligible source edges after each edit, including hidden faces. It reuses reference corner angles only for unchanged face positions. There is one ordered floating-point sum per fan and a final normalisation pass. Temporary storage for the old corner-sized normal-sum array is eliminated.

The table shows reference / compact kernel times in milliseconds at 2,408,448 source triangles. The reference topology and angle cache are retained across five edits. The fourth local edit follows three prior releases. The broad edit changes the full surface. These are isolated release kernel times. End-of-stroke display and GPU costs are measured in the changed-region workload below.

| Surface | First release | Fourth local release | Broad edit |
| --- | ---: | ---: | ---: |
| Smooth | 189.5 / 134.6 | 182.6 / 144.9 | 265.9 / 224.9 |
| Masked organic | 194.2 / 130.9 | 192.6 / 153.7 | 279.9 / 226.8 |
| Hidden organic | 180.3 / 130.1 | 183.7 / 137.3 | 258.9 / 227.7 |
| Sharpness crossing | 188.0 / 149.4 | 178.0 / 138.3 | 280.9 / 212.4 |

At 602,112 triangles the first masked release fell from 36.8 to 22.7 ms and the hidden release from 36.3 to 23.2 ms. The sharpness control alternates fan counts between 1,205,290 and 1,204,226 at the larger size. Both implementations produced the same source-to-fan mapping and normal bits on each release. The all-edge pass avoids a discovery scan followed by a second full fallback when a mask boundary, sharp edge or broad edit changes connectivity. The retained angle cache remains the same size. The original reconstruction handles indices above the compact representation.

On the first large smooth release the reference spent 37.91 ms joining fans, 57.66 ms accumulating weighted corners and 40.72 ms emitting the result. The candidate spent 42.15 ms joining, 53.03 ms accumulating and remapping together, and 2.19 ms normalising. The faster release came principally from combining accumulation and emission. On this 64-bit machine, the 2.4m case reduces the parent array from 55.13 to 27.56 MiB and eliminates an 82.69 MiB array of temporary normal sums. The retained angle cache occupied 41.34 MiB in both arms. Across the paired controls the process peaked at 204.43 MiB at 602k and 742.95 MiB at 2.4m. Those peaks include both implementations in one process.

[602k samples](release-3.json.gz) · [2.4m samples](release-4.json.gz) · [Paired release run](https://github.com/dustyghost/clay-form/actions/runs/44959959909)

An additional open star placed 4,096 faces at one vertex with an explicit crease. At 2.4m source triangles plus the star, first release took 169.9 / 93.8 ms on an EPYC 9V45. This control also compared every emitted normal and corner index across accumulated and broad edits. GPU captures compare packed data, coverage and pixels against a fresh canonical reconstruction with ordinary and forced-split buffers. [High-valence 602k](high-valence-3.json.gz) · [High-valence 2.4m](high-valence-4.json.gz) · [High-valence run](https://github.com/dustyghost/clay-form/actions/runs/44961724331)

In the release captures, the smooth sphere retains the local brush relief, the organic surface keeps its masked transition, and the rear view of the partly hidden surface exposes a clean crescent-shaped boundary. Ordinary and forced-split buffers each matched the freshly rebuilt image pixel for pixel, including all 7,229 covered pixels in the rear view. [Smooth sphere](release-sphere.png) · [Masked organic surface](release-organic.png) · [Hidden boundary](release-hidden.png) · [Coverage and pixel counts](visual-3.json.gz)

## Camera picking (#111)

A camera move can now reuse world-space boxes for consecutive groups of 128 source triangles. The boxes are built lazily on the next view of an immutable mesh. Each view projects those boxes and keeps source order within each candidate group, including equal-depth faces. A group crossing the near plane stays eligible. Base picking and the rendered preview keep separate scene caches. A change to positions or topology invalidates the corresponding bounds.

| Surface and projection | Ten views before / after, ms | Largest single complete view before / after, ms |
| --- | ---: | ---: |
| 602k, orthographic | 68.2 / 33.6 | 8.3 / 11.5 |
| 602k, perspective | 62.1 / 32.7 | 7.3 / 7.3 |
| 2.4m, orthographic | 383.1 / 190.3 | 64.8 / 59.1 |
| 2.4m, perspective | 394.1 / 206.7 | 61.8 / 59.2 |
| 2.4m, orthographic after sculpting | 374.0 / 172.3 | 58.2 / 52.7 |
| 2.4m, perspective after sculpting | 378.3 / 205.6 | 55.3 / 54.9 |

At 2.4m the median orthographic first hit fell from 7.07 to 0.56 ms and the next distinct query from 17.09 to 0.03 ms. Camera preparation, the first-ever view and the first installation of the bounds still cost time. The 602k orthographic control shows a 3.2 ms increase in its worst frame. Separate samples cover first hits and misses, the second distinct query, repeated queries, footprint preparation and full orbit-pointer latency. A regression compares selection and tie ordering with exhaustive traversal on coincident and hidden models, both projections, near-plane crossings and history changes. Published sampling uses ten successive camera positions per arm and projection, before and after sculpting.

[602k samples](picking-3.json.gz) · [2.4m samples](picking-4.json.gz) · [First orbit comparison](https://github.com/dustyghost/clay-form/actions/runs/44959190339)

A separate sparse deformation compared rebuilding the world boxes with maintaining a reverse vertex-to-block index. At 2.4m, 6,267 changed vertices touched 135 of 11,315 blocks. Refitting took 0.29 ms against 13.30 ms for rebuilding, but constructing the reverse map cost 16.18 ms and retained 8.41 MiB. Scanning positions to discover the edits took another 2.62 ms. At 602k, refitting 36 blocks took 0.09 ms against 3.32 ms, with 4.61 ms to build a 2.07 MiB reverse map. Refit bounds matched the rebuild exactly. The shipped path retains the boxes alone (265 KiB at 2.4m), discards them when geometry changes, and rebuilds on a subsequent camera reuse. The larger map would need repeated camera reuse after edits to recover its setup cost. [602k refit](refit-3.json.gz) · [2.4m refit](refit-4.json.gz) · [Refit run](https://github.com/dustyghost/clay-form/actions/runs/44961868806)

## Live display and GPU uploads (#112)

A bounded, document-owned edit history carries changed positions, mask values and complete vertex materials through the dense fixed-topology path. Stamps include document identity and revision. Unknown mutations, topology changes, stale frames and oversized regions take the full path. The journal retains at most eight edits and 16,384 indices per edit. A sparse normal update returns the affected fans in canonical order. The uploader uses the fan list only when its packed bytes belong to the exact previous display geometry. When Paint or Mask is the first edit, the uploader uses the existing packer until sculpting builds the shared corner adjacency. Split buffers keep the existing packing path. Older immutable snapshots continue to own their payloads.

Each cell gives reference / candidate median milliseconds for live display preparation, GPU upload, submission and completion. The comparison includes eight frames per brush, two dabs before selected frames, and successive Standard, Clay, Mask and Paint strokes. The data includes per-frame allocation, vertex packing, changed buffers, initial setup, release and image checks. CPU brush execution is timed separately.

| Source triangles and brush area | Standard | Clay | Mask | Paint |
| --- | ---: | ---: | ---: | ---: |
| 602k, local | 13.57 / 3.04 | 13.43 / 3.06 | 11.93 / 1.75 | 11.38 / 4.17 |
| 602k, broad | 17.34 / 17.45 | 17.07 / 17.27 | 12.43 / 12.43 | 12.28 / 12.49 |
| 2.4m, local | 56.29 / 16.85 | 59.56 / 11.37 | 49.11 / 6.35 | 88.96 / 58.41 |
| 2.4m, broad | 75.89 / 75.22 | 73.21 / 74.98 | 50.69 / 50.86 | 90.15 / 90.10 |

The 2.4m process peaked at 3,002 MiB across both arms and both brush areas. Local snapshots retained about 321 MiB of display payload. Changed immutable arrays are still copied. Material-heavy Paint has the largest remaining local display cost. Broad edits use the original full comparison and have comparable latencies. Sampled candidate and reference captures agreed after the first and eighth frames of each brush.

[602k samples](display-3.json.gz) · [2.4m samples](display-4.json.gz) · [End-to-end display run](https://github.com/dustyghost/clay-form/actions/runs/44960306091)

A second comparison includes the brush and disables the entire edit journal in the reference arm. The candidate records changed mask indices at the point of writing. This removes a mask copy and a second discovery scan used by an earlier prototype. On a Xeon 6975P-C, at 2.4m the broad Mask median for the earlier prototype was 107.67 ms against 94.58 ms in its reference. The final direct-write comparison measured 93.38 ms against 90.79 ms.

| Brush input through completed upload, reference / candidate medians in ms | Standard | Clay | Mask | Paint |
| --- | ---: | ---: | ---: | ---: |
| 602k, local | 20.58 / 8.38 | 21.87 / 9.61 | 16.58 / 5.19 | 16.62 / 8.09 |
| 602k, broad | 28.02 / 28.43 | 29.25 / 30.25 | 18.42 / 19.50 | 49.65 / 56.72 |
| 2.4m, local | 99.85 / 48.17 | 114.96 / 71.34 | 75.85 / 34.36 | 88.29 / 55.06 |
| 2.4m, broad | 138.58 / 146.39 | 172.90 / 190.19 | 90.79 / 93.38 | 311.45 / 310.05 |

All four brushes benefited locally at both densities. Broad dabs still pay for edits and full-array processing once the journal budget is exceeded. In this run the largest broad increases were 7.07 ms for Paint at 602k and 17.29 ms for Clay at 2.4m. This comparison records eight live frames per brush per arm, including skipped dabs, with release between brushes. The first and last frame of every brush matched the reference pixels and fan counts. A separate regression compares actual uploaded GPU bytes and rendered pixels across ordinary and forced-split buffers, mixed strokes, an unseen final dab, Undo/Redo and model switching. [Final 602k](input-through-upload-3.json.gz) · [Final 2.4m](input-through-upload-4.json.gz) · [Final comparison](https://github.com/dustyghost/clay-form/actions/runs/44963229938) · [Earlier mask-tracking prototype](initial-mask-tracking-4.json.gz)

## Dense material drawing (#113)

Untextured base-colour, roughness and metallic inspection can share indexed vertices even with the 77-float extended material layout. Lit extended layers need their original per-corner shader inputs. UV seams, sampled maps, lit categorical face groups and UV inspection stay on the general pipeline.

The software Vulkan comparison used a 256 × 192 framebuffer. Each mode was rendered in alternating order four times. The following medians use the three draws after first use. The raw samples separate pipeline setup, CPU display preparation, initial upload, encoding, submission and completion.

| 2.4m case on lavapipe | General / selected drawing, ms |
| --- | ---: |
| Basic lit | 196.0 / 99.2 |
| Extended lit, general fallback | 220.7 / 213.9 |
| Extended colour inspection | 203.3 / 104.2 |
| Extended roughness inspection | 225.4 / 116.1 |
| Extended metallic inspection | 223.4 / 122.4 |
| UV textured lit, general fallback | 194.5 / 182.4 |

At 2.4m the basic and extended mesh buffers occupied 160.1 and 319.9 MiB respectively. The extended layout split across ten draws. Both paths shared their geometry allocations in the comparison. All 48 Linux size/layout/mode combinations compared alpha, coverage and RGB against the original pipeline. The dense comparison allows one 8-bit rounding step in colour only, matching the previous test. All observed Linux pixels were identical. Normal and forced-split regressions additionally cover translucent backs, wire modes, masks, brush alpha, symmetry and perspective.

[602k samples](drawing-3.json.gz) · [2.4m samples](drawing-4.json.gz) · [Linux drawing run](https://github.com/dustyghost/clay-form/actions/runs/44959838775) · [Original HD 620 basic draw measurements](../indexed-surface-draw-2026-10-31/README.md)

The Metal comparison on an Apple Silicon VM identified a unified-memory integrated adapter. At 2.4m, extended inspection medians were 13.27 / 9.70 ms for colour, 18.68 / 9.46 ms for roughness and 18.00 / 9.87 ms for metallic. At 602k, the short colour comparison was 8.23 / 9.92 ms while roughness and metallic were close. On the Intel Mac VM, wgpu reported a discrete paravirtual adapter. At 602k its three comparisons were 21.55 / 9.75, 24.59 / 5.69 and 21.96 / 6.51 ms. Its recommended device budget was 1 GiB, so the larger case was confined to the unified-memory device. The environments identify guest-visible devices and the working-set limits reported by Metal. [Integrated 602k](metalarm64-3.json.gz) · [Integrated 2.4m](metalarm64-4.json.gz) · [Discrete 602k](metalx86_64-3.json.gz) · [Metal run](https://github.com/dustyghost/clay-form/actions/runs/44960865971)

A repeat on the integrated device used twelve warmed draws per pipeline per case. At 602k its medians were 9.35 / 9.47, 9.52 / 9.57 and 9.50 / 9.14 ms, with overlapping ranges. At 2.4m the same three channels measured 18.11 / 9.67, 17.28 / 9.75 and 17.66 / 9.85 ms. The smaller integrated case provides no reliable speedup. The larger integrated case and both the discrete and software devices support retaining the extension. [Repeated integrated 602k](metal-repeat-3.json.gz) · [Repeated integrated 2.4m](metal-repeat-4.json.gz) · [Twelve warm samples per case](https://github.com/dustyghost/clay-form/actions/runs/44962202270)

## Mixed sessions and the upper boundary (#114)

The pinned Water Bottle from Microsoft is distributed under CC0 in the Khronos sample collection. Its original 2,549 position records and 4,510 triangles become 2,385 exactly welded positions with independent face-corner UVs. The workload normalises its size, preserves each UV seam when interpolating the child triangles, and uses a 128 × 128 copy of the original colour map. Two offset submodels share the texture. The script mixes symmetric sculpt, vertex Paint, mask and UV-image Paint, includes hidden faces, cancels image edits, changes subdivision topology and verifies Undo/Redo as well as save/reopen. [Asset and licence](https://github.com/KhronosGroup/glTF-Sample-Assets/blob/4995a638c96dbc94376d3d6164e70fee3ac7f7a2/Models/WaterBottle/README.md) · [Pinned asset preparation](../../../scripts/prepare-capacity-asset.py)

| Two-model 96-stroke session on the EPYC 7763 | Smaller asset | Denser asset |
| --- | ---: | ---: |
| Active vertices per model | 9,282 | 145,374 |
| Source polygons / triangles per model | 18,040 | 288,640 |
| Total rendered triangles | 32,366 | 515,446 |
| Peak retained history entries | 24 | 15 |
| Peak estimated history | 102.28 MiB | 1,020.80 MiB |
| Peak display payload / GPU mesh buffers | 5.89 / 3.20 MiB | 91.04 / 50.67 MiB |
| Whole-process sampled high-water RSS | 314.5 MiB | 1,952.6 MiB |
| Two dabs through live and release uploads, p50 / p95 | 27.4 / 34.1 ms | 489.2 / 612.2 ms |
| Two dabs through live and release uploads, slowest | 46.0 ms | 646.7 ms |

The denser model retained 4,510, 18,040 and 72,160 source triangles in its three lower levels, per model. The 1 GiB history budget evicted entries before reaching the 24-edit default. At the end of the denser session, RSS was about 1,834 MiB with caches and document live and 1,662 MiB after dropping both. Allocator and driver memory can remain resident. GPU mesh buffer figures sum actual vertex, index, colour and UV buffer sizes. Texture atlas, swapchain, driver bookkeeping and transient allocations are outside that figure. The X11 presentation timings are a separate workload. [Small session samples](session-3.json.gz) · [Dense session samples](session-4.json.gz)

The longer run on the Xeon lasted 352 s. It applied **1,024 strokes** and **2,048 dabs** to the denser bottle scene, switching the active bottle every 32 strokes and making 512 strokes on each. The revision counter detected actual changes on 255 of 256 Standard strokes and all 256 strokes in each of vertex Paint, Mask and image Paint. This count includes 64 deliberately cancelled image edits. Each cancellation restored its exact before-image. Another 64 image edits passed exact Undo/Redo, including the shared texture on both models. Mid-session subdivision cancelled during execution, and Undo restored a replaced subdivision level. Saving at the midpoint and end reproduced the complete project on reopen.

| 1,024-stroke session | Measured value |
| --- | ---: |
| Two dabs through live and release uploads, p50 / p95 / p99 / maximum | 376.90 / 468.05 / 483.86 / 518.46 ms |
| Live CPU display preparation, p50 / p95 (2,048 samples) | 153.64 / 164.84 ms |
| Live GPU upload and wait, p50 / p95 (2,048 samples) | 15.94 / 22.05 ms |
| Peak estimated document geometry / retained history | 81.66 / 1,020.80 MiB |
| Peak display cache / GPU mesh buffers | 92.14 / 50.68 MiB |
| Peak entries / cumulative history evictions | 15 / 980 |
| Whole-process RSS high-water / minimum system memory available | 2,123.23 MiB / 12.40 GiB |
| Median RSS for strokes 1–64 / 513–576 / 961–1024 | 1,821.20 / 1,920.82 / 1,987.02 MiB |
| RSS before / after dropping the renderer and document | 2,009.93 / 1,895.31 MiB |

These memory counters have overlapping ownership and must not be summed. RSS rose between the first and last windows despite bounded history, and the allocator and headless device remained live after document release. This workload establishes repeatability and high-water behaviour over 1,024 strokes. The largest measured live cost was rebuilding the textured multi-model display. [Long session, all samples](long-session-4.json.gz) · [Smaller model-switching control](model-switch-3.json.gz) · [Guarded long-session run](https://github.com/dustyghost/clay-form/actions/runs/44962926403)

A fresh guarded process reached **9,633,792 active triangles and 4,816,898 vertices** on a synthetic sphere, with another 3,208,128 triangles in its retained subdivision stack. It validated the source, applied a Standard stroke, restored the exact hashes with Undo/Redo, built the display, uploaded and drew at 800 × 600 through software Vulkan. Peak RSS was **5.57 GiB** with at least **8.61 GiB** system memory available. The final subdivision took 14.7 s, document validation 25.3 s, CPU display construction 4.2 s and initial upload 2.7 s. The 0.98 s final interval includes draw, GPU completion and pixel readback. This establishes a bounded functional result at 9.6m. The real-asset session provides the evidence for repeated editing. A 12m interactive workload and a physical tablet-to-monitor latency measurement remain outside this run. [Stage and memory checkpoints](boundary-3.json.gz) · [Session and boundary measurements](https://github.com/dustyghost/clay-form/actions/runs/44961238925)

In a separate Linux allocation-failure probe, an isolated child had a 384 MiB virtual address-space ceiling, a 60-second timeout and core dumps disabled. While subdividing from 602,112 triangles the allocator refused a 64 MiB request. The child aborted with SIGABRT. The 967,602-byte checkpoint remained byte-identical and reopened to the exact project. The independent supervising document could still Undo and Redo the saved edit exactly. An actual editor process hitting this allocation failure would terminate and lose its in-memory history. The saved checkpoint is recoverable. The guard and this observed failure both weigh against raising product defaults. [Bounded refusal and project check](allocation-3.json.gz) · [Two isolated repetitions](https://github.com/dustyghost/clay-form/actions/runs/44963569082)

## Native desktop input and visible edits (#114)

The release application ran on a maximised 1920 × 1061 client inside a 1920 × 1080 X11 desktop on a AMD EPYC 7763 64-Core Processor, with Mesa llvmpipe Vulkan and two software rendering threads. Each trial injected a press and a short fixed drag. The timer stopped at the first X11 client read showing at least eight changed pixels that retained exactly the same values after release and pointer removal. The observer excluded pixels shared with the starting hover and the pre-hover image. The raw records include the last unchanged read, the first verified change, the brush target, the count of persistent pixels and the released-image digest.

| Workload | Visible trials / attempts | Median, ms | Slowest, ms | Trials over 50 ms |
| --- | ---: | ---: | ---: | ---: |
| Default sphere, 9,408 source triangles | 12 / 12 | 135.4 | 151.2 | 12 |
| Two bottles, 36,080 source triangles | 12 / 12 | 234.1 | 250.6 | 12 |
| Two bottles, 577,280 source triangles | 12 / 12 | 716.4 | 824.5 | 12 |

These are window-visible upper bounds that include event routing, brush work, rendering and X11 readback. This Xvfb and software Vulkan combination produced zero X Present completion notifications for the window. Server presentation timestamps, physical pen sampling and monitor scan-out remain unmeasured. The parent laptop target requires 100 strokes on its physical hardware. This virtual desktop supplies a separate software-rendered comparison. An earlier two-bottle run aimed near the boundary between the submodels and produced only five visible edits in the smaller fixture. The accepted runs use an exposed part of the active bottle. [sphere observations](desktop-sphere.json) · [bottle observations](desktop-bottle.json) · [dense-bottle observations](desktop-dense-bottle.json) · [Native desktop run](https://github.com/dustyghost/clay-form/actions/runs/44964402973)

## Reproduction

Build the optimised library tests. Use the executable path printed by Cargo. Choose a fresh output directory for each run. Budget figures below are for the 15.33 GiB host. The guard checks available RAM before launch, then checks process RSS, system headroom and elapsed time while the workload runs.

```sh
cargo test --locked --release --lib --no-run
python3 scripts/profile-capacity-followups.py "$test_binary" display target/compare-display --level 4 --max-rss-mib 8000 --min-available-mib 1200
python3 scripts/prepare-capacity-asset.py target/bottle
python3 scripts/profile-capacity-followups.py "$test_binary" session target/mixed-session --asset target/bottle --level 3 --max-rss-mib 8000 --min-available-mib 1200
```

Set `CLAY_SESSION_STROKES=1024` and `--level 4` to reproduce the longer two-bottle run. The other probes are `release`, `visual`, `picking`, `drawing`, `allocation` and `boundary`. Each uses a fresh process. The boundary probe sets capacity only for that process and checkpoints each completed stage before the next allocation.

For the native Linux desktop capture, build the matching executable and run the isolated X11 launcher. It uses Python 3.11 or later, python-xlib, Xvfb, xauth, Openbox, wmctrl, xdotool, x11-utils, Mesa Vulkan, libxkbcommon-x11 and the libxcb-present development headers. Each command creates its own 1920 × 1080 display and temporary settings. For a different project, set CLAY_DESKTOP_X and CLAY_DESKTOP_Y to the client-relative fractions of an exposed editable point. The guard requires 3,000 MiB for the app plus 1,200 MiB available system reserve.

```sh
cargo build --release --locked --bin sketchotter
bash scripts/profile-desktop-session.sh target/release/sketchotter target/desktop-sphere
bash scripts/profile-desktop-session.sh target/release/sketchotter target/desktop-bottle target/mixed-session/mixed-session.clay
```
