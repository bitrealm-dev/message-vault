INSERT INTO messages_fts(messages_fts) VALUES('delete-all');
INSERT INTO messages_fts(rowid, body, subject, attachment_text)
SELECT
    m.id,
    coalesce(m.body, ''),
    coalesce(m.subject, ''),
    coalesce((
        SELECT group_concat(
            trim(coalesce(a.original_name, '') || ' ' || coalesce(a.transcription, '')),
            ' '
        )
        FROM attachments a
        WHERE a.message_id = m.id
    ), '')
FROM messages m;
