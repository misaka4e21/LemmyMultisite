CREATE TABLE local_image (
    pictrs_alias text CONSTRAINT image_upload_pictrs_alias_not_null NOT NULL,
    published_at timestamptz CONSTRAINT image_upload_published_not_null NOT NULL DEFAULT now(),
    person_id int REFERENCES person (id) ON UPDATE CASCADE ON DELETE CASCADE,
    thumbnail_for_post_id int REFERENCES post (id) ON UPDATE CASCADE ON DELETE CASCADE,
    CONSTRAINT image_upload_pkey PRIMARY KEY (pictrs_alias)
);

CREATE INDEX idx_image_upload_person_id ON local_image (person_id);

CREATE INDEX idx_local_image_thumbnail_post ON local_image (thumbnail_for_post_id);

INSERT INTO local_image (pictrs_alias, published_at, person_id, thumbnail_for_post_id)
SELECT DISTINCT ON (mo.canonical_pictrs_alias)
    mo.canonical_pictrs_alias,
    mr.created_at,
    mr.owner_person_id,
    mr.post_id
FROM
    media_object mo
    JOIN media_reference mr ON mr.media_id = mo.id
ORDER BY
    mo.canonical_pictrs_alias,
    mr.created_at;
