-- Contentless FTS5 index over message body/subject plus attachment text.
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    -- Indexed message body text (synced from messages.body).
    body,
    -- Indexed message subject text (synced from messages.subject).
    subject,
    -- Indexed attachment filenames + transcriptions for the message.
    attachment_text,
    content='',
    tokenize='unicode61 remove_diacritics 2'
);
