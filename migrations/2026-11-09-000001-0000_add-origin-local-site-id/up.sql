ALTER TABLE person
    ADD COLUMN origin_local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

ALTER TABLE community
    ADD COLUMN origin_local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

ALTER TABLE post
    ADD COLUMN origin_local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

ALTER TABLE comment
    ADD COLUMN origin_local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

ALTER TABLE private_message
    ADD COLUMN origin_local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

ALTER TABLE multi_community
    ADD COLUMN origin_local_site_id int REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE;

UPDATE person
SET origin_local_site_id = (SELECT min(id) FROM local_site)
WHERE local;

UPDATE community
SET origin_local_site_id = (SELECT min(id) FROM local_site)
WHERE local;

UPDATE post
SET origin_local_site_id = (SELECT min(id) FROM local_site)
WHERE local;

UPDATE comment
SET origin_local_site_id = (SELECT min(id) FROM local_site)
WHERE local;

UPDATE private_message
SET origin_local_site_id = (SELECT min(id) FROM local_site)
WHERE local;

UPDATE multi_community
SET origin_local_site_id = (SELECT min(id) FROM local_site)
WHERE local;
