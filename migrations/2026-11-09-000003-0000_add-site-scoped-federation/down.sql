DROP INDEX idx_sent_activity_local_site_id;

DROP INDEX idx_federation_allowlist_instance_id;

DROP INDEX idx_federation_blocklist_instance_id;

DROP INDEX idx_federation_queue_state_instance_id;

ALTER TABLE sent_activity
    DROP COLUMN local_site_id;

ALTER TABLE received_activity
    DROP CONSTRAINT received_activity_pkey;

ALTER TABLE received_activity
    ADD PRIMARY KEY (ap_id);

ALTER TABLE received_activity
    DROP COLUMN local_site_id;

ALTER TABLE federation_allowlist
    DROP CONSTRAINT federation_allowlist_pkey;

ALTER TABLE federation_allowlist
    ADD PRIMARY KEY (instance_id);

ALTER TABLE federation_allowlist
    DROP COLUMN local_site_id;

ALTER TABLE federation_blocklist
    DROP CONSTRAINT federation_blocklist_pkey;

ALTER TABLE federation_blocklist
    ADD PRIMARY KEY (instance_id);

ALTER TABLE federation_blocklist
    DROP COLUMN local_site_id;

ALTER TABLE federation_queue_state
    DROP CONSTRAINT federation_queue_state_pkey;

ALTER TABLE federation_queue_state
    ADD PRIMARY KEY (instance_id);

ALTER TABLE federation_queue_state
    DROP COLUMN local_site_id;
