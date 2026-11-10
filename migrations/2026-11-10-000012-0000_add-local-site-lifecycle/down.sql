DROP INDEX idx_local_site_disabled;
ALTER TABLE local_site DROP COLUMN removed_at;
ALTER TABLE local_site DROP COLUMN disabled;
