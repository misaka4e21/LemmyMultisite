-- Preserve thumbnails that were generated into the old table after the media tables were added.
INSERT INTO media_object (content_hash, canonical_pictrs_alias, size_bytes, width, height, content_type, created_at)
SELECT
    'legacy:' || pictrs_alias,
    pictrs_alias,
    0,
    0,
    0,
    'application/octet-stream',
    published_at
FROM
    local_image
ON CONFLICT (canonical_pictrs_alias) DO NOTHING;

INSERT INTO media_reference (local_site_id, media_id, owner_person_id, post_id, purpose, created_at)
SELECT
    site.id,
    mo.id,
    li.person_id,
    li.thumbnail_for_post_id,
    'other',
    li.published_at
FROM
    local_image li
    JOIN media_object mo ON mo.canonical_pictrs_alias = li.pictrs_alias
    CROSS JOIN LATERAL (
        SELECT
            min(id) AS id
        FROM
            local_site) site
WHERE
    site.id IS NOT NULL
    AND NOT EXISTS (
        SELECT
            1
        FROM
            media_reference mr
        WHERE
            mr.local_site_id = site.id
            AND mr.media_id = mo.id);

DROP TABLE local_image;
