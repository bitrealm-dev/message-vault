use crate::emit::{ConvertExportArgs, convert_export};
use anyhow::Result;
use message_ir_format::{ExportTransforms, FormatSinkResult};
use message_vault_io_core::testutil::{assert_csv_header, empty_contacts};
use message_vault_io_core::{ExportReport, OutputFormat};
use std::path::{Path, PathBuf};

fn convert(input_dir: &Path, output_dir: &Path) -> Result<(ExportReport, FormatSinkResult)> {
    convert_export(ConvertExportArgs {
        input_dir,
        output_dir,
        owner_phones: &["+15555550100".into()],
        transforms: ExportTransforms::none(),
        output_format: OutputFormat::Csv,
        cancel: None,
        resume: false,
    })
}

#[test]
fn convert_smoke_writes_csv_not_json() {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_export");
    assert!(input.is_dir(), "missing fixture: {}", input.display());

    let tmp = tempfile::tempdir().expect("tempdir");
    let (report, _) = convert(input.as_path(), tmp.path()).expect("convert_export should succeed");
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
    let err = convert(input.as_path(), input.as_path()).expect_err("output == input must fail");
    assert!(
        err.to_string()
            .contains("must not be the same as, or contain"),
        "unexpected error: {err}"
    );
    // The backup directory must not have been cleaned by the failed run.
    assert!(input.join("gosms_sys_smoke.xml").is_file());
    assert!(input.join("I_1609459200_recv.pdu").is_file());
}

#[test]
fn jsonl_drains_the_write_queue_and_a_second_run_resumes_it() {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_export");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).expect("out dir");

    let run = |resume: bool| {
        convert_export(ConvertExportArgs {
            input_dir: input.as_path(),
            output_dir: &out,
            owner_phones: &["+15555550100".into()],
            transforms: ExportTransforms::none(),
            output_format: OutputFormat::Jsonl,
            cancel: None,
            resume,
        })
    };

    let (report, _) = run(false).expect("convert");
    assert!(report.conversations >= 1);

    let jsonl_files = |dir: &Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
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
        .map(|n| std::fs::read_to_string(out.join(n)).expect("read jsonl"))
        .collect();

    run(true).expect("resume convert");

    assert_eq!(jsonl_files(&out), first, "same file set after a resume");
    for (name, before) in first.iter().zip(bodies) {
        assert_eq!(
            std::fs::read_to_string(out.join(name)).expect("reread"),
            before,
            "a resumed run must not rewrite {name}"
        );
    }
}
