//! Shared scaffolding for exporter `convert_smoke` tests (behind `testutil`).

use contacts::ContactsBook;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// An empty contacts CSV book in a temp dir (header-only, no rows).
pub fn empty_contacts(dir: &tempfile::TempDir) -> ContactsBook {
    let path = dir.path().join("contacts.csv");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(f, "First Name,Last Name,Mobile Phone").unwrap();
    ContactsBook::load_vcard_csv(&path).unwrap()
}

/// Sorted `.csv` paths under `root` (the smoke-test file collection block).
pub fn csv_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("csv"))
        .collect();
    files.sort();
    files
}

/// Assert that the first CSV under `root` has every `contains` header
/// column, none of the `not_contains` columns, and that the file body
/// contains `body_contains`; also assert no `.meta.json` files remain.
pub fn assert_csv_header(
    root: &Path,
    contains: &[&str],
    not_contains: &[&str],
    body_contains: &str,
) {
    let files = csv_files(root);
    assert!(!files.is_empty(), "expected at least one .csv");
    let json_count = fs::read_dir(root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|x| x.to_str()) == Some("json")
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".meta.json"))
        })
        .count();
    assert_eq!(json_count, 0);
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut fs::File::open(&files[0]).unwrap(), &mut contents).unwrap();
    let header = contents.lines().next().unwrap();
    for col in contains {
        assert!(header.contains(col), "header missing {col:?}");
    }
    for col in not_contains {
        assert!(!header.contains(col), "header unexpectedly has {col:?}");
    }
    assert!(contents.contains(body_contains));
}
