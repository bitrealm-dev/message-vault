use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use message_json::imessage::{
    AttachmentRecord, ConversationRecord, ExportRecord, MessageRecord, ParticipantRecord,
    RECORD_MESSAGE, TapbackRecord,
};
use rand::Rng;
use rand::seq::IndexedRandom;
use serde_json;

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

const SERVICES: &[&str] = &["iMessage", "SMS", "RCS"];
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

    // Clear existing JSONL conversation files
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

    // 1:1 with CSV contacts — only OneToOne and Both scopes
    for contact in roster
        .contacts
        .iter()
        .filter(|c| !c.phones.is_empty() && c.has_one_to_one())
    {
        let phone = contact.primary_phone();
        let count = individual_message_count(contact, rng);
        write_individual(staging, phone, contact, count, rng, &mut stats)?;
    }

    // Unassigned 1:1 — mostly short threads
    for ua in &roster.unassigned {
        let count = unassigned_message_count(rng);
        write_unassigned(staging, ua, count, rng, &mut stats)?;
    }

    // Group chats — seed group-only contacts so each appears in at least one thread
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

    // orphaned.jsonl
    write_orphaned(staging, rng, &mut stats)?;

    // Header-only conversations (2)
    write_header_only(staging, EMPTY_THREAD_HANDLE, "individual", &[])?;
    write_header_only(
        staging,
        EMPTY_GROUP_HANDLE,
        "group",
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
    let participants = vec![ParticipantRecord {
        handle: chat_id.into(),
        name_hint: Some(contact.display_hint()),
    }];
    let path = staging.join(sanitize_filename(chat_id) + ".jsonl");
    let mut file = open_jsonl(&path)?;
    write_conversation_header(&mut file, chat_id, "individual", None, participants)?;

    let mut messages = Vec::new();
    let mut origin_guid: Option<String> = None;
    for i in 0..msg_count {
        let year = year_for_activity(i, msg_count, contact.activity, rng);
        let from_me = i % 3 != 0;
        let guid = format!("1to1-{chat_id}-{i}");
        let mut msg = text_message(&guid, year, i, from_me, chat_id, rng);
        if should_attach_jpg(i, msg_count) {
            add_jpg_attachment(&mut msg, i, stats);
            if i > 0 && i % 14 == 0 {
                msg.text = Some(PHOTO_CAPTIONS.choose(rng).unwrap().to_string());
            }
        } else if should_attach_photo_only(i, msg_count) {
            msg.text = None;
            add_jpg_attachment(&mut msg, i + 1, stats);
        } else if should_attach_other(i, msg_count) {
            add_attachment(&mut msg, i, stats, OTHER_ATTACHMENTS);
        }
        if i % 23 == 0 && !from_me && msg_count >= 20 {
            msg.tapbacks.push(tapback_loved(chat_id, false));
        }
        if i % 31 == 0 && origin_guid.is_some() && msg_count >= 25 {
            msg.is_reply = true;
            msg.thread_originator_guid = origin_guid.clone();
            msg.thread_originator_part = Some(0);
        }
        if i % 29 == 0 {
            origin_guid = Some(guid.clone());
            msg.num_replies = rng.random_range(1..4);
        }
        if i % 41 == 0 {
            msg.service = Some(SERVICES.choose(rng).unwrap().to_string());
        }
        messages.push(msg);
    }
    for msg in messages {
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

fn write_unassigned(
    staging: &Path,
    ua: &Unassigned,
    msg_count: usize,
    rng: &mut impl Rng,
    stats: &mut GenStats,
) -> Result<()> {
    let chat_id = &ua.handle;
    let participants = vec![ParticipantRecord {
        handle: chat_id.clone(),
        name_hint: ua.name_hint.clone(),
    }];
    let fname = if ua.email_only {
        format!("email-{}.jsonl", chat_id.replace('@', "_at_"))
    } else {
        sanitize_filename(chat_id) + ".jsonl"
    };
    let path = staging.join(fname);
    let mut file = open_jsonl(&path)?;
    write_conversation_header(&mut file, chat_id, "individual", None, participants)?;

    for i in 0..msg_count {
        let guid = format!("unassigned-{chat_id}-{i}");
        let from_me = i % 4 == 0;
        let mut msg = text_message(&guid, (2023 + (i % 4)) as i32, i, from_me, chat_id, rng);
        // Unverified: empty sender on inbound with name_hint only
        if i == 2 && ua.name_hint.is_some() && !from_me {
            msg.sender = Some(String::new());
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

/// ~45% untitled so many groups show participant names only.
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

    let participants: Vec<ParticipantRecord> = members
        .iter()
        .map(|c| ParticipantRecord {
            handle: c.primary_phone().into(),
            name_hint: Some(c.display_hint()),
        })
        .collect();

    let path = staging.join(format!("group-{index:03}.jsonl"));
    let mut file = open_jsonl(&path)?;
    write_conversation_header(&mut file, &chat_id, "group", title.clone(), participants)?;

    if index == 0 {
        write_message(
            &mut file,
            MessageRecord {
                guid: Some(format!("ann-{index}")),
                timestamp: ts_local(2018, 6, 1, 10, 0),
                timestamp_utc: Some(ts_utc(2018, 6, 1, 14, 0)),
                is_from_me: false,
                sender: Some(members[0].primary_phone().into()),
                is_announcement: true,
                announcement: Some(format!(
                    "You named the conversation {}",
                    title.clone().unwrap_or_else(|| "Group".into())
                )),
                ..default_message()
            },
        )?;
        stats.messages += 1;
    }

    let msg_count = group_message_count(rng);
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
        let mut msg = MessageRecord {
            guid: Some(guid),
            timestamp: ts_local(
                year,
                ((i % 12) + 1) as u32,
                ((i % 28) + 1) as u32,
                (9 + (i % 10)) as u32,
                i % 60,
            ),
            timestamp_utc: Some(ts_utc(
                year,
                ((i % 12) + 1) as u32,
                ((i % 28) + 1) as u32,
                (13 + (i % 10)) as u32,
                i % 60,
            )),
            is_from_me: from_me,
            sender,
            text: Some(format!("{} {}", label, CHAT_SNIPPETS.choose(rng).unwrap())),
            service: if i % 11 == 0 {
                Some("SMS".into())
            } else {
                Some("iMessage".into())
            },
            ..default_message()
        };
        if should_attach_jpg(i, msg_count) {
            add_jpg_attachment(&mut msg, i + index, stats);
            if i > 0 && i % 12 == 0 {
                msg.text = Some(format!("{} {}", label, PHOTO_CAPTIONS.choose(rng).unwrap()));
            }
        } else if should_attach_other(i, msg_count) {
            add_attachment(&mut msg, i, stats, OTHER_ATTACHMENTS);
        }
        if i % 13 == 0 && msg_count >= 18 {
            let reactor = members[(i + 1) % members.len()].primary_phone();
            msg.tapbacks.push(TapbackRecord {
                part_index: 0,
                kind: TAPBACK_KINDS.choose(rng).unwrap().to_string(),
                emoji: if i % 26 == 0 {
                    Some("🎉".into())
                } else {
                    None
                },
                is_from_me: false,
                sender: Some(reactor.into()),
            });
        }
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

/// Group chat whose participants are phone numbers only (no contact names / CSV rows).
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
    // Untitled so the UI shows phone handles as the identity.
    let title = None;

    let participants: Vec<ParticipantRecord> = handles
        .iter()
        .map(|h| ParticipantRecord {
            handle: h.clone(),
            name_hint: None,
        })
        .collect();

    let path = staging.join(format!("group-{index:03}.jsonl"));
    let mut file = open_jsonl(&path)?;
    write_conversation_header(&mut file, &chat_id, "group", title, participants)?;

    let msg_count = group_message_count(rng);
    for i in 0..msg_count {
        let year = year_span(i, msg_count, 2016, 2026);
        let from_me = i % 7 == 0;
        let sender = if from_me {
            None
        } else {
            Some(handles[i % handles.len()].clone())
        };
        let guid = format!("grp-phone-{index}-{i}");
        let mut msg = MessageRecord {
            guid: Some(guid),
            timestamp: ts_local(
                year,
                ((i % 12) + 1) as u32,
                ((i % 28) + 1) as u32,
                (9 + (i % 10)) as u32,
                i % 60,
            ),
            timestamp_utc: Some(ts_utc(
                year,
                ((i % 12) + 1) as u32,
                ((i % 28) + 1) as u32,
                (13 + (i % 10)) as u32,
                i % 60,
            )),
            is_from_me: from_me,
            sender,
            text: Some(CHAT_SNIPPETS.choose(rng).unwrap().to_string()),
            service: Some("iMessage".into()),
            ..default_message()
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
        msg.text = Some(format!(
            "Orphaned message #{i} (no conversation association)"
        ));
        write_message(&mut file, msg)?;
        stats.messages += 1;
    }
    stats.conversation_files += 1;
    Ok(())
}

fn write_header_only(
    staging: &Path,
    chat_id: &str,
    conv_type: &str,
    member_phones: &[&str],
) -> Result<()> {
    let path = staging.join(format!("empty-{}.jsonl", sanitize_filename(chat_id)));
    let mut file = open_jsonl(&path)?;
    let participants: Vec<ParticipantRecord> = member_phones
        .iter()
        .map(|h| ParticipantRecord {
            handle: (*h).into(),
            name_hint: None,
        })
        .collect();
    write_conversation_header(&mut file, chat_id, conv_type, None, participants)?;
    Ok(())
}

fn open_jsonl(path: &Path) -> Result<BufWriter<File>> {
    let f = File::create(path).with_context(|| format!("create {}", path.display()))?;
    Ok(BufWriter::new(f))
}

fn write_conversation_header(
    file: &mut BufWriter<File>,
    chat_id: &str,
    conv_type: &str,
    group_title: Option<String>,
    participants: Vec<ParticipantRecord>,
) -> Result<()> {
    let header = ConversationRecord::header(
        chat_id,
        conv_type,
        group_title,
        participants,
        "2026-07-14T12:00:00-04:00",
    );
    let line = serde_json::to_string(&ExportRecord::Conversation(header))?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn write_message(file: &mut BufWriter<File>, msg: MessageRecord) -> Result<()> {
    let mut value = serde_json::to_value(&msg)?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.insert(
            "record".into(),
            serde_json::Value::String(RECORD_MESSAGE.into()),
        );
    }
    writeln!(file, "{}", serde_json::to_string(&value)?)?;
    Ok(())
}

fn text_message(
    guid: &str,
    year: i32,
    index: usize,
    from_me: bool,
    peer: &str,
    rng: &mut impl Rng,
) -> MessageRecord {
    MessageRecord {
        guid: Some(guid.into()),
        timestamp: ts_local(
            year,
            ((index % 12) + 1) as u32,
            ((index % 28) + 1) as u32,
            (8 + (index % 12)) as u32,
            index % 60,
        ),
        timestamp_utc: Some(ts_utc(
            year,
            ((index % 12) + 1) as u32,
            ((index % 28) + 1) as u32,
            (12 + (index % 12)) as u32,
            index % 60,
        )),
        is_from_me: from_me,
        sender: if from_me { None } else { Some(peer.into()) },
        text: Some(CHAT_SNIPPETS.choose(rng).unwrap().to_string()),
        service: Some("iMessage".into()),
        ..default_message()
    }
}

fn default_message() -> MessageRecord {
    MessageRecord {
        guid: None,
        timestamp: String::new(),
        timestamp_utc: None,
        is_from_me: false,
        sender: None,
        service: None,
        subject: None,
        text: None,
        is_announcement: false,
        announcement: None,
        attachments: vec![],
        tapbacks: vec![],
        is_reply: false,
        thread_originator_guid: None,
        thread_originator_part: None,
        num_replies: 0,
    }
}

fn individual_message_count(contact: &Contact, rng: &mut impl Rng) -> usize {
    if contact.has_label("Inactive") {
        return rng.random_range(3..10);
    }
    if contact.high_volume {
        // Two whales: thousands of messages each.
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
    // Keep group threads short so ~200 groups + whales still land near ~5k total.
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

/// Scale attachment density to thread length (sparse for whale threads).
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

fn add_jpg_attachment(msg: &mut MessageRecord, idx: usize, stats: &mut GenStats) {
    let photo = &JPG_PHOTOS[idx % JPG_PHOTOS.len()];
    msg.attachments.push(AttachmentRecord {
        path: Some(photo.path.into()),
        original_name: Some(photo.original_name.into()),
        mime_type: Some("image/jpeg".into()),
        is_sticker: false,
        transcription: None,
    });
    stats.attachment_refs += 1;
}

fn add_attachment(
    msg: &mut MessageRecord,
    idx: usize,
    stats: &mut GenStats,
    files: &[(&str, &str, bool)],
) {
    let (path, mime, is_sticker) = files[idx % files.len()];
    let mut att = AttachmentRecord {
        path: Some(path.into()),
        original_name: Some(path.rsplit('/').next().unwrap_or(path).into()),
        mime_type: Some(mime.into()),
        is_sticker,
        transcription: None,
    };
    if mime.starts_with("audio/") {
        att.transcription = Some("Hey, just leaving a quick voice note.".into());
    }
    msg.attachments.push(att);
    stats.attachment_refs += 1;
    if msg.text.as_deref() == Some("") {
        msg.text = None;
    }
}

fn tapback_loved(sender: &str, from_me: bool) -> TapbackRecord {
    TapbackRecord {
        part_index: 0,
        kind: "loved".into(),
        emoji: None,
        is_from_me: from_me,
        sender: if from_me { None } else { Some(sender.into()) },
    }
}

/// Even spread across [start, end] inclusive.
fn year_span(i: usize, total: usize, start: i32, end: i32) -> i32 {
    let span = (end - start).max(0) as usize;
    if total <= 1 || span == 0 {
        return end;
    }
    start + ((i * span) / (total - 1)) as i32
}

/// 10-year window (2016–2026) with activity-biased density.
fn year_for_activity(i: usize, total: usize, activity: Activity, rng: &mut impl Rng) -> i32 {
    match activity {
        Activity::Frequent => {
            // ~80% in past 3 years; remainder older history.
            if rng.random_bool(0.80) {
                year_span(i, total, 2023, 2026)
            } else {
                year_span(i, total, 2016, 2022)
            }
        }
        Activity::Lapsed => {
            // Almost all older; rare recent blip.
            if rng.random_bool(0.92) {
                year_span(i, total, 2016, 2022)
            } else {
                year_span(i, total, 2023, 2024)
            }
        }
        Activity::Normal => year_span(i, total, 2016, 2026),
    }
}

fn ts_local(year: i32, month: u32, day: u32, hour: u32, minute: usize) -> String {
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00-04:00")
}

fn ts_utc(year: i32, month: u32, day: u32, hour: u32, minute: usize) -> String {
    let dt = Utc
        .with_ymd_and_hms(year, month, day, hour, minute as u32, 0)
        .unwrap();
    dt.to_rfc3339()
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
