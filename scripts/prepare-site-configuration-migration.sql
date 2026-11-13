-- Run before the site-configuration migration. The operator must review each row and insert one
-- mapping for every existing record when the installation has more than one site.
CREATE TABLE IF NOT EXISTS site_configuration_migration_map (
    resource_type text NOT NULL CHECK (resource_type IN ('oauth_provider', 'custom_emoji', 'tagline')),
    resource_id integer NOT NULL,
    local_site_id integer NOT NULL REFERENCES local_site (id) ON DELETE CASCADE,
    PRIMARY KEY (resource_type, resource_id)
);

SELECT 'oauth_provider' AS resource_type, id AS resource_id, display_name AS label
FROM oauth_provider
UNION ALL
SELECT 'custom_emoji', id, shortcode FROM custom_emoji
UNION ALL
SELECT 'tagline', id, content FROM tagline
ORDER BY resource_type, resource_id;

SELECT id, hostname FROM local_site ORDER BY id;
