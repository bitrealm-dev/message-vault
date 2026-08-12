//! Body parse + GUID-based attachment index resolution.

use std::collections::{HashMap, HashSet};

use imessage_database::tables::{
    attachment::Attachment,
    messages::{
        Message,
        models::{AttributedRange, BubbleComponent},
    },
};
use rusqlite::Connection;

/// Apply typedstream body when parse succeeds (fills `components` / text).
pub(crate) fn apply_body(msg: &mut Message, db: &Connection) {
    if let Ok(body) = msg.parse_body(db) {
        msg.apply_body(body);
    }
}

pub(crate) struct AttachmentResolver {
    by_guid: HashMap<String, usize>,
    /// Attachment indices that have a GUID (prefer GUID matching for these).
    has_guid: HashSet<usize>,
    claimed: HashSet<usize>,
    next_positional: usize,
    len: usize,
}

impl AttachmentResolver {
    pub(crate) fn new(attachments: &[Attachment]) -> Self {
        let mut by_guid = HashMap::new();
        let mut has_guid = HashSet::new();
        for (i, a) in attachments.iter().enumerate() {
            if let Some(g) = a.guid.clone() {
                by_guid.insert(g, i);
                has_guid.insert(i);
            }
        }
        Self {
            by_guid,
            has_guid,
            claimed: HashSet::new(),
            next_positional: 0,
            len: attachments.len(),
        }
    }

    pub(crate) fn resolve(&mut self, range: &AttributedRange) -> usize {
        if let Some(idx) = range
            .attachment
            .as_ref()
            .and_then(|meta| meta.guid.as_deref())
            .and_then(|guid| self.by_guid.get(guid).copied())
        {
            self.claimed.insert(idx);
            return idx;
        }
        // Prefer unclaimed rows that have no GUID — those are the ones the
        // positional fallback is meant for. Never reuse a GUID-claimed index.
        if let Some(idx) = (0..self.len).find(|i| {
            !self.claimed.contains(i) && !self.has_guid.contains(i)
        }) {
            self.claimed.insert(idx);
            if idx >= self.next_positional {
                self.next_positional = idx + 1;
            }
            return idx;
        }
        while self.next_positional < self.len && self.claimed.contains(&self.next_positional) {
            self.next_positional += 1;
        }
        let idx = self.next_positional;
        self.next_positional += 1;
        self.claimed.insert(idx);
        idx
    }
}

pub(crate) fn resolve_run<'r>(
    ranges: &'r [AttributedRange],
    resolver: &mut AttachmentResolver,
) -> Vec<(&'r AttributedRange, Option<usize>)> {
    ranges
        .iter()
        .map(|range| {
            let idx = range.attachment.is_some().then(|| resolver.resolve(range));
            (range, idx)
        })
        .collect()
}

/// Indices into `attachments` referenced by the message body.
///
/// When `components` is empty (parse failure), falls back to every join row.
pub(crate) fn referenced_attachment_indices(message: &Message, attachments: &[Attachment]) -> Vec<usize> {
    if attachments.is_empty() {
        return Vec::new();
    }

    if message.components.is_empty() {
        return (0..attachments.len()).collect();
    }

    let mut resolver = AttachmentResolver::new(attachments);
    let mut indices = HashSet::new();

    for (part_idx, component) in message.components.iter().enumerate() {
        match component {
            BubbleComponent::Run(ranges) => {
                if message.is_part_edited(part_idx) {
                    continue;
                }
                for (_, idx) in resolve_run(ranges, &mut resolver) {
                    if let Some(i) = idx
                        && i < attachments.len()
                    {
                        indices.insert(i);
                    }
                }
            }
            BubbleComponent::App | BubbleComponent::Retracted => {}
        }
    }

    let mut out: Vec<_> = indices.into_iter().collect();
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use imessage_database::tables::messages::models::AttachmentMeta;

    fn stub_attachment(guid: Option<&str>) -> Attachment {
        Attachment {
            rowid: 0,
            guid: guid.map(str::to_string),
            filename: None,
            uti: None,
            mime_type: None,
            transfer_name: None,
            total_bytes: 0,
            is_sticker: false,
            hide_attachment: 0,
            emoji_description: None,
            copied_path: None,
        }
    }

    fn att_range(guid: Option<&str>) -> AttributedRange {
        AttributedRange::attachment(
            0,
            1,
            AttachmentMeta {
                guid: guid.map(str::to_string),
                ..AttachmentMeta::default()
            },
        )
    }

    #[test]
    fn positional_skips_guid_claimed_indices() {
        let attachments = vec![
            stub_attachment(Some("guid-a")),
            stub_attachment(Some("guid-b")),
            stub_attachment(None),
        ];
        let mut resolver = AttachmentResolver::new(&attachments);
        // First body range points at attachment 0 by GUID.
        assert_eq!(resolver.resolve(&att_range(Some("guid-a"))), 0);
        // Second range has no usable GUID — must take the next free slot (2),
        // not reuse 0.
        assert_eq!(resolver.resolve(&att_range(None)), 2);
        // Third range resolves the remaining GUID attachment.
        assert_eq!(resolver.resolve(&att_range(Some("guid-b"))), 1);
    }
}
