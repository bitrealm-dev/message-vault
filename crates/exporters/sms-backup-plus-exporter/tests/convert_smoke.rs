use crate::emit::convert_export;
use contacts::{ContactsBook, NameMapping};
use message_csv::DateRange;
use message_ir_format::ExportTransforms;
use message_vault_io_core::OutputFormat;
use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn empty_book() -> ContactsBook {
    ContactsBook::empty()
}

fn empty_mapping() -> NameMapping {
    NameMapping::empty()
}

#[test]
fn output_equals_input_bails_before_cleaning() {
    let input = fixtures();
    let sample = input.join("flat_smssync_276_sam.eml");
    assert!(
        sample.is_file() || input.is_dir(),
        "missing fixtures under {}",
        input.display()
    );
    let err = convert_export(
        &[input.as_path()],
        input.as_path(),
        &["+15555550100".into()],
        &["owner@example.com".into()],
        &empty_book(),
        &empty_mapping(),
        &DateRange::default(),
        false,
        ExportTransforms::none(),
        OutputFormat::Csv,
        None,
        None,
    )
    .expect_err("output == input must fail");
    assert!(
        err.to_string()
            .contains("must not be the same as, or contain"),
        "unexpected error: {err}"
    );
    // Fixture EMLs must still be present after the refused run.
    let eml_still_there = fs::read_dir(&input)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("eml"));
    assert!(eml_still_there, "fixture .eml files must survive");
}

#[test]
fn convert_smoke_writes_csv_not_json() {
    let input = fixtures();
    let tmp = tempfile::tempdir().unwrap();
    let (report, _) = convert_export(
        &[input.as_path()],
        tmp.path(),
        &["+15555550100".into()],
        &["owner@example.com".into()],
        &empty_book(),
        &empty_mapping(),
        &DateRange::default(),
        false,
        ExportTransforms::none(),
        OutputFormat::Csv,
        None,
        None,
    )
    .unwrap();

    assert!(report.conversations >= 1);
    let flat = report.extra.get("flat_eml").copied().unwrap_or(0);
    let archive = report.extra.get("archive_eml").copied().unwrap_or(0);
    assert!(flat >= 1 || archive >= 1);

    let mut csv_files: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("csv"))
        .collect();
    csv_files.sort();
    assert!(!csv_files.is_empty());

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
    assert!(header.contains("attachments_json"));
    assert!(header.contains("export_source"));
    assert!(header.contains("export_tool"));
    assert!(header.contains("export_tool_version"));
    assert!(header.contains("timestamp_unix_ms"));
    assert!(header.contains("android_type"));
    assert!(header.contains("source_fields_json"));
    assert!(header.contains("owner_handle"));
    assert!(header.contains("participants_json"));
    assert!(header.contains("read_receipt")); // unified header; empty for SMS
    assert!(header.contains("tapbacks_json"));
    assert!(!header.contains("date_ms"));
    assert!(!header.contains("contact_name"));
    assert!(!header.contains("xml_fields_json"));
    assert!(contents.contains("sms-backup-plus"));
    // Vendor fields (source_kind, smssync_id, eml_path) live inside source_fields_json.
    assert!(contents.contains("source_kind"));
}

#[test]
fn end_dedupe_collapses_duplicate_flats() {
    let tmp = tempfile::tempdir().unwrap();
    let input_dir = tmp.path().join("in");
    fs::create_dir_all(&input_dir).unwrap();

    let src = fixtures().join("flat_received.eml");
    let bytes = fs::read(&src).unwrap();
    fs::write(input_dir.join("a.eml"), &bytes).unwrap();
    fs::write(input_dir.join("b.eml"), &bytes).unwrap();

    let out = tmp.path().join("out");
    let (report, _) = convert_export(
        &[input_dir.as_path()],
        &out,
        &["+15555550100".into()],
        &["owner@example.com".into()],
        &empty_book(),
        &empty_mapping(),
        &DateRange::default(),
        false,
        ExportTransforms::none(),
        OutputFormat::Csv,
        None,
        None,
    )
    .unwrap();

    assert_eq!(report.extra.get("flat_eml").copied().unwrap_or(0), 2);
    assert_eq!(
        report
            .extra
            .get("messages_before_dedupe")
            .copied()
            .unwrap_or(0),
        2
    );
    assert_eq!(report.messages, 1);
    assert_eq!(report.duplicates_dropped, 1);
    assert_eq!(report.conversations, 1);
}

#[test]
fn dedupe_collapses_archive_and_flat_despite_ms_mismatch() {
    use chrono::{Local, NaiveDateTime, TimeZone};

    let tmp = tempfile::tempdir().unwrap();
    let input_dir = tmp.path().join("in");
    fs::create_dir_all(&input_dir).unwrap();

    let naive = NaiveDateTime::parse_from_str("2020-01-01 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let local_ts = Local
        .from_local_datetime(&naive)
        .single()
        .unwrap()
        .timestamp();
    let ms = local_ts * 1000 + 488;

    fs::write(
        input_dir.join("archive.eml"),
        b"From: <4075551234@sms-backup-plus.local>\r\n\
To: me@example.com\r\n\
Subject: SMS archive Alice\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Alice\r\n\
2020-01-01 12:00:00 - Me\r\n\
Will do\r\n",
    )
    .unwrap();

    fs::write(
        input_dir.join("flat.eml"),
        format!(
            "From: me@example.com\r\n\
To: 4075551234@sms-backup-plus.local\r\n\
Subject: SMS with Alice\r\n\
X-smssync-type: 2\r\n\
X-smssync-address: 4075551234\r\n\
X-smssync-date: {ms}\r\n\
X-smssync-id: 999\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Will do\r\n"
        ),
    )
    .unwrap();

    let out = tmp.path().join("out");
    let (report, _) = convert_export(
        &[input_dir.as_path()],
        &out,
        &["+15555550100".into()],
        &["owner@example.com".into()],
        &empty_book(),
        &empty_mapping(),
        &DateRange::default(),
        false,
        ExportTransforms::none(),
        OutputFormat::Csv,
        None,
        None,
    )
    .unwrap();

    assert_eq!(
        report
            .extra
            .get("messages_before_dedupe")
            .copied()
            .unwrap_or(0),
        2
    );
    assert_eq!(report.messages, 1);
    assert_eq!(report.duplicates_dropped, 1);

    let csv = fs::read_to_string(out.join("+14075551234.csv")).unwrap();
    assert!(csv.contains("Will do"));
    // source_kind/smssync_id now live inside the source_fields_json cell.
    assert!(csv.contains("flat"));
    assert!(csv.contains("999"));
}
