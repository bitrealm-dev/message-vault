CREATE TABLE IF NOT EXISTS schema_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    body,
    subject,
    attachment_text,
    content='',
    tokenize='unicode61 remove_diacritics 2'
);
