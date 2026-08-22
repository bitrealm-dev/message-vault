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
        date_range: &DateRange::default(),
        transforms: ExportTransforms::none(),
        output_format: OutputFormat::Csv,
        cancel: None,
    })
}

#[test]
fn output_equals_input_dir_bails_before_cleaning() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let csv = fixture.join("all_conversations.csv");
    assert!(csv.is_file(), "missing {}", csv.display());
    let book = ContactsBook::empty();
    // Output = fixture dir that holds the source CSV — open_prepared would
    // delete every *.csv before discovery.
    let err = convert(&fixture, &fixture, &book).expect_err("output == input must fail");
    assert!(
        err.to_string()
            .contains("must not be the same as, or contain"),
        "unexpected error: {err}"
    );
    assert!(
        csv.is_file(),
        "source CSV must survive the refused run: {}",
        csv.display()
    );
}

#[test]
fn convert_all_conversations_with_vcf() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let csv = fixture.join("all_conversations.csv");
    let vcf = fixture.join("contacts.vcf");
    assert!(csv.is_file(), "missing {}", csv.display());
    assert!(vcf.is_file(), "missing {}", vcf.display());

    let book = ContactsBook::load_vcf(&vcf).expect("load vcf");
    let tmp = tempfile::tempdir().expect("tempdir");
    let (report, _) = convert(&csv, tmp.path(), &book).expect("convert");

    assert_eq!(report.conversations, 1);
    assert_eq!(report.messages, 2);
    assert_eq!(
        report
            .extra
            .get("unresolved_chat_phone")
            .copied()
            .unwrap_or(0),
        0
    );

    let out = tmp.path().join("+15555550122.csv");
    let body = fs::read_to_string(&out).expect("read csv");
    assert!(body.contains("chat_identifier"));
    assert!(body.contains("openextract"));
    assert!(body.contains("Sam Example"));
    assert!(body.contains("all-conversations"));
}
