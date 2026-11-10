ALTER TABLE image_details ADD COLUMN source_hash text;

CREATE UNIQUE INDEX idx_image_details_source_hash
    ON image_details (source_hash)
    WHERE source_hash IS NOT NULL;

CREATE TABLE post_thumbnail_source (
    post_id int PRIMARY KEY REFERENCES post (id) ON UPDATE CASCADE ON DELETE CASCADE,
    source_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_post_thumbnail_source_hash ON post_thumbnail_source (source_hash);
