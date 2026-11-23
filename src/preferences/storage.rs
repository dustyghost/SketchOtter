//! Keep preferences and recovery together when adopting the SketchOtter name.
use std::path::{Path, PathBuf};

pub(crate) fn path(base: &Path) -> PathBuf {
    let current = base.join("sketchotter");
    let legacy = base.join("clay-form");
    // Existing installations keep their complete data directory until it is
    // migrated. A current directory takes precedence, including a legacy alias
    // pointing to it. Never split recovery from preferences or merge two stores.
    let directory = if current.is_dir() || !legacy.is_dir() {
        current
    } else {
        legacy
    };
    directory.join("preferences.json")
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "sketchotter-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn new_installation_uses_sketchotter_without_creating_files() {
        let root = Directory::new();
        assert_eq!(path(&root.0), root.0.join("sketchotter/preferences.json"));
        assert!(!root.0.join("sketchotter").exists());
    }

    #[test]
    fn legacy_recovery_is_found_even_without_preferences() {
        let root = Directory::new();
        let recovery = root.0.join("clay-form/recovery");
        std::fs::create_dir_all(&recovery).unwrap();
        std::fs::write(recovery.join("saved.json"), b"retained recovery").unwrap();
        let selected = path(&root.0);
        assert_eq!(selected, root.0.join("clay-form/preferences.json"));
        assert_eq!(selected.with_file_name("recovery"), recovery);
        assert!(!root.0.join("sketchotter").exists());
    }

    #[test]
    fn current_directory_wins_without_overwriting_legacy_data() {
        let root = Directory::new();
        for (name, data) in [("clay-form", "legacy"), ("sketchotter", "current")] {
            std::fs::create_dir(root.0.join(name)).unwrap();
            std::fs::write(root.0.join(name).join("preferences.json"), data).unwrap();
        }
        assert_eq!(path(&root.0), root.0.join("sketchotter/preferences.json"));
        assert_eq!(
            std::fs::read_to_string(root.0.join("clay-form/preferences.json")).unwrap(),
            "legacy"
        );
        assert_eq!(std::fs::read_to_string(path(&root.0)).unwrap(), "current");
    }

    #[cfg(unix)]
    #[test]
    fn legacy_alias_keeps_old_and_new_releases_on_the_same_data() {
        let root = Directory::new();
        std::fs::create_dir(root.0.join("sketchotter")).unwrap();
        std::os::unix::fs::symlink("sketchotter", root.0.join("clay-form")).unwrap();
        std::fs::write(path(&root.0), b"shared preferences").unwrap();
        assert_eq!(
            std::fs::read(root.0.join("clay-form/preferences.json")).unwrap(),
            b"shared preferences"
        );
        assert_eq!(path(&root.0), root.0.join("sketchotter/preferences.json"));
    }
}
