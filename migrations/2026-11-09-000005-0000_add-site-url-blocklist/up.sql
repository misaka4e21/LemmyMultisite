ALTER TABLE local_site_url_blocklist
    ADD COLUMN local_site_id int;

UPDATE local_site_url_blocklist
SET local_site_id = (SELECT min(id) FROM local_site);

ALTER TABLE local_site_url_blocklist
    ALTER COLUMN local_site_id SET NOT NULL;

ALTER TABLE local_site_url_blocklist
    ADD CONSTRAINT local_site_url_blocklist_local_site_id_fkey FOREIGN KEY (local_site_id) REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

ALTER TABLE local_site_url_blocklist
    DROP CONSTRAINT local_site_url_blocklist_url_key;

ALTER TABLE local_site_url_blocklist
    ADD CONSTRAINT local_site_url_blocklist_site_url_key UNIQUE (local_site_id, url);

CREATE INDEX idx_local_site_url_blocklist_local_site_id ON local_site_url_blocklist (local_site_id);
