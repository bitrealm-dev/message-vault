//! Write message-ir JSONL conversations for the demo bundle.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Duration, FixedOffset, TimeZone, Utc};
use message_ir::{
    ConversationHeader, ConversationMeta, ConversationStats, ExportMeta, IrAttachment,
    IrConversationType, IrDirection, IrImessage, IrMessage, IrMessageKind, IrParticipant, IrService,
    SCHEMA_VERSION,
};
use rand::Rng;
use rand::seq::{IndexedRandom, SliceRandom};
use serde_json::json;

use crate::assets::{JPG_PHOTOS, OTHER_ATTACHMENTS};
use crate::config::SeedConfig;
use crate::corpus::Corpus;
use crate::personas::{
    EMPTY_GROUP_HANDLE, EMPTY_THREAD_HANDLE, ORPHAN_SENDER, OWNER_PHONE, Contact, Roster,
    Unassigned,
};

const IMESSAGE_SOURCE: &str = "imessage";
const SBR_SOURCE: &str = "sms-backup-restore";
const WHATSAPP_SOURCE: &str = "whatsapp";

#[derive(Clone, Copy)]
enum SourceFlavor {
    IMessage,
    SmsBackupRestore,
    Whatsapp,
}

#[derive(Debug, Default)]
pub struct GenStats {
    pub contacts: usize,
    pub conversation_files: usize,
    pub messages: usize,
    pub attachment_refs: usize,
    pub groups: usize,
}

const TAPBACK_KINDS: &[&str] = &[
    "loved",
    "liked",
    "disliked",
    "laughed",
    "emphasized",
    "questioned",
    "emoji",
];

const PHOTO_CAPTIONS: &[&str] = &[
    "Check this out",
    "Thought you'd like this",
    "From yesterday",
    "Saw this and thought of you",
    "",
];

const EMOJI_ONLY: &[&str] = &["👍", "😂", "❤️", "🎉", "😊"];

fn export_meta(source: &str) -> ExportMeta {
    ExportMeta {
        source: source.into(),
        tool: "demo-seed".into(),
        tool_version: "0.2.0".into(),
        owner_handle: Some(OWNER_PHONE.into()),
        owner_display_name: Some("Me".into()),
    }
}

pub fn write_all(
    imessage_staging: &Path,
    sbr_staging: &Path,
    whatsapp_staging: &Path,
    roster: &Roster,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    attachment_digests: &HashMap<String, (String, u64)>,
) -> Result<GenStats> {
    let mut stats = GenStats {
        contacts: roster.contacts.len(),
        groups: roster.groups.len(),
        ..Default::default()
    };

    clear_jsonl(imessage_staging)?;
    clear_jsonl(sbr_staging)?;
    clear_jsonl(whatsapp_staging)?;

    let mut one_to_one: Vec<&Contact> = roster
        .contacts
        .iter()
        .filter(|c| !c.phones.is_empty() && c.has_one_to_one())
        .collect();
    one_to_one.shuffle(rng);

    let overlap_n = cfg.sources.overlap_count.min(one_to_one.len());
    let (overlap, rest) = one_to_one.split_at(overlap_n);
    let android_n = ((rest.len() as f64) * cfg.sources.android_only_fraction)
        .round()
        .clamp(0.0, rest.len() as f64) as usize;
    let (android_only, imessage_only) = rest.split_at(android_n);

    for contact in imessage_only {
        write_individual(
            imessage_staging,
            contact.primary_phone(),
            contact.display_hint(),
            contact.span_years,
            contact.message_count(),
            SourceFlavor::IMessage,
            cfg,
            corpus,
            rng,
            &mut stats,
            attachment_digests,
        )?;
    }

    for contact in android_only {
        write_individual(
            sbr_staging,
            contact.primary_phone(),
            contact.display_hint(),
            contact.span_years,
            contact.message_count(),
            SourceFlavor::SmsBackupRestore,
            cfg,
            corpus,
            rng,
            &mut stats,
            attachment_digests,
        )?;
    }

    for contact in overlap {
        write_overlap_individual(
            imessage_staging,
            sbr_staging,
            contact,
            cfg,
            corpus,
            rng,
            &mut stats,
            attachment_digests,
        )?;
    }

    for ua in &roster.unassigned {
        let count = rng.random_range(4..16);
        write_unassigned(
            imessage_staging,
            ua,
            count,
            cfg,
            corpus,
            rng,
            &mut stats,
            attachment_digests,
        )?;
    }

    for group in &roster.groups {
        write_group(
            imessage_staging,
            roster,
            group,
            cfg,
            corpus,
            rng,
            &mut stats,
            attachment_digests,
        )?;
    }

    write_orphaned(imessage_staging, cfg, corpus, rng, &mut stats)?;

    if cfg.edge_cases.empty_individual {
        write_header_only(
            imessage_staging,
            EMPTY_THREAD_HANDLE,
            IrConversationType::Individual,
            &[],
            IMESSAGE_SOURCE,
        )?;
        stats.conversation_files += 1;
    }
    if cfg.edge_cases.empty_group {
        write_header_only(
            imessage_staging,
            EMPTY_GROUP_HANDLE,
            IrConversationType::Group,
            &["+12125554503", "+13035555604"],
            IMESSAGE_SOURCE,
        )?;
        stats.conversation_files += 1;
    }

    // WhatsApp slice: same phone chat id, separate platform handle on import.
    for contact in roster.contacts.iter().filter(|c| c.has_whatsapp) {
        let wa_count = contact.message_count().clamp(20, 80);
        write_individual(
            whatsapp_staging,
            contact.primary_phone(),
            contact.display_hint(),
            contact.span_years.min(3.0),
            wa_count,
            SourceFlavor::Whatsapp,
            cfg,
            corpus,
            rng,
            &mut stats,
            attachment_digests,
        )?;
    }

    Ok(stats)
}

fn clear_jsonl(staging: &Path) -> Result<()> {
    if !staging.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|e| e == "jsonl" || e == "json")
        {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_individual(
    staging: &Path,
    chat_id: &str,
    display: String,
    span_years: f64,
    msg_count: usize,
    flavor: SourceFlavor,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    attachment_digests: &HashMap<String, (String, u64)>,
) -> Result<()> {
    let source = source_id(flavor);
    let display_name = if display.is_empty() {
        None
    } else {
        Some(display)
    };
    let participants = vec![IrParticipant {
        handle: chat_id.into(),
        display_name,
        handle_type: None,
    }];
    let path = staging.join(sanitize_filename(chat_id) + ".jsonl");
    let mut file = open_jsonl(&path)?;
    write_conversation_header(
        &mut file,
        chat_id,
        IrConversationType::Individual,
        None,
        participants,
        msg_count,
        source,
    )?;

    let timestamps = bursty_timestamps(msg_count, span_years, sample_direct_day_burst, rng);
    let mut origin_guid: Option<String> = None;
    for (i, &ts) in timestamps.iter().enumerate() {
        let from_me = i % 3 != 0;
        let guid = format!("{}1to1-{chat_id}-{i}", guid_prefix(flavor));
        let mut msg = text_message(&guid, ts, from_me, chat_id, cfg, corpus, rng, flavor);
        match flavor {
            SourceFlavor::IMessage => {
                decorate_message(
                    &mut msg,
                    i,
                    msg_count,
                    chat_id,
                    from_me,
                    cfg,
                    rng,
                    stats,
                    &mut origin_guid,
                    attachment_digests,
                );
            }
            SourceFlavor::SmsBackupRestore => {
                decorate_android_message(&mut msg, i, msg_count, cfg, rng, stats, attachment_digests);
            }
            SourceFlavor::Whatsapp => {
                // Keep WhatsApp threads simple (no Apple decorations).
            }
        }
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_overlap_individual(
    imessage_staging: &Path,
    sbr_staging: &Path,
    contact: &Contact,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    attachment_digests: &HashMap<String, (String, u64)>,
) -> Result<()> {
    let chat_id = contact.primary_phone();
    let display = contact.display_hint();
    let msg_count = contact.message_count().max(1);
    let span_years = contact.span_years;
    let shared_n = ((msg_count as f64) * cfg.sources.overlap_shared_fraction)
        .round()
        .clamp(1.0, msg_count as f64) as usize;
    let extra_lo = cfg.sources.overlap_android_extra_min;
    let extra_hi = cfg.sources.overlap_android_extra_max.max(extra_lo + 1);
    let extra_n = rng.random_range(extra_lo..extra_hi);

    let display_name = if display.is_empty() {
        None
    } else {
        Some(display)
    };
    let participants = |dn: Option<String>| {
        vec![IrParticipant {
            handle: chat_id.into(),
            display_name: dn,
            handle_type: None,
        }]
    };

    let timestamps = bursty_timestamps(msg_count, span_years, sample_direct_day_burst, rng);
    let mut shared: Vec<(i64, bool, String)> = Vec::with_capacity(shared_n);
    for i in 0..shared_n {
        let ts = timestamps[i];
        let from_me = i % 3 != 0;
        let text = format!("Shared demo message {i} with {chat_id}");
        shared.push((ts, from_me, text));
    }

    // --- iMessage tree: shared plain rows + remaining decorated rows ---
    {
        let path = imessage_staging.join(sanitize_filename(chat_id) + ".jsonl");
        let mut file = open_jsonl(&path)?;
        write_conversation_header(
            &mut file,
            chat_id,
            IrConversationType::Individual,
            None,
            participants(display_name.clone()),
            msg_count,
            IMESSAGE_SOURCE,
        )?;
        let mut origin_guid: Option<String> = None;
        for (i, (ts, from_me, text)) in shared.iter().enumerate() {
            let guid = format!("1to1-{chat_id}-{i}");
            let msg = IrMessage {
                guid,
                timestamp_unix_ms: *ts,
                direction: if *from_me {
                    IrDirection::Outgoing
                } else {
                    IrDirection::Incoming
                },
                service: IrService::IMessage,
                message_kind: IrMessageKind::IMessage,
                sender_handle: if *from_me {
                    None
                } else {
                    Some(chat_id.into())
                },
                sender_display_name: None,
                subject: None,
                text: text.clone(),
                attachments: vec![],
                imessage: None,
                source: None,
            };
            write_message(&mut file, msg)?;
            stats.messages += 1;
        }
        for i in shared_n..msg_count {
            let ts = timestamps[i];
            let from_me = i % 3 != 0;
            let guid = format!("1to1-{chat_id}-{i}");
            let mut msg = text_message(
                &guid,
                ts,
                from_me,
                chat_id,
                cfg,
                corpus,
                rng,
                SourceFlavor::IMessage,
            );
            decorate_message(
                &mut msg,
                i,
                msg_count,
                chat_id,
                from_me,
                cfg,
                rng,
                stats,
                &mut origin_guid,
                attachment_digests,
            );
            write_message(&mut file, msg)?;
            stats.messages += 1;
        }
        stats.conversation_files += 1;
    }

    // --- Android tree: same shared rows + unique extras ---
    {
        let android_total = shared_n + extra_n;
        let path = sbr_staging.join(sanitize_filename(chat_id) + ".jsonl");
        let mut file = open_jsonl(&path)?;
        write_conversation_header(
            &mut file,
            chat_id,
            IrConversationType::Individual,
            None,
            participants(display_name),
            android_total,
            SBR_SOURCE,
        )?;
        for (i, (ts, from_me, text)) in shared.iter().enumerate() {
            let guid = format!("sbr-shared-{chat_id}-{i}");
            let msg = IrMessage {
                guid,
                timestamp_unix_ms: *ts,
                direction: if *from_me {
                    IrDirection::Outgoing
                } else {
                    IrDirection::Incoming
                },
                service: IrService::Sms,
                message_kind: IrMessageKind::Sms,
                sender_handle: if *from_me {
                    None
                } else {
                    Some(chat_id.into())
                },
                sender_display_name: None,
                subject: None,
                text: text.clone(),
                attachments: vec![],
                imessage: None,
                source: None,
            };
            write_message(&mut file, msg)?;
            stats.messages += 1;
        }
        let base_ts = shared
            .last()
            .map(|(ts, _, _)| *ts)
            .or_else(|| timestamps.last().copied())
            .unwrap_or_else(|| Utc::now().timestamp_millis());
        for j in 0..extra_n {
            let ts = base_ts + ((j as i64) + 1) * 60_000;
            let from_me = j % 4 == 0;
            let guid = format!("sbr-extra-{chat_id}-{j}");
            let mut msg = text_message(
                &guid,
                ts,
                from_me,
                chat_id,
                cfg,
                corpus,
                rng,
                SourceFlavor::SmsBackupRestore,
            );
            decorate_android_message(&mut msg, j, extra_n, cfg, rng, stats, attachment_digests);
            write_message(&mut file, msg)?;
            stats.messages += 1;
        }
        stats.conversation_files += 1;
    }

    Ok(())
}

fn source_id(flavor: SourceFlavor) -> &'static str {
    match flavor {
        SourceFlavor::IMessage => IMESSAGE_SOURCE,
        SourceFlavor::SmsBackupRestore => SBR_SOURCE,
        SourceFlavor::Whatsapp => WHATSAPP_SOURCE,
    }
}

fn guid_prefix(flavor: SourceFlavor) -> &'static str {
    match flavor {
        SourceFlavor::IMessage => "",
        SourceFlavor::SmsBackupRestore => "sbr-",
        SourceFlavor::Whatsapp => "wa-",
    }
}

fn write_unassigned(
    staging: &Path,
    ua: &Unassigned,
    msg_count: usize,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    attachment_digests: &HashMap<String, (String, u64)>,
) -> Result<()> {
    let chat_id = &ua.handle;
    let participants = vec![IrParticipant {
        handle: chat_id.clone(),
        display_name: ua.name_alias.clone(),
        handle_type: None,
    }];
    let fname = if ua.email_only {
        format!("email-{}.jsonl", chat_id.replace('@', "_at_"))
    } else {
        sanitize_filename(chat_id) + ".jsonl"
    };
    let path = staging.join(fname);
    let mut file = open_jsonl(&path)?;
    write_conversation_header(
        &mut file,
        chat_id,
        IrConversationType::Individual,
        None,
        participants,
        msg_count,
        IMESSAGE_SOURCE,
    )?;

    let span_years = 1.5;
    let timestamps = bursty_timestamps(msg_count, span_years, sample_direct_day_burst, rng);
    for (i, &ts) in timestamps.iter().enumerate() {
        let from_me = i % 4 == 0;
        let guid = format!("unassigned-{chat_id}-{i}");
        let mut msg = text_message(
            &guid,
            ts,
            from_me,
            chat_id,
            cfg,
            corpus,
            rng,
            SourceFlavor::IMessage,
        );
        if i == 2 && ua.name_alias.is_some() && !from_me {
            msg.sender_handle = Some(String::new());
        }
        if should_attach_jpg(i, msg_count, cfg) {
            add_jpg_attachment(&mut msg, i, stats, attachment_digests);
        }
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

fn write_group(
    staging: &Path,
    roster: &Roster,
    group: &crate::personas::GroupSpec,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    attachment_digests: &HashMap<String, (String, u64)>,
) -> Result<()> {
    let chat_id = group_chat_id(group.index);
    let participants: Vec<IrParticipant> = if group.phone_only {
        group
            .phone_only_handles
            .iter()
            .map(|h| IrParticipant {
                handle: h.clone(),
                display_name: None,
                handle_type: None,
            })
            .collect()
    } else {
        group
            .member_idxs
            .iter()
            .filter_map(|&i| roster.contacts.get(i))
            .map(|c| {
                let hint = c.display_hint();
                IrParticipant {
                    handle: c.primary_phone().into(),
                    display_name: if hint.is_empty() { None } else { Some(hint) },
                    handle_type: None,
                }
            })
            .collect()
    };
    if participants.len() < 2 && !group.phone_only {
        return Ok(());
    }

    let handles: Vec<String> = participants.iter().map(|p| p.handle.clone()).collect();
    let msg_count = ((group.msgs_per_year * group.span_years).round() as isize)
        .max(1) as usize;
    let timestamps = bursty_timestamps(msg_count, group.span_years, sample_group_day_burst, rng);
    let path = staging.join(format!("group-{:03}.jsonl", group.index));
    let mut file = open_jsonl(&path)?;
    write_conversation_header(
        &mut file,
        &chat_id,
        IrConversationType::Group,
        group.title.clone(),
        participants,
        msg_count + usize::from(group.index == 0),
        IMESSAGE_SOURCE,
    )?;

    // First group: synthetic rename announcement.
    if group.index == 0 {
        let ann_ts = timestamps
            .first()
            .copied()
            .unwrap_or_else(|| Utc::now().timestamp_millis())
            - 60_000;
        let mut ann = text_message(
            "grp-0-rename",
            ann_ts,
            true,
            OWNER_PHONE,
            cfg,
            corpus,
            rng,
            SourceFlavor::IMessage,
        );
        ann.text.clear();
        ann.message_kind = IrMessageKind::Announcement;
        let im = ann.imessage.get_or_insert_with(IrImessage::default);
        im.announcement = Some("Demo User named the conversation “Weekend Trip”.".into());
        write_message(&mut file, ann)?;
        stats.messages += 1;
    }

    let mut origin_guid: Option<String> = None;
    for i in 0..msg_count {
        let from_me = i % 7 == 0;
        let sender = if from_me {
            None
        } else {
            Some(handles[i % handles.len()].clone())
        };
        let guid = format!("grp-{}-{i}", group.index);
        let mut msg = text_message(
            &guid,
            timestamps[i],
            from_me,
            sender.as_deref().unwrap_or(OWNER_PHONE),
            cfg,
            corpus,
            rng,
            SourceFlavor::IMessage,
        );
        msg.sender_handle = sender;
        if should_attach_jpg(i, msg_count, cfg) {
            add_jpg_attachment(&mut msg, i + group.index, stats, attachment_digests);
        } else if should_attach_other(i, msg_count, cfg) {
            add_attachment(
                &mut msg,
                i,
                stats,
                OTHER_ATTACHMENTS,
                attachment_digests,
            );
        }
        if cfg.messages.tapback_stride > 0
            && i % cfg.messages.tapback_stride == 0
            && msg_count >= 10
            && !handles.is_empty()
        {
            let reactor = &handles[(i + 1) % handles.len()];
            let kind = TAPBACK_KINDS.choose(rng).unwrap();
            let emoji = if *kind == "emoji" && rng.random_bool(0.5) {
                Some((*EMOJI_ONLY.choose(rng).unwrap()).to_string())
            } else {
                None
            };
            push_tapback(&mut msg, kind, emoji, reactor, false);
        }
        if cfg.messages.reply_stride > 0
            && i % cfg.messages.reply_stride == 0
            && origin_guid.is_some()
        {
            let im = msg.imessage.get_or_insert_with(IrImessage::default);
            im.is_reply = true;
            im.in_reply_to_guid = origin_guid.clone();
            im.thread_originator_part = Some(0);
        }
        if i % (cfg.messages.reply_stride.max(1) + 17) == 0 {
            origin_guid = Some(guid.clone());
        }
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

fn write_orphaned(
    staging: &Path,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    stats: &mut GenStats,
) -> Result<()> {
    let n = cfg.edge_cases.orphaned_messages.max(1);
    let path = staging.join("orphaned.jsonl");
    let mut file = open_jsonl(&path)?;
    write_conversation_header(
        &mut file,
        "orphaned",
        IrConversationType::Individual,
        None,
        vec![],
        n,
        IMESSAGE_SOURCE,
    )?;
    let timestamps = bursty_timestamps(n, 2.0, sample_direct_day_burst, rng);
    for (i, &ts) in timestamps.iter().enumerate() {
        let guid = format!("orphan-{i}");
        let mut msg = text_message(
            &guid,
            ts,
            i % 2 == 0,
            ORPHAN_SENDER,
            cfg,
            corpus,
            rng,
            SourceFlavor::IMessage,
        );
        msg.text = format!("Orphaned message #{i} (no conversation association)");
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

fn write_header_only(
    staging: &Path,
    chat_id: &str,
    conv_type: IrConversationType,
    member_phones: &[&str],
    source: &str,
) -> Result<()> {
    let path = staging.join(format!("empty-{}.jsonl", sanitize_filename(chat_id)));
    let mut file = open_jsonl(&path)?;
    let participants: Vec<IrParticipant> = member_phones
        .iter()
        .map(|h| IrParticipant {
            handle: (*h).into(),
            display_name: None,
            handle_type: None,
        })
        .collect();
    write_conversation_header(&mut file, chat_id, conv_type, None, participants, 0, source)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn decorate_message(
    msg: &mut IrMessage,
    i: usize,
    msg_count: usize,
    peer: &str,
    from_me: bool,
    cfg: &SeedConfig,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    origin_guid: &mut Option<String>,
    attachment_digests: &HashMap<String, (String, u64)>,
) {
    if should_attach_jpg(i, msg_count, cfg) {
        add_jpg_attachment(msg, i, stats, attachment_digests);
        if i > 0 && i.is_multiple_of(40) {
            msg.text = PHOTO_CAPTIONS.choose(rng).unwrap().to_string();
        }
    } else if should_attach_photo_only(i, msg_count, cfg) {
        msg.text.clear();
        add_jpg_attachment(msg, i + 1, stats, attachment_digests);
    } else if should_attach_other(i, msg_count, cfg) {
        add_attachment(msg, i, stats, OTHER_ATTACHMENTS, attachment_digests);
    }
    if cfg.messages.tapback_stride > 0
        && i.is_multiple_of(cfg.messages.tapback_stride)
        && !from_me
        && msg_count >= 20
    {
        push_tapback(msg, "loved", None, peer, false);
    }
    if cfg.messages.reply_stride > 0
        && i.is_multiple_of(cfg.messages.reply_stride)
        && origin_guid.is_some()
        && msg_count >= 25
    {
        let im = msg.imessage.get_or_insert_with(IrImessage::default);
        im.is_reply = true;
        im.in_reply_to_guid = origin_guid.clone();
        im.thread_originator_part = Some(0);
    }
    if i.is_multiple_of(cfg.messages.reply_stride.max(1) + 29) {
        *origin_guid = Some(msg.guid.clone());
        let im = msg.imessage.get_or_insert_with(IrImessage::default);
        im.num_replies = Some(rng.random_range(1..4));
    }
    // Mix SMS/RCS into Apple threads so per-message transport badges are visible.
    if rng.random_bool(cfg.messages.apple_fallback_transport_fraction.clamp(0.0, 1.0)) {
        if rng.random_bool(0.5) {
            msg.service = IrService::Sms;
            if !matches!(msg.message_kind, IrMessageKind::Mms | IrMessageKind::Announcement) {
                msg.message_kind = IrMessageKind::Sms;
            }
        } else {
            msg.service = IrService::Rcs;
            if !matches!(msg.message_kind, IrMessageKind::Mms | IrMessageKind::Announcement) {
                msg.message_kind = IrMessageKind::Sms;
            }
        }
    }
}

fn open_jsonl(path: &Path) -> Result<BufWriter<File>> {
    let f = File::create(path).with_context(|| format!("create {}", path.display()))?;
    Ok(BufWriter::new(f))
}

fn write_conversation_header(
    file: &mut BufWriter<File>,
    chat_id: &str,
    conv_type: IrConversationType,
    group_title: Option<String>,
    participants: Vec<IrParticipant>,
    message_count: usize,
    source: &str,
) -> Result<()> {
    let header = ConversationHeader {
        schema_version: SCHEMA_VERSION,
        export: export_meta(source),
        conversation: ConversationMeta {
            chat_identifier: chat_id.into(),
            conversation_type: conv_type,
            group_title,
            participants,
            stats: ConversationStats {
                message_count: message_count as u64,
                attachment_count: 0,
                first_timestamp_unix_ms: None,
                last_timestamp_unix_ms: None,
            },
        },
    };
    writeln!(file, "{}", serde_json::to_string(&header)?)?;
    Ok(())
}

fn write_message(file: &mut BufWriter<File>, mut msg: IrMessage) -> Result<()> {
    if let Some(im) = msg.imessage.take() {
        msg.imessage = im.into_option();
    }
    writeln!(file, "{}", serde_json::to_string(&msg)?)?;
    Ok(())
}

fn text_message(
    guid: &str,
    timestamp_unix_ms: i64,
    from_me: bool,
    peer: &str,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    flavor: SourceFlavor,
) -> IrMessage {
    let text = if rng.random_bool(cfg.messages.emoji_probability) {
        (*EMOJI_ONLY.choose(rng).unwrap()).to_string()
    } else {
        corpus.pick_message(rng)
    };
    let (service, message_kind) = match flavor {
        SourceFlavor::IMessage => (IrService::IMessage, IrMessageKind::IMessage),
        SourceFlavor::SmsBackupRestore => (IrService::Sms, IrMessageKind::Sms),
        SourceFlavor::Whatsapp => (IrService::Whatsapp, IrMessageKind::Sms),
    };
    IrMessage {
        guid: guid.into(),
        timestamp_unix_ms,
        direction: if from_me {
            IrDirection::Outgoing
        } else {
            IrDirection::Incoming
        },
        service,
        message_kind,
        sender_handle: if from_me { None } else { Some(peer.into()) },
        sender_display_name: None,
        subject: None,
        text,
        attachments: vec![],
        imessage: None,
        source: None,
    }
}

fn decorate_android_message(
    msg: &mut IrMessage,
    i: usize,
    msg_count: usize,
    cfg: &SeedConfig,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    attachment_digests: &HashMap<String, (String, u64)>,
) {
    msg.service = IrService::Sms;
    if should_attach_jpg(i, msg_count, cfg) {
        add_jpg_attachment(msg, i, stats, attachment_digests);
        if i > 0 && i.is_multiple_of(40) {
            msg.text = PHOTO_CAPTIONS.choose(rng).unwrap().to_string();
        }
        msg.message_kind = IrMessageKind::Mms;
    } else if should_attach_other(i, msg_count, cfg) {
        add_attachment(msg, i, stats, OTHER_ATTACHMENTS, attachment_digests);
        msg.message_kind = IrMessageKind::Mms;
    } else {
        msg.message_kind = IrMessageKind::Sms;
    }
}

/// Several messages on active days, quiet gaps, occasional floods.
fn bursty_timestamps<R: Rng, F: FnMut(&mut R) -> usize>(
    total: usize,
    span_years: f64,
    mut sample_burst: F,
    rng: &mut R,
) -> Vec<i64> {
    if total == 0 {
        return Vec::new();
    }
    let now = Utc::now();
    let span_days = ((span_years * 365.25).round() as i64).max(1);
    let start = now - Duration::days(span_days);
    let offset = FixedOffset::west_opt(4 * 3600).unwrap();

    let mut per_day: HashMap<i64, usize> = HashMap::new();
    let mut left = total;
    while left > 0 {
        let burst = sample_burst(rng).min(left);
        // Bias toward recent days; most calendar days stay empty.
        let u: f64 = rng.random::<f64>().powf(0.65);
        let day = ((span_days - 1) as f64 * u).round() as i64;
        *per_day.entry(day.clamp(0, span_days - 1)).or_default() += burst;
        left -= burst;
    }

    let mut days: Vec<(i64, usize)> = per_day.into_iter().collect();
    days.sort_by_key(|(d, _)| *d);

    let mut out = Vec::with_capacity(total);
    for (day, count) in days {
        let day_start = start + Duration::days(day);
        let mut seconds: Vec<i64> = (0..count)
            .map(|_| rng.random_range(8 * 3600..23 * 3600))
            .collect();
        seconds.sort_unstable();
        for (i, secs) in seconds.into_iter().enumerate() {
            // Spread collisions so bursts aren't identical timestamps.
            let spaced = secs + (i as i64) * rng.random_range(8..45);
            let mut dt = day_start + Duration::seconds(spaced.min(23 * 3600 + 3599));
            if let Some(&prev) = out.last()
                && dt.timestamp_millis() <= prev {
                    dt = Utc
                        .timestamp_millis_opt(prev)
                        .single()
                        .unwrap_or(now)
                        + Duration::seconds(rng.random_range(12..90));
                }
            let local = offset.from_utc_datetime(&dt.naive_utc());
            out.push(local.timestamp_millis());
        }
    }
    out.sort_unstable();
    out
}

fn sample_direct_day_burst(rng: &mut impl Rng) -> usize {
    let roll: f64 = rng.random();
    if roll < 0.08 {
        // Heavy day — a lot.
        rng.random_range(12..=45)
    } else if roll < 0.20 {
        // Light day.
        rng.random_range(1..=2)
    } else {
        // Typical active day — several.
        rng.random_range(3..=10)
    }
}

fn sample_group_day_burst(rng: &mut impl Rng) -> usize {
    let roll: f64 = rng.random();
    if roll < 0.10 {
        // Heavy day — a lot.
        rng.random_range(16..=70)
    } else if roll < 0.22 {
        // Light day.
        rng.random_range(1..=2)
    } else {
        // Typical active day — several.
        rng.random_range(3..=12)
    }
}

fn group_chat_id(index: usize) -> String {
    match index % 5 {
        0 => format!("chat{:010}", 1_000_000_000u64 + index as u64),
        1 => format!("+1800555{:04}", 1000 + (index % 9000)),
        2 => format!("+4477009{:05}", 10000 + (index % 80000)),
        3 => format!("+1212555{:04}", 2000 + (index % 7000)),
        _ => format!("chat{:010}", 2_000_000_000u64 + index as u64),
    }
}

fn should_attach_jpg(i: usize, total: usize, cfg: &SeedConfig) -> bool {
    if total < 8 {
        return false;
    }
    let stride = (cfg.messages.jpg_base_stride + total / 50).max(20);
    i > 0 && i.is_multiple_of(stride)
}

fn should_attach_photo_only(i: usize, total: usize, cfg: &SeedConfig) -> bool {
    if total < 20 {
        return false;
    }
    let stride = (cfg.messages.jpg_base_stride * 2 + total / 40).max(40);
    i % stride == 5
}

fn should_attach_other(i: usize, total: usize, cfg: &SeedConfig) -> bool {
    if total < 30 {
        return false;
    }
    let stride = (cfg.messages.other_base_stride + total / 30).max(50);
    i > 0 && i.is_multiple_of(stride)
}

fn add_jpg_attachment(
    msg: &mut IrMessage,
    idx: usize,
    stats: &mut GenStats,
    digests: &HashMap<String, (String, u64)>,
) {
    let photo = &JPG_PHOTOS[idx % JPG_PHOTOS.len()];
    let (sha, size) = digests
        .get(photo.path)
        .map(|(s, z)| (s.clone(), *z))
        .unwrap_or_default();
    let has_sha = !sha.is_empty();
    msg.attachments.push(IrAttachment {
        path: Some(photo.path.into()),
        original_name: Some(photo.original_name.into()),
        mime_type: Some("image/jpeg".into()),
        digest_sha256: if has_sha { Some(sha) } else { None },
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        bytes: None,
        size_bytes: if has_sha { Some(size) } else { None },
    });
    stats.attachment_refs += 1;
}

fn add_attachment(
    msg: &mut IrMessage,
    idx: usize,
    stats: &mut GenStats,
    files: &[(&str, &str, bool)],
    digests: &HashMap<String, (String, u64)>,
) {
    let (path, mime, is_sticker) = files[idx % files.len()];
    let (sha, size) = digests
        .get(path)
        .map(|(s, z)| (s.clone(), *z))
        .unwrap_or_default();
    let has_sha = !sha.is_empty();
    let transcription = if mime.starts_with("audio/") {
        Some("Hey, just leaving a quick voice note.".into())
    } else {
        None
    };
    msg.attachments.push(IrAttachment {
        path: Some(path.into()),
        original_name: Some(path.rsplit('/').next().unwrap_or(path).into()),
        mime_type: Some(mime.into()),
        digest_sha256: if has_sha { Some(sha) } else { None },
        is_sticker,
        transcription,
        sticker_effect: None,
        bytes: None,
        size_bytes: if has_sha { Some(size) } else { None },
    });
    stats.attachment_refs += 1;
}

fn push_tapback(
    msg: &mut IrMessage,
    kind: &str,
    emoji: Option<String>,
    sender: &str,
    from_me: bool,
) {
    let im = msg.imessage.get_or_insert_with(IrImessage::default);
    let mut taps = match im.tapbacks.take() {
        Some(serde_json::Value::Array(items)) => items,
        Some(other) if !other.is_null() => vec![other],
        _ => Vec::new(),
    };
    taps.push(json!({
        "part_index": 0,
        "kind": kind,
        "emoji": emoji,
        "is_from_me": from_me,
        "sender": if from_me { serde_json::Value::Null } else { json!(sender) },
    }));
    im.tapbacks = Some(serde_json::Value::Array(taps));
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '+' => 'p',
            '@' => 'a',
            ':' => '_',
            '/' | '\\' => '_',
            _ if c.is_ascii_alphanumeric() => c,
            _ => '_',
        })
        .collect()
}
