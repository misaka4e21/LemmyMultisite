DROP INDEX idx_local_user_local_site_id;

DROP INDEX idx_local_user_email_site;

ALTER TABLE local_user
    ADD CONSTRAINT local_user_email_key UNIQUE (email);

ALTER TABLE local_user
    DROP COLUMN local_site_id;
