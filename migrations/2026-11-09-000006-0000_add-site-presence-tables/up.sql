CREATE TABLE person_site (
    local_site_id int NOT NULL REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE,
    person_id int NOT NULL REFERENCES person (id) ON UPDATE CASCADE ON DELETE CASCADE,
    known_at timestamptz NOT NULL DEFAULT now(),
    last_refreshed_at timestamptz,
    PRIMARY KEY (local_site_id, person_id)
);

CREATE TABLE community_site (
    local_site_id int NOT NULL REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE,
    community_id int NOT NULL REFERENCES community (id) ON UPDATE CASCADE ON DELETE CASCADE,
    known_at timestamptz NOT NULL DEFAULT now(),
    last_refreshed_at timestamptz,
    removed bool NOT NULL DEFAULT FALSE,
    PRIMARY KEY (local_site_id, community_id)
);

CREATE TABLE post_site (
    local_site_id int NOT NULL REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE,
    post_id int NOT NULL REFERENCES post (id) ON UPDATE CASCADE ON DELETE CASCADE,
    known_at timestamptz NOT NULL DEFAULT now(),
    last_refreshed_at timestamptz,
    removed bool NOT NULL DEFAULT FALSE,
    locked bool NOT NULL DEFAULT FALSE,
    featured bool NOT NULL DEFAULT FALSE,
    PRIMARY KEY (local_site_id, post_id)
);

CREATE TABLE comment_site (
    local_site_id int NOT NULL REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE,
    comment_id int NOT NULL REFERENCES comment (id) ON UPDATE CASCADE ON DELETE CASCADE,
    known_at timestamptz NOT NULL DEFAULT now(),
    last_refreshed_at timestamptz,
    removed bool NOT NULL DEFAULT FALSE,
    PRIMARY KEY (local_site_id, comment_id)
);

CREATE TABLE multi_community_site (
    local_site_id int NOT NULL REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE,
    multi_community_id int NOT NULL REFERENCES multi_community (id) ON UPDATE CASCADE ON DELETE CASCADE,
    known_at timestamptz NOT NULL DEFAULT now(),
    last_refreshed_at timestamptz,
    PRIMARY KEY (local_site_id, multi_community_id)
);

CREATE TABLE private_message_site (
    local_site_id int NOT NULL REFERENCES local_site (id) ON UPDATE CASCADE ON DELETE CASCADE,
    private_message_id int NOT NULL REFERENCES private_message (id) ON UPDATE CASCADE ON DELETE CASCADE,
    role text NOT NULL,
    known_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (local_site_id, private_message_id),
    CONSTRAINT private_message_site_role_check CHECK (role IN ('sender', 'recipient', 'both'))
);

CREATE INDEX idx_person_site_person_id ON person_site (person_id);
CREATE INDEX idx_community_site_community_id ON community_site (community_id);
CREATE INDEX idx_post_site_post_id ON post_site (post_id);
CREATE INDEX idx_comment_site_comment_id ON comment_site (comment_id);
CREATE INDEX idx_multi_community_site_multi_community_id ON multi_community_site (multi_community_id);
CREATE INDEX idx_private_message_site_private_message_id ON private_message_site (private_message_id);

-- Existing installations have one site which currently sees all known objects.
INSERT INTO person_site (local_site_id, person_id, known_at, last_refreshed_at)
SELECT (SELECT min(id) FROM local_site), id, published_at, last_refreshed_at
FROM person
WHERE EXISTS (SELECT 1 FROM local_site);

INSERT INTO community_site (local_site_id, community_id, known_at, last_refreshed_at, removed)
SELECT (SELECT min(id) FROM local_site), id, published_at, last_refreshed_at, removed
FROM community
WHERE EXISTS (SELECT 1 FROM local_site);

INSERT INTO post_site (local_site_id, post_id, known_at, last_refreshed_at, removed, locked, featured)
SELECT (SELECT min(id) FROM local_site), id, published_at, updated_at, removed, locked, featured_local
FROM post
WHERE EXISTS (SELECT 1 FROM local_site);

INSERT INTO comment_site (local_site_id, comment_id, known_at, last_refreshed_at, removed)
SELECT (SELECT min(id) FROM local_site), id, published_at, updated_at, removed
FROM comment
WHERE EXISTS (SELECT 1 FROM local_site);

INSERT INTO multi_community_site (local_site_id, multi_community_id, known_at, last_refreshed_at)
SELECT (SELECT min(id) FROM local_site), id, published_at, updated_at
FROM multi_community
WHERE EXISTS (SELECT 1 FROM local_site);

INSERT INTO private_message_site (local_site_id, private_message_id, role, known_at)
SELECT (SELECT min(id) FROM local_site), id, 'both', published_at
FROM private_message
WHERE EXISTS (SELECT 1 FROM local_site);
