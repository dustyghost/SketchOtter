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

[602k samples](release-3.json.gz) · [2.4m samples](release-4.json.gz) · [Paired release run](https://github.com/dustyghost/clay-form/actions/runs/44959959909)

An additional open star placed 4,096 faces at one vertex with an explicit crease. At 2.4m source triangles plus the star, first release took 169.9 / 93.8 ms on an EPYC 9V45. This control also compared every emitted normal and corner index across accumulated and broad edits. GPU captures compare packed data, coverage and pixels against a fresh canonical reconstruction with ordinary and forced-split buffers. [High-valence 602k](high-valence-3.json.gz) · [High-valence 2.4m](high-valence-4.json.gz) · [High-valence run](https://github.com/dustyghost/clay-form/actions/runs/44961724331)

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

A bounded, document-owned edit history carries changed positions, mask values and complete vertex materials through the dense fixed-topology path. Stamps include document identity and revision. Unknown mutations, topology changes, stale frames and oversized regions take the full path. The journal retains at most eight edits and 16,384 indices per edit. A sparse normal update returns the affected fans in canonical order. The uploader uses the fan list only when its packed bytes belong to the exact previous display geometry. Split buffers keep the existing packing path. Older immutable snapshots continue to own their payloads.

Each cell gives reference / candidate median milliseconds for live display preparation, GPU upload, submission and completion. The comparison includes eight frames per brush, two dabs before selected frames, and successive Standard, Clay, Mask and Paint strokes. The data includes per-frame allocation, vertex packing, changed buffers, initial setup, release and image checks. CPU brush execution is timed separately.

| Source triangles and brush area | Standard | Clay | Mask | Paint |
| --- | ---: | ---: | ---: | ---: |
| 602k, local | 13.57 / 3.04 | 13.43 / 3.06 | 11.93 / 1.75 | 11.38 / 4.17 |
| 602k, broad | 17.34 / 17.45 | 17.07 / 17.27 | 12.43 / 12.43 | 12.28 / 12.49 |
| 2.4m, local | 56.29 / 16.85 | 59.56 / 11.37 | 49.11 / 6.35 | 88.96 / 58.41 |
| 2.4m, broad | 75.89 / 75.22 | 73.21 / 74.98 | 50.69 / 50.86 | 90.15 / 90.10 |

The 2.4m process peaked at 3,002 MiB across both arms and both brush areas. Local snapshots retained about 321 MiB of display payload. Changed immutable arrays are still copied. Material-heavy Paint has the largest remaining local display cost. Broad edits use the original full comparison and have comparable latencies. Sampled candidate and reference captures agreed after the first and eighth frames of each brush.

[602k samples](display-3.json.gz) · [2.4m samples](display-4.json.gz) · [End-to-end display run](https://github.com/dustyghost/clay-form/actions/runs/44960306091)

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

A fresh guarded process reached **9,633,792 active triangles and 4,816,898 vertices** on a synthetic sphere, with another 3,208,128 triangles in its retained subdivision stack. It validated the source, applied a Standard stroke, restored the exact hashes with Undo/Redo, built the display, uploaded and drew at 800 × 600 through software Vulkan. Peak RSS was **5.57 GiB** with at least **8.61 GiB** system memory available. The final subdivision took 14.7 s, document validation 25.3 s, CPU display construction 4.2 s and initial upload 2.7 s. The 0.98 s final interval includes draw, GPU completion and pixel readback. This establishes a bounded functional result at 9.6m. The real-asset session provides the evidence for repeated editing. A 12m interactive workload and a physical tablet-to-monitor latency measurement remain outside this run. [Stage and memory checkpoints](boundary-3.json.gz) · [Session and boundary measurements](https://github.com/dustyghost/clay-form/actions/runs/44961238925)

## Reproduction

Build the optimised library tests. Use the executable path printed by Cargo. Choose a fresh output directory for each run. Budget figures below are for the 15.33 GiB host. The guard checks available RAM before launch, then checks process RSS, system headroom and elapsed time while the workload runs.

```sh
cargo test --locked --release --lib --no-run
python3 scripts/profile-capacity-followups.py "$test_binary" display target/compare-display --level 4 --max-rss-mib 8000 --min-available-mib 1200
python3 scripts/prepare-capacity-asset.py target/bottle
python3 scripts/profile-capacity-followups.py "$test_binary" session target/mixed-session --asset target/bottle --level 3 --max-rss-mib 8000 --min-available-mib 1200
```

The other probes are `release`, `visual`, `picking`, `drawing` and `boundary`. Each uses a fresh process. The boundary probe sets capacity only for that process and checkpoints each completed stage before the next allocation.
