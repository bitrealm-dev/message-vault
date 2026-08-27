//! Round-trip tests for reading and writing conversation documents.

use super::*;
use mail::clean_previous_mail_output;
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, HandleType,
    IrConversationType, IrDirection, IrImessage, IrMessage, IrMessageKind, IrParticipant,
    IrService, SCHEMA_VERSION,
};
use message_vault_io_core::OutputFormat;
use serde_json::{Map, Value, json};
use std::fs;

#[test]
fn writes_json_csv_jsonl_and_eml() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = message_ir::testutil::sample_document("hello ir");

    let json_path = write_format(tmp.path(), OutputFormat::Json, doc.clone()).unwrap();
    assert!(json_path.ends_with("+15555550101.json"));
    let raw = fs::read_to_string(&json_path).unwrap();
    let parsed: ConversationDocument = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed.schema_version, 3);
    assert_eq!(parsed.messages[0].text, "hello ir");
    assert!(parsed.messages[0].attachments.is_empty());
    assert_eq!(
        parsed.messages[0].source.as_ref().unwrap().android_type,
        Some(1)
    );
    assert!(
        parsed.messages[0]
            .source
            .as_ref()
            .unwrap()
            .fields
            .contains_key("address")
    );
    assert_eq!(
        parsed.messages[0].sender_handle.as_deref(),
        Some("+15555550101")
    );
    assert_eq!(parsed.conversation.stats.message_count, 1);
    assert!(!raw.contains("filename_suffix"));
    assert!(!raw.contains("\"bytes\""));
    // Stable null keys present.
    assert!(raw.contains("\"group_title\": null") || raw.contains("\"group_title\":null"));
    assert!(raw.contains("\"imessage\": null") || raw.contains("\"imessage\":null"));

    let jsonl_path = write_format(tmp.path(), OutputFormat::Jsonl, doc.clone()).unwrap();
    assert!(jsonl_path.ends_with("+15555550101.jsonl"));
    let jsonl = fs::read_to_string(&jsonl_path).unwrap();
    let mut lines = jsonl.lines();
    let header: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(header["schema_version"], 3);
    assert!(header.get("messages").is_none());
    assert_eq!(header["conversation"]["stats"]["message_count"], 1);
    let msg_line: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(msg_line["text"], "hello ir");
    assert!(msg_line["source"]["fields"].is_object());
    assert_eq!(msg_line["source"]["android_type"], 1);

    let csv_path = write_format(tmp.path(), OutputFormat::Csv, doc.clone()).unwrap();
    let csv = fs::read_to_string(&csv_path).unwrap();
    assert!(csv.contains("hello ir"));
    assert!(csv.contains("sms-backup-restore"));
    assert!(csv.contains("source_fields_json"));
    assert!(csv.contains("timestamp_unix_ms"));
    assert!(csv.contains("+15555550100")); // owner handle filled

    let _ = clean_previous_mail_output(tmp.path());
    let eml_dir = write_format(tmp.path(), OutputFormat::Eml, doc.clone()).unwrap();
    assert!(eml_dir.is_dir());
}

fn sample_imessage_doc() -> ConversationDocument {
    let mut doc = ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export: ExportMeta {
            source: "imessage".into(),
            tool: "imessage-ir-exporter".into(),
            tool_version: "0.1.0".into(),
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
                handle_type: Some(HandleType::Phone),
            }],
            stats: ConversationStats::default(),
        },
        messages: vec![
            IrMessage {
                guid: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".into(),
                timestamp_unix_ms: 1_400_773_261_000,
                direction: IrDirection::Incoming,
                service: IrService::IMessage,
                message_kind: IrMessageKind::IMessage,
                sender_handle: Some("+15555550101".into()),
                sender_display_name: Some("Sam".into()),
                subject: None,
                text: "hello imessage".into(),
                attachments: vec![],
                imessage: Some(IrImessage {
                    is_reply: true,
                    in_reply_to_guid: Some("parent-guid-1111".into()),
                    thread_originator_part: Some(0),
                    num_replies: Some(2),
                    send_effect: Some("Sent with Balloons".into()),
                    tapbacks: Some(json!([{"part_index": 0, "kind": "loved"}])),
                    parts: Some(json!([{"index": 0, "kind": "run", "text": "hello imessage"}])),
                    ..IrImessage::default()
                }),
                source: None,
            },
            IrMessage {
                guid: "TAPBACK-GUID-0001".into(),
                timestamp_unix_ms: 1_400_773_262_000,
                direction: IrDirection::Outgoing,
                service: IrService::IMessage,
                message_kind: IrMessageKind::Tapback,
                sender_handle: Some("+15555550100".into()),
                sender_display_name: Some("Me".into()),
                subject: None,
                text: "Loved a message".into(),
                attachments: vec![],
                imessage: Some(IrImessage {
                    associated_guid: Some("parent-guid-1111".into()),
                    associated_part: Some(0),
                    tapback_kind: Some("loved".into()),
                    tapback_action: Some("add".into()),
                    in_reply_to_guid: Some("parent-guid-1111".into()),
                    ..IrImessage::default()
                }),
                source: None,
            },
        ],
        packaging_stem_suffix: None,
    };
    doc.finalize_stats();
    doc
}

#[test]
fn imessage_bag_restores_mail_extension_headers() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = sample_imessage_doc();
    let mail_messages = document_to_mail_messages(&doc, tmp.path()).unwrap();

    let reply = &mail_messages[0];
    assert!(reply.is_reply);
    assert_eq!(reply.in_reply_to_guid.as_deref(), Some("parent-guid-1111"));
    assert_eq!(reply.thread_originator_part, Some(0));
    assert_eq!(reply.num_replies, Some(2));
    assert_eq!(reply.send_effect.as_deref(), Some("Sent with Balloons"));
    assert!(reply.tapbacks_json.as_deref().unwrap().contains("loved"));
    assert!(
        reply
            .parts_json
            .as_deref()
            .unwrap()
            .contains("hello imessage")
    );
    assert_eq!(reply.owner_display_name.as_deref(), Some("Me"));

    let tapback = &mail_messages[1];
    assert_eq!(tapback.associated_guid.as_deref(), Some("parent-guid-1111"));
    assert_eq!(tapback.associated_part, Some(0));
    assert_eq!(tapback.tapback_kind.as_deref(), Some("loved"));
    assert_eq!(tapback.tapback_action.as_deref(), Some("add"));
    assert_eq!(tapback.owner_display_name.as_deref(), Some("Me"));
    assert_eq!(tapback.sender_handle.as_deref(), Some("+15555550100"));
}

#[test]
fn unified_csv_headers_for_all_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = sample_imessage_doc();

    let csv_path = write_format(tmp.path(), OutputFormat::Csv, doc.clone()).unwrap();
    let csv = fs::read_to_string(&csv_path).unwrap();
    let header_line = csv.lines().next().unwrap();
    assert_eq!(header_line, CSV_HEADERS.join(","));
    assert!(csv.contains("timestamp_unix_ms"));
    assert!(csv.contains("source_fields_json"));
    assert!(csv.contains("owner_handle"));
    assert!(!header_line.contains("date_ms"));
    assert!(!header_line.contains("contact_name"));
    assert!(!header_line.contains("xml_fields_json"));
    assert!(csv.contains("hello imessage"));
    assert!(csv.contains("Loved a message"));
    assert!(csv.contains("Sent with Balloons"));
    assert!(csv.contains("true")); // is_reply
    assert!(csv.contains("loved"));
    assert!(csv.contains("+15555550100")); // outgoing sender / owner

    let sbr_doc = message_ir::testutil::sample_document("hello ir");
    let sbr_csv_path = write_format(tmp.path(), OutputFormat::Csv, sbr_doc).unwrap();
    let sbr_csv = fs::read_to_string(&sbr_csv_path).unwrap();
    assert_eq!(sbr_csv.lines().next().unwrap(), CSV_HEADERS.join(","));
    assert!(!sbr_csv.contains("xml_fields_json"));
    assert!(sbr_csv.contains("source_fields_json"));
}

#[test]
fn packaging_stem_suffix_affects_filename_not_json() {
    let mut doc = message_ir::testutil::sample_document("hello ir");
    doc.packaging_stem_suffix = Some("__whatsapp".into());
    let tmp = tempfile::tempdir().unwrap();
    let path = write_format(tmp.path(), OutputFormat::Json, doc).unwrap();
    assert!(
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("__whatsapp")
    );
    let raw = fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("filename_suffix"));
    assert!(!raw.contains("__whatsapp"));
}

fn assert_docs_equal_after_normalize(mut a: ConversationDocument, mut b: ConversationDocument) {
    normalize_document_for_compare(&mut a);
    normalize_document_for_compare(&mut b);
    let va = serde_json::to_value(&a).unwrap();
    let vb = serde_json::to_value(&b).unwrap();
    assert_eq!(va, vb);
}

#[test]
fn roundtrip_csv_sms_and_imessage() {
    for doc in [
        message_ir::testutil::sample_document("hello ir"),
        sample_imessage_doc(),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let csv_path = write_conversation_csv(tmp.path(), &doc).unwrap();
        let csv = fs::read_to_string(&csv_path).unwrap();
        // Nested bag cells are empty strings, never the literal `null`.
        let header = csv.lines().next().unwrap();
        let cols: Vec<&str> = header.split(',').collect();
        let bag_names = [
            "source_fields_json",
            "parts_json",
            "edits_json",
            "tapbacks_json",
            "app_json",
        ];
        for line in csv.lines().skip(1) {
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .from_reader(line.as_bytes());
            let record = rdr.records().next().unwrap().unwrap();
            for name in bag_names {
                let idx = cols.iter().position(|c| *c == name).unwrap();
                assert_ne!(
                    record.get(idx),
                    Some("null"),
                    "column {name} must not be literal null"
                );
            }
        }
        assert!(!csv_path.with_extension("meta.json").is_file());

        let back = read_conversation_csv(&csv_path).unwrap();
        assert_docs_equal_after_normalize(doc, back);
    }
}

#[test]
fn csv_omits_trivial_parts_json_keeps_rich_parts() {
    let tmp = tempfile::tempdir().unwrap();
    let mut doc = sample_imessage_doc();
    // First message has a single run equal to text → omit parts_json.
    // Add a second body with multi-part parts that must be kept.
    doc.messages.push(IrMessage {
        guid: "MULTI-PART-GUID".into(),
        timestamp_unix_ms: 1_400_773_263_000,
        direction: IrDirection::Incoming,
        service: IrService::IMessage,
        message_kind: IrMessageKind::IMessage,
        sender_handle: Some("+15555550101".into()),
        sender_display_name: Some("Sam".into()),
        subject: None,
        text: "hello".into(),
        attachments: vec![],
        imessage: Some(IrImessage {
            parts: Some(json!([
                {"index": 0, "kind": "run", "text": "hello"},
                {"index": 1, "kind": "attachment", "transfer_name": "a.jpg"}
            ])),
            ..IrImessage::default()
        }),
        source: None,
    });
    doc.finalize_stats();

    let csv_path = write_conversation_csv(tmp.path(), &doc).unwrap();
    let csv = fs::read_to_string(&csv_path).unwrap();
    let header = csv.lines().next().unwrap();
    let cols: Vec<&str> = header.split(',').collect();
    let parts_idx = cols.iter().position(|c| *c == "parts_json").unwrap();
    let text_idx = cols.iter().position(|c| *c == "text").unwrap();

    let mut saw_trivial_empty = false;
    let mut saw_rich = false;
    for line in csv.lines().skip(1) {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(line.as_bytes());
        let record = rdr.records().next().unwrap().unwrap();
        let text = record.get(text_idx).unwrap_or("");
        let parts = record.get(parts_idx).unwrap_or("");
        if text == "hello imessage" {
            assert!(parts.is_empty(), "trivial parts_json should be empty");
            saw_trivial_empty = true;
        }
        if text == "hello" {
            assert!(
                parts.contains("attachment"),
                "rich parts_json should be kept: {parts}"
            );
            saw_rich = true;
        }
    }
    assert!(saw_trivial_empty && saw_rich);
}

#[test]
fn roundtrip_json_and_jsonl() {
    for doc in [
        message_ir::testutil::sample_document("hello ir"),
        sample_imessage_doc(),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let json_path = write_format(tmp.path(), OutputFormat::Json, doc.clone()).unwrap();
        let back_json = read_conversation_json(&json_path).unwrap();
        assert_docs_equal_after_normalize(doc.clone(), back_json);

        let _ = clean_previous_ir_output(tmp.path());
        let jsonl_path = write_format(tmp.path(), OutputFormat::Jsonl, doc.clone()).unwrap();
        let back_jsonl = read_conversation_jsonl(&jsonl_path).unwrap();
        assert_docs_equal_after_normalize(doc, back_jsonl);
    }
}

#[test]
fn csv_serializes_handle_type_in_cell_and_column() {
    fn first_row_cols(csv: &str) -> (Vec<String>, csv::StringRecord) {
        let mut lines = csv.lines();
        let headers = lines.next().unwrap().to_string();
        let cols: Vec<String> = headers.split(',').map(str::to_string).collect();
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(lines.next().unwrap().as_bytes());
        let row = rdr.records().next().unwrap().unwrap();
        (cols, row)
    }

    let tmp = tempfile::tempdir().unwrap();
    let csv_path = write_conversation_csv(
        tmp.path(),
        &message_ir::testutil::sample_document("hello ir"),
    )
    .unwrap();
    let csv = fs::read_to_string(&csv_path).unwrap();
    let (cols, row) = first_row_cols(&csv);
    let participants_idx = cols.iter().position(|c| c == "participants_json").unwrap();
    let handle_type_idx = cols.iter().position(|c| c == "handle_type").unwrap();
    // Participants cell carries the typed participant.
    assert!(
        row.get(participants_idx)
            .unwrap()
            .contains(r#""handle_type":"phone""#),
        "participants_json must carry handle_type"
    );
    // Dedicated column carries the sender handle type.
    assert_eq!(row.get(handle_type_idx).unwrap(), "phone");
    // Empty sender handle yields an empty cell, never "other".
    let mut doc = message_ir::testutil::sample_document("hello ir");
    doc.messages[0].sender_handle = None;
    let csv_path = write_conversation_csv(tmp.path(), &doc).unwrap();
    let csv = fs::read_to_string(&csv_path).unwrap();
    let (cols, row) = first_row_cols(&csv);
    let handle_type_idx = cols.iter().position(|c| c == "handle_type").unwrap();
    assert_eq!(row.get(handle_type_idx).unwrap(), "");
}

#[test]
fn roundtrip_eml_and_mbox() {
    for doc in [
        message_ir::testutil::sample_document("hello ir"),
        sample_imessage_doc(),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let _ = clean_previous_mail_output(tmp.path());
        let eml_dir = write_format(tmp.path(), OutputFormat::Eml, doc.clone()).unwrap();
        let back_eml = read_conversation_eml_dir(&eml_dir).unwrap();
        // Outgoing EML must carry sender + owner identity headers. Only the
        // iMessage fixture has an outgoing row (the tapback).
        if doc
            .messages
            .iter()
            .any(|m| m.direction == IrDirection::Outgoing)
        {
            let outgoing_eml = fs::read_dir(&eml_dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .find(|p| {
                    let bytes = fs::read(p).unwrap();
                    let text = String::from_utf8_lossy(&bytes);
                    text.contains("X-ME-Direction: outgoing")
                })
                .expect("outgoing eml");
            let outgoing_text = fs::read_to_string(&outgoing_eml).unwrap();
            assert!(outgoing_text.contains("X-ME-Sender-Handle:"));
            assert!(outgoing_text.contains("X-ME-Owner-Handle:"));
        }

        assert_docs_equal_after_normalize(doc.clone(), back_eml);

        let _ = clean_previous_mail_output(tmp.path());
        let mbox_path = write_format(tmp.path(), OutputFormat::Mbox, doc.clone()).unwrap();
        let back_mbox = read_conversation_mbox(&mbox_path).unwrap();
        assert_docs_equal_after_normalize(doc, back_mbox);
    }
}

#[test]
fn sbr_xml_session_writes_smses_backup() {
    let tmp = tempfile::tempdir().unwrap();
    let mut session = SbrBackupSession::create(tmp.path()).unwrap();
    session
        .append_document(&message_ir::testutil::sample_document("hello ir"))
        .unwrap();
    session.append_document(&sample_imessage_doc()).unwrap();
    let path = session.finish().unwrap();
    assert_eq!(path.file_name().unwrap(), "smses.xml");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(r#"count="3""#)); // 1 SMS + 2 iMessage rows
    assert!(text.contains("hello ir"));
    assert!(text.contains(r#"type="1""#) || text.contains(r#"msg_box="1""#));
    assert!(text.contains("hello imessage"));
    // iMessage bags are not mirrored as Apple attrs.
    assert!(!text.contains("X-ME-"));
    assert!(!text.contains("Sent with Balloons"));
    assert!(!text.contains("tapback_kind"));
    // write_format(Xml) is intentionally unsupported for multi-chat.
    assert!(
        write_format(
            tmp.path(),
            OutputFormat::Xml,
            message_ir::testutil::sample_document("hello ir")
        )
        .is_err()
    );
}

#[test]
fn sbr_xml_restores_source_fields_attrs() {
    let mut doc = message_ir::testutil::sample_document("hello ir");
    // SyncTech-shaped bag (same as sms-backup-restore-exporter XmlFields JSON).
    if let Some(source) = doc.messages[0].source.as_mut() {
        let mut attrs = Map::new();
        attrs.insert("protocol".into(), json!("0"));
        attrs.insert("address".into(), json!("+15555550101"));
        attrs.insert("date".into(), json!("1400773261000"));
        attrs.insert("type".into(), json!("1"));
        attrs.insert("body".into(), json!("hello ir"));
        attrs.insert("service_center".into(), json!("+15550009999"));
        attrs.insert("contact_name".into(), json!("Sam"));
        source.fields = {
            let mut m = Map::new();
            m.insert("kind".into(), json!("sms"));
            m.insert("attrs".into(), Value::Object(attrs));
            m
        };
    }
    let tmp = tempfile::tempdir().unwrap();
    let mut session = SbrBackupSession::create(tmp.path()).unwrap();
    session.append_document(&doc).unwrap();
    let path = session.finish().unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(r#"service_center="+15550009999""#));
    assert!(text.contains("hello ir"));
}
