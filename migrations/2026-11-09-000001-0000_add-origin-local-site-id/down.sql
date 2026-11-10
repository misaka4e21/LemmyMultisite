ALTER TABLE person
    DROP COLUMN origin_local_site_id;

ALTER TABLE community
    DROP COLUMN origin_local_site_id;

ALTER TABLE post
    DROP COLUMN origin_local_site_id;

ALTER TABLE comment
    DROP COLUMN origin_local_site_id;

ALTER TABLE private_message
    DROP COLUMN origin_local_site_id;

ALTER TABLE multi_community
    DROP COLUMN origin_local_site_id;
