DROP INDEX idx_local_site_hostname_lower;

ALTER TABLE local_site
    DROP COLUMN hostname;
