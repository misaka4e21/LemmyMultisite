CREATE TABLE IF NOT EXISTS site_configuration_migration_map (
    resource_type text NOT NULL CHECK (resource_type IN ('oauth_provider', 'custom_emoji', 'tagline')),
    resource_id integer NOT NULL,
    local_site_id integer NOT NULL REFERENCES local_site (id) ON DELETE CASCADE,
    PRIMARY KEY (resource_type, resource_id)
);

DO $$
BEGIN
    IF (SELECT count(*) FROM local_site) <> 1 AND EXISTS (
        SELECT 1 FROM (
            SELECT 'oauth_provider'::text AS resource_type, id FROM oauth_provider
            UNION ALL SELECT 'custom_emoji', id FROM custom_emoji
            UNION ALL SELECT 'tagline', id FROM tagline
        ) records
        LEFT JOIN site_configuration_migration_map mapping
          ON mapping.resource_type = records.resource_type AND mapping.resource_id = records.id
        WHERE mapping.local_site_id IS NULL
    ) THEN
        RAISE EXCEPTION 'Site configuration ownership is incomplete; run scripts/prepare-site-configuration-migration.sql and map every record';
    END IF;
    IF EXISTS (
        SELECT 1 FROM site_configuration_migration_map mapping
        WHERE NOT EXISTS (
            SELECT 1 FROM (
                SELECT 'oauth_provider'::text AS resource_type, id FROM oauth_provider
                UNION ALL SELECT 'custom_emoji', id FROM custom_emoji
                UNION ALL SELECT 'tagline', id FROM tagline
            ) records
            WHERE mapping.resource_type = records.resource_type AND mapping.resource_id = records.id
        )
    ) THEN
        RAISE EXCEPTION 'Site configuration ownership contains an unknown record';
    END IF;
END $$;

ALTER TABLE oauth_provider ADD COLUMN local_site_id integer REFERENCES local_site (id) ON DELETE CASCADE;
ALTER TABLE custom_emoji ADD COLUMN local_site_id integer REFERENCES local_site (id) ON DELETE CASCADE;
ALTER TABLE tagline ADD COLUMN local_site_id integer REFERENCES local_site (id) ON DELETE CASCADE;

UPDATE oauth_provider record SET local_site_id = COALESCE(
    (SELECT local_site_id FROM site_configuration_migration_map WHERE resource_type = 'oauth_provider' AND resource_id = record.id),
    (SELECT min(id) FROM local_site HAVING count(*) = 1)
);
UPDATE custom_emoji record SET local_site_id = COALESCE(
    (SELECT local_site_id FROM site_configuration_migration_map WHERE resource_type = 'custom_emoji' AND resource_id = record.id),
    (SELECT min(id) FROM local_site HAVING count(*) = 1)
);
UPDATE tagline record SET local_site_id = COALESCE(
    (SELECT local_site_id FROM site_configuration_migration_map WHERE resource_type = 'tagline' AND resource_id = record.id),
    (SELECT min(id) FROM local_site HAVING count(*) = 1)
);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM oauth_account account
        JOIN local_user user_account ON user_account.id = account.local_user_id
        JOIN oauth_provider provider ON provider.id = account.oauth_provider_id
        WHERE user_account.local_site_id IS DISTINCT FROM provider.local_site_id
    ) THEN
        RAISE EXCEPTION 'OAuth account and provider site ownership conflict; resolve the mapping before retrying';
    END IF;
END $$;

ALTER TABLE oauth_provider ALTER COLUMN local_site_id SET NOT NULL;
ALTER TABLE custom_emoji ALTER COLUMN local_site_id SET NOT NULL;
ALTER TABLE tagline ALTER COLUMN local_site_id SET NOT NULL;

ALTER TABLE oauth_provider DROP CONSTRAINT IF EXISTS oauth_provider_client_id_key;
ALTER TABLE oauth_provider ADD CONSTRAINT oauth_provider_site_client_id_key UNIQUE (local_site_id, client_id);
ALTER TABLE custom_emoji DROP CONSTRAINT IF EXISTS custom_emoji_shortcode_key;
ALTER TABLE custom_emoji DROP CONSTRAINT IF EXISTS custom_emoji_image_url_key;
ALTER TABLE custom_emoji ADD CONSTRAINT custom_emoji_site_shortcode_key UNIQUE (local_site_id, shortcode);
ALTER TABLE custom_emoji ADD CONSTRAINT custom_emoji_site_image_url_key UNIQUE (local_site_id, image_url);
CREATE INDEX idx_tagline_local_site_id ON tagline (local_site_id);
