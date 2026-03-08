ALTER TABLE compositions ADD COLUMN namespace TEXT;
CREATE INDEX IF NOT EXISTS idx_compositions_namespace ON compositions(namespace);
