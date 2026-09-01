use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::Result;
use message_ir_format::{ExportTransforms, FormatSinkResult};
use message_vault_io_core::{ExportReport, OutputFormat};
use std::fs;
use std::path::{Path, PathBuf};

fn convert(input: &Path, output: &Path) -> Result<(ExportReport, FormatSinkResult)> {
    convert_export(ConvertExportArgs {
        input,
        output,
        timezone: Some("UTC"),
        transforms: ExportTransforms::none(),
        output_format: OutputFormat::Csv,
        cancel: None,
        resume: false,
    })
}

#[test]
fn convert_messages_keys_the_chat_by_its_number() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let messages = fixture.join("messages.csv");
    assert!(messages.is_file(), "missing {}", messages.display());

    let tmp = tempfile::tempdir().expect("tempdir");
    let (report, _) = convert(&messages, tmp.path()).expect("convert");

    assert_eq!(report.conversations, 1);
    assert_eq!(report.messages, 3);
    assert_eq!(report.extra.get("messages_files").copied().unwrap_or(0), 1);
    assert_eq!(report.extra.get("whatsapp_files").copied().unwrap_or(0), 0);
    assert_eq!(report.extra.get("name_only_chat").copied().unwrap_or(0), 0);

    let out = tmp.path().join("+13212462167.csv");
    let body = fs::read_to_string(&out).expect("read csv");
    assert!(body.contains("chat_identifier"));
    assert!(body.contains("imazing"));
    assert!(body.contains("iMazing"));
    assert!(body.contains("3.5.5"));
    assert!(body.contains("Bob McRoy"));
    assert!(body.contains("image000000.jpg"));
    assert!(body.contains("imazing_type"));
}

#[test]
fn convert_whatsapp_csv_direct() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let whatsapp = fixture.join("whatsapp.csv");
    let tmp = tempfile::tempdir().expect("tempdir");
    let (report, _) = convert(&whatsapp, tmp.path()).expect("convert");

    assert_eq!(report.conversations, 1);
    assert_eq!(report.messages, 3);
    assert_eq!(report.extra.get("whatsapp_files").copied().unwrap_or(0), 1);
    let out = tmp.path().join("+13212462167__whatsapp.csv");
    let body = fs::read_to_string(&out).expect("read csv");
    assert!(body.contains("WhatsApp"));
    assert!(body.contains("forwarded"));
    assert!(body.contains("Yes"));
    assert!(body.contains("12.34 KB"));
}

#[test]
fn convert_export_root_recursively_keeps_services_separate() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/export_root");
    let tmp = tempfile::tempdir().expect("tempdir");
    let (report, _) = convert(&root, tmp.path()).expect("convert");

    assert_eq!(report.extra.get("messages_files").copied().unwrap_or(0), 2);
    assert_eq!(report.extra.get("whatsapp_files").copied().unwrap_or(0), 1);
    assert!(report.conversations >= 3);
    assert!(tmp.path().join("+13212462167.csv").is_file());
    assert!(tmp.path().join("+13212462167__whatsapp.csv").is_file());
    // Silent Carol never sent a message, so the source records no address for
    // her. She is reported rather than given an invented number.
    let group = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.ends_with(".csv") && n.starts_with("group") && !n.contains("whatsapp"))
        .expect("group csv");
    let body = fs::read_to_string(tmp.path().join(group)).unwrap();
    assert!(body.contains("group"));
    assert!(body.contains("Notification") || body.contains("notification"));
    assert!(
        report
            .extra
            .get("unresolved_group_participants")
            .copied()
            .unwrap_or(0)
            >= 1
    );
}

#[test]
fn jsonl_drains_the_write_queue_and_a_second_run_resumes_it() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let messages = fixture.join("messages.csv");
    let tmp = tempfile::tempdir().expect("tempdir");

    let convert_jsonl = |resume: bool| {
        convert_export(ConvertExportArgs {
            input: &messages,
            output: tmp.path(),
            timezone: Some("UTC"),
            transforms: ExportTransforms::none(),
            output_format: OutputFormat::Jsonl,
            cancel: None,
            resume,
        })
    };

    let (report, _) = convert_jsonl(false).expect("convert");
    assert_eq!(report.conversations, 1);

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
    let first = jsonl_files(tmp.path());
    assert_eq!(first.len(), 1, "the queue wrote a file per conversation");
    let before = fs::read_to_string(tmp.path().join(&first[0])).expect("read jsonl");

    let (resumed, _) = convert_jsonl(true).expect("resume convert");
    assert_eq!(resumed.conversations, 1, "resume still accounts for it");
    assert_eq!(jsonl_files(tmp.path()), first, "same file set");
    assert_eq!(
        fs::read_to_string(tmp.path().join(&first[0])).expect("reread"),
        before,
        "a resumed run must not rewrite a conversation it already has"
    );
}
