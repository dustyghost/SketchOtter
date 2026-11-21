#[cfg(windows)]
fn embed_windows_icon() {
    use std::{env, path::PathBuf, process::Command};
    let target = env::var("TARGET").unwrap();
    if !target.ends_with("-windows-msvc") {
        return;
    }
    let compiler = find_msvc_tools::find_windows_sdk(env::consts::ARCH)
        .and_then(|sdk| {
            sdk.path()
                .map(|dir| dir.join("rc.exe"))
                .find(|file| file.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("rc.exe"));
    let resource = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("sketchotter.res");
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg("assets/branding/windows.rc")
        .status()
        .expect("Windows SDK resource compiler is required to embed the application icon");
    assert!(status.success(), "compiling the SketchOtter icon failed");
    println!(
        "cargo:rustc-link-arg-bin=sketchotter={}",
        resource.display()
    );
}

fn main() {
    #[cfg(windows)]
    embed_windows_icon();
    // Track source edits and commits, including linked worktrees. Never watch target/.
    for path in [
        "assets/branding",
        "build.rs",
        "src",
        "Cargo.toml",
        "Cargo.lock",
        "README.md",
        "docs",
        "legal",
        "LICENSE",
        "COPYRIGHT.md",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    for name in ["HEAD", "index", "refs", "packed-refs"] {
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--git-path", name])
            .output()
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout);
            let path = path.trim();
            if std::path::Path::new(path).exists() {
                println!("cargo:rerun-if-changed={path}");
            }
        }
    }
    if std::path::Path::new("SOURCE-COMMIT").exists() {
        println!("cargo:rerun-if-changed=SOURCE-COMMIT");
    }
    // No paths or environment dump: only source identity and build mode.
    let revision = std::process::Command::new("git")
        .args(["describe", "--always", "--dirty", "--abbrev=12"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .or_else(|| {
            std::fs::read_to_string("SOURCE-COMMIT")
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        })
        .unwrap_or_else(|| "source-revision-unavailable".into());
    println!(
        "cargo:rustc-env=CLAY_BUILD_ID={revision} ({})",
        std::env::var("PROFILE").unwrap_or_default()
    );
}
