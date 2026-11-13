CREATE TABLE report_site (
    local_site_id integer NOT NULL REFERENCES local_site (id) ON DELETE CASCADE,
    report_combined_id integer NOT NULL REFERENCES report_combined (id) ON DELETE CASCADE,
    resolved boolean NOT NULL DEFAULT FALSE,
    resolver_id integer REFERENCES person (id) ON DELETE SET NULL,
    conclusion text,
    updated_at timestamptz,
    PRIMARY KEY (local_site_id, report_combined_id)
);

CREATE INDEX idx_report_site_report ON report_site (report_combined_id);

CREATE TABLE report_activity (
    ap_id text PRIMARY KEY,
    report_combined_id integer NOT NULL REFERENCES report_combined (id) ON DELETE CASCADE
);

-- A local reporter identifies one site. A resolution by another site is ambiguous and is left for
-- operator review rather than exposing its state to the reporter's site.
INSERT INTO report_site (local_site_id, report_combined_id, resolved, resolver_id, conclusion, updated_at)
SELECT creator.local_site_id, rc.id, rc.resolved, rc.resolver_id,
    COALESCE(pr.conclusion, cr.conclusion, cmr.conclusion, pmr.conclusion),
    COALESCE(pr.updated_at, cr.updated_at, cmr.updated_at, pmr.updated_at)
FROM report_combined rc
JOIN local_user creator ON creator.person_id = rc.report_creator_id
LEFT JOIN local_user resolver ON resolver.person_id = rc.resolver_id
LEFT JOIN post_report pr ON pr.id = rc.post_report_id
LEFT JOIN comment_report cr ON cr.id = rc.comment_report_id
LEFT JOIN community_report cmr ON cmr.id = rc.community_report_id
LEFT JOIN private_message_report pmr ON pmr.id = rc.private_message_report_id
WHERE rc.resolver_id IS NULL OR resolver.local_site_id = creator.local_site_id;
