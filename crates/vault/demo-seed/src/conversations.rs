//! Write conversation files for the demo dataset.
//!
//! Each file is JSON Lines: a header object, then one JSON object per message.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Duration, FixedOffset, TimeZone, Utc};
use message_ir::{
    ConversationHeader, ConversationMeta, ConversationStats, ExportMeta, IrAttachment,
    IrConversationType, IrDirection, IrImessage, IrMessage, IrMessageKind, IrParticipant,
    IrService, SCHEMA_VERSION,
};
use rand::Rng;
use rand::seq::{IndexedRandom, SliceRandom};
use serde_json::json;

use crate::assets::{JPG_PHOTOS, OTHER_ATTACHMENTS};
use crate::config::SeedConfig;
use crate::corpus::Corpus;
use crate::personas::{
    Contact, EMPTY_GROUP_HANDLE, EMPTY_THREAD_HANDLE, ORPHAN_SENDER, OWNER_PHONE, Roster,
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

/// Counts of contacts, conversations, messages, and attachments written this run.
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

/// Export metadata stamped on every conversation header.
fn export_meta(source: &str) -> ExportMeta {
    ExportMeta {
        source: source.into(),
        tool: "demo-seed".into(),
        tool_version: "0.2.0".into(),
        owner_handle: Some(OWNER_PHONE.into()),
        owner_display_name: Some("Me".into()),
    }
}

/// Write every conversation file into the three backup folders and return counts.
///
/// One-to-one contacts are split into iMessage-only, Android-only, and overlap.
/// Groups, unassigned handles, empty threads, and WhatsApp copies are written
/// after that.
///
/// # Errors
///
/// Returns an error if a conversation file cannot be created or written.
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

    let mut one_to_one = contacts_with_one_to_one(roster);
    one_to_one.shuffle(rng);

    let overlap_count = cfg.sources.overlap_count.min(one_to_one.len());
    let (overlap, rest) = one_to_one.split_at(overlap_count);
    let android_only_count = crate::rounded_fraction(rest.len(), cfg.sources.android_only_fraction);
    let (android_only, imessage_only) = rest.split_at(android_only_count);

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

    // WhatsApp threads reuse the contact's phone number. Import treats them as
    // a separate platform on the same person.
    for contact in &roster.contacts {
        if !contact.has_whatsapp {
            continue;
        }
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

/// Contacts that have a phone number and a one-to-one conversation.
fn contacts_with_one_to_one(roster: &Roster) -> Vec<&Contact> {
    let mut contacts = Vec::new();
    for contact in &roster.contacts {
        if contact.phones.is_empty() {
            continue;
        }
        if !contact.has_one_to_one() {
            continue;
        }
        contacts.push(contact);
    }
    contacts
}

/// Delete existing `.jsonl` and `.json` files in `staging` so a regenerate does not leave leftovers.
///
/// # Errors
///
/// Returns an error if the directory cannot be listed or a file cannot be deleted.
fn clear_jsonl(staging: &Path) -> Result<()> {
    if !staging.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        let path = entry.path();
        if is_json_conversation_file(&path) {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// True when `path` is a conversation file (`.jsonl` or `.json`).
fn is_json_conversation_file(path: &Path) -> bool {
    match path.extension() {
        Some(extension) => extension == "jsonl" || extension == "json",
        None => false,
    }
}

/// Write one one-to-one conversation for a single backup source.
///
/// # Errors
///
/// Returns an error if the file cannot be created or a line cannot be written.
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
    let display_name = optional_display_name(display);
    let participants = individual_participants(chat_id, display_name);
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

    let timestamps = bursty_timestamps(
        msg_count,
        span_years,
        cfg.reference_time,
        sample_direct_day_burst,
        rng,
    );
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
                decorate_android_message(
                    &mut msg,
                    i,
                    msg_count,
                    cfg,
                    rng,
                    stats,
                    attachment_digests,
                );
            }
            SourceFlavor::Whatsapp => {
                // WhatsApp threads skip iMessage-only fields such as tapbacks and replies.
            }
        }
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

/// Write the same contact into both the iMessage and Android folders.
///
/// Shared messages use the same text and time so import can treat them as
/// duplicates. The Android copy also gets extra messages that only exist there.
///
/// # Errors
///
/// Returns an error if either conversation file cannot be written.
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
    let display_name = optional_display_name(contact.display_hint());
    let msg_count = contact.message_count().max(1);
    let span_years = contact.span_years;
    let shared_raw = (msg_count as f64) * cfg.sources.overlap_shared_fraction;
    let shared_n = shared_raw.round().clamp(1.0, msg_count as f64) as usize;
    let extra_lo = cfg.sources.overlap_android_extra_min;
    let extra_hi = cfg.sources.overlap_android_extra_max.max(extra_lo + 1);
    let extra_n = rng.random_range(extra_lo..extra_hi);

    let timestamps = bursty_timestamps(
        msg_count,
        span_years,
        cfg.reference_time,
        sample_direct_day_burst,
        rng,
    );
    let mut shared: Vec<(i64, bool, String)> = Vec::with_capacity(shared_n);
    for i in 0..shared_n {
        let timestamp = timestamps[i];
        let from_me = i % 3 != 0;
        let text = format!("Shared demo message {i} with {chat_id}");
        shared.push((timestamp, from_me, text));
    }

    write_overlap_imessage(
        imessage_staging,
        chat_id,
        display_name.clone(),
        msg_count,
        &timestamps,
        &shared,
        shared_n,
        cfg,
        corpus,
        rng,
        stats,
        attachment_digests,
    )?;
    write_overlap_android(
        sbr_staging,
        chat_id,
        display_name,
        &timestamps,
        &shared,
        shared_n,
        extra_n,
        cfg,
        corpus,
        rng,
        stats,
        attachment_digests,
    )?;

    Ok(())
}

/// Write the iMessage side of an overlapping conversation: shared rows, then the rest.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
#[allow(clippy::too_many_arguments)]
fn write_overlap_imessage(
    imessage_staging: &Path,
    chat_id: &str,
    display_name: Option<String>,
    msg_count: usize,
    timestamps: &[i64],
    shared: &[(i64, bool, String)],
    shared_n: usize,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    attachment_digests: &HashMap<String, (String, u64)>,
) -> Result<()> {
    let path = imessage_staging.join(sanitize_filename(chat_id) + ".jsonl");
    let mut file = open_jsonl(&path)?;
    write_conversation_header(
        &mut file,
        chat_id,
        IrConversationType::Individual,
        None,
        individual_participants(chat_id, display_name),
        msg_count,
        IMESSAGE_SOURCE,
    )?;
    let mut origin_guid: Option<String> = None;
    for (i, (timestamp, from_me, text)) in shared.iter().enumerate() {
        let guid = format!("1to1-{chat_id}-{i}");
        let msg = overlap_shared_message(
            guid,
            *timestamp,
            *from_me,
            chat_id,
            text.clone(),
            IrService::IMessage,
            IrMessageKind::IMessage,
        );
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    for i in shared_n..msg_count {
        let timestamp = timestamps[i];
        let from_me = i % 3 != 0;
        let guid = format!("1to1-{chat_id}-{i}");
        let mut msg = text_message(
            &guid,
            timestamp,
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
    Ok(())
}

/// Write the Android side of an overlapping conversation: shared rows, then extra messages.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
#[allow(clippy::too_many_arguments)]
fn write_overlap_android(
    sbr_staging: &Path,
    chat_id: &str,
    display_name: Option<String>,
    timestamps: &[i64],
    shared: &[(i64, bool, String)],
    shared_n: usize,
    extra_n: usize,
    cfg: &SeedConfig,
    corpus: &Corpus,
    rng: &mut impl Rng,
    stats: &mut GenStats,
    attachment_digests: &HashMap<String, (String, u64)>,
) -> Result<()> {
    let android_total = shared_n + extra_n;
    let path = sbr_staging.join(sanitize_filename(chat_id) + ".jsonl");
    let mut file = open_jsonl(&path)?;
    write_conversation_header(
        &mut file,
        chat_id,
        IrConversationType::Individual,
        None,
        individual_participants(chat_id, display_name),
        android_total,
        SBR_SOURCE,
    )?;
    for (i, (timestamp, from_me, text)) in shared.iter().enumerate() {
        let guid = format!("sbr-shared-{chat_id}-{i}");
        let msg = overlap_shared_message(
            guid,
            *timestamp,
            *from_me,
            chat_id,
            text.clone(),
            IrService::Sms,
            IrMessageKind::Sms,
        );
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    let base_ts = overlap_android_base_timestamp(shared, timestamps, cfg);
    for j in 0..extra_n {
        let timestamp = base_ts + ((j as i64) + 1) * 60_000;
        let from_me = j % 4 == 0;
        let guid = format!("sbr-extra-{chat_id}-{j}");
        let mut msg = text_message(
            &guid,
            timestamp,
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
    Ok(())
}

/// Timestamp to start extra Android messages from: last shared message, else last iMessage time.
fn overlap_android_base_timestamp(
    shared: &[(i64, bool, String)],
    timestamps: &[i64],
    cfg: &SeedConfig,
) -> i64 {
    if let Some((timestamp, _, _)) = shared.last() {
        return *timestamp;
    }
    if let Some(timestamp) = timestamps.last() {
        return *timestamp;
    }
    cfg.reference_time.timestamp_millis()
}

/// Build a plain shared message used on both the iMessage and Android sides.
fn overlap_shared_message(
    guid: String,
    timestamp_unix_ms: i64,
    from_me: bool,
    chat_id: &str,
    text: String,
    service: IrService,
    message_kind: IrMessageKind,
) -> IrMessage {
    IrMessage {
        guid,
        timestamp_unix_ms,
        direction: if from_me {
            IrDirection::Outgoing
        } else {
            IrDirection::Incoming
        },
        service,
        message_kind,
        sender_handle: if from_me { None } else { Some(chat_id.into()) },
        sender_display_name: None,
        subject: None,
        text,
        attachments: vec![],
        imessage: None,
        source: None,
    }
}

/// `None` when the display name is empty, otherwise `Some`.
fn optional_display_name(display: String) -> Option<String> {
    if display.is_empty() {
        None
    } else {
        Some(display)
    }
}

/// One participant for a one-to-one conversation: the other person's phone or email.
fn individual_participants(chat_id: &str, display_name: Option<String>) -> Vec<IrParticipant> {
    vec![IrParticipant {
        handle: chat_id.into(),
        display_name,
        handle_type: None,
    }]
}

/// Folder name for this backup source (`imessage`, `sms-backup-restore`, or `whatsapp`).
fn source_id(flavor: SourceFlavor) -> &'static str {
    match flavor {
        SourceFlavor::IMessage => IMESSAGE_SOURCE,
        SourceFlavor::SmsBackupRestore => SBR_SOURCE,
        SourceFlavor::Whatsapp => WHATSAPP_SOURCE,
    }
}

/// Prefix on message IDs so Android and WhatsApp IDs do not collide with iMessage.
fn guid_prefix(flavor: SourceFlavor) -> &'static str {
    match flavor {
        SourceFlavor::IMessage => "",
        SourceFlavor::SmsBackupRestore => "sbr-",
        SourceFlavor::Whatsapp => "wa-",
    }
}

/// Write a conversation for a phone or email that has no contact card.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
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
    let participants = individual_participants(chat_id, ua.name_alias.clone());
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
    let timestamps = bursty_timestamps(
        msg_count,
        span_years,
        cfg.reference_time,
        sample_direct_day_burst,
        rng,
    );
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

/// Write one group conversation. The first group starts with a rename announcement.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
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
    let participants = group_participants(roster, group);
    if participants.len() < 2 && !group.phone_only {
        return Ok(());
    }

    let mut handles = Vec::with_capacity(participants.len());
    for participant in &participants {
        handles.push(participant.handle.clone());
    }
    let msg_count = ((group.msgs_per_year * group.span_years).round() as isize).max(1) as usize;
    let timestamps = bursty_timestamps(
        msg_count,
        group.span_years,
        cfg.reference_time,
        sample_group_day_burst,
        rng,
    );
    let path = staging.join(format!("group-{:03}.jsonl", group.index));
    let mut file = open_jsonl(&path)?;
    let header_message_count = if group.index == 0 {
        msg_count + 1
    } else {
        msg_count
    };
    write_conversation_header(
        &mut file,
        &chat_id,
        IrConversationType::Group,
        group.title.clone(),
        participants,
        header_message_count,
        IMESSAGE_SOURCE,
    )?;

    // The first group starts with a rename announcement so the UI has one to show.
    if group.index == 0 {
        let first_message_ts = match timestamps.first() {
            Some(timestamp) => *timestamp,
            None => cfg.reference_time.timestamp_millis(),
        };
        let announcement_ts = first_message_ts - 60_000;
        let mut ann = text_message(
            "grp-0-rename",
            announcement_ts,
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
        let peer = match sender.as_deref() {
            Some(handle) => handle,
            None => OWNER_PHONE,
        };
        let mut msg = text_message(
            &guid,
            timestamps[i],
            from_me,
            peer,
            cfg,
            corpus,
            rng,
            SourceFlavor::IMessage,
        );
        msg.sender_handle = sender;
        if should_attach_jpg(i, msg_count, cfg) {
            add_jpg_attachment(&mut msg, i + group.index, stats, attachment_digests);
        } else if should_attach_other(i, msg_count, cfg) {
            add_attachment(&mut msg, i, stats, OTHER_ATTACHMENTS, attachment_digests);
        }
        if cfg.messages.tapback_stride > 0
            && i % cfg.messages.tapback_stride == 0
            && msg_count >= 10
            && !handles.is_empty()
        {
            let reactor = &handles[(i + 1) % handles.len()];
            let kind = TAPBACK_KINDS.choose(rng).unwrap();
            let emoji = tapback_emoji(kind, rng);
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

/// Participants for a group: named contacts, or phone numbers with no names.
fn group_participants(roster: &Roster, group: &crate::personas::GroupSpec) -> Vec<IrParticipant> {
    if group.phone_only {
        return phone_only_participants(&group.phone_only_handles);
    }
    named_group_participants(roster, &group.member_idxs)
}

/// Group members who are only phone numbers, with no display names.
fn phone_only_participants(handles: &[String]) -> Vec<IrParticipant> {
    let mut participants = Vec::with_capacity(handles.len());
    for handle in handles {
        participants.push(IrParticipant {
            handle: handle.clone(),
            display_name: None,
            handle_type: None,
        });
    }
    participants
}

/// Group members looked up from the roster by index.
fn named_group_participants(roster: &Roster, member_idxs: &[usize]) -> Vec<IrParticipant> {
    let mut participants = Vec::new();
    for &index in member_idxs {
        let Some(contact) = roster.contacts.get(index) else {
            continue;
        };
        let hint = contact.display_hint();
        let display_name = optional_display_name(hint);
        participants.push(IrParticipant {
            handle: contact.primary_phone().into(),
            display_name,
            handle_type: None,
        });
    }
    participants
}

/// Write messages that have a sender but no conversation to attach them to.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
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
    let timestamps = bursty_timestamps(n, 2.0, cfg.reference_time, sample_direct_day_burst, rng);
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

/// Write a conversation header with no messages (empty individual or empty group).
///
/// # Errors
///
/// Returns an error if the file cannot be written.
fn write_header_only(
    staging: &Path,
    chat_id: &str,
    conv_type: IrConversationType,
    member_phones: &[&str],
    source: &str,
) -> Result<()> {
    let path = staging.join(format!("empty-{}.jsonl", sanitize_filename(chat_id)));
    let mut file = open_jsonl(&path)?;
    let mut participants = Vec::with_capacity(member_phones.len());
    for handle in member_phones {
        participants.push(IrParticipant {
            handle: (*handle).into(),
            display_name: None,
            handle_type: None,
        });
    }
    write_conversation_header(&mut file, chat_id, conv_type, None, participants, 0, source)?;
    Ok(())
}

/// Add photos, other files, tapbacks, replies, and occasional SMS/RCS labels to an iMessage.
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
    maybe_mark_as_sms_or_rcs(msg, cfg.messages.apple_fallback_transport_fraction, rng);
}

/// Sometimes mark an iMessage as SMS or RCS so the conversation view can show those labels.
fn maybe_mark_as_sms_or_rcs(msg: &mut IrMessage, fraction: f64, rng: &mut impl Rng) {
    if !rng.random_bool(fraction.clamp(0.0, 1.0)) {
        return;
    }
    if rng.random_bool(0.5) {
        msg.service = IrService::Sms;
    } else {
        msg.service = IrService::Rcs;
    }
    if matches!(
        msg.message_kind,
        IrMessageKind::Mms | IrMessageKind::Announcement
    ) {
        return;
    }
    msg.message_kind = IrMessageKind::Sms;
}

/// Create a buffered writer for a new conversation file.
///
/// # Errors
///
/// Returns an error if the file cannot be created.
fn open_jsonl(path: &Path) -> Result<BufWriter<File>> {
    let f = File::create(path).with_context(|| format!("create {}", path.display()))?;
    Ok(BufWriter::new(f))
}

/// Write the first line of a conversation file: schema, export info, and participants.
///
/// # Errors
///
/// Returns an error if the header cannot be serialized or written.
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

/// Write one message as a JSON line. Empty iMessage extras are dropped first.
///
/// # Errors
///
/// Returns an error if the message cannot be serialized or written.
fn write_message(file: &mut BufWriter<File>, mut msg: IrMessage) -> Result<()> {
    if let Some(im) = msg.imessage.take() {
        msg.imessage = im.into_option();
    }
    writeln!(file, "{}", serde_json::to_string(&msg)?)?;
    Ok(())
}

/// Build a text message with a body from the book, or sometimes only an emoji.
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

/// Add photos or other files to an Android message and mark it as MMS when it has an attachment.
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

/// Spread messages across the date range with busy days, quiet gaps, and occasional floods.
fn bursty_timestamps<R: Rng, F: FnMut(&mut R) -> usize>(
    total: usize,
    span_years: f64,
    reference_time: chrono::DateTime<Utc>,
    mut sample_burst: F,
    rng: &mut R,
) -> Vec<i64> {
    if total == 0 {
        return Vec::new();
    }
    let span_days = ((span_years * 365.25).round() as i64).max(1);
    let start = reference_time - Duration::days(span_days);
    let offset = FixedOffset::west_opt(4 * 3600).unwrap();

    let per_day = assign_messages_to_days(total, span_days, &mut sample_burst, rng);
    let mut days: Vec<(i64, usize)> = per_day.into_iter().collect();
    days.sort_by_key(|(day, _)| *day);

    let mut out = Vec::with_capacity(total);
    for (day, count) in days {
        let day_start = start + Duration::days(day);
        append_day_timestamps(&mut out, day_start, count, offset, reference_time, rng);
    }
    out.sort_unstable();
    out
}

/// Assign bursts of messages to days, preferring recent days.
fn assign_messages_to_days<R: Rng, F: FnMut(&mut R) -> usize>(
    total: usize,
    span_days: i64,
    sample_burst: &mut F,
    rng: &mut R,
) -> HashMap<i64, usize> {
    let mut per_day: HashMap<i64, usize> = HashMap::new();
    let mut remaining = total;
    while remaining > 0 {
        let burst = sample_burst(rng).min(remaining);
        // Prefer recent days. Most days in the span get no messages.
        let unit: f64 = rng.random::<f64>().powf(0.65);
        let day_index = ((span_days - 1) as f64 * unit).round() as i64;
        let day = day_index.clamp(0, span_days - 1);
        *per_day.entry(day).or_default() += burst;
        remaining -= burst;
    }
    per_day
}

/// Place a day's messages between 8am and 11pm, a few seconds apart.
fn append_day_timestamps<R: Rng>(
    out: &mut Vec<i64>,
    day_start: chrono::DateTime<Utc>,
    count: usize,
    offset: FixedOffset,
    reference_time: chrono::DateTime<Utc>,
    rng: &mut R,
) {
    let mut seconds = Vec::with_capacity(count);
    for _ in 0..count {
        seconds.push(rng.random_range(8 * 3600..23 * 3600));
    }
    seconds.sort_unstable();
    for (i, secs) in seconds.into_iter().enumerate() {
        // Nudge messages a few seconds apart so a burst is not one identical timestamp.
        let spacing = (i as i64) * rng.random_range(8..45);
        let spaced = secs + spacing;
        let latest_second = 23 * 3600 + 3599;
        let mut dt = day_start + Duration::seconds(spaced.min(latest_second));
        if let Some(&prev) = out.last()
            && dt.timestamp_millis() <= prev
        {
            let prev_time = Utc
                .timestamp_millis_opt(prev)
                .single()
                .unwrap_or(reference_time);
            let gap = Duration::seconds(rng.random_range(12..90));
            dt = prev_time + gap;
        }
        let local = offset.from_utc_datetime(&dt.naive_utc());
        out.push(local.timestamp_millis());
    }
}

/// How many messages land on one active day in a one-to-one conversation.
fn sample_direct_day_burst(rng: &mut impl Rng) -> usize {
    let roll: f64 = rng.random();
    if roll < 0.08 {
        rng.random_range(12..=45)
    } else if roll < 0.20 {
        rng.random_range(1..=2)
    } else {
        rng.random_range(3..=10)
    }
}

/// How many messages land on one active day in a group conversation.
fn sample_group_day_burst(rng: &mut impl Rng) -> usize {
    let roll: f64 = rng.random();
    if roll < 0.10 {
        rng.random_range(16..=70)
    } else if roll < 0.22 {
        rng.random_range(1..=2)
    } else {
        rng.random_range(3..=12)
    }
}

/// Chat identifier for a group: a mix of `chat…` IDs and phone-looking IDs.
fn group_chat_id(index: usize) -> String {
    match index % 5 {
        0 => format!("chat{:010}", 1_000_000_000u64 + index as u64),
        1 => format!("+1800555{:04}", 1000 + (index % 9000)),
        2 => format!("+4477009{:05}", 10000 + (index % 80000)),
        3 => format!("+1212555{:04}", 2000 + (index % 7000)),
        _ => format!("chat{:010}", 2_000_000_000u64 + index as u64),
    }
}

/// True when this message index should get a JPEG with a caption.
fn should_attach_jpg(i: usize, total: usize, cfg: &SeedConfig) -> bool {
    if total < 8 {
        return false;
    }
    let stride = (cfg.messages.jpg_base_stride + total / 50).max(20);
    i > 0 && i.is_multiple_of(stride)
}

/// True when this message should be a photo with no text.
fn should_attach_photo_only(i: usize, total: usize, cfg: &SeedConfig) -> bool {
    if total < 20 {
        return false;
    }
    let stride = (cfg.messages.jpg_base_stride * 2 + total / 40).max(40);
    i % stride == 5
}

/// True when this message should get a non-JPEG attachment (PDF, audio, sticker, and so on).
fn should_attach_other(i: usize, total: usize, cfg: &SeedConfig) -> bool {
    if total < 30 {
        return false;
    }
    let stride = (cfg.messages.other_base_stride + total / 30).max(50);
    i > 0 && i.is_multiple_of(stride)
}

/// Attach a JPEG from the demo photo list and record it in `stats`.
fn add_jpg_attachment(
    msg: &mut IrMessage,
    idx: usize,
    stats: &mut GenStats,
    digests: &HashMap<String, (String, u64)>,
) {
    let photo = &JPG_PHOTOS[idx % JPG_PHOTOS.len()];
    let (digest_sha256, size_bytes) = digest_fields(digests, photo.path);
    msg.attachments.push(IrAttachment {
        path: Some(photo.path.into()),
        original_name: Some(photo.original_name.into()),
        mime_type: Some("image/jpeg".into()),
        digest_sha256,
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        bytes: None,
        size_bytes,
        missing_reason: None,
    });
    stats.attachment_refs += 1;
}

/// Attach a non-JPEG file. Audio files get a short transcription.
fn add_attachment(
    msg: &mut IrMessage,
    idx: usize,
    stats: &mut GenStats,
    files: &[(&str, &str, bool)],
    digests: &HashMap<String, (String, u64)>,
) {
    let (path, mime, is_sticker) = files[idx % files.len()];
    let (digest_sha256, size_bytes) = digest_fields(digests, path);
    let transcription = if mime.starts_with("audio/") {
        Some("Hey, just leaving a quick voice note.".into())
    } else {
        None
    };
    msg.attachments.push(IrAttachment {
        path: Some(path.into()),
        original_name: Some(filename_from_relative_path(path).into()),
        mime_type: Some(mime.into()),
        digest_sha256,
        is_sticker,
        transcription,
        sticker_effect: None,
        bytes: None,
        size_bytes,
        missing_reason: None,
    });
    stats.attachment_refs += 1;
}

/// Look up the content fingerprint and size for an attachment path, if the file was written.
fn digest_fields(
    digests: &HashMap<String, (String, u64)>,
    path: &str,
) -> (Option<String>, Option<u64>) {
    match digests.get(path) {
        Some((digest, byte_len)) if !digest.is_empty() => (Some(digest.clone()), Some(*byte_len)),
        _ => (None, None),
    }
}

/// Last path segment of `path`, or the whole string if it has no `/`.
fn filename_from_relative_path(path: &str) -> &str {
    match path.rsplit('/').next() {
        Some(name) => name,
        None => path,
    }
}

/// Optional emoji for a tapback of kind `"emoji"`.
fn tapback_emoji(kind: &str, rng: &mut impl Rng) -> Option<String> {
    if kind != "emoji" {
        return None;
    }
    if !rng.random_bool(0.5) {
        return None;
    }
    let emoji = *EMOJI_ONLY.choose(rng).unwrap();
    Some(emoji.to_string())
}

/// Add a tapback (heart, thumbs-up, and similar) from `sender` onto `msg`.
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
    let sender_value = if from_me {
        serde_json::Value::Null
    } else {
        json!(sender)
    };
    taps.push(json!({
        "part_index": 0,
        "kind": kind,
        "emoji": emoji,
        "is_from_me": from_me,
        "sender": sender_value,
    }));
    im.tapbacks = Some(serde_json::Value::Array(taps));
}

/// Turn a phone or email into a safe file name (`+` becomes `p`, `@` becomes `a`).
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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn fixed_reference_time_makes_timestamps_reproducible() {
        let first = SeedConfig::load(&SeedConfig::default_path()).expect("load first config");
        let second = SeedConfig::load(&SeedConfig::default_path()).expect("load second config");
        assert_eq!(
            first.reference_time,
            Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0)
                .single()
                .expect("valid expected reference time")
        );
        assert_eq!(first.reference_time, second.reference_time);

        let mut first_rng = ChaCha8Rng::seed_from_u64(first.seed);
        let mut second_rng = ChaCha8Rng::seed_from_u64(second.seed);
        let first_timestamps = bursty_timestamps(
            50,
            2.0,
            first.reference_time,
            sample_direct_day_burst,
            &mut first_rng,
        );
        let second_timestamps = bursty_timestamps(
            50,
            2.0,
            second.reference_time,
            sample_direct_day_burst,
            &mut second_rng,
        );

        assert_eq!(first_timestamps, second_timestamps);
    }
}
