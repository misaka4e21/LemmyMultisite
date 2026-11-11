-- First-site setup and site provisioning created the administrator and system account before the
-- LocalSite row existed. Person::create could not resolve a site, so those rows have no origin and
-- no person_site row. Fill both for local People that map to a Site.

UPDATE person p
SET origin_local_site_id = ls.id
FROM local_site ls
JOIN site s ON s.id = ls.site_id
WHERE p.instance_id = s.instance_id
  AND p.origin_local_site_id IS NULL
  AND ls.id = (
    SELECT min(inner_ls.id)
    FROM local_site inner_ls
    WHERE inner_ls.site_id = s.id
  );

INSERT INTO person_site (local_site_id, person_id, known_at, last_refreshed_at)
SELECT p.origin_local_site_id, p.id, p.published_at, p.last_refreshed_at
FROM person p
WHERE p.origin_local_site_id IS NOT NULL
ON CONFLICT DO NOTHING;
