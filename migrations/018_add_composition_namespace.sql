ALTER TABLE compositions ADD COLUMN namespace TEXT;
CREATE INDEX idx_compositions_namespace ON compositions(namespace);
