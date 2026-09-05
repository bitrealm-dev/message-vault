//! The response shapes the vault's HTTP API sends, defined once for the
//! server that writes them and the client crates that read them.
//!
//! Two crates sit on either side of these shapes: `message-vault-server`
//! serializes them, and `vault-pull` deserializes them on its way to
//! `message-ir`. While each kept its own struct, the two could disagree
//! silently and did, three times: `vault-pull` declared `handle` a `String`
//! after the server started sending `null` for a participant a backup named
//! without an address, kept `#[serde(default)]` on a field the server had
//! removed, and read a `service` off the conversation the server has never
//! sent there. Each of those was a pull that failed at runtime, or quietly
//! produced worse data, with nothing in either crate's tests to catch it —
//! `vault-pull`'s "real export page" was a JSON literal it wrote itself, so it
//! agreed with whatever the mirror said.
//!
//! One definition makes the compiler the check. A field the server renames
//! stops compiling in the client, which is the whole point of putting the
//! shape here.
//!
//! `#[serde(skip_serializing_if = ...)]` and `#[serde(default)]` come as a
//! pair here, and only as a pair: a field the server may leave out is a field
//! a reader has to be able to do without, and a field the server always sends
//! stays required in the OpenAPI document rather than turning optional in
//! every generated client.
//!
//! The `schema` feature adds `utoipa::ToSchema`, so the same structs describe
//! themselves in the OpenAPI document. The server turns it on; a client crate
//! leaves it off and never builds utoipa.

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};

/// Derive `ToSchema` only when the `schema` feature is on.
macro_rules! api_shape {
    ($(#[$meta:meta])* pub struct $name:ident { $($body:tt)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
        pub struct $name { $($body)* }
    };
}

api_shape! {
    /// One participant of a conversation, carrying the name to show for them:
    /// the Contact's name, else what that backup called them in that
    /// conversation, else the handle.
    pub struct Participant {
        /// What to show for this person. Never empty — the vault falls back to
        /// the handle when nothing else names them, and to the name alone for
        /// someone a backup named without recording any address.
        pub name: String,
        /// Raw handle value (phone, email, or username). `None` when the source
        /// named this person without recording any address for them.
        pub handle: Option<String>,
        /// Platform service, e.g. `imessage`. `None` for the same reason as
        /// `handle`: with no address there is nothing to carry a service on.
        pub service: Option<String>,
        /// Linked vault contact id: when the handle is on a Contact, or — for a
        /// participant with no handle — the contact the vault bound the name to
        /// directly, since that is the only place the link is recorded for
        /// them. Matches the `id` every other contact shape uses, so a caller
        /// can compare the two without converting either.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub contact_id: Option<i64>,
    }
}

api_shape! {
    /// One exported message.
    pub struct Message {
        /// Message row id.
        pub id: i64,
        /// Import source id.
        pub source: String,
        /// Platform service, e.g. `imessage`, when known. It rides on the
        /// message, never on the conversation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub service: Option<String>,
        /// Export GUID for replies and grouping.
        pub guid: Option<String>,
        /// The instant the message was sent: RFC 3339 in UTC with a `Z`
        /// suffix. A caller shows it in the account's time zone
        /// (`AccountProfileResponse.time_zone`); the vault stores nothing
        /// about where the phone was.
        pub timestamp: String,
        /// Ordering key within the conversation.
        pub sort_order: i64,
        /// True for messages sent by the account owner.
        pub is_from_me: bool,
        /// Sender handle for incoming messages.
        pub sender: Option<String>,
        /// Subject line, when set.
        pub subject: Option<String>,
        /// Body text, when present.
        pub text: Option<String>,
        /// True for group announcements.
        pub is_announcement: bool,
        /// True when part of a reply thread.
        pub is_reply: bool,
        /// GUID of the message this replies to.
        pub thread_originator_guid: Option<String>,
        /// Part index of the originator (for tapbacks).
        pub thread_originator_part: Option<i64>,
        /// Replies in this thread.
        pub num_replies: i64,
        /// The conversation this message belongs to.
        pub conversation: MessageConversation,
        /// Attachments on this message.
        pub attachments: Vec<Attachment>,
        /// Reactions on this message.
        pub tapbacks: Vec<Tapback>,
    }
}

api_shape! {
    /// The conversation a message belongs to.
    pub struct MessageConversation {
        /// Conversation row id.
        pub id: i64,
        /// Original chat id from the export.
        pub chat_identifier: String,
        /// `individual` or `group`.
        pub conversation_type: String,
        /// Group label, when set.
        pub group_title: Option<String>,
        /// Participants of the conversation.
        pub participants: Vec<Participant>,
    }
}

api_shape! {
    /// One attachment of an exported message.
    pub struct Attachment {
        /// Path inside the export.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        /// File name from the export.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub original_name: Option<String>,
        /// MIME type, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub mime_type: Option<String>,
        /// Content fingerprint of the stored bytes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub sha256: Option<String>,
        /// True for sticker files.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        pub is_sticker: bool,
        /// OCR/ASR transcription, when processed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub transcription: Option<String>,
        /// Why the file is missing, when it is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub missing_reason: Option<String>,
    }
}

api_shape! {
    /// One tapback reaction on an exported message.
    pub struct Tapback {
        /// Attachment part the reaction applies to.
        pub part_index: i64,
        /// Reaction type, e.g. `love`.
        pub kind: String,
        /// Emoji form of the reaction, when one exists.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub emoji: Option<String>,
        /// True when the account owner reacted.
        pub is_from_me: bool,
        /// Reactor handle for incoming reactions.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub sender: Option<String>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pairing rule this module states, checked rather than trusted: a
    /// field the server may leave out has to read back without it. Serializing
    /// a value whose every skippable field is absent and reading it again is
    /// the shortest way to say that, and it fails the moment a
    /// `skip_serializing_if` arrives without its `default`.
    #[test]
    fn a_value_with_every_skippable_field_absent_reads_back() {
        let message = Message {
            id: 1,
            source: "imessage".into(),
            service: None,
            guid: None,
            timestamp: "2024-01-01T00:00:00Z".into(),
            sort_order: 0,
            is_from_me: false,
            sender: None,
            subject: None,
            text: None,
            is_announcement: false,
            is_reply: false,
            thread_originator_guid: None,
            thread_originator_part: None,
            num_replies: 0,
            conversation: MessageConversation {
                id: 9,
                chat_identifier: "+15555550100".into(),
                conversation_type: "individual".into(),
                group_title: None,
                participants: vec![Participant {
                    name: "Sarah Vale".into(),
                    handle: None,
                    service: None,
                    contact_id: None,
                }],
            },
            attachments: vec![Attachment {
                path: None,
                original_name: None,
                mime_type: None,
                sha256: None,
                is_sticker: false,
                transcription: None,
                missing_reason: None,
            }],
            tapbacks: vec![Tapback {
                part_index: 0,
                kind: "loved".into(),
                emoji: None,
                is_from_me: true,
                sender: None,
            }],
        };

        let json = serde_json::to_string(&message).expect("serializes");
        let written: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(
            written.get("service").is_none(),
            "a message with no service must not carry the key at all: {json}"
        );
        assert_eq!(
            written["attachments"][0],
            serde_json::json!({}),
            "an attachment with nothing known about it writes as an empty object: {json}"
        );

        let read: Message = serde_json::from_str(&json).expect("reads back");
        assert_eq!(read.id, 1);
        assert_eq!(read.conversation.participants[0].name, "Sarah Vale");
        assert_eq!(read.conversation.participants[0].handle, None);
        assert_eq!(read.attachments.len(), 1);
        assert_eq!(read.tapbacks[0].kind, "loved");
    }
}
