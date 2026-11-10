CREATE TABLE media_object (
    id bigserial PRIMARY KEY,
    content_hash text NOT NULL UNIQUE,
    canonical_pictrs_alias text NOT NULL UNIQUE,
    size_bytes bigint NOT NULL,
    width int NOT NULL,
    height int NOT NULL,
    content_type text NOT NULL,
    blurhash varchar(50),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE media_reference (
    id bigserial PRIMARY KEY,
    local_site_id int NOT NULL REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE,
    media_id bigint NOT NULL REFERENCES media_object (id) ON UPDATE CASCADE ON DELETE CASCADE,
    owner_person_id int REFERENCES person (id) ON UPDATE CASCADE ON DELETE CASCADE,
    post_id int REFERENCES post (id) ON UPDATE CASCADE ON DELETE CASCADE,
    purpose text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_media_reference_site ON media_reference (local_site_id);
CREATE INDEX idx_media_reference_media ON media_reference (media_id);
CREATE INDEX idx_media_reference_owner ON media_reference (owner_person_id);
CREATE INDEX idx_media_reference_post ON media_reference (post_id);

-- Backfill uploads that predate the media tables. The original content hash and dimensions were
-- never stored, so use a stable alias-derived hash and zero dimensions. These rows stay readable
-- and deletable through the normal media reference API, but they cannot participate in dedupe.
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
    site.id IS NOT NULL;
