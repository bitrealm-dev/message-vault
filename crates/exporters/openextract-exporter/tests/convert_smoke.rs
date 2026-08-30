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
        resume: false,
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

#[test]
fn jsonl_drains_the_write_queue_and_a_second_run_resumes_it() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let csv = fixture.join("all_conversations.csv");
    let book = ContactsBook::empty();
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("out");
    fs::create_dir_all(&out).expect("out dir");

    let run = |resume: bool| {
        convert_export(ConvertExportArgs {
            input: &csv,
            output: &out,
            book: &book,
            date_range: &DateRange::default(),
            transforms: ExportTransforms::none(),
            output_format: OutputFormat::Jsonl,
            cancel: None,
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
