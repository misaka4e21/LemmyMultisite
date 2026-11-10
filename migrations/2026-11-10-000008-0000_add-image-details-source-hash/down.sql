DROP TABLE post_thumbnail_source;
DROP INDEX idx_image_details_source_hash;
ALTER TABLE image_details DROP COLUMN source_hash;
