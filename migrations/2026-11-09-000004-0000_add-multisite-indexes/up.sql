CREATE INDEX idx_person_origin_local_site_id ON person (origin_local_site_id);

CREATE INDEX idx_person_origin_local_site_name ON person (origin_local_site_id, lower(name))
WHERE
    origin_local_site_id IS NOT NULL;

CREATE INDEX idx_community_origin_local_site_id ON community (origin_local_site_id);

CREATE INDEX idx_community_origin_local_site_name ON community (origin_local_site_id, lower(name))
WHERE
    origin_local_site_id IS NOT NULL;

CREATE INDEX idx_post_origin_local_site_id ON post (origin_local_site_id);

CREATE INDEX idx_comment_origin_local_site_id ON comment (origin_local_site_id);

CREATE INDEX idx_private_message_origin_local_site_id ON private_message (origin_local_site_id);

CREATE INDEX idx_multi_community_origin_local_site_id ON multi_community (origin_local_site_id);

CREATE INDEX idx_multi_community_origin_local_site_name ON multi_community (origin_local_site_id, lower(name))
WHERE
    origin_local_site_id IS NOT NULL;

CREATE INDEX idx_received_activity_local_site_published ON received_activity (local_site_id, published_at);
