use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::Result;
use contacts::{ContactsBook, NameMapping};
use message_csv::DateRange;
use message_ir_format::{ExportTransforms, FormatSinkResult};
use message_vault_io_core::testutil::{assert_csv_header, csv_files};
use message_vault_io_core::{ExportReport, OutputFormat};
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn empty_book() -> ContactsBook {
    ContactsBook::empty()
}

fn empty_mapping() -> NameMapping {
    NameMapping::empty()
}

fn convert(inputs: &[&Path], output_dir: &Path) -> Result<(ExportReport, FormatSinkResult)> {
    convert_export(ConvertExportArgs {
        inputs,
        output_dir,
        owner_phones: &["+15555550100".into()],
        owner_emails: &["owner@example.com".into()],
        contacts: &empty_book(),
        name_mapping: &empty_mapping(),
        date_range: &DateRange::default(),
        verbose: false,
        transforms: ExportTransforms::none(),
        output_format: OutputFormat::Csv,
        cancel: None,
        log: None,
        resume: false,
    })
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
    let err = convert(&[input.as_path()], input.as_path()).expect_err("output == input must fail");
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
    let (report, _) = convert(&[input.as_path()], tmp.path()).unwrap();

    assert!(report.conversations >= 1);
    let flat = report.extra.get("flat_eml").copied().unwrap_or(0);
    let archive = report.extra.get("archive_eml").copied().unwrap_or(0);
    assert!(flat >= 1 || archive >= 1);

    assert_csv_header(
        tmp.path(),
        &[
            "chat_identifier",
            "attachments_json",
            "export_source",
            "export_tool",
            "export_tool_version",
            "timestamp_unix_ms",
            "android_type",
            "source_fields_json",
            "owner_handle",
            "participants_json",
            "read_receipt", // unified header; empty for SMS
            "tapbacks_json",
        ],
        &["date_ms", "contact_name", "xml_fields_json"],
        "sms-backup-plus",
    );
    // Vendor fields (source_kind, smssync_id, eml_path) live inside source_fields_json.
    let contents = fs::read_to_string(&csv_files(tmp.path())[0]).unwrap();
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
    let (report, _) = convert(&[input_dir.as_path()], &out).unwrap();

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
    let (report, _) = convert(&[input_dir.as_path()], &out).unwrap();

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

#[test]
fn jsonl_drains_the_write_queue_and_a_second_run_resumes_it() {
    let input = fixtures();
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("out");
    fs::create_dir_all(&out).expect("out dir");

    let run = |resume: bool| {
        convert_export(ConvertExportArgs {
            inputs: &[input.as_path()],
            output_dir: &out,
            owner_phones: &["+15555550100".into()],
            owner_emails: &["owner@example.com".into()],
            contacts: &empty_book(),
            name_mapping: &empty_mapping(),
            date_range: &DateRange::default(),
            verbose: false,
            transforms: ExportTransforms::none(),
            output_format: OutputFormat::Jsonl,
            cancel: None,
            log: None,
            resume,
        })
    };

    let (report, _) = run(false).expect("convert");
    assert!(report.conversations >= 1);

    let jsonl_files = |dir: &Path| -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read output")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".jsonl"))
            .collect();
        names.sort();
        names
    };
    let first = jsonl_files(&out);
    assert!(!first.is_empty(), "the queue wrote conversation files");
    let bodies: Vec<String> = first
        .iter()
        .map(|n| fs::read_to_string(out.join(n)).expect("read jsonl"))
        .collect();

    run(true).expect("resume convert");

    assert_eq!(jsonl_files(&out), first, "same file set after a resume");
    for (name, before) in first.iter().zip(bodies) {
        assert_eq!(
            fs::read_to_string(out.join(name)).expect("reread"),
            before,
            "a resumed run must not rewrite {name}"
        );
    }
}
