use crate::emit::convert_export;
use contacts::ContactsBook;
use message_csv::DateRange;
use message_ir_format::ExportTransforms;
use message_vault_io_core::OutputFormat;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

fn empty_contacts(dir: &tempfile::TempDir) -> ContactsBook {
    let path = dir.path().join("contacts.csv");
    let mut f = File::create(&path).unwrap();
    writeln!(f, "First Name,Last Name,Mobile Phone").unwrap();
    ContactsBook::load_vcard_csv(&path).unwrap()
}

#[test]
fn convert_smoke_writes_csv_not_json() {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_export");
    assert!(input.is_dir(), "missing fixture: {}", input.display());

    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let (report, _) = convert_export(
        input.as_path(),
        tmp.path(),
        &["+15555550100".into()],
        &contacts,
        &DateRange::default(),
        ExportTransforms::none(),
        OutputFormat::Csv,
        None,
    )
    .expect("convert_export should succeed");
    assert!(report.conversations >= 1);
    assert!(report.extra.get("xml_messages_seen").copied().unwrap_or(0) >= 2);

    let mut csv_files: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("csv"))
        .collect();
    csv_files.sort();
    assert!(!csv_files.is_empty(), "expected at least one .csv");

    let json_count = fs::read_dir(tmp.path())
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
    File::open(&csv_files[0])
        .unwrap()
        .read_to_string(&mut contents)
        .unwrap();
    let header = contents.lines().next().unwrap();
    assert!(header.contains("chat_identifier"));
    assert!(header.contains("direction"));
    assert!(header.contains("attachments_json"));
    assert!(!header.contains("export_schema"));
}

#[test]
fn output_equals_input_bails_before_cleaning() {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_export");
    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let err = convert_export(
        input.as_path(),
        input.as_path(),
        &["+15555550100".into()],
        &contacts,
        &DateRange::default(),
        ExportTransforms::none(),
        OutputFormat::Csv,
        None,
    )
    .expect_err("output == input must fail");
    assert!(
        err.to_string()
            .contains("must not be the same as, or contain"),
        "unexpected error: {err}"
    );
    // The backup directory must not have been cleaned by the failed run.
    assert!(input.join("gosms_sys_smoke.xml").is_file());
    assert!(input.join("I_1609459200_recv.pdu").is_file());
}
