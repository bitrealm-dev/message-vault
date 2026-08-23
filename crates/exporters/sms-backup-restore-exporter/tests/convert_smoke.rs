use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::Result;
use contacts::ContactsBook;
use message_csv::DateRange;
use message_ir_format::{ExportTransforms, FormatSinkResult};
use message_vault_io_core::testutil::{assert_csv_header, empty_contacts};
use message_vault_io_core::{ExportReport, OutputFormat};
use std::fs;
use std::path::{Path, PathBuf};

fn convert(
    input: &Path,
    output: &Path,
    contacts: &ContactsBook,
    owner_phones: &[String],
    output_format: OutputFormat,
) -> Result<(ExportReport, FormatSinkResult)> {
    convert_export(ConvertExportArgs {
        input,
        output_dir: output,
        owner_phones,
        contacts,
        date_range: &DateRange::default(),
        transforms: ExportTransforms::none(),
        output_format,
        cancel: None,
    })
}

#[test]
fn convert_export_smoke_on_sample_fixture() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xml");
    assert!(fixture.is_file(), "missing fixture: {}", fixture.display());

    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let (report, _) = convert(
        &fixture,
        tmp.path(),
        &contacts,
        &["+15555550100".into()],
        OutputFormat::Csv,
    )
    .expect("convert_export should succeed");

    assert!(
        report.conversations >= 1,
        "expected >=1 conversations, got {}",
        report.conversations
    );

    assert_csv_header(
        tmp.path(),
        &[
            "chat_identifier",
            "export_source",
            "export_tool",
            "export_tool_version",
            "message_kind",
            "timestamp_unix_ms",
            "source_fields_json",
            "owner_handle",
            "participants_json",
            "subject",
        ],
        &["date_ms", "contact_name", "xml_fields_json"],
        "sms-backup-restore",
    );

    let attachments = tmp.path().join("attachments");
    let mut found = false;
    if attachments.is_dir() {
        for entry in std::fs::read_dir(&attachments).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "expected at least one attachment file under {}",
        attachments.display()
    );
}

#[test]
fn dedupes_overlapping_xml_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let input_dir = tmp.path().join("in");
    fs::create_dir_all(&input_dir).unwrap();

    let xml = r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="1">
  <sms address="+15555550101" date="1400773261000" type="1" body="same text" contact_name="Sam" />
</smses>"#;
    fs::write(input_dir.join("a.xml"), xml).unwrap();
    fs::write(input_dir.join("b.xml"), xml).unwrap();

    let out = tmp.path().join("out");
    let contacts = empty_contacts(&tmp);
    let (report, _) = convert(
        &input_dir,
        &out,
        &contacts,
        &["+15555550100".into()],
        OutputFormat::Csv,
    )
    .unwrap();
    assert_eq!(report.extra.get("sms_seen").copied().unwrap_or(0), 2);
    assert_eq!(report.conversations, 1);
    assert_eq!(report.received, 1); // one row after dedupe

    let chat = out.join("+15555550101.csv");
    let body = fs::read_to_string(&chat).unwrap();
    // header + one message row (duplicate dropped)
    assert_eq!(body.lines().count(), 2);
    assert!(body.contains("same text"));
}

#[test]
fn rejects_owner_phone_without_digits() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xml");
    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let err = convert(
        &fixture,
        tmp.path(),
        &contacts,
        &["not-a-phone".into()],
        OutputFormat::Csv,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("owner phone"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn convert_export_eml_writes_conversation_folder() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xml");
    assert!(fixture.is_file(), "missing fixture: {}", fixture.display());

    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let (report, _) = convert(
        &fixture,
        tmp.path(),
        &contacts,
        &["+15555550100".into()],
        OutputFormat::Eml,
    )
    .expect("convert_export eml should succeed");

    assert!(
        report.conversations >= 1,
        "expected >=1 conversations, got {}",
        report.conversations
    );

    let mut eml_dirs = Vec::new();
    let mut eml_files = 0usize;
    for entry in fs::read_dir(tmp.path()).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == "attachments" {
            continue;
        }
        let count = fs::read_dir(&path)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("eml"))
            .count();
        if count > 0 {
            eml_dirs.push(path);
            eml_files += count;
        }
    }
    assert!(
        !eml_dirs.is_empty(),
        "expected at least one conversation directory with .eml"
    );
    assert!(eml_files >= 1, "expected at least one .eml file");

    let sample = fs::read(
        fs::read_dir(&eml_dirs[0])
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().and_then(|x| x.to_str()) == Some("eml"))
            .unwrap(),
    )
    .unwrap();
    let text = String::from_utf8_lossy(&sample);
    assert!(text.contains("X-ME-Export-Source: sms-backup-restore"));
    assert!(text.contains("X-ME-Guid:"));
}

#[test]
fn convert_export_json_and_jsonl_use_pristine_v3() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.xml");
    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);

    let (report, _) = convert(
        &fixture,
        tmp.path(),
        &contacts,
        &["+15555550100".into()],
        OutputFormat::Json,
    )
    .expect("json export");

    let json_path = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .expect("expected .json");
    let raw = fs::read_to_string(&json_path).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(doc["schema_version"], 3);
    assert!(
        doc["conversation"]["stats"]["message_count"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(!raw.contains("filename_suffix"));
    assert!(!raw.contains("\"bytes\""));
    let msg = &doc["messages"][0];
    assert!(msg["source"]["fields"].is_object());
    assert!(msg["source"].get("contact_name").is_none());
    assert!(msg["source"]["android_type"].is_number() || msg["source"]["android_type"].is_null());
    assert!(msg["service"].as_str() == Some("sms"));
    // Outgoing rows carry owner identity. The fixture includes an SMS type=2
    // (sent) and an MMS where the owner is a recipient; both must share the same
    // individual conversation after owner-filtered peer resolution.
    assert_eq!(
        report.conversations, 1,
        "MMS must not fragment into a group chat"
    );
    let has_outgoing = doc["messages"].as_array().unwrap().iter().any(|m| {
        m["direction"] == "outgoing" && m["sender_handle"].as_str() == Some("+15555550100")
    });
    assert!(has_outgoing, "expected outgoing message with owner sender");

    let out_jsonl = tmp.path().join("jsonl-out");
    fs::create_dir_all(&out_jsonl).unwrap();
    let (_report, _) = convert(
        &fixture,
        &out_jsonl,
        &contacts,
        &["+15555550100".into()],
        OutputFormat::Jsonl,
    )
    .expect("jsonl export");

    let jsonl_path = fs::read_dir(&out_jsonl)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|x| x.to_str()) == Some("jsonl"))
        .expect("expected .jsonl");
    let body = fs::read_to_string(&jsonl_path).unwrap();
    let mut lines = body.lines();
    let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(header["schema_version"], 3);
    assert!(header.get("messages").is_none());
    assert!(
        header["conversation"]["stats"]["message_count"]
            .as_u64()
            .unwrap()
            >= 1
    );
    let msg_line: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert!(msg_line["source"]["fields"].is_object());
}
