CREATE TABLE internal_activity_delivery (
    id bigserial PRIMARY KEY,
    origin_site_id int NOT NULL REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE,
    destination_site_id int NOT NULL REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE,
    activity_id bigint NOT NULL REFERENCES sent_activity (id) ON UPDATE CASCADE ON DELETE CASCADE,
    state text NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'processing', 'done', 'failed')),
    attempts int NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz,
    UNIQUE (origin_site_id, destination_site_id, activity_id)
);

CREATE INDEX idx_internal_activity_delivery_state
    ON internal_activity_delivery (state, created_at);

CREATE INDEX idx_internal_activity_delivery_destination_site_id
    ON internal_activity_delivery (destination_site_id);

CREATE INDEX idx_internal_activity_delivery_activity_id
    ON internal_activity_delivery (activity_id);
