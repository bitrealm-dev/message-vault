-- Postgres twin of fts_virtual.sql + fts_triggers_create.sql: a tsvector
-- column on messages, a GIN index over it, and sync triggers that keep the
-- vector in step with messages.body/subject and attachment text. The 'simple'
-- config has no stemming, matching FTS5's unicode61 tokenizer default.
-- Idempotent: the column and index are IF NOT EXISTS, functions are
-- CREATE OR REPLACE, and triggers are re-created by dropping first (see
-- fts_postgres_drop.sql, which install_messages_fts_triggers runs first).
ALTER TABLE messages ADD COLUMN IF NOT EXISTS search_tsv tsvector;

CREATE INDEX IF NOT EXISTS ix_messages_search_tsv ON messages USING GIN (search_tsv);

CREATE OR REPLACE FUNCTION messages_fts_sync() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        UPDATE messages SET search_tsv = NULL WHERE id = OLD.id;
        RETURN OLD;
    END IF;
    UPDATE messages SET search_tsv = fts.vec
    FROM (
        SELECT m.id,
               to_tsvector('simple',
                   coalesce(m.body, '') || ' ' || coalesce(m.subject, '') || ' ' || coalesce(a.attachment_text, '')) AS vec
        FROM messages m
        LEFT JOIN (
            SELECT message_id,
                   string_agg(trim(coalesce(original_name, '') || ' ' || coalesce(transcription, '')), ' ') AS attachment_text
            FROM attachments
            GROUP BY message_id
        ) a ON a.message_id = m.id
        WHERE m.id = NEW.id
    ) fts
    WHERE messages.id = fts.id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER messages_fts_ai AFTER INSERT ON messages FOR EACH ROW EXECUTE FUNCTION messages_fts_sync();
CREATE TRIGGER messages_fts_au AFTER UPDATE OF body, subject ON messages FOR EACH ROW EXECUTE FUNCTION messages_fts_sync();
CREATE TRIGGER messages_fts_ad AFTER DELETE ON messages FOR EACH ROW EXECUTE FUNCTION messages_fts_sync();

CREATE OR REPLACE FUNCTION attachments_fts_sync() RETURNS trigger AS $$
DECLARE mid bigint;
BEGIN
    mid := COALESCE(NEW.message_id, OLD.message_id);
    UPDATE messages SET search_tsv = fts.vec
    FROM (
        SELECT m.id,
               to_tsvector('simple',
                   coalesce(m.body, '') || ' ' || coalesce(m.subject, '') || ' ' || coalesce(a.attachment_text, '')) AS vec
        FROM messages m
        LEFT JOIN (
            SELECT message_id,
                   string_agg(trim(coalesce(original_name, '') || ' ' || coalesce(transcription, '')), ' ') AS attachment_text
            FROM attachments
            GROUP BY message_id
        ) a ON a.message_id = m.id
        WHERE m.id = mid
    ) fts
    WHERE messages.id = fts.id;
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER attachments_fts_ai AFTER INSERT ON attachments FOR EACH ROW EXECUTE FUNCTION attachments_fts_sync();
CREATE TRIGGER attachments_fts_ad AFTER DELETE ON attachments FOR EACH ROW EXECUTE FUNCTION attachments_fts_sync();
CREATE TRIGGER attachments_fts_au AFTER UPDATE OF original_name, transcription ON attachments FOR EACH ROW EXECUTE FUNCTION attachments_fts_sync();
