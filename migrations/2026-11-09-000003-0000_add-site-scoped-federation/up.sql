ALTER TABLE sent_activity
    ADD COLUMN local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

UPDATE sent_activity
SET local_site_id = (SELECT min(id) FROM local_site);

ALTER TABLE sent_activity
    ALTER COLUMN local_site_id SET NOT NULL;

CREATE INDEX idx_sent_activity_local_site_id ON sent_activity (local_site_id, id);

ALTER TABLE received_activity
    ADD COLUMN local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

UPDATE received_activity
SET local_site_id = (SELECT min(id) FROM local_site);

ALTER TABLE received_activity
    ALTER COLUMN local_site_id SET NOT NULL;

ALTER TABLE received_activity
    DROP CONSTRAINT received_activity_pkey;

ALTER TABLE received_activity
    ADD PRIMARY KEY (local_site_id, ap_id);

ALTER TABLE federation_allowlist
    ADD COLUMN local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

UPDATE federation_allowlist
SET local_site_id = (SELECT min(id) FROM local_site);

ALTER TABLE federation_allowlist
    ALTER COLUMN local_site_id SET NOT NULL;

ALTER TABLE federation_allowlist
    DROP CONSTRAINT federation_allowlist_pkey;

ALTER TABLE federation_allowlist
    ADD PRIMARY KEY (local_site_id, instance_id);

CREATE INDEX idx_federation_allowlist_instance_id ON federation_allowlist (instance_id);

ALTER TABLE federation_blocklist
    ADD COLUMN local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

UPDATE federation_blocklist
SET local_site_id = (SELECT min(id) FROM local_site);

ALTER TABLE federation_blocklist
    ALTER COLUMN local_site_id SET NOT NULL;

ALTER TABLE federation_blocklist
    DROP CONSTRAINT federation_blocklist_pkey;

ALTER TABLE federation_blocklist
    ADD PRIMARY KEY (local_site_id, instance_id);

CREATE INDEX idx_federation_blocklist_instance_id ON federation_blocklist (instance_id);

ALTER TABLE federation_queue_state
    ADD COLUMN local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

UPDATE federation_queue_state
SET local_site_id = (SELECT min(id) FROM local_site);

ALTER TABLE federation_queue_state
    ALTER COLUMN local_site_id SET NOT NULL;

ALTER TABLE federation_queue_state
    DROP CONSTRAINT federation_queue_state_pkey;

ALTER TABLE federation_queue_state
    ADD PRIMARY KEY (local_site_id, instance_id);

CREATE INDEX idx_federation_queue_state_instance_id ON federation_queue_state (instance_id);
