CREATE TABLE installation_admin (
    id bigserial PRIMARY KEY,
    username text NOT NULL UNIQUE,
    password_encrypted text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz
);
