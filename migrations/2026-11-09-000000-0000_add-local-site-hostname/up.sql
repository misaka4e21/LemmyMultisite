ALTER TABLE local_site
    ADD COLUMN hostname varchar(255);

UPDATE local_site ls
SET hostname = i.domain
FROM site s
    JOIN instance i ON s.instance_id = i.id
WHERE ls.site_id = s.id;

ALTER TABLE local_site
    ALTER COLUMN hostname SET NOT NULL;

CREATE UNIQUE INDEX idx_local_site_hostname_lower ON local_site (lower(hostname));
