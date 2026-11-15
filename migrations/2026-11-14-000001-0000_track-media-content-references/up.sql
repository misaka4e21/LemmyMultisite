-- Content references are kept separately from upload ownership. A content key identifies a
-- particular stored object; the site and media columns account for each site's use of it.
CREATE TABLE media_content_reference (
    local_site_id integer NOT NULL REFERENCES local_site (id) ON DELETE CASCADE,
    serving_site_id integer NOT NULL REFERENCES local_site (id) ON DELETE NO ACTION,
    media_id bigint NOT NULL REFERENCES media_object (id) ON DELETE CASCADE,
    content_kind text NOT NULL,
    content_key text NOT NULL,
    PRIMARY KEY (local_site_id, serving_site_id, media_id, content_kind, content_key)
);

CREATE INDEX idx_media_content_reference_media ON media_content_reference (media_id);
CREATE INDEX idx_media_content_reference_serving ON media_content_reference (serving_site_id, media_id);
CREATE INDEX idx_media_content_reference_content ON media_content_reference (content_kind, content_key);

-- Deletion callers must require completion before final alias cleanup.
CREATE TABLE media_reference_backfill_state (
    singleton boolean PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    completed boolean NOT NULL DEFAULT FALSE
);

INSERT INTO media_reference_backfill_state DEFAULT VALUES;

-- A queued row survives a process restart or a failure between the storage and database calls.
CREATE TABLE media_deletion_queue (
    media_id bigint PRIMARY KEY REFERENCES media_object (id) ON DELETE CASCADE,
    queued_at timestamptz NOT NULL DEFAULT now()
);

-- A reference can disappear through a foreign-key cascade, outside an API handler. Queue it
-- at transaction end so replacements in the same transaction are visible before the decision.
CREATE FUNCTION queue_unreferenced_media() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM 1 FROM media_object WHERE id = OLD.media_id FOR UPDATE;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;
    IF EXISTS (SELECT 1 FROM media_reference_backfill_state WHERE singleton AND completed)
        AND NOT EXISTS (SELECT 1 FROM media_reference WHERE media_id = OLD.media_id)
        AND NOT EXISTS (SELECT 1 FROM media_content_reference WHERE media_id = OLD.media_id) THEN
        INSERT INTO media_deletion_queue (media_id) VALUES (OLD.media_id) ON CONFLICT DO NOTHING;
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER media_upload_reference_deleted
    AFTER DELETE ON media_reference
    FOR EACH ROW EXECUTE FUNCTION queue_unreferenced_media();
CREATE TRIGGER media_content_reference_deleted
    AFTER DELETE ON media_content_reference
    FOR EACH ROW EXECUTE FUNCTION queue_unreferenced_media();

-- Remove polymorphic content references when their source or site-presence row is deleted.
CREATE FUNCTION forget_media_content_reference() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    old_row jsonb := to_jsonb(OLD);
    object_key text := old_row ->> TG_ARGV[1];
    owner_site integer;
BEGIN
    IF TG_TABLE_NAME = 'private_message_site' THEN
        IF EXISTS (
            SELECT 1 FROM private_message_site
            WHERE local_site_id = OLD.local_site_id AND private_message_id = OLD.private_message_id
        ) THEN
            RETURN OLD;
        END IF;
    END IF;
    IF TG_ARGV[3] <> '' THEN
        object_key := object_key || ':' || (old_row ->> TG_ARGV[3]);
    END IF;
    IF TG_ARGV[2] <> '' THEN
        owner_site := (old_row ->> TG_ARGV[2])::integer;
    END IF;
    DELETE FROM media_content_reference
    WHERE content_kind = TG_ARGV[0] AND content_key = object_key
        AND (owner_site IS NULL OR local_site_id = owner_site);
    RETURN OLD;
END;
$$;

CREATE TRIGGER forget_post_media AFTER DELETE ON post_site FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('post', 'post_id', 'local_site_id', '');
CREATE TRIGGER forget_comment_media AFTER DELETE ON comment_site FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('comment', 'comment_id', 'local_site_id', '');
CREATE TRIGGER forget_person_media AFTER DELETE ON person_site FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('person', 'person_id', 'local_site_id', '');
CREATE TRIGGER forget_community_media AFTER DELETE ON community_site FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('community', 'community_id', 'local_site_id', '');
CREATE TRIGGER forget_multi_community_media AFTER DELETE ON multi_community_site FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('multi_community', 'multi_community_id', 'local_site_id', '');
CREATE TRIGGER forget_private_message_media AFTER DELETE ON private_message_site FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('private_message', 'private_message_id', 'local_site_id', '');
CREATE TRIGGER forget_report_media AFTER DELETE ON report_site FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('report', 'report_combined_id', 'local_site_id', '');
CREATE TRIGGER forget_site_media AFTER DELETE ON site FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('site', 'id', '', '');
CREATE TRIGGER forget_tagline_media AFTER DELETE ON tagline FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('tagline', 'id', 'local_site_id', '');
CREATE TRIGGER forget_emoji_media AFTER DELETE ON custom_emoji FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('custom_emoji', 'id', 'local_site_id', '');
CREATE TRIGGER forget_person_note_media AFTER DELETE ON person_actions FOR EACH ROW
    EXECUTE FUNCTION forget_media_content_reference('person_note', 'person_id', '', 'target_id');
