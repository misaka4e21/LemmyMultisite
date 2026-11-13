-- Run after the report-site migration to identify records needing operator review.
-- A remote reporter or a resolver from another site does not identify a safe receiving site.
SELECT rc.id AS report_combined_id,
       rc.post_report_id,
       rc.comment_report_id,
       rc.community_report_id,
       rc.private_message_report_id,
       rc.report_creator_id,
       creator.local_site_id AS creator_site_id,
       rc.resolver_id,
       resolver.local_site_id AS resolver_site_id,
       rc.resolved
FROM report_combined rc
LEFT JOIN local_user creator ON creator.person_id = rc.report_creator_id
LEFT JOIN local_user resolver ON resolver.person_id = rc.resolver_id
WHERE NOT EXISTS (
    SELECT 1
    FROM report_site rs
    WHERE rs.report_combined_id = rc.id
)
ORDER BY rc.id;
