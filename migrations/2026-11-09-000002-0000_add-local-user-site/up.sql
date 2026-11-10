ALTER TABLE local_user
    ADD COLUMN local_site_id int;

UPDATE local_user
SET local_site_id = (SELECT min(id) FROM local_site);

ALTER TABLE local_user
    ALTER COLUMN local_site_id SET NOT NULL;

ALTER TABLE local_user
    DROP CONSTRAINT IF EXISTS local_user_email_key;

CREATE UNIQUE INDEX idx_local_user_email_site ON local_user (local_site_id, lower(email))
WHERE
    email IS NOT NULL;

CREATE INDEX idx_local_user_local_site_id ON local_user (local_site_id);
