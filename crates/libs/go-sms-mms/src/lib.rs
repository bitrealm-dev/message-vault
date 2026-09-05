//! Decode GO SMS Pro MMS protocol data units (PDUs).
//!
//! A PDU is the binary packet an Android MMS app stores for one message.
//! This crate scans WAP-209 fragments and parses PDU files into text and
//! attachments. GO SMS Pro is an Android messaging app whose backups store
//! MMS this way.

mod decoders;
mod emoji;
mod mms_enc;
mod pdu;

pub use emoji::decode_gosms_emojis;
pub use pdu::{ParsedAttachment, ParsedPdu, parse_pdu_file};
