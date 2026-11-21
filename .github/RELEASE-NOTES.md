**SketchOtter Alpha 5 (0.1.0-alpha.5)**

Clay Form is now SketchOtter. The teal clay otter appears on the welcome screen and the desktop icon. Existing `.clay` projects, brushes, preferences and recovery files work in the same locations.

Launch `sketchotter` on Linux, `sketchotter.exe` on Windows or `SketchOtter.app` on macOS. Linux packages also accept the previous `clay-form` command.

Download the archive for your operating system and CPU. `aarch64` means ARM64;
Apple Silicon Macs use `macos-aarch64`. Linux archives target glibc 2.35+
desktops with a working Vulkan driver. They are not Alpine/musl builds.

Windows requires the Microsoft Visual C++ v14 runtime for the selected
architecture. Use [Microsoft's supported redistributable installer](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist/)
if it is not already installed. The runtime is not bundled with SketchOtter.

**Corresponding source:** the `source.tar.gz` asset contains the exact app
source, build scripts, lockfile and vendored Cargo dependencies for these
binaries. See its README and pinned Rust toolchain to rebuild. The compiler
and operating-system SDKs are installed separately. Licence notices are
included in each binary archive and in the app's About window. `SHA256SUMS`
contains archive hashes.

Testing to date includes maintainer use on Arch Linux, automated CPU and headless Vulkan checks, and recorded visual/interaction reviews of modeling, snapping, bevels, thickness, surface arrays, saved cameras, lights and workspace controls. Guided retopology has recorded solver comparisons and preview reviews. Windows x86-64 and ARM64 packaging checks also exercise pen routing and brush selection. Desktop, GPU and physical tablet compatibility still needs broader user testing, particularly on Windows and macOS; a native build alone does not establish runtime compatibility.

Windows packages are unsigned. macOS packages have an ad-hoc signature and are not notarized. Operating systems may show download or launch warnings.

Meshes default to 500,000 vertices and 1,000,000 triangles. Configurable limits extend to 16 million vertices and 32 million triangles per mesh, but these are safety ceilings, not a performance guarantee. Large scenes can become slow or exceed available memory well below them. Live instances have separate project and expanded-geometry budgets; make an array editable before exporting its result. Preserve surfaces requires closed, consistently oriented solids, and local joins still need wider production-sculpt testing. Please include your OS, GPU and input device when reporting problems.
