# Assigning existing site configuration

Back up the database and stop writes before upgrading. On a single-site installation, the migration assigns existing OAuth providers, custom emoji, and taglines to that site. On a multi-site installation, run the following command before starting the upgraded server:

```sh
psql -v ON_ERROR_STOP=1 -f scripts/prepare-site-configuration-migration.sql
```

The script creates `site_configuration_migration_map` and lists existing records and sites. Review each record and insert exactly one mapping using its listed type, record ID, and owning local site ID. OAuth account associations must belong to the same site as their mapped provider. Do not copy a provider's credentials to another site as part of the mapping.

```sql
INSERT INTO site_configuration_migration_map (resource_type, resource_id, local_site_id)
VALUES ('oauth_provider', 123, 456);
```

Repeat for every listed record. The migration rejects missing or unknown mappings and conflicting OAuth associations; the migration transaction leaves the original records unchanged on failure. Resolve conflicts and retry. Once upgraded, check each site's OAuth login, emoji, and tagline configuration.

Downgrading removes site ownership and requires client IDs, emoji shortcodes, and emoji image URLs to be globally unique again. If they are not, the downgrade fails. Stop writes and restore a verified backup when the new site-specific data cannot be represented safely in the old schema.
