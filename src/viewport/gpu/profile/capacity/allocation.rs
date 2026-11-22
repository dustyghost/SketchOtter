//! Observe an actual allocator refusal in a disposable child with a hard address-space limit.
use super::*;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;

#[test]
#[ignore = "manual bounded failure and checkpoint recovery probe on Linux"]
fn gpu_load_profile_allocation_refusal() {
    const CHILD_PATH: &str = "CLAY_ALLOCATION_CHILD_PROJECT";
    const LIMIT_BYTES: usize = 384 * 1024 * 1024;
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        crate::sculpt::capacity::initialize(crate::sculpt::capacity::MeshCapacity {
            vertices: 5_500_000,
            triangles: 11_000_000,
        })
        .unwrap();
        let p = crate::persistence::load_project(path.to_str().unwrap()).unwrap();
        let mut geometry = Geometry::new(p.mesh);
        geometry.settings = p.mesh_settings;
        for _ in 0..5 {
            eprintln!(
                "starting subdivision from {} triangles",
                geometry.mesh.triangles.len()
            );
            geometry.subdivide().unwrap();
        }
        panic!("the child unexpectedly completed every allocation");
    }

    let out = std::path::PathBuf::from(std::env::var_os("CLAY_REGION_OUTPUT").unwrap());
    let path = out.with_file_name("allocation-checkpoint.clay");
    let mut doc = document(fixture("sphere", 0));
    let before = doc.geometry_snapshot();
    assert!(doc.apply_dab(&brush(Kind::Standard), dab(0)));
    doc.end_stroke();
    let after = doc.geometry_snapshot();
    let project = doc.project();
    crate::persistence::save_project(path.to_str().unwrap(), &project).unwrap();
    let saved_bytes = std::fs::read(&path).unwrap();
    let child = Command::new("timeout")
        .args([
            "60",
            "prlimit",
            "--core=0",
            &format!("--as={LIMIT_BYTES}"),
            "--",
        ])
        .arg(std::env::current_exe().unwrap())
        .args([
            "viewport::gpu::profile::capacity::allocation::gpu_load_profile_allocation_refusal",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_PATH, &path)
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&child.stderr);
    let stages: Vec<_> = log
        .lines()
        .filter_map(|s| s.strip_prefix("starting subdivision from "))
        .collect();
    let refusal = log
        .lines()
        .find(|s| s.contains("memory allocation of") && s.contains("failed"));
    assert!(
        !stages.is_empty() && refusal.is_some() && !child.status.success(),
        "{log}"
    );
    let restored = crate::persistence::load_project(path.to_str().unwrap()).unwrap();
    assert_eq!(saved_bytes, std::fs::read(&path).unwrap());
    assert!(restored == project);
    doc.undo();
    assert_eq!(doc.geometry_snapshot(), before);
    doc.redo();
    assert_eq!(doc.geometry_snapshot(), after);
    let record = serde_json::json!({
        "child_address_space_bytes": LIMIT_BYTES, "child_timeout_seconds": 60,
        "exit_code":child.status.code(),"signal":child.status.signal(),"refusal":refusal,
        "attempted_from":stages,"checkpoint_bytes":saved_bytes.len(),
        "checkpoint_byte_exact":true,"reopened_project_exact":true,"supervising_history_exact":true,
    });
    std::fs::write(out, serde_json::to_vec(&record).unwrap()).unwrap();
}
