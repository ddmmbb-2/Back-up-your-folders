use crate::{model::RestoreMode, restore, snapshot};
use std::fs;

#[test]
fn backup_preview_and_both_restore_modes() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("project");
    let store = source.join(".versions");
    fs::create_dir_all(source.join("src")).unwrap();
    fs::create_dir_all(source.join("empty-folder")).unwrap();
    fs::write(source.join("src/test.php"), b"first").unwrap();
    let mut noop = |_: &str, _: f32| {};
    let archived = snapshot::create_snapshot(&source, &store, "initial", "original files", &mut noop).unwrap();
    assert_eq!(archived.meta.file_count, 1);
    assert!(archived.zip_bytes > 0);

    fs::write(source.join("src/test.php"), b"other").unwrap();
    fs::write(source.join("more.txt"), b"new file").unwrap();
    let plan = restore::preview(&source, &store, archived.clone()).unwrap();
    assert_eq!(plan.changed.len(), 1);
    assert_eq!(plan.extra.len(), 1);

    restore::restore(&source, &store, &plan, RestoreMode::Overwrite, false, &mut noop).unwrap();
    assert_eq!(fs::read(source.join("src/test.php")).unwrap(), b"first");
    assert!(source.join("more.txt").exists());

    let plan = restore::preview(&source, &store, archived).unwrap();
    restore::restore(&source, &store, &plan, RestoreMode::Exact, true, &mut noop).unwrap();
    assert!(!source.join("more.txt").exists());
    assert!(source.join("empty-folder").is_dir());
    assert!(snapshot::list_snapshots(&store).unwrap().len() >= 2);
}

#[test]
fn corrupted_non_version_zip_does_not_appear() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("bad.zip"), b"not a zip").unwrap();
    assert!(snapshot::list_snapshots(root.path()).unwrap().is_empty());
}
