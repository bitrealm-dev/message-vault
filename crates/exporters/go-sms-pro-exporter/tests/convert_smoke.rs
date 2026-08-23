use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::Result;
use contacts::ContactsBook;
use message_csv::DateRange;
use message_ir_format::{ExportTransforms, FormatSinkResult};
use message_vault_io_core::testutil::{assert_csv_header, empty_contacts};
use message_vault_io_core::{ExportReport, OutputFormat};
use std::path::{Path, PathBuf};

fn convert(
    input_dir: &Path,
    output_dir: &Path,
    contacts: &ContactsBook,
) -> Result<(ExportReport, FormatSinkResult)> {
    convert_export(ConvertExportArgs {
        input_dir,
        output_dir,
        owner_phones: &["+15555550100".into()],
        contacts,
        date_range: &DateRange::default(),
        transforms: ExportTransforms::none(),
        output_format: OutputFormat::Csv,
        cancel: None,
    })
}

#[test]
fn convert_smoke_writes_csv_not_json() {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_export");
    assert!(input.is_dir(), "missing fixture: {}", input.display());

    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let (report, _) =
        convert(input.as_path(), tmp.path(), &contacts).expect("convert_export should succeed");
    assert!(report.conversations >= 1);
    assert!(report.extra.get("xml_messages_seen").copied().unwrap_or(0) >= 2);

    // Mirror of the original block: header columns only, no body substring.
    assert_csv_header(
        tmp.path(),
        &["chat_identifier", "direction", "attachments_json"],
        &["export_schema"],
        "",
    );
}

#[test]
fn output_equals_input_bails_before_cleaning() {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_export");
    let tmp = tempfile::tempdir().expect("tempdir");
    let contacts = empty_contacts(&tmp);
    let err = convert(input.as_path(), input.as_path(), &contacts)
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
