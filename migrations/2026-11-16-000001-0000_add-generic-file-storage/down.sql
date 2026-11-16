DROP TABLE file_reference;
DROP TABLE file_object;

ALTER TABLE local_site
    DROP COLUMN file_upload_disabled,
    DROP COLUMN file_max_upload_size_bytes,
    DROP COLUMN file_allowed_mime_types;
