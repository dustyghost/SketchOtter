**Alpha 4 — 0.1.0-alpha.4.** An early testing release, with features and file
formats still evolving. Keep backups of important projects.
The startup notice can be dismissed for the current version; new prereleases
show it again.

Changes since Alpha 3:

- **More modeling tools.** Build faces from selected points or edges, connect vertices, bridge partial boundary chains, and edit face regions. Create detached copies, snap and weld moved or extruded components, and grow selections along connected geometry. Chamfer and rounded bevel tools work on corners and selected edge chains or loops.
- **Protected edits and thickness fixes.** Modeling transforms and slides respect protected vertices; loop edits stop at protected areas. Thickness offers Inward, Outward and Both directions, with fixes for distorted open surfaces and thin sliced shells. Extrusion preserves valid cap UVs and supports switching between attached and detached geometry during a drag.
- **Surface arrays.** Repeat selected faces over a surface, choose whether to keep or replace the source region, and adjust rotation per copy. Per-copy edits survive array updates. Preview the result before applying it or making it editable.
- **Saved cameras and named lights.** Save and restore named viewport views, confirm camera overwrites, switch projection and reset roll independently. Name lights, toggle them without losing their strength, and see which side of the current view they illuminate.
- **A clearer, customizable workspace.** Search for actions and settings, arrange menus and controls, and use Zen mode with an operation strip that keeps the active tool visible. Modeling quick menus support dragging, Keep open, shared settings and presets. Brush labels, Paint headings and shared Slice controls are clearer; switching brushes offers recovery choices for pending edits.
- **Experimental guided quad retopology.** Preview a fresh quad layout with path direction guides, painted density, sharp-bend protection and optional layout symmetry. Apply the result as a new model while retaining the source. The target quad count is approximate; coarse fingers and uneven poles remain limitations.
- **Path Brush workflow.** Draw and edit surface or free paths from the Brush Library, offset them from the surface, round corners and use saved paths for brush strokes, assemblies or retopology guidance.

**Project compatibility:** Alpha 4 reads native project formats 1–7 and saves
format 7. Alpha 3 and earlier cannot open format 7 projects. Keep a separate
backup before saving an older project in Alpha 4.

Clay Form is free and open source under GPLv3. Your models, textures, renders
and other creations remain yours to use, sell, license or keep private.

Download the archive for your operating system and CPU. `aarch64` means ARM64;
Apple Silicon Macs use `macos-aarch64`. Linux archives target glibc 2.35+
desktops with a working Vulkan driver. They are not Alpine/musl builds.

Windows requires the Microsoft Visual C++ v14 runtime for the selected
architecture. Use [Microsoft's supported redistributable installer](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist/)
if it is not already installed. The runtime is not bundled with Clay Form.

**Corresponding source:** the `source.tar.gz` asset contains the exact app
source, build scripts, lockfile and vendored Cargo dependencies for these
binaries. See its README and pinned Rust toolchain to rebuild. The compiler
and operating-system SDKs are installed separately. Licence notices are
included in each binary archive and in the app's About window. `SHA256SUMS`
contains archive hashes.

Testing to date includes maintainer use on Arch Linux, automated CPU and headless Vulkan checks, and recorded visual/interaction reviews of modeling, snapping, bevels, thickness, surface arrays, saved cameras, lights and workspace controls. Guided retopology has recorded solver comparisons and preview reviews. Windows x86-64 and ARM64 packaging checks also exercise pen routing and brush selection. Desktop, GPU and physical tablet compatibility still needs broader user testing, particularly on Windows and macOS; a native build alone does not establish runtime compatibility.

Windows packages are unsigned. macOS packages have an ad-hoc signature and are not notarized. Operating systems may show download or launch warnings.

Meshes default to 500,000 vertices and 1,000,000 triangles. Configurable limits extend to 16 million vertices and 32 million triangles per mesh, but these are safety ceilings, not a performance guarantee. Large scenes can become slow or exceed available memory well below them. Live instances have separate project and expanded-geometry budgets; make an array editable before exporting its result. Preserve surfaces requires closed, consistently oriented solids, and local joins still need wider production-sculpt testing. Please include your OS, GPU and input device when reporting problems.

[Changes since Alpha 3](https://github.com/dustyghost/clay-form/compare/v0.1.0-alpha.3...v0.1.0-alpha.4).
