use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{FixedOffset, TimeZone};
use message_ir::{
    ConversationHeader, ConversationMeta, ConversationStats, ExportMeta, IrAttachment,
    IrConversationType, IrDirection, IrImessage, IrMessage, IrMessageKind, IrParticipant, IrService,
    SCHEMA_VERSION,
};
use rand::Rng;
use rand::seq::IndexedRandom;
use serde_json::json;

use crate::assets::{JPG_PHOTOS, OTHER_ATTACHMENTS};
use crate::personas::{
    Activity, Contact, EMPTY_GROUP_HANDLE, EMPTY_THREAD_HANDLE, ORPHAN_SENDER, Roster, Unassigned,
    phone_only_handles,
};

const GROUP_COUNT: usize = 200;
/// Roughly 1 in 7 groups are phone-number-only (no named contacts).
const PHONE_ONLY_GROUP_EVERY: usize = 7;

#[derive(Debug, Default)]
pub struct GenStats {
    pub contacts: usize,
    pub conversation_files: usize,
    pub messages: usize,
    pub attachment_refs: usize,
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

const CHAT_SNIPPETS: &[&str] = &[
    "Hey, are we still on for tonight?",
    "Running a few minutes late!",
    "Sounds good to me.",
    "Did you see the game last night?",
    "I'll send the photos in a sec.",
    "Can you pick up milk on the way home?",
    "LOL that's hilarious",
    "Let me know when you land.",
    "Happy birthday!! 🎂",
    "We should plan a trip this summer.",
    "Meeting moved to 3pm.",
    "Thanks for checking in.",
    "On my way now.",
    "Call me when you're free.",
    "That restaurant was amazing.",
];

const PHOTO_CAPTIONS: &[&str] = &[
    "Check this out",
    "Thought you'd like this",
    "From yesterday",
    "Saw this and thought of you",
    "",
];

const GROUP_TITLES: &[&str] = &[
    "Weekend Trip",
    "Book Club",
    "Soccer Parents",
    "Apartment 4B",
    "Project Atlas",
    "Family Chat",
    "College Reunion 2024",
    "Neighborhood Watch",
    "Hiking Crew",
    "Potluck Planning",
    "Fantasy Draft",
    "Office Lunch",
    "Road Trip West",
    "Baby Shower",
    "Game Night",
    "Volunteer Squad",
];

fn export_meta() -> ExportMeta {
    ExportMeta {
        source: "imessage".into(),
        tool: "demo-seed".into(),
        tool_version: "0.1.0".into(),
        owner_handle: Some("+15555550100".into()),
        owner_display_name: Some("Me".into()),
    }
}

pub fn write_all(
    staging: &Path,
    _attachments: &Path,
    roster: &Roster,
    rng: &mut impl Rng,
) -> Result<GenStats> {
    let mut stats = GenStats {
        contacts: roster.contacts.len(),
        ..Default::default()
    };

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

    for contact in roster
        .contacts
        .iter()
        .filter(|c| !c.phones.is_empty() && c.has_one_to_one())
    {
        let phone = contact.primary_phone();
        let count = individual_message_count(contact, rng);
        write_individual(staging, phone, contact, count, rng, &mut stats)?;
    }

    for ua in &roster.unassigned {
        let count = unassigned_message_count(rng);
        write_unassigned(staging, ua, count, rng, &mut stats)?;
    }

    let group_only: Vec<&Contact> = roster
        .contacts
        .iter()
        .filter(|c| c.message_scope == crate::personas::MessageScope::Group)
        .collect();
    let mut phone_only_offset = 0usize;
    for i in 0..GROUP_COUNT {
        if i > 0 && i % PHONE_ONLY_GROUP_EVERY == 0 {
            write_phone_only_group(staging, i, phone_only_offset, rng, &mut stats)?;
            phone_only_offset += 24;
        } else {
            let anchor = group_only.get(i % group_only.len().max(1)).copied();
            write_group(staging, roster, i, anchor, rng, &mut stats)?;
        }
    }

    write_orphaned(staging, rng, &mut stats)?;

    write_header_only(staging, EMPTY_THREAD_HANDLE, IrConversationType::Individual, &[])?;
    write_header_only(
        staging,
        EMPTY_GROUP_HANDLE,
        IrConversationType::Group,
        &["+12125554503", "+13035555604"],
    )?;
    stats.conversation_files += 2;

    Ok(stats)
}

fn write_individual(
    staging: &Path,
    chat_id: &str,
    contact: &Contact,
    msg_count: usize,
    rng: &mut impl Rng,
    stats: &mut GenStats,
) -> Result<()> {
    let participants = vec![IrParticipant {
        handle: chat_id.into(),
        display_name: Some(contact.display_hint()),
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
    )?;

    let mut origin_guid: Option<String> = None;
    for i in 0..msg_count {
        let year = year_for_activity(i, msg_count, contact.activity, rng);
        let from_me = i % 3 != 0;
        let guid = format!("1to1-{chat_id}-{i}");
        let mut msg = text_message(&guid, year, i, from_me, chat_id, rng);
        if should_attach_jpg(i, msg_count) {
            add_jpg_attachment(&mut msg, i, stats);
            if i > 0 && i % 14 == 0 {
                msg.text = PHOTO_CAPTIONS.choose(rng).unwrap().to_string();
            }
        } else if should_attach_photo_only(i, msg_count) {
            msg.text.clear();
            add_jpg_attachment(&mut msg, i + 1, stats);
        } else if should_attach_other(i, msg_count) {
            add_attachment(&mut msg, i, stats, OTHER_ATTACHMENTS);
        }
        if i % 23 == 0 && !from_me && msg_count >= 20 {
            push_tapback(&mut msg, "loved", None, chat_id, false);
        }
        if i % 31 == 0 && origin_guid.is_some() && msg_count >= 25 {
            let im = msg.imessage.get_or_insert_with(IrImessage::default);
            im.is_reply = true;
            im.in_reply_to_guid = origin_guid.clone();
            im.thread_originator_part = Some(0);
        }
        if i % 29 == 0 {
            origin_guid = Some(guid.clone());
            let im = msg.imessage.get_or_insert_with(IrImessage::default);
            im.num_replies = Some(rng.random_range(1..4));
        }
        if i % 41 == 0 {
            msg.service = match *SERVICES.choose(rng).unwrap() {
                "SMS" => IrService::Sms,
                "RCS" => IrService::Rcs,
                _ => IrService::IMessage,
            };
        }
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

const SERVICES: &[&str] = &["iMessage", "SMS", "RCS"];

fn write_unassigned(
    staging: &Path,
    ua: &Unassigned,
    msg_count: usize,
    rng: &mut impl Rng,
    stats: &mut GenStats,
) -> Result<()> {
    let chat_id = &ua.handle;
    let participants = vec![IrParticipant {
        handle: chat_id.clone(),
        display_name: ua.name_hint.clone(),
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
    )?;

    for i in 0..msg_count {
        let guid = format!("unassigned-{chat_id}-{i}");
        let from_me = i % 4 == 0;
        let mut msg = text_message(&guid, (2023 + (i % 4)) as i32, i, from_me, chat_id, rng);
        if i == 2 && ua.name_hint.is_some() && !from_me {
            msg.sender_handle = Some(String::new());
        }
        if should_attach_jpg(i, msg_count) {
            add_jpg_attachment(&mut msg, i, stats);
        } else if should_attach_other(i, msg_count) {
            add_attachment(&mut msg, i, stats, OTHER_ATTACHMENTS);
        }
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

fn group_participant_size(rng: &mut impl Rng, pool_len: usize) -> usize {
    let roll: f64 = rng.random();
    let ideal = if roll < 0.70 {
        rng.random_range(3..9)
    } else if roll < 0.95 {
        rng.random_range(9..15)
    } else {
        rng.random_range(15..21)
    };
    ideal.min(pool_len.max(3)).max(3.min(pool_len.max(1)))
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

fn group_title(index: usize, rng: &mut impl Rng) -> Option<String> {
    if rng.random_bool(0.45) {
        None
    } else {
        let base = GROUP_TITLES[index % GROUP_TITLES.len()];
        if index >= GROUP_TITLES.len() && rng.random_bool(0.35) {
            Some(format!("{base} {}", (index / GROUP_TITLES.len()) + 1))
        } else {
            Some(base.to_string())
        }
    }
}

fn write_group(
    staging: &Path,
    roster: &Roster,
    index: usize,
    anchor: Option<&Contact>,
    rng: &mut impl Rng,
    stats: &mut GenStats,
) -> Result<()> {
    let mut members: Vec<&Contact> = roster.contacts.iter().filter(|c| c.has_group()).collect();
    let size = group_participant_size(rng, members.len());
    members.shuffle(rng);
    if let Some(a) = anchor {
        members.retain(|c| c.primary_phone() != a.primary_phone());
        members.insert(0, a);
    }
    members.truncate(size);

    let chat_id = group_chat_id(index);
    let title = group_title(index, rng);

    let participants: Vec<IrParticipant> = members
        .iter()
        .map(|c| IrParticipant {
            handle: c.primary_phone().into(),
            display_name: Some(c.display_hint()),
        })
        .collect();

    let path = staging.join(format!("group-{index:03}.jsonl"));
    let mut file = open_jsonl(&path)?;
    let msg_count = group_message_count(rng);
    let header_msgs = msg_count + usize::from(index == 0);
    write_conversation_header(
        &mut file,
        &chat_id,
        IrConversationType::Group,
        title.clone(),
        participants,
        header_msgs,
    )?;

    if index == 0 {
        let announcement = format!(
            "You named the conversation {}",
            title.clone().unwrap_or_else(|| "Group".into())
        );
        write_message(
            &mut file,
            IrMessage {
                guid: format!("ann-{index}"),
                timestamp_unix_ms: ts_unix_ms(2018, 6, 1, 10, 0),
                direction: IrDirection::Incoming,
                service: IrService::IMessage,
                message_kind: IrMessageKind::Announcement,
                sender_handle: Some(members[0].primary_phone().into()),
                sender_display_name: None,
                subject: None,
                text: String::new(),
                attachments: vec![],
                imessage: Some(IrImessage {
                    announcement: Some(announcement),
                    ..Default::default()
                }),
                source: None,
            },
        )?;
        stats.messages += 1;
    }

    for i in 0..msg_count {
        let year = year_span(i, msg_count, 2016, 2026);
        let from_me = i % 7 == 0;
        let sender = if from_me {
            None
        } else {
            Some(members[i % members.len()].primary_phone().into())
        };
        let guid = format!("grp-{index}-{i}");
        let name = members[i % members.len()].first_name.as_str();
        let label = if name.is_empty() {
            members[i % members.len()].last_name.as_str()
        } else {
            name
        };
        let mut msg = IrMessage {
            guid,
            timestamp_unix_ms: ts_unix_ms(
                year,
                ((i % 12) + 1) as u32,
                ((i % 28) + 1) as u32,
                (9 + (i % 10)) as u32,
                i % 60,
            ),
            direction: if from_me {
                IrDirection::Outgoing
            } else {
                IrDirection::Incoming
            },
            service: if i % 11 == 0 {
                IrService::Sms
            } else {
                IrService::IMessage
            },
            message_kind: IrMessageKind::IMessage,
            sender_handle: sender,
            sender_display_name: None,
            subject: None,
            text: format!("{} {}", label, CHAT_SNIPPETS.choose(rng).unwrap()),
            attachments: vec![],
            imessage: None,
            source: None,
        };
        if should_attach_jpg(i, msg_count) {
            add_jpg_attachment(&mut msg, i + index, stats);
            if i > 0 && i % 12 == 0 {
                msg.text = format!("{} {}", label, PHOTO_CAPTIONS.choose(rng).unwrap());
            }
        } else if should_attach_other(i, msg_count) {
            add_attachment(&mut msg, i, stats, OTHER_ATTACHMENTS);
        }
        if i % 13 == 0 && msg_count >= 18 {
            let reactor = members[(i + 1) % members.len()].primary_phone();
            let kind = TAPBACK_KINDS.choose(rng).unwrap().to_string();
            let emoji = if i % 26 == 0 {
                Some("🎉".into())
            } else {
                None
            };
            push_tapback(&mut msg, &kind, emoji, reactor, false);
        }
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

fn write_phone_only_group(
    staging: &Path,
    index: usize,
    handle_offset: usize,
    rng: &mut impl Rng,
    stats: &mut GenStats,
) -> Result<()> {
    let size = group_participant_size(rng, 20);
    let handles = phone_only_handles(handle_offset, size);
    let chat_id = group_chat_id(index);

    let participants: Vec<IrParticipant> = handles
        .iter()
        .map(|h| IrParticipant {
            handle: h.clone(),
            display_name: None,
        })
        .collect();

    let path = staging.join(format!("group-{index:03}.jsonl"));
    let mut file = open_jsonl(&path)?;
    let msg_count = group_message_count(rng);
    write_conversation_header(
        &mut file,
        &chat_id,
        IrConversationType::Group,
        None,
        participants,
        msg_count,
    )?;

    for i in 0..msg_count {
        let year = year_span(i, msg_count, 2016, 2026);
        let from_me = i % 7 == 0;
        let sender = if from_me {
            None
        } else {
            Some(handles[i % handles.len()].clone())
        };
        let guid = format!("grp-phone-{index}-{i}");
        let mut msg = IrMessage {
            guid,
            timestamp_unix_ms: ts_unix_ms(
                year,
                ((i % 12) + 1) as u32,
                ((i % 28) + 1) as u32,
                (9 + (i % 10)) as u32,
                i % 60,
            ),
            direction: if from_me {
                IrDirection::Outgoing
            } else {
                IrDirection::Incoming
            },
            service: IrService::IMessage,
            message_kind: IrMessageKind::IMessage,
            sender_handle: sender,
            sender_display_name: None,
            subject: None,
            text: CHAT_SNIPPETS.choose(rng).unwrap().to_string(),
            attachments: vec![],
            imessage: None,
            source: None,
        };
        if should_attach_jpg(i, msg_count) {
            add_jpg_attachment(&mut msg, i + index, stats);
        }
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

fn write_orphaned(staging: &Path, rng: &mut impl Rng, stats: &mut GenStats) -> Result<()> {
    let path = staging.join("orphaned.jsonl");
    let mut file = open_jsonl(&path)?;
    write_conversation_header(
        &mut file,
        "orphaned",
        IrConversationType::Individual,
        None,
        vec![],
        6,
    )?;
    for i in 0..6 {
        let guid = format!("orphan-{i}");
        let mut msg = text_message(
            &guid,
            (2022 + (i % 3)) as i32,
            i,
            i % 2 == 0,
            ORPHAN_SENDER,
            rng,
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
) -> Result<()> {
    let path = staging.join(format!("empty-{}.jsonl", sanitize_filename(chat_id)));
    let mut file = open_jsonl(&path)?;
    let participants: Vec<IrParticipant> = member_phones
        .iter()
        .map(|h| IrParticipant {
            handle: (*h).into(),
            display_name: None,
        })
        .collect();
    write_conversation_header(&mut file, chat_id, conv_type, None, participants, 0)?;
    Ok(())
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
) -> Result<()> {
    let header = ConversationHeader {
        schema_version: SCHEMA_VERSION,
        export: export_meta(),
        conversation: ConversationMeta {
            chat_identifier: chat_id.into(),
            conversation_type: conv_type,
            group_title,
            participants,
            stats: ConversationStats {
                message_count,
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
    year: i32,
    index: usize,
    from_me: bool,
    peer: &str,
    rng: &mut impl Rng,
) -> IrMessage {
    IrMessage {
        guid: guid.into(),
        timestamp_unix_ms: ts_unix_ms(
            year,
            ((index % 12) + 1) as u32,
            ((index % 28) + 1) as u32,
            (8 + (index % 12)) as u32,
            index % 60,
        ),
        direction: if from_me {
            IrDirection::Outgoing
        } else {
            IrDirection::Incoming
        },
        service: IrService::IMessage,
        message_kind: IrMessageKind::IMessage,
        sender_handle: if from_me { None } else { Some(peer.into()) },
        sender_display_name: None,
        subject: None,
        text: CHAT_SNIPPETS.choose(rng).unwrap().to_string(),
        attachments: vec![],
        imessage: None,
        source: None,
    }
}

fn individual_message_count(contact: &Contact, rng: &mut impl Rng) -> usize {
    if contact.has_label("Inactive") {
        return rng.random_range(3..10);
    }
    if contact.high_volume {
        return rng.random_range(1000..1301);
    }
    match contact.activity {
        Activity::Frequent => {
            let roll: f64 = rng.random();
            if roll < 0.55 {
                rng.random_range(28..55)
            } else if roll < 0.90 {
                rng.random_range(55..95)
            } else {
                rng.random_range(95..140)
            }
        }
        Activity::Lapsed => rng.random_range(20..48),
        Activity::Normal => {
            let roll: f64 = rng.random();
            if roll < 0.55 {
                rng.random_range(5..16)
            } else if roll < 0.90 {
                rng.random_range(16..36)
            } else {
                rng.random_range(36..60)
            }
        }
    }
}

fn group_message_count(rng: &mut impl Rng) -> usize {
    let roll: f64 = rng.random();
    if roll < 0.65 {
        rng.random_range(2..7)
    } else if roll < 0.92 {
        rng.random_range(7..12)
    } else {
        rng.random_range(12..20)
    }
}

fn unassigned_message_count(rng: &mut impl Rng) -> usize {
    let roll: f64 = rng.random();
    if roll < 0.55 {
        rng.random_range(2..6)
    } else if roll < 0.88 {
        rng.random_range(6..12)
    } else {
        rng.random_range(12..18)
    }
}

fn should_attach_jpg(i: usize, total: usize) -> bool {
    if total < 3 {
        return false;
    }
    if i == 1 || (total >= 6 && i == total - 1) {
        return true;
    }
    let stride = if total < 15 {
        9
    } else if total < 50 {
        7
    } else if total < 200 {
        12
    } else {
        80
    };
    i > 0 && i % stride == 0
}

fn should_attach_photo_only(i: usize, total: usize) -> bool {
    if total >= 200 {
        return total >= 12 && i % 120 == 5;
    }
    total >= 12 && i % 13 == 5
}

fn should_attach_other(i: usize, total: usize) -> bool {
    if total >= 200 {
        return total >= 20 && i % 150 == 0;
    }
    total >= 20 && i % 19 == 0
}

fn add_jpg_attachment(msg: &mut IrMessage, idx: usize, stats: &mut GenStats) {
    let photo = &JPG_PHOTOS[idx % JPG_PHOTOS.len()];
    msg.attachments.push(IrAttachment {
        path: Some(photo.path.into()),
        original_name: Some(photo.original_name.into()),
        mime_type: Some("image/jpeg".into()),
        digest_sha256: None,
        is_sticker: false,
        transcription: None,
        sticker_effect: None,
        bytes: None,
    });
    stats.attachment_refs += 1;
}

fn add_attachment(
    msg: &mut IrMessage,
    idx: usize,
    stats: &mut GenStats,
    files: &[(&str, &str, bool)],
) {
    let (path, mime, is_sticker) = files[idx % files.len()];
    let transcription = if mime.starts_with("audio/") {
        Some("Hey, just leaving a quick voice note.".into())
    } else {
        None
    };
    msg.attachments.push(IrAttachment {
        path: Some(path.into()),
        original_name: Some(path.rsplit('/').next().unwrap_or(path).into()),
        mime_type: Some(mime.into()),
        digest_sha256: None,
        is_sticker,
        transcription,
        sticker_effect: None,
        bytes: None,
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

fn year_span(i: usize, total: usize, start: i32, end: i32) -> i32 {
    let span = (end - start).max(0) as usize;
    if total <= 1 || span == 0 {
        return end;
    }
    start + ((i * span) / (total - 1)) as i32
}

fn year_for_activity(i: usize, total: usize, activity: Activity, rng: &mut impl Rng) -> i32 {
    match activity {
        Activity::Frequent => {
            if rng.random_bool(0.80) {
                year_span(i, total, 2023, 2026)
            } else {
                year_span(i, total, 2016, 2022)
            }
        }
        Activity::Lapsed => {
            if rng.random_bool(0.92) {
                year_span(i, total, 2016, 2022)
            } else {
                year_span(i, total, 2023, 2024)
            }
        }
        Activity::Normal => year_span(i, total, 2016, 2026),
    }
}

/// Demo timestamps as America/New_York offset (-04:00) unix millis.
fn ts_unix_ms(year: i32, month: u32, day: u32, hour: u32, minute: usize) -> i64 {
    let offset = FixedOffset::west_opt(4 * 3600).unwrap();
    offset
        .with_ymd_and_hms(year, month, day, hour, minute as u32, 0)
        .unwrap()
        .timestamp_millis()
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

trait ShuffleSlice<T> {
    fn shuffle(&mut self, rng: &mut impl Rng);
}

impl<T> ShuffleSlice<T> for Vec<T> {
    fn shuffle(&mut self, rng: &mut impl Rng) {
        for i in (1..self.len()).rev() {
            let j = rng.random_range(0..=i);
            self.swap(i, j);
        }
    }
}
