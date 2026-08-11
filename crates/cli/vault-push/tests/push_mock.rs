//! Mock-server smoke for auth + JSONL push + journal skip.

use std::fs;
use std::io::Write;
use std::path::Path;

use httpmock::prelude::*;
use message_ir::{
    ConversationDocument,
    ConversationMeta,
    ConversationStats,
    ExportMeta,
    IrAttachment,
    IrConversationType,
    IrDirection,
    IrMessage,
    IrMessageKind,
    IrParticipant,
    IrService,
    SCHEMA_VERSION,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use vault_push::{AuthError, ProgressEvent, VaultPushConfig, authenticate, run};

fn sample_doc() -> ConversationDocument {
    ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export: ExportMeta {
            source: "sms-backup-restore".into(),
            tool: "SMS Backup & Restore".into(),
            tool_version: "10.26.003".into(),
            owner_handle: Some("+15555550100".into()),
            owner_display_name: Some("Me".into()),
        },
        conversation: ConversationMeta {
            chat_identifier: "+15555550101".into(),
            conversation_type: IrConversationType::Individual,
            group_title: None,
            participants: vec![IrParticipant {
                handle: "+15555550101".into(),
                display_name: Some("Sam".into()),
                handle_type: None,
            }],
            stats: ConversationStats::default(),
        },
        messages: vec![IrMessage {
            guid: "guid-1".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: IrDirection::Incoming,
            service: IrService::Sms,
            message_kind: IrMessageKind::Sms,
            sender_handle: Some("+15555550101".into()),
            sender_display_name: Some("Sam".into()),
            subject: None,
            text: "hello vault".into(),
            attachments: vec![],
            imessage: None,
            source: None,
        }],
        packaging_stem_suffix: None,
    }
}

fn sample_doc_for(handle: &str, guid: &str) -> ConversationDocument {
    let mut doc = sample_doc();
    doc.conversation.chat_identifier = handle.into();
    doc.conversation.participants[0].handle = handle.into();
    doc.messages[0].guid = guid.into();
    doc.messages[0].sender_handle = Some(handle.into());
    doc
}

fn write_jsonl(dir: &Path, doc: &ConversationDocument) {
    let stem = doc.filename_stem();
    let path = dir.join(format!("{stem}.jsonl"));
    let mut f = fs::File::create(&path).unwrap();
    let header = json!({
        "schema_version": doc.schema_version,
        "export": doc.export,
        "conversation": doc.conversation,
    });
    writeln!(f, "{}", serde_json::to_string(&header).unwrap()).unwrap();
    for msg in &doc.messages {
        writeln!(f, "{}", serde_json::to_string(msg).unwrap()).unwrap();
    }
}

fn text_only_config(dir: &Path, base_url: String) -> VaultPushConfig {
    VaultPushConfig {
        input: dir.to_path_buf(),
        base_url,
        username: "alice".into(),
        key: "mv_test".into(),
        mode: "append".into(),
        continue_on_error: true,
        force: false,
        skip_attachments: false,
        verify_digests: false,
        trust_export: false,

        max_retries: 0,
        batch_size: 50,
        asset_upload_workers: 1,
        asset_multipart_threshold: vault_push::MAX_PROXY_BODY_BYTES,
        asset_max_bytes: vault_push::DEFAULT_ASSET_MAX_BYTES,
        report_path: Some(dir.join("vault-push-report.json")),
        log_path: Some(dir.join("vault-push.log")),
        journal_path: Some(dir.join(".vault-import-state.jsonl")),
        cancel: None,
        contact_name_mode: "fill_missing".into(),
        import_id: None,
    }
}

#[test]
fn authenticate_and_push_text_only_conversation() {
    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true,
            "sources": ["sms-backup-restore"]
        }));
    });
    let _start_import = server.mock(|when, then| {
        when.method(POST).path("/v1/imports");
        then.status(200).json_body(json!({ "ok": true, "id": 42 }));
    });
    let _complete_import = server.mock(|when, then| {
        when.method(POST).path("/v1/imports/42/complete");
        then.status(200).json_body(json!({
            "ok": true,
            "id": 42,
            "status": "completed",
            "message_count": 1,
            "attachment_count": 0,
            "bytes_uploaded": 0
        }));
    });
    let import = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/import")
            .query_param("import_id", "42");
        then.status(200).json_body(json!({
            "ok": true,
            "source": "sms-backup-restore",
            "account": "acct-1",
            "messages": 1,
            "messages_appended": 1,
            "conversations": 1,
            "attachments": 0,
            "assets_copied": 0,
            "assets_missing": 0,
            "mode": "append"
        }));
    });

    let info = authenticate(&server.base_url(), "mv_test", "alice").unwrap();
    assert_eq!(info.account_id, "acct-1");

    let dir = tempdir().unwrap();
    write_jsonl(dir.path(), &sample_doc());

    let cfg = text_only_config(dir.path(), server.base_url());
    let report = run(&cfg, None).unwrap();
    assert!(report.ok);
    assert_eq!(report.conversations_ok, 1);
    let report_json = serde_json::to_value(&report).unwrap();
    assert!(report_json.get("elapsed_ms").and_then(|v| v.as_u64()).is_some());
    import.assert();

    // Second run should skip via journal.
    let report2 = run(&cfg, None).unwrap();
    assert!(report2.ok);
    assert_eq!(report2.conversations_skipped, 1);
}

#[test]
fn reuses_supplied_import_session_without_starting_or_completing_one() {
    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true
        }));
    });
    let start = server.mock(|when, then| {
        when.method(POST).path("/v1/imports");
        then.status(200).json_body(json!({
            "ok": true,
            "id": 42
        }));
    });
    let complete = server.mock(|when, then| {
        when.method(POST).path("/v1/imports/99/complete");
        then.status(200).json_body(json!({
            "ok": true,
            "id": 99,
            "status": "completed",
            "message_count": 1,
            "attachment_count": 0,
            "bytes_uploaded": 0
        }));
    });
    let import = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/import")
            .query_param("import_id", "99");
        then.status(200).json_body(json!({
            "ok": true,
            "source": "sms-backup-restore",
            "account": "acct-1",
            "messages": 1,
            "messages_appended": 1,
            "conversations": 1,
            "attachments": 0,
            "assets_copied": 0,
            "assets_missing": 0,
            "mode": "append"
        }));
    });

    let dir = tempdir().unwrap();
    write_jsonl(dir.path(), &sample_doc());

    let cfg = VaultPushConfig {
        import_id: Some(99),
        ..text_only_config(dir.path(), server.base_url())
    };
    let report = run(&cfg, None).unwrap();

    assert!(report.ok);
    assert_eq!(report.conversations_ok, 1);
    assert_eq!(start.hits(), 0, "push must not start a new session");
    assert_eq!(complete.hits(), 0, "push must not complete a reused session");
    import.assert();
}

#[test]
fn aggregates_multiple_conversations_into_one_import_request() {
    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true
        }));
    });
    let import = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/import")
            .body_contains("+15555550101")
            .body_contains("+15555550102");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": 2,
            "messages_appended": 2,
            "conversations": 2
        }));
    });

    let dir = tempdir().unwrap();
    let first = sample_doc();
    let second = sample_doc_for("+15555550102", "guid-2");
    write_jsonl(dir.path(), &first);
    write_jsonl(dir.path(), &second);

    let report = run(&text_only_config(dir.path(), server.base_url()), None).unwrap();

    assert!(report.ok);
    assert_eq!(report.conversations_ok, 2);
    assert_eq!(report.messages, 2);
    assert_eq!(import.hits(), 1);
    let log = fs::read_to_string(dir.path().join("vault-push.log")).unwrap();
    assert!(log.contains("IMPORT_REQUEST ok"));
    assert!(log.contains("conversations=2 messages=2"));
}

#[test]
fn flushes_at_message_limit_and_replaces_only_first_request() {
    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true
        }));
    });
    let replace = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/import")
            .query_param("mode", "replace")
            .body_contains("+15555550101")
            .body_contains("+15555550102");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": 2,
            "messages_appended": 2,
            "conversations": 2
        }));
    });
    let append = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/import")
            .query_param("mode", "append")
            .body_contains("+15555550103");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": 1,
            "messages_appended": 1,
            "conversations": 1
        }));
    });

    let dir = tempdir().unwrap();
    write_jsonl(dir.path(), &sample_doc());
    write_jsonl(dir.path(), &sample_doc_for("+15555550102", "guid-2"));
    write_jsonl(dir.path(), &sample_doc_for("+15555550103", "guid-3"));
    let mut cfg = text_only_config(dir.path(), server.base_url());
    cfg.mode = "replace".into();
    cfg.batch_size = 2;

    let report = run(&cfg, None).unwrap();

    assert!(report.ok);
    assert_eq!(report.conversations_ok, 3);
    assert_eq!(replace.hits(), 1);
    assert_eq!(append.hits(), 1);
}

#[test]
fn failed_combined_request_only_fails_its_files() {
    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true
        }));
    });
    let failed = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/import")
            .body_contains("+15555550101")
            .body_contains("+15555550102");
        then.status(500).json_body(json!({
            "ok": false,
            "error": "intentional batch failure"
        }));
    });
    let succeeded = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/import")
            .body_contains("+15555550103");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": 1,
            "messages_appended": 1,
            "conversations": 1
        }));
    });

    let dir = tempdir().unwrap();
    write_jsonl(dir.path(), &sample_doc());
    write_jsonl(dir.path(), &sample_doc_for("+15555550102", "guid-2"));
    write_jsonl(dir.path(), &sample_doc_for("+15555550103", "guid-3"));
    let mut cfg = text_only_config(dir.path(), server.base_url());
    cfg.batch_size = 2;

    let report = run(&cfg, None).unwrap();

    assert!(!report.ok);
    assert_eq!(report.conversations_failed, 2);
    assert_eq!(report.conversations_ok, 1);
    assert_eq!(failed.hits(), 1);
    assert_eq!(succeeded.hits(), 1);
    assert_eq!(
        report
            .results
            .iter()
            .filter(|result| result.status == "failed")
            .count(),
        2
    );
}

#[test]
fn resumes_message_batches_from_compacted_journal() {
    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true
        }));
    });
    let import = server.mock(|when, then| {
        when.method(POST).path("/v1/import");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": 1,
            "messages_appended": 1,
            "conversations": 1
        }));
    });

    let dir = tempdir().unwrap();
    write_jsonl(dir.path(), &sample_doc());
    let cfg = text_only_config(dir.path(), server.base_url());
    run(&cfg, None).unwrap();
    assert_eq!(import.hits(), 1);

    let journal_path = dir.path().join(".vault-import-state.jsonl");
    let compacted = fs::read_to_string(&journal_path).unwrap();
    assert!(compacted.contains("\"event\":\"message_batch_ok\""));
    let without_file_success = compacted
        .lines()
        .filter(|line| !line.contains("\"event\":\"file_ok\""))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&journal_path, format!("{without_file_success}\n")).unwrap();

    let resumed = run(&cfg, None).unwrap();
    assert!(resumed.ok);
    assert_eq!(resumed.conversations_ok, 1);
    assert_eq!(resumed.messages, 0);
    assert_eq!(import.hits(), 1);
}

#[test]
fn profiles_attachment_upload_phases() {
    const ASSET_BYTES: &[u8] = b"attachment profile fixture";

    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true,
            "sources": ["sms-backup-restore"]
        }));
    });
    let digest = hex::encode(Sha256::digest(ASSET_BYTES));
    let head = server.mock(|when, then| {
        when.method("HEAD").path(format!("/v1/assets/{digest}"));
        then.status(404).json_body(json!({
            "ok": false,
            "error": "asset not found"
        }));
    });
    let asset = server.mock(|when, then| {
        when.method(PUT).path(format!("/v1/assets/{digest}"));
        then.status(200).json_body(json!({
            "ok": true,
            "already_present": false
        }));
    });
    let import = server.mock(|when, then| {
        when.method(POST).path("/v1/import");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": 1,
            "messages_appended": 1
        }));
    });

    let dir = tempdir().unwrap();
    let attachment_dir = dir.path().join("attachments");
    fs::create_dir(&attachment_dir).unwrap();
    fs::write(attachment_dir.join("fixture.txt"), ASSET_BYTES).unwrap();
    let mut doc = sample_doc();
    doc.messages[0].attachments.push(IrAttachment {
        path: Some("attachments/fixture.txt".into()),
        original_name: Some("fixture.txt".into()),
        mime_type: Some("text/plain".into()),
        digest_sha256: None,
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: None,
        bytes: None,
    });
    write_jsonl(dir.path(), &doc);

    let report_path = dir.path().join("vault-push-report.json");
    let log_path = dir.path().join("vault-push.log");
    let cfg = VaultPushConfig {
        input: dir.path().to_path_buf(),
        base_url: server.base_url(),
        username: "alice".into(),
        key: "mv_test".into(),
        mode: "append".into(),
        continue_on_error: false,
        force: false,
        skip_attachments: false,
        verify_digests: false,
        trust_export: false,

        max_retries: 0,
        batch_size: 50,
        asset_upload_workers: 2,
        asset_multipart_threshold: vault_push::MAX_PROXY_BODY_BYTES,
        asset_max_bytes: vault_push::DEFAULT_ASSET_MAX_BYTES,
        report_path: Some(report_path.clone()),
        log_path: Some(log_path.clone()),
        journal_path: Some(dir.path().join(".vault-import-state.jsonl")),
        cancel: None,
        contact_name_mode: "fill_missing".into(),
        import_id: None,
    };
    let mut progress_lines = Vec::new();
    let report = {
        let mut progress = |event| {
            if let ProgressEvent::Log(line) = event {
                progress_lines.push(line);
            }
        };
        run(&cfg, Some(&mut progress)).unwrap()
    };

    head.assert();
    asset.assert();
    import.assert();
    let profile = report.results[0].profile.as_ref().unwrap();
    assert_eq!(profile.unique_assets, 1);
    assert_eq!(profile.asset_bytes, ASSET_BYTES.len() as u64);
    assert!(
        progress_lines
            .iter()
            .any(|line| line.starts_with("files ") && line.contains("import time="))
    );

    let persisted_report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(report_path).unwrap()).unwrap();
    assert_eq!(
        persisted_report["results"][0]["profile"]["asset_bytes"],
        ASSET_BYTES.len() as u64
    );
    let persisted_log = fs::read_to_string(log_path).unwrap();
    assert!(persisted_log.contains("attachment_scan_hash_ms="));
    assert!(persisted_log.contains("Import "));
}

#[test]
fn skips_put_when_head_reports_asset_present() {
    const ASSET_BYTES: &[u8] = b"already on vault";

    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true,
            "sources": ["sms-backup-restore"]
        }));
    });
    let digest = hex::encode(Sha256::digest(ASSET_BYTES));
    let head = server.mock(|when, then| {
        when.method("HEAD").path(format!("/v1/assets/{digest}"));
        then.status(200).json_body(json!({
            "ok": true,
            "sha256": digest,
            "assets_path": format!("ab/{digest}.txt"),
            "already_present": true
        }));
    });
    let put = server.mock(|when, then| {
        when.method(PUT).path(format!("/v1/assets/{digest}"));
        then.status(200).json_body(json!({
            "ok": true,
            "already_present": false
        }));
    });
    let import = server.mock(|when, then| {
        when.method(POST).path("/v1/import");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": 1,
            "messages_appended": 1
        }));
    });

    let dir = tempdir().unwrap();
    let attachment_dir = dir.path().join("attachments");
    fs::create_dir(&attachment_dir).unwrap();
    fs::write(attachment_dir.join("fixture.txt"), ASSET_BYTES).unwrap();
    let mut doc = sample_doc();
    doc.messages[0].attachments.push(IrAttachment {
        path: Some("attachments/fixture.txt".into()),
        original_name: Some("fixture.txt".into()),
        mime_type: Some("text/plain".into()),
        digest_sha256: Some(digest.clone()),
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: None,
        bytes: None,
    });
    write_jsonl(dir.path(), &doc);

    let mut cfg = text_only_config(dir.path(), server.base_url());
    cfg.force = true;
    let report = run(&cfg, None).unwrap();

    head.assert();
    assert_eq!(put.hits(), 0, "PUT must be skipped when HEAD says present");
    import.assert();
    assert_eq!(report.assets_uploaded, 0);
    assert_eq!(report.assets_skipped, 1);
}

#[test]
fn multipart_upload_when_over_proxy_threshold() {
    // File larger than the test threshold; mock vault returns a tiny part_size.
    const ASSET_BYTES: &[u8] = b"0123456789abcdef0123456789abcdef01234567"; // 40 bytes

    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true,
            "sources": ["sms-backup-restore"]
        }));
    });
    let digest = hex::encode(Sha256::digest(ASSET_BYTES));
    let head = server.mock(|when, then| {
        when.method("HEAD").path(format!("/v1/assets/{digest}"));
        then.status(404).json_body(json!({
            "ok": false,
            "error": "asset not found"
        }));
    });
    let start = server.mock(|when, then| {
        when.method(POST).path(format!("/v1/assets/{digest}/uploads"));
        then.status(200).json_body(json!({
            "ok": true,
            "upload_id": "up-1",
            "part_size": 16
        }));
    });
    let part1 = server.mock(|when, then| {
        when.method(PUT).path(format!("/v1/assets/{digest}/uploads/up-1/parts/1"));
        then.status(200).json_body(json!({ "ok": true, "part": 1, "bytes": 16 }));
    });
    let part2 = server.mock(|when, then| {
        when.method(PUT).path(format!("/v1/assets/{digest}/uploads/up-1/parts/2"));
        then.status(200).json_body(json!({ "ok": true, "part": 2, "bytes": 16 }));
    });
    let part3 = server.mock(|when, then| {
        when.method(PUT).path(format!("/v1/assets/{digest}/uploads/up-1/parts/3"));
        then.status(200).json_body(json!({ "ok": true, "part": 3, "bytes": 8 }));
    });
    let complete = server.mock(|when, then| {
        when.method(POST).path(format!("/v1/assets/{digest}/uploads/up-1/complete"));
        then.status(200).json_body(json!({
            "ok": true,
            "sha256": digest,
            "assets_path": format!("ab/{digest}"),
            "already_present": false
        }));
    });
    let single_put = server.mock(|when, then| {
        when.method(PUT).path(format!("/v1/assets/{digest}"));
        then.status(200).json_body(json!({
            "ok": true,
            "already_present": false
        }));
    });
    let import = server.mock(|when, then| {
        when.method(POST).path("/v1/import");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": 1,
            "messages_appended": 1
        }));
    });

    let dir = tempdir().unwrap();
    let attachment_dir = dir.path().join("attachments");
    fs::create_dir(&attachment_dir).unwrap();
    fs::write(attachment_dir.join("large.bin"), ASSET_BYTES).unwrap();
    let mut doc = sample_doc();
    doc.messages[0].attachments.push(IrAttachment {
        path: Some("attachments/large.bin".into()),
        original_name: Some("large.bin".into()),
        mime_type: Some("application/octet-stream".into()),
        digest_sha256: Some(digest.clone()),
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: None,
        bytes: None,
    });
    write_jsonl(dir.path(), &doc);

    let mut cfg = text_only_config(dir.path(), server.base_url());
    cfg.force = true;
    cfg.asset_multipart_threshold = 20; // force multipart for 40-byte file
    let report = run(&cfg, None).unwrap();

    head.assert();
    start.assert();
    part1.assert();
    part2.assert();
    part3.assert();
    complete.assert();
    assert_eq!(single_put.hits(), 0, "single PUT must not be used for multipart path");
    import.assert();
    assert_eq!(report.assets_uploaded, 1);
}

#[test]
fn multipart_aborts_on_hash_mismatch_complete() {
    const ASSET_BYTES: &[u8] = b"mismatch-fixture-bytes!!";

    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true,
            "sources": ["sms-backup-restore"]
        }));
    });
    let digest = hex::encode(Sha256::digest(ASSET_BYTES));
    let _head = server.mock(|when, then| {
        when.method("HEAD").path(format!("/v1/assets/{digest}"));
        then.status(404);
    });
    let _start = server.mock(|when, then| {
        when.method(POST).path(format!("/v1/assets/{digest}/uploads"));
        then.status(200).json_body(json!({
            "ok": true,
            "upload_id": "up-bad",
            "part_size": 64
        }));
    });
    let _part = server.mock(|when, then| {
        when.method(PUT)
            .path_contains(format!("/v1/assets/{digest}/uploads/up-bad/parts/"));
        then.status(200).json_body(json!({ "ok": true, "part": 1, "bytes": 24 }));
    });
    let _complete = server.mock(|when, then| {
        when.method(POST).path(format!("/v1/assets/{digest}/uploads/up-bad/complete"));
        then.status(400).json_body(json!({
            "ok": false,
            "error": "sha256 mismatch: claimed abc, got def"
        }));
    });
    let abort = server.mock(|when, then| {
        when.method(DELETE).path(format!("/v1/assets/{digest}/uploads/up-bad"));
        then.status(200).json_body(json!({ "ok": true }));
    });

    let dir = tempdir().unwrap();
    let attachment_dir = dir.path().join("attachments");
    fs::create_dir(&attachment_dir).unwrap();
    fs::write(attachment_dir.join("bad.bin"), ASSET_BYTES).unwrap();
    let mut doc = sample_doc();
    doc.messages[0].attachments.push(IrAttachment {
        path: Some("attachments/bad.bin".into()),
        original_name: Some("bad.bin".into()),
        mime_type: Some("application/octet-stream".into()),
        digest_sha256: Some(digest),
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: None,
        bytes: None,
    });
    write_jsonl(dir.path(), &doc);

    let mut cfg = text_only_config(dir.path(), server.base_url());
    cfg.force = true;
    cfg.continue_on_error = true;
    cfg.asset_multipart_threshold = 8;
    let report = run(&cfg, None).unwrap();
    assert!(!report.ok);
    abort.assert();
}

#[test]
fn authenticate_maps_http_failures_to_typed_errors() {
    let server = MockServer::start();

    let _unauthorized = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(401).body("unauthorized");
    });
    let err = authenticate(&server.base_url(), "bad", "").unwrap_err();
    assert_eq!(err.kind(), "invalid_key");
    assert!(!err.user_message().contains("unauthorized"));
    assert!(err.detail().contains("invalid vault key"));
}

#[test]
fn authenticate_maps_html_and_status_failures() {
    let server = MockServer::start();
    let _html = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200)
            .body("<!DOCTYPE html><html><body>browse ui</body></html>");
    });
    let err = authenticate(&server.base_url(), "mv_test", "").unwrap_err();
    assert_eq!(err.kind(), "wrong_host");
    assert!(err.user_message().contains("website"));
    assert!(err.detail().contains("HTML"));

    // Fresh server for a non-401 status.
    let server = MockServer::start();
    let _forbidden = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(403).body("username does not match vault key");
    });
    let err = authenticate(&server.base_url(), "mv_test", "").unwrap_err();
    assert_eq!(err.kind(), "forbidden");
    assert!(!err.user_message().contains("username does not match"));
    assert!(err.detail().contains("username does not match vault key"));
}

#[test]
fn authenticate_rejects_invalid_url() {
    let err = authenticate("not a url", "mv_test", "").unwrap_err();
    assert_eq!(err.kind(), "invalid_url");
    assert!(matches!(err, AuthError::InvalidUrl { .. }));
}

#[test]
fn verify_digests_fails_on_mismatch() {
    const ASSET_BYTES: &[u8] = b"on-disk bytes";
    let wrong_digest = hex::encode(Sha256::digest(b"other bytes"));

    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true,
            "sources": ["sms-backup-restore"]
        }));
    });
    let put = server.mock(|when, then| {
        when.method(PUT).path_contains("/v1/assets/");
        then.status(200).json_body(json!({ "ok": true, "already_present": false }));
    });

    let dir = tempdir().unwrap();
    let attachment_dir = dir.path().join("attachments");
    fs::create_dir(&attachment_dir).unwrap();
    fs::write(attachment_dir.join("fixture.txt"), ASSET_BYTES).unwrap();
    let mut doc = sample_doc();
    doc.messages[0].attachments.push(IrAttachment {
        path: Some("attachments/fixture.txt".into()),
        original_name: Some("fixture.txt".into()),
        mime_type: Some("text/plain".into()),
        digest_sha256: Some(wrong_digest),
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: None,
        bytes: None,
    });
    write_jsonl(dir.path(), &doc);

    let mut cfg = text_only_config(dir.path(), server.base_url());
    cfg.verify_digests = true;
    cfg.continue_on_error = false;
    let report = run(&cfg, None).unwrap();
    assert!(!report.ok);
    assert_eq!(report.conversations_failed, 1);
    assert_eq!(put.hits(), 0, "mismatch must fail before upload");
}

#[test]
fn shared_attachment_uploaded_once_across_conversations() {
    const ASSET_BYTES: &[u8] = b"shared attachment bytes";
    let digest = hex::encode(Sha256::digest(ASSET_BYTES));

    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true,
            "sources": ["sms-backup-restore"]
        }));
    });
    let head = server.mock(|when, then| {
        when.method("HEAD").path(format!("/v1/assets/{digest}"));
        then.status(404).json_body(json!({ "ok": false, "error": "asset not found" }));
    });
    let put = server.mock(|when, then| {
        when.method(PUT).path(format!("/v1/assets/{digest}"));
        then.status(200).json_body(json!({ "ok": true, "already_present": false }));
    });
    let import = server.mock(|when, then| {
        when.method(POST).path("/v1/import");
        then.status(200).json_body(json!({
            "ok": true,
            "messages": 2,
            "messages_appended": 2
        }));
    });

    let dir = tempdir().unwrap();
    let attachment_dir = dir.path().join("attachments");
    fs::create_dir(&attachment_dir).unwrap();
    fs::write(attachment_dir.join("shared.txt"), ASSET_BYTES).unwrap();

    let mut doc_a = sample_doc_for("+15555550101", "guid-a");
    doc_a.messages[0].attachments.push(IrAttachment {
        path: Some("attachments/shared.txt".into()),
        original_name: Some("shared.txt".into()),
        mime_type: Some("text/plain".into()),
        digest_sha256: Some(digest.clone()),
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: None,
        bytes: None,
    });
    let mut doc_b = sample_doc_for("+15555550102", "guid-b");
    doc_b.messages[0].attachments.push(IrAttachment {
        path: Some("attachments/shared.txt".into()),
        original_name: Some("shared.txt".into()),
        mime_type: Some("text/plain".into()),
        digest_sha256: Some(digest),
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        size_bytes: None,
        bytes: None,
    });
    write_jsonl(dir.path(), &doc_a);
    write_jsonl(dir.path(), &doc_b);

    let cfg = text_only_config(dir.path(), server.base_url());
    let report = run(&cfg, None).unwrap();
    assert!(report.ok);
    assert_eq!(report.conversations_ok, 2);
    assert_eq!(put.hits(), 1, "shared digest must upload once");
    assert!(head.hits() >= 1);
    import.assert();
    assert_eq!(report.assets_uploaded, 1);
}
