use crate::source::media::MediaReference;
use diesel::{
  QueryableByName,
  sql_types::{Bool, Integer, Nullable, Text},
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use lemmy_db_schema_file::newtypes::{LocalSiteId, ReportCombinedId};
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyError, LemmyResult};

#[derive(QueryableByName)]
struct BackfillState {
  #[diesel(sql_type = Bool)]
  completed: bool,
}

#[derive(QueryableByName)]
struct Content {
  #[diesel(sql_type = Integer)]
  local_site_id: i32,
  #[diesel(sql_type = Text)]
  kind: String,
  #[diesel(sql_type = Text)]
  content_key: String,
  #[diesel(sql_type = Nullable<Text>)]
  body: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  url1: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  url2: Option<String>,
  #[diesel(sql_type = Nullable<Text>)]
  url3: Option<String>,
}

/// Rebuilds all known content references before media can be deleted. A failed or ambiguous
/// scan rolls back and leaves deletion disabled; running the scan again is safe.
pub async fn backfill_media_content(pool: &mut DbPool<'_>) -> LemmyResult<bool> {
  let conn = &mut get_conn(pool).await?;
  let state = diesel::sql_query(
    "SELECT completed FROM media_reference_backfill_state WHERE singleton = true",
  )
  .get_result::<BackfillState>(conn)
  .await?;
  if state.completed {
    return Ok(false);
  }
  conn.transaction::<bool, LemmyError, _>(async |conn| {
    diesel::sql_query("SELECT pg_advisory_xact_lock(hashtextextended('media-site-catalog', 0))")
      .execute(conn).await?;
    diesel::sql_query(
      "LOCK TABLE site, local_site, person, person_site, community, community_site, \
       post, post_site, comment, comment_site, multi_community, multi_community_site, \
       private_message, private_message_site, tagline, custom_emoji, person_actions, local_user, \
       report_combined, report_site, comment_report, post_report, private_message_report, \
       community_report, media_object, media_reference, media_content_reference, \
       media_deletion_queue, media_reference_backfill_state IN SHARE ROW EXCLUSIVE MODE",
    ).execute(conn).await?;
    let state = diesel::sql_query(
      "SELECT completed FROM media_reference_backfill_state WHERE singleton = true",
    ).get_result::<BackfillState>(conn).await?;
    if state.completed {
      return Ok(false);
    }
    // A pending alias must not be removed while historical uses are being discovered.
    diesel::sql_query("DELETE FROM media_deletion_queue").execute(conn).await?;
    diesel::sql_query(
      "CREATE TEMP TABLE media_backfill_seen (local_site_id integer, kind text, content_key text, \
       PRIMARY KEY (local_site_id, kind, content_key)) ON COMMIT DROP",
    ).execute(conn).await?;
    let entries = diesel::sql_query(
      "SELECT DISTINCT content.*, video.embed_video_url::text AS url3 FROM ( \
       SELECT ps.local_site_id, 'post'::text AS kind, p.id::text AS content_key, p.body::text AS body, p.url::text AS url1, p.thumbnail_url::text AS url2 FROM post_site ps JOIN post p ON p.id = ps.post_id \
       UNION ALL SELECT cs.local_site_id, 'comment', c.id::text, c.content, NULL, NULL FROM comment_site cs JOIN comment c ON c.id = cs.comment_id \
       UNION ALL SELECT ps.local_site_id, 'person', p.id::text, p.bio, p.avatar::text, p.banner::text FROM person_site ps JOIN person p ON p.id = ps.person_id \
       UNION ALL SELECT cs.local_site_id, 'community', c.id::text, c.sidebar, c.icon::text, c.banner::text FROM community_site cs JOIN community c ON c.id = cs.community_id \
       UNION ALL SELECT ms.local_site_id, 'multi_community', m.id::text, m.sidebar, NULL, NULL FROM multi_community_site ms JOIN multi_community m ON m.id = ms.multi_community_id \
       UNION ALL SELECT ms.local_site_id, 'private_message', m.id::text, m.content, NULL, NULL FROM private_message_site ms JOIN private_message m ON m.id = ms.private_message_id \
       UNION ALL SELECT ls.id, 'site', s.id::text, s.sidebar, s.icon::text, s.banner::text FROM site s CROSS JOIN local_site ls LEFT JOIN local_site owner ON owner.site_id = s.id WHERE owner.id IS NULL OR owner.id = ls.id \
       UNION ALL SELECT t.local_site_id, 'tagline', t.id::text, t.content, NULL, NULL FROM tagline t \
       UNION ALL SELECT e.local_site_id, 'custom_emoji', e.id::text, NULL, e.image_url::text, NULL FROM custom_emoji e \
       UNION ALL SELECT lu.local_site_id, 'person_note', pa.person_id::text || ':' || pa.target_id::text, pa.note, NULL, NULL FROM person_actions pa JOIN local_user lu ON lu.person_id = pa.person_id WHERE pa.note IS NOT NULL \
       UNION ALL SELECT rs.local_site_id, 'report', rs.report_combined_id::text, NULL, NULL, NULL FROM report_site rs \
       ) content LEFT JOIN post video ON content.kind = 'post' AND video.id::text = content.content_key \
       ORDER BY kind, content_key, local_site_id",
    ).load::<Content>(conn).await?;
    for entry in entries {
      diesel::sql_query(
        "INSERT INTO media_backfill_seen (local_site_id, kind, content_key) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
      )
      .bind::<Integer, _>(entry.local_site_id)
      .bind::<Text, _>(&entry.kind)
      .bind::<Text, _>(&entry.content_key)
      .execute(conn).await?;
      let site_id = LocalSiteId(entry.local_site_id);
      let result = if entry.kind == "report" {
        let id = entry.content_key.parse::<i32>().map_err(|_| lemmy_utils::error::LemmyErrorType::CouldntUpdate)?;
        MediaReference::sync_report_content(&mut conn.into(), site_id, ReportCombinedId(id)).await
      } else {
        let urls: Vec<&str> = [entry.url1.as_deref(), entry.url2.as_deref(), entry.url3.as_deref()].into_iter().flatten().collect();
        MediaReference::sync_content(&mut conn.into(), site_id, &entry.kind, &entry.content_key, entry.body.as_deref(), &urls).await
      };
      result.map_err(|error| lemmy_utils::error::LemmyErrorType::Unknown(format!(
        "media backfill failed for {} {} on site {}: {error}",
        entry.kind, entry.content_key, entry.local_site_id
      )))?;
    }
    diesel::sql_query(
      "DELETE FROM media_content_reference c WHERE c.content_kind = ANY(ARRAY['post', 'comment', 'person', 'community', 'multi_community', 'private_message', 'site', 'tagline', 'custom_emoji', 'person_note', 'report']) \
       AND NOT EXISTS (SELECT 1 FROM media_backfill_seen s WHERE s.local_site_id = c.local_site_id AND s.kind = c.content_kind AND s.content_key = c.content_key)",
    ).execute(conn).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = true WHERE singleton = true")
      .execute(conn).await?;
    diesel::sql_query(
      "INSERT INTO media_deletion_queue (media_id) SELECT id FROM media_object m \
       WHERE NOT EXISTS (SELECT 1 FROM media_reference r WHERE r.media_id = m.id) \
       AND NOT EXISTS (SELECT 1 FROM media_content_reference c WHERE c.media_id = m.id) ON CONFLICT DO NOTHING",
    ).execute(conn).await?;
    Ok(true)
  }).await
}
