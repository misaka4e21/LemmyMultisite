CREATE TABLE removed_site (
    hostname varchar(255) PRIMARY KEY,
    removed_at timestamptz NOT NULL DEFAULT now()
);
