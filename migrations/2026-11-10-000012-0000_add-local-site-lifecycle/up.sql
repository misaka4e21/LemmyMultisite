ALTER TABLE local_site ADD COLUMN disabled bool NOT NULL DEFAULT FALSE;
ALTER TABLE local_site ADD COLUMN removed_at timestamptz;

CREATE INDEX idx_local_site_disabled ON local_site (disabled);
