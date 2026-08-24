-- Post-hoc foreign keys that cannot be declared inline: Postgres validates
-- FK targets at DDL time, so a REFERENCES to a table created by a later DDL
-- file (here: `handles` from pg_contacts.sql) fails. This file must run
-- after both the accounts and contacts DDL sets. Idempotent via
-- pg_constraint, so re-applying `ensure_vault_schema` is a no-op here.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'account_handles_handle_id_fkey'
    ) THEN
        ALTER TABLE account_handles
            ADD CONSTRAINT account_handles_handle_id_fkey
            FOREIGN KEY (handle_id) REFERENCES handles(id) ON DELETE CASCADE;
    END IF;
END $$;
