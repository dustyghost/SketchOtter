# SketchOtter migration

Clay Form became SketchOtter in Alpha 5. On 23 November 2026 the existing GitHub
repository was renamed to [dustyghost/SketchOtter](https://github.com/dustyghost/SketchOtter).
Its history, issues, pull requests, project membership and releases are retained.
Old GitHub links redirect to the same resources. The development board is now
**SketchOtter Development**.

## Source checkout

New checkouts can use a lowercase directory:

```sh
git clone https://github.com/dustyghost/SketchOtter.git sketchotter
cd sketchotter
```

For an existing checkout, update its remote before moving the directory:

```sh
git remote set-url origin https://github.com/dustyghost/SketchOtter.git
git fetch origin
```

Reopen the moved directory in your editor and update launchers or scripts that
refer to its absolute path. The application binary and Rust library are named
`sketchotter`. macOS packages use `SketchOtter.app` and the bundle identifier
`io.github.dustyghost.sketchotter`. Linux tablet rules now use the
`72-sketchotter-` filename prefix; their device-access behavior is unchanged.

## Preferences and recovery

New installations store preferences and recovery under `sketchotter` in the
platform's configuration directory. Existing installations continue using
`clay-form` until a `sketchotter` directory is present. Both types of data always
come from the same directory; the application never combines separate stores.

To migrate an existing Linux or macOS installation, close the app first. When
the configuration root contains `clay-form` and no `sketchotter` directory, move
the complete directory to `sketchotter` and create a relative symbolic link
named `clay-form` pointing to it. Older releases then read the same files through
the link. Keep recovery files and preferences together. If both directories
already contain data, review them before choosing which to use.

## Compatibility retained

- `.clay` projects and brush files keep their existing formats.
- Default brush identifiers and the GIMP UV-guide marker stay stable so saved
  presets and external working documents still load correctly.
- Linux downloads keep the `clay-form` command alias; the Arch package retains
  its upgrade declarations for the former package name.
- Internal solver names, profiling variables and dated validation evidence
  remain usable. They do not change the application name.
- Published archives and their checksums remain unchanged. Alpha 1–4 downloads
  retain the Clay Form name; Alpha 5 downloads use SketchOtter.
