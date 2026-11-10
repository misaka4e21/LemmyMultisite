DROP INDEX idx_local_site_url_blocklist_local_site_id;

ALTER TABLE local_site_url_blocklist
    DROP CONSTRAINT local_site_url_blocklist_site_url_key;

ALTER TABLE local_site_url_blocklist
    ADD CONSTRAINT local_site_url_blocklist_url_key UNIQUE (url);

ALTER TABLE local_site_url_blocklist
    DROP CONSTRAINT local_site_url_blocklist_local_site_id_fkey;

ALTER TABLE local_site_url_blocklist
    DROP COLUMN local_site_id;
