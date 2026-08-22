use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::Result;
use contacts::ContactsBook;
use message_csv::DateRange;
use message_ir_format::{ExportTransforms, FormatSinkResult};
use message_vault_io_core::{ExportReport, OutputFormat};
use std::fs;
use std::path::{Path, PathBuf};

fn convert(
    input: &Path,
    output: &Path,
    book: &ContactsBook,
) -> Result<(ExportReport, FormatSinkResult)> {
    convert_export(ConvertExportArgs {
        input,
        output,
        book,
        timezone: Some("UTC"),
        date_range: &DateRange::default(),
        transforms: ExportTransforms::none(),
        output_format: OutputFormat::Csv,
        cancel: None,
    })
}

#[test]
fn convert_messages_with_vcard_csv_contacts() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let messages = fixture.join("messages.csv");
    let contacts = fixture.join("contacts.csv");
    assert!(messages.is_file(), "missing {}", messages.display());
    assert!(contacts.is_file(), "missing {}", contacts.display());

    let book = ContactsBook::load_vcard_csv(&contacts).expect("load contacts");
    let tmp = tempfile::tempdir().expect("tempdir");
    let (report, _) = convert(&messages, tmp.path(), &book).expect("convert");

    assert_eq!(report.conversations, 1);
    assert_eq!(report.messages, 3);
    assert_eq!(report.extra.get("messages_files").copied().unwrap_or(0), 1);
    assert_eq!(report.extra.get("whatsapp_files").copied().unwrap_or(0), 0);
    assert_eq!(
        report
            .extra
            .get("unresolved_chat_phone")
            .copied()
            .unwrap_or(0),
        0
    );

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
    let contacts = fixture.join("contacts.csv");
    let book = ContactsBook::load_vcard_csv(&contacts).expect("load contacts");
    let tmp = tempfile::tempdir().expect("tempdir");
    let (report, _) = convert(&whatsapp, tmp.path(), &book).expect("convert");

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
    let contacts = root.join("Contacts/All contacts/All/Contacts - synthetic.csv");
    let book = ContactsBook::load_vcard_csv(&contacts).expect("load contacts");
    let tmp = tempfile::tempdir().expect("tempdir");
    let (report, _) = convert(&root, tmp.path(), &book).expect("convert");

    assert_eq!(report.extra.get("messages_files").copied().unwrap_or(0), 2);
    assert_eq!(report.extra.get("whatsapp_files").copied().unwrap_or(0), 1);
    assert!(report.conversations >= 3);
    assert!(tmp.path().join("+13212462167.csv").is_file());
    assert!(tmp.path().join("+13212462167__whatsapp.csv").is_file());
    // Silent Carol should be resolved into the group chat id via contacts.
    let group = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.ends_with(".csv") && n.contains("15555550133") && !n.contains("whatsapp"))
        .expect("group csv with silent Carol");
    let body = fs::read_to_string(tmp.path().join(group)).unwrap();
    assert!(body.contains("group"));
    assert!(body.contains("Notification") || body.contains("notification"));
    assert_eq!(
        report
            .extra
            .get("unresolved_group_participants")
            .copied()
            .unwrap_or(0),
        0
    );
}
