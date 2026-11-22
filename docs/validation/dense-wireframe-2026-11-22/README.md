# Visible wireframe on the default sphere

On the HP the slowdown appeared when subdividing the default sphere with Visible wireframe enabled. The timing comparison measures the first wireframe draw at the resulting density, seven orbit frames and one displayed Standard brush dab. A separate GPU regression keeps Visible wireframe enabled during subdivision and when switching back and forth between levels. Three subdivisions produce 301,056 quad polygons and 602,112 rendering triangles. A fourth produces 1,204,224 quads and 2,408,448 triangles and required a higher mesh budget for these runs. After the measurements, the application defaults increased to 1,500,000 vertices / 3,000,000 triangles, permitting that fourth subdivision. Existing saved settings require Reset mesh limits and a restart to adopt the new defaults.

The previous renderer collected edge positions, both adjacent face normals and colours on the CPU. It uploaded separate line lists for polygon edges and all triangle edges, then drew the requested list over the surface. Displaying a deformed mesh invalidated both lists. On the three-subdivision sphere these two GPU buffers alone used 137.81 MiB. At four subdivisions they used 551.25 MiB.

For dense basic surfaces in Visible mode, the new fragment shader uses the distance from each pixel to the triangle edges to draw an antialiased line as part of surface shading. A validated pair of triangles supplies each quad. The shader omits their shared diagonal unless Show triangle edges is on. An all-triangle surface uses all three edges. The original geometry, silhouette depth test, masks and brush preview still supply the surface. The shader needs zero additional edge-buffer bytes. [Single-pass wireframe rendering technique](https://orbit.dtu.dk/en/publications/single-pass-wireframe-rendering/)

Front and Transparent modes retain the explicit edges, as do coloured seams and creases, extended or textured materials, translucent backs, split GPU buffers and irregular polygon layouts. Explicit rendering now builds just the requested edge list. The surface path starts at 32,768 displayed triangles. Subdivision triangles must follow the validated quad order for polygon display. The renderer checks this on demand and reuses the answer while the triangle and quad arrays stay the same.

## Measured timings

The table gives milliseconds from CPU request to completed offscreen draw at 800 × 600. The HP and EPYC runs used two llvmpipe rendering threads. The Apple arm64 runner used a Metal paravirtual integrated GPU. Each arm uses a fresh renderer. The first frame includes initial target and driver work. Orbit is the median of seven changing views. The final sample includes one local Standard dab, live display preparation, upload and draw. The solid row is a control. First-frame and dab values are single observations per arm. All individual orbit samples are in the linked JSON.

| Host and source triangles | Display | First frame, ms | Warm orbit median, ms | Displayed sculpt dab, ms |
| --- | --- | ---: | ---: | ---: |
| HP i7-7500U, 602,112 | Solid | 638 | 115 | 175 |
| HP i7-7500U, 602,112 | Previous wireframe | 1,735 | 252 | 1,290 |
| HP i7-7500U, 602,112 | Surface wireframe | 624 | 170 | 198 |
| Apple Metal, 602,112 | Solid | 184 | 9.8 | 93 |
| Apple Metal, 602,112 | Previous wireframe | 1,093 | 13.6 | 1,105 |
| Apple Metal, 602,112 | Surface wireframe | 186 | 13.1 | 76 |
| EPYC 9V74, 602,112 | Solid | 719 | 88 | 105 |
| EPYC 9V74, 602,112 | Previous wireframe | 1,222 | 173 | 1,170 |
| EPYC 9V74, 602,112 | Surface wireframe | 403 | 103 | 128 |
| EPYC 9V74, 2,408,448 | Solid | 241 | 192 | 330 |
| EPYC 9V74, 2,408,448 | Previous wireframe | 5,699 | 507 | 5,528 |
| EPYC 9V74, 2,408,448 | Surface wireframe | 321 | 297 | 429 |

The HP run used Mesa software Vulkan on the laptop CPU. Direct Intel HD 620 and physical pen-to-screen timings remain unmeasured. Its first-frame samples exclude the one-off ordered-quad check. The hosted first-frame samples include that check. GPU completion excludes desktop presentation. The HP process peaked at 1,010 MiB, with at least 2,084 MiB of system RAM available. The hosted four-subdivision process peaked at 2,838 MiB. These peaks cover all three arms in sequence. [HP samples](hp-level-3.json) · [Hosted three subdivisions](host-level-3.json) · [Hosted four subdivisions](host-level-4.json) · [Guarded run](https://github.com/dustyghost/clay-form/actions/runs/45011983709)

On Metal at three subdivisions, warmed orbit times were similar. The first visible frame and the first displayed sculpt dab showed the largest gains. The first dab landed below the single solid control sample. Repeated dab timings would be needed to distinguish those two values reliably. [Metal samples](metal-level-3.json) · [Metal benchmark run](https://github.com/dustyghost/clay-form/actions/runs/45012736438)

The matched HP images show the same sphere at the seventh orbit angle. Thin wires remain visible across the central quads. Subpixel lines toward the silhouette blend into the surface. [Previous wireframe](visible_edges.png) · [Surface wireframe](visible_surface.png)

## Reproduce

Build the optimised library tests and use the executable printed by Cargo in the second command. Use a fresh output directory. The following budgets are for the three-subdivision laptop run. The hosted four-subdivision case used an 8,000 MiB RSS ceiling and a 1,200 MiB system reserve.

```sh
cargo test --release --locked --lib --no-run -j 1
CLAY_GPU_SOFTWARE=1 LP_NUM_THREADS=2 python3 scripts/profile-capacity-followups.py "$test_binary" wire target/wire-level-3 --level 3 --max-rss-mib 1800 --min-available-mib 640
```
