use crate::source::{
  comment::Comment,
  community::Community,
  media::{MediaObject, MediaObjectInsertForm, MediaReference, MediaReferenceInsertForm},
  multi_community::MultiCommunity,
  person::Person,
  post::Post,
  private_message::PrivateMessage,
  site::Site,
};
use diesel::{
  ExpressionMethods, JoinOnDsl, OptionalExtension, PgExpressionMethods, QueryDsl,
  dsl::{delete, exists, insert_into},
  select,
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use lemmy_db_schema_file::schema::{
  comment, comment_site, community, community_site, local_site, media_object, media_reference,
  multi_community, multi_community_site, person, person_site, post, post_site, private_message,
  private_message_site, site,
};
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::{
  error::{LemmyError, LemmyErrorExt, LemmyErrorType, LemmyResult},
  utils::markdown::image_links::{
    local_image_source, markdown_find_image_links, markdown_find_links, markdown_handle_title,
    markdown_local_media_sources, unresolved_local_media_url,
  },
};
use url::Url;

#[derive(diesel::QueryableByName)]
struct BooleanResult {
  #[diesel(sql_type = diesel::sql_types::Bool)]
  value: bool,
}

pub enum UploadRegistration {
  Ready(MediaObject, bool),
  Retry(Option<String>),
}

impl MediaObject {
  /// Inserts a new media object, or returns the existing object with the same content hash.
  ///
  /// The boolean is true when this call created the object. Callers use the canonical alias from
  /// the returned object, so duplicate uploads can be purged from pict-rs.
  pub async fn create_or_read(
    pool: &mut DbPool<'_>,
    form: &MediaObjectInsertForm,
  ) -> LemmyResult<(Self, bool)> {
    let conn = &mut get_conn(pool).await?;

    let existing = media_object::table
      .filter(media_object::content_hash.eq(&form.content_hash))
      .first::<Self>(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::NotFound)?;
    if let Some(existing) = existing {
      return Ok((existing, false));
    }

    let inserted = insert_into(media_object::table)
      .values(form)
      .on_conflict(media_object::content_hash)
      .do_nothing()
      .get_result::<Self>(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
    if let Some(inserted) = inserted {
      return Ok((inserted, true));
    }

    media_object::table
      .filter(media_object::content_hash.eq(&form.content_hash))
      .first::<Self>(conn)
      .await
      .map(|existing| (existing, false))
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }

  pub async fn read_by_alias(pool: &mut DbPool<'_>, alias: &str) -> LemmyResult<Option<Self>> {
    let conn = &mut get_conn(pool).await?;
    media_object::table
      .filter(media_object::canonical_pictrs_alias.eq(alias))
      .first(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  pub async fn deletion_queued(pool: &mut DbPool<'_>, media_id: i64) -> LemmyResult<bool> {
    let conn = &mut get_conn(pool).await?;
    Ok(
      diesel::sql_query(
        "SELECT EXISTS(SELECT 1 FROM media_deletion_queue WHERE media_id = $1) AS value",
      )
      .bind::<diesel::sql_types::BigInt, _>(media_id)
      .get_result::<BooleanResult>(conn)
      .await?
      .value,
    )
  }

  pub async fn queued_aliases(pool: &mut DbPool<'_>) -> LemmyResult<Vec<String>> {
    #[derive(diesel::QueryableByName)]
    struct Alias {
      #[diesel(sql_type = diesel::sql_types::Text)]
      alias: String,
    }
    let conn = &mut get_conn(pool).await?;
    Ok(
      diesel::sql_query(
        "SELECT media_object.canonical_pictrs_alias AS alias FROM media_deletion_queue \
         JOIN media_object ON media_object.id = media_deletion_queue.media_id \
         WHERE (SELECT completed FROM media_reference_backfill_state WHERE singleton = true) ORDER BY media_id",
      )
      .load::<Alias>(conn)
      .await?
      .into_iter()
      .map(|row| row.alias)
      .collect(),
    )
  }

  pub async fn delete_queued_by_alias(pool: &mut DbPool<'_>, alias: &str) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    Ok(
      diesel::sql_query(
        "DELETE FROM media_object WHERE canonical_pictrs_alias = $1 \
         AND EXISTS (SELECT 1 FROM media_deletion_queue WHERE media_id = media_object.id)",
      )
      .bind::<diesel::sql_types::Text, _>(alias)
      .execute(conn)
      .await?,
    )
  }

  /// Deletes one media object by alias. References are removed by the foreign key cascade.
  pub async fn delete_by_alias(pool: &mut DbPool<'_>, alias: &str) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    delete(media_object::table.filter(media_object::canonical_pictrs_alias.eq(alias)))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }

  /// Deletes every media object for the given aliases. References are removed by the cascade.
  pub async fn delete_by_aliases(pool: &mut DbPool<'_>, aliases: &[String]) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    delete(media_object::table.filter(media_object::canonical_pictrs_alias.eq_any(aliases)))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }
}

impl MediaReference {
  /// Atomically selects or creates a media object and records the upload reference.
  pub async fn register_upload(
    pool: &mut DbPool<'_>,
    media_form: &MediaObjectInsertForm,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    owner: Option<lemmy_db_schema_file::PersonId>,
    post_id: Option<lemmy_db_schema_file::newtypes::PostId>,
    purpose: &str,
  ) -> LemmyResult<UploadRegistration> {
    let conn = &mut get_conn(pool).await?;
    conn
      .transaction::<UploadRegistration, LemmyError, _>(async |conn| {
        let inserted = insert_into(media_object::table)
          .values(media_form)
          .on_conflict(media_object::content_hash)
          .do_nothing()
          .get_result::<MediaObject>(conn)
          .await
          .optional()?;
        let created = inserted.is_some();
        let media = if let Some(media) = inserted {
          Some(media)
        } else {
          media_object::table
            .filter(media_object::content_hash.eq(&media_form.content_hash))
            .for_update()
            .first::<MediaObject>(conn)
            .await
            .optional()?
        };
        let Some(media) = media else {
          return Ok(UploadRegistration::Retry(None));
        };
        let pending = diesel::sql_query(
          "SELECT EXISTS(SELECT 1 FROM media_deletion_queue WHERE media_id = $1) AS value",
        )
        .bind::<diesel::sql_types::BigInt, _>(media.id)
        .get_result::<BooleanResult>(conn)
        .await?;
        if pending.value {
          return Ok(UploadRegistration::Retry(Some(
            media.canonical_pictrs_alias,
          )));
        }
        Self::ensure(
          &mut conn.into(),
          &MediaReferenceInsertForm {
            local_site_id: site_id,
            media_id: media.id,
            owner_person_id: owner,
            post_id,
            purpose: purpose.to_string(),
          },
        )
        .await?;
        Ok(UploadRegistration::Ready(media, created))
      })
      .await
  }

  /// Tracks the immutable snapshots and reason stored with one report for a receiving site.
  pub async fn sync_report_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    report_id: lemmy_db_schema_file::newtypes::ReportCombinedId,
  ) -> LemmyResult<()> {
    #[derive(diesel::QueryableByName)]
    struct ReportText {
      #[diesel(sql_type = diesel::sql_types::Text)]
      body: String,
      #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
      post_url: Option<String>,
      #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
      community_icon: Option<String>,
      #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
      community_banner: Option<String>,
    }
    let conn = &mut get_conn(pool).await?;
    let report = diesel::sql_query(
      "SELECT concat_ws(E'\\n', cr.original_comment_text, cr.reason, \
       pr.original_post_body, pr.reason, \
       pm.original_pm_text, pm.reason, \
       cor.original_community_sidebar, cor.reason, rs.conclusion) AS body, \
       pr.original_post_url::text AS post_url, cor.original_community_icon::text AS community_icon, \
       cor.original_community_banner::text AS community_banner \
       FROM report_combined rc \
       JOIN report_site rs ON rs.report_combined_id = rc.id AND rs.local_site_id = $2 \
       LEFT JOIN comment_report cr ON cr.id = rc.comment_report_id \
       LEFT JOIN post_report pr ON pr.id = rc.post_report_id \
       LEFT JOIN private_message_report pm ON pm.id = rc.private_message_report_id \
       LEFT JOIN community_report cor ON cor.id = rc.community_report_id WHERE rc.id = $1",
    )
    .bind::<diesel::sql_types::Integer, _>(report_id.0)
    .bind::<diesel::sql_types::Integer, _>(site_id.0)
    .get_result::<ReportText>(conn)
    .await?;
    let urls: Vec<&str> = [
      report.post_url.as_deref(),
      report.community_icon.as_deref(),
      report.community_banner.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    Self::sync_content(
      &mut conn.into(),
      site_id,
      "report",
      &report_id.0.to_string(),
      Some(&report.body),
      &urls,
    )
    .await
  }

  pub async fn sync_owned_site_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::SiteId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let local = local_site::table
      .filter(local_site::site_id.eq(site_id))
      .select(local_site::id)
      .first::<lemmy_db_schema_file::newtypes::LocalSiteId>(conn)
      .await
      .optional()?;
    let desired = if let Some(local_site_id) = local {
      vec![local_site_id]
    } else {
      local_site::table
        .select(local_site::id)
        .load::<lemmy_db_schema_file::newtypes::LocalSiteId>(conn)
        .await?
    };
    #[derive(diesel::QueryableByName)]
    struct ExistingSite {
      #[diesel(sql_type = diesel::sql_types::Integer)]
      local_site_id: i32,
    }
    let prior = diesel::sql_query(
      "SELECT DISTINCT local_site_id FROM media_content_reference WHERE content_kind = 'site' AND content_key = $1",
    )
    .bind::<diesel::sql_types::Text, _>(site_id.0.to_string())
    .load::<ExistingSite>(conn)
    .await?;
    for local_site_id in &desired {
      Self::sync_local_site_content(&mut conn.into(), *local_site_id, site_id).await?;
    }
    for old in prior {
      let old_site = lemmy_db_schema_file::newtypes::LocalSiteId(old.local_site_id);
      if !desired.contains(&old_site) {
        Self::replace_for_content(
          &mut conn.into(),
          old_site,
          "site",
          &site_id.0.to_string(),
          &[],
        )
        .await?;
      }
    }
    Ok(())
  }

  pub async fn sync_local_site_content(
    pool: &mut DbPool<'_>,
    local_site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    site_id: lemmy_db_schema_file::newtypes::SiteId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let stored = site::table.find(site_id).first::<Site>(conn).await?;
    let urls: Vec<&str> = [stored.icon.as_ref(), stored.banner.as_ref()]
      .into_iter()
      .flatten()
      .map(|url| url.as_str())
      .collect();
    Self::sync_content(
      &mut conn.into(),
      local_site_id,
      "site",
      &site_id.0.to_string(),
      stored.sidebar.as_deref(),
      &urls,
    )
    .await
  }

  pub async fn sync_private_message_all_sites(
    pool: &mut DbPool<'_>,
    id: lemmy_db_schema_file::newtypes::PrivateMessageId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let sites = private_message_site::table
      .filter(private_message_site::private_message_id.eq(id))
      .select(private_message_site::local_site_id)
      .distinct()
      .load::<lemmy_db_schema_file::newtypes::LocalSiteId>(conn)
      .await?;
    for site_id in sites {
      Self::sync_private_message_content(&mut conn.into(), site_id, id).await?;
    }
    Ok(())
  }

  pub async fn sync_private_message_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    id: lemmy_db_schema_file::newtypes::PrivateMessageId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let stored = private_message::table
      .find(id)
      .first::<PrivateMessage>(conn)
      .await?;
    Self::sync_content(
      &mut conn.into(),
      site_id,
      "private_message",
      &id.0.to_string(),
      Some(&stored.content),
      &[],
    )
    .await
  }

  pub async fn sync_multi_community_all_sites(
    pool: &mut DbPool<'_>,
    id: lemmy_db_schema_file::newtypes::MultiCommunityId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let sites = multi_community_site::table
      .filter(multi_community_site::multi_community_id.eq(id))
      .select(multi_community_site::local_site_id)
      .load::<lemmy_db_schema_file::newtypes::LocalSiteId>(conn)
      .await?;
    for site_id in sites {
      Self::sync_multi_community_content(&mut conn.into(), site_id, id).await?;
    }
    Ok(())
  }

  pub async fn sync_multi_community_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    id: lemmy_db_schema_file::newtypes::MultiCommunityId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let stored = multi_community::table
      .find(id)
      .first::<MultiCommunity>(conn)
      .await?;
    Self::sync_content(
      &mut conn.into(),
      site_id,
      "multi_community",
      &id.0.to_string(),
      stored.sidebar.as_deref(),
      &[],
    )
    .await
  }

  pub async fn sync_community_all_sites(
    pool: &mut DbPool<'_>,
    community_id: lemmy_db_schema_file::newtypes::CommunityId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let sites = community_site::table
      .filter(community_site::community_id.eq(community_id))
      .select(community_site::local_site_id)
      .load::<lemmy_db_schema_file::newtypes::LocalSiteId>(conn)
      .await?;
    for site_id in sites {
      Self::sync_community_content(&mut conn.into(), site_id, community_id).await?;
    }
    Ok(())
  }

  pub async fn sync_community_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    community_id: lemmy_db_schema_file::newtypes::CommunityId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let stored = community::table
      .find(community_id)
      .first::<Community>(conn)
      .await?;
    let urls: Vec<&str> = [stored.icon.as_ref(), stored.banner.as_ref()]
      .into_iter()
      .flatten()
      .map(|url| url.as_str())
      .collect();
    Self::sync_content(
      &mut conn.into(),
      site_id,
      "community",
      &community_id.0.to_string(),
      stored.sidebar.as_deref(),
      &urls,
    )
    .await
  }

  pub async fn sync_person_all_sites(
    pool: &mut DbPool<'_>,
    person_id: lemmy_db_schema_file::PersonId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let sites = person_site::table
      .filter(person_site::person_id.eq(person_id))
      .select(person_site::local_site_id)
      .load::<lemmy_db_schema_file::newtypes::LocalSiteId>(conn)
      .await?;
    for site_id in sites {
      Self::sync_person_content(&mut conn.into(), site_id, person_id).await?;
    }
    Ok(())
  }

  pub async fn sync_person_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    person_id: lemmy_db_schema_file::PersonId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let stored = person::table.find(person_id).first::<Person>(conn).await?;
    let urls: Vec<&str> = [stored.avatar.as_ref(), stored.banner.as_ref()]
      .into_iter()
      .flatten()
      .map(|url| url.as_str())
      .collect();
    Self::sync_content(
      &mut conn.into(),
      site_id,
      "person",
      &person_id.0.to_string(),
      stored.bio.as_deref(),
      &urls,
    )
    .await
  }

  pub async fn sync_comment_all_sites(
    pool: &mut DbPool<'_>,
    comment_id: lemmy_db_schema_file::newtypes::CommentId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let sites = comment_site::table
      .filter(comment_site::comment_id.eq(comment_id))
      .select(comment_site::local_site_id)
      .load::<lemmy_db_schema_file::newtypes::LocalSiteId>(conn)
      .await?;
    for site_id in sites {
      Self::sync_comment_content(&mut conn.into(), site_id, comment_id).await?;
    }
    Ok(())
  }

  pub async fn sync_comment_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    comment_id: lemmy_db_schema_file::newtypes::CommentId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let stored = comment::table
      .find(comment_id)
      .first::<Comment>(conn)
      .await?;
    Self::sync_content(
      &mut conn.into(),
      site_id,
      "comment",
      &comment_id.0.to_string(),
      Some(&stored.content),
      &[],
    )
    .await
  }

  pub async fn sync_post_all_sites(
    pool: &mut DbPool<'_>,
    post_id: lemmy_db_schema_file::newtypes::PostId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let sites = post_site::table
      .filter(post_site::post_id.eq(post_id))
      .select(post_site::local_site_id)
      .load::<lemmy_db_schema_file::newtypes::LocalSiteId>(conn)
      .await?;
    for site_id in sites {
      Self::sync_post_content(&mut conn.into(), site_id, post_id).await?;
    }
    Ok(())
  }

  /// Rebuilds a post's local media references for a site. The caller must serialize this with
  /// updates to the post and its site presence.
  pub async fn sync_post_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    post_id: lemmy_db_schema_file::newtypes::PostId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let stored_post = post::table.find(post_id).first::<Post>(conn).await?;
    let urls: Vec<&str> = [
      stored_post.url.as_ref(),
      stored_post.thumbnail_url.as_ref(),
      stored_post.embed_video_url.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|url| url.as_str())
    .collect();
    Self::sync_content(
      &mut conn.into(),
      site_id,
      "post",
      &post_id.0.to_string(),
      stored_post.body.as_deref(),
      &urls,
    )
    .await
  }

  /// Tracks Markdown and direct local media URLs for a content object and one site.
  pub async fn sync_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    kind: &str,
    key: &str,
    markdown: Option<&str>,
    urls: &[&str],
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    let sites = local_site::table
      .inner_join(site::table)
      .select((local_site::id, site::ap_id, local_site::hostname))
      .load::<(
        lemmy_db_schema_file::newtypes::LocalSiteId,
        lemmy_diesel_utils::dburl::DbUrl,
        String,
      )>(conn)
      .await?;
    let mut site_origins = Vec::new();
    for (id, origin, hostname) in sites {
      let origin = Url::parse(origin.as_str())?;
      let configured = Url::parse(&format!("{}://{}", origin.scheme(), hostname))?;
      site_origins.push((id, origin));
      site_origins.push((id, configured));
    }
    let origins: Vec<Url> = site_origins
      .iter()
      .map(|(_, origin)| origin.clone())
      .collect();
    if let Some(text) = markdown {
      let mut ranges = markdown_find_links(text);
      ranges.extend(markdown_find_image_links(text));
      if ranges.into_iter().any(|(start, end)| {
        unresolved_local_media_url(markdown_handle_title(text, start, end).0, &origins)
      }) {
        return Err(LemmyErrorType::CouldntUpdate.into());
      }
    }
    if urls
      .iter()
      .any(|url| unresolved_local_media_url(url, &origins))
    {
      return Err(LemmyErrorType::CouldntUpdate.into());
    }
    let mut sources = markdown
      .map(|body| markdown_local_media_sources(body, &origins))
      .unwrap_or_default();
    for url in urls {
      if let Some(source) = local_image_source(&Url::parse(url)?, &origins) {
        sources.push(source);
      }
    }
    sources.sort_unstable();
    sources.dedup();
    let aliases: Vec<&str> = sources.iter().map(|(alias, _)| alias.as_str()).collect();
    let media = media_object::table
      .filter(media_object::canonical_pictrs_alias.eq_any(&aliases))
      .select((media_object::id, media_object::canonical_pictrs_alias))
      .load::<(i64, String)>(conn)
      .await?;
    let mut bindings = Vec::new();
    for (alias, origin) in sources {
      let media_id = media
        .iter()
        .find(|(_, name)| *name == alias)
        .map(|(id, _)| *id)
        .ok_or(LemmyErrorType::NotFound)?;
      let serving_site = site_origins
        .iter()
        .find(|(_, site_origin)| site_origin.origin().ascii_serialization() == origin)
        .map(|(id, _)| *id)
        .ok_or(LemmyErrorType::NotFound)?;
      if site_origins.iter().any(|(id, site_origin)| {
        *id != serving_site && site_origin.origin().ascii_serialization() == origin
      }) {
        return Err(LemmyErrorType::CouldntUpdate.into());
      }
      bindings.push((media_id, serving_site));
    }
    Self::replace_content_bindings(&mut conn.into(), site_id, kind, key, &bindings).await
  }

  /// Reconciles one content object's references for one site. Callers updating content must
  /// invoke this using the same transaction as the content or presence change.
  pub async fn replace_for_content(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    kind: &str,
    key: &str,
    media_ids: &[i64],
  ) -> LemmyResult<()> {
    let bindings: Vec<_> = media_ids.iter().map(|id| (*id, site_id)).collect();
    Self::replace_content_bindings(pool, site_id, kind, key, &bindings).await
  }

  pub async fn replace_content_bindings(
    pool: &mut DbPool<'_>,
    site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    kind: &str,
    key: &str,
    bindings: &[(i64, lemmy_db_schema_file::newtypes::LocalSiteId)],
  ) -> LemmyResult<()> {
    use diesel::sql_types::{Array, BigInt, Integer, Text};

    #[derive(diesel::QueryableByName)]
    struct LockedMedia {
      #[diesel(sql_type = BigInt)]
      id: i64,
    }

    let media_ids: Vec<i64> = bindings.iter().map(|(id, _)| *id).collect();
    let serving_sites: Vec<i32> = bindings.iter().map(|(_, site)| site.0).collect();
    let conn = &mut get_conn(pool).await?;
    conn
      .transaction::<(), LemmyError, _>(async |conn| {
        // Lock both removed and added media in a stable order. Deletion must take the same lock.
        let locked = diesel::sql_query(
          "SELECT id FROM media_object WHERE id = ANY($1) OR id IN \
           (SELECT media_id FROM media_content_reference WHERE local_site_id = $2 \
            AND content_kind = $3 AND content_key = $4) ORDER BY id FOR UPDATE",
        )
        .bind::<Array<BigInt>, _>(&media_ids)
        .bind::<Integer, _>(site_id.0)
        .bind::<Text, _>(kind)
        .bind::<Text, _>(key)
        .load::<LockedMedia>(conn)
        .await?;
        let locked_ids: Vec<i64> = locked.into_iter().map(|row| row.id).collect();
        if media_ids.iter().any(|id| !locked_ids.contains(id)) {
          return Err(LemmyErrorType::NotFound.into());
        }
        let pending = diesel::sql_query(
          "SELECT media_id AS id FROM media_deletion_queue WHERE media_id = ANY($1) LIMIT 1",
        )
        .bind::<Array<BigInt>, _>(&media_ids)
        .load::<LockedMedia>(conn)
        .await?;
        if !pending.is_empty() {
          return Err(LemmyErrorType::CouldntUpdate.into());
        }
        let unauthorized = diesel::sql_query(
          "SELECT EXISTS (SELECT 1 FROM unnest($1::bigint[], $2::integer[]) AS b(media_id, site_id) \
           WHERE NOT EXISTS (SELECT 1 FROM media_reference r WHERE r.media_id = b.media_id AND r.local_site_id = b.site_id) \
           AND NOT EXISTS (SELECT 1 FROM media_content_reference c WHERE c.media_id = b.media_id AND c.serving_site_id = b.site_id)) AS value",
        )
        .bind::<Array<BigInt>, _>(&media_ids)
        .bind::<Array<Integer>, _>(&serving_sites)
        .get_result::<BooleanResult>(conn)
        .await?;
        if unauthorized.value {
          return Err(LemmyErrorType::NotFound.into());
        }
        diesel::sql_query(
          "INSERT INTO media_content_reference (local_site_id, serving_site_id, media_id, content_kind, content_key) \
           SELECT $1, b.site_id, b.media_id, $2, $3 FROM unnest($4::bigint[], $5::integer[]) AS b(media_id, site_id) ON CONFLICT DO NOTHING",
        )
        .bind::<Integer, _>(site_id.0)
        .bind::<Text, _>(kind)
        .bind::<Text, _>(key)
        .bind::<Array<BigInt>, _>(&media_ids)
        .bind::<Array<Integer>, _>(&serving_sites)
        .execute(conn)
        .await?;
        diesel::sql_query(
          "DELETE FROM media_content_reference r WHERE r.local_site_id = $1 \
           AND r.content_kind = $2 AND r.content_key = $3 AND NOT EXISTS (\
           SELECT 1 FROM unnest($4::bigint[], $5::integer[]) AS b(media_id, site_id) \
           WHERE b.media_id = r.media_id AND b.site_id = r.serving_site_id)",
        )
        .bind::<Integer, _>(site_id.0)
        .bind::<Text, _>(kind)
        .bind::<Text, _>(key)
        .bind::<Array<BigInt>, _>(&media_ids)
        .bind::<Array<Integer>, _>(&serving_sites)
        .execute(conn)
        .await?;
        diesel::sql_query(
          "INSERT INTO media_deletion_queue (media_id) \
           SELECT id FROM media_object WHERE id = ANY($1) \
           AND (SELECT completed FROM media_reference_backfill_state WHERE singleton = true) \
           AND NOT EXISTS (SELECT 1 FROM media_reference WHERE media_id = media_object.id) \
           AND NOT EXISTS (SELECT 1 FROM media_content_reference WHERE media_id = media_object.id) \
           ON CONFLICT DO NOTHING",
        )
        .bind::<Array<BigInt>, _>(&locked_ids)
        .execute(conn)
        .await?;
        Ok(())
      })
      .await
  }

  /// Ensures that the active site has a reference to a media object.
  pub async fn ensure(pool: &mut DbPool<'_>, form: &MediaReferenceInsertForm) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    conn
      .transaction::<(), LemmyError, _>(async |conn| {
        media_object::table
          .find(form.media_id)
          .for_update()
          .first::<MediaObject>(conn)
          .await?;
        let pending = diesel::sql_query(
          "SELECT EXISTS(SELECT 1 FROM media_deletion_queue WHERE media_id = $1) AS value",
        )
        .bind::<diesel::sql_types::BigInt, _>(form.media_id)
        .get_result::<BooleanResult>(conn)
        .await?;
        if pending.value {
          return Err(LemmyErrorType::CouldntUpdate.into());
        }
        let exists = select(exists(
          media_reference::table
            .filter(media_reference::local_site_id.eq(form.local_site_id))
            .filter(media_reference::media_id.eq(form.media_id))
            .filter(media_reference::owner_person_id.is_not_distinct_from(form.owner_person_id))
            .filter(media_reference::post_id.is_not_distinct_from(form.post_id))
            .filter(media_reference::purpose.eq(&form.purpose)),
        ))
        .get_result::<bool>(conn)
        .await
        .with_lemmy_type(LemmyErrorType::NotFound)?;
        if exists {
          return Ok(());
        }

        insert_into(media_reference::table)
          .values(form)
          .execute(conn)
          .await
          .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
        Ok(())
      })
      .await
  }

  pub async fn validate_by_alias_and_user(
    pool: &mut DbPool<'_>,
    alias: &str,
    local_site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    person_id: lemmy_db_schema_file::PersonId,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;
    select(exists(
      media_reference::table
        .inner_join(media_object::table.on(media_object::id.eq(media_reference::media_id)))
        .filter(media_object::canonical_pictrs_alias.eq(alias))
        .filter(media_reference::local_site_id.eq(local_site_id))
        .filter(media_reference::owner_person_id.eq(Some(person_id))),
    ))
    .get_result::<bool>(conn)
    .await?
    .then_some(())
    .ok_or(LemmyErrorType::NotFound.into())
  }

  /// Deletes the user's references and queues the media when no references remain.
  pub async fn delete_for_site_and_user(
    pool: &mut DbPool<'_>,
    alias: &str,
    local_site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    person_id: lemmy_db_schema_file::PersonId,
  ) -> LemmyResult<bool> {
    Self::delete_for_site(pool, alias, local_site_id, Some(person_id)).await
  }

  /// Removes references owned by a site, optionally limited to one user. The returned flag
  /// indicates that the alias was queued for deletion after its final reference disappeared.
  pub async fn delete_for_site(
    pool: &mut DbPool<'_>,
    alias: &str,
    local_site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    person_id: Option<lemmy_db_schema_file::PersonId>,
  ) -> LemmyResult<bool> {
    use diesel::sql_types::{BigInt, Integer, Nullable};
    let conn = &mut get_conn(pool).await?;
    conn
      .transaction::<bool, LemmyError, _>(async |conn| {
        let media = media_object::table
          .filter(media_object::canonical_pictrs_alias.eq(alias))
          .for_update()
          .first::<MediaObject>(conn)
          .await?;
        let backfill = diesel::sql_query(
          "SELECT completed AS value FROM media_reference_backfill_state WHERE singleton = true",
        )
        .get_result::<BooleanResult>(conn)
        .await?;
        if !backfill.value {
          return Err(LemmyErrorType::CouldntUpdate.into());
        }
        let removed = diesel::sql_query(
          "DELETE FROM media_reference WHERE media_id = $1 AND local_site_id = $2 \
           AND ($3::integer IS NULL OR owner_person_id = $3)",
        )
        .bind::<BigInt, _>(media.id)
        .bind::<Integer, _>(local_site_id.0)
        .bind::<Nullable<Integer>, _>(person_id.map(|id| id.0))
        .execute(conn)
        .await?;
        if removed == 0 {
          return Err(LemmyErrorType::NotFound.into());
        }
        let remaining = diesel::sql_query(
          "SELECT (EXISTS(SELECT 1 FROM media_reference WHERE media_id = $1) OR \
           EXISTS(SELECT 1 FROM media_content_reference WHERE media_id = $1)) AS value",
        )
        .bind::<BigInt, _>(media.id)
        .get_result::<BooleanResult>(conn)
        .await?;
        if !remaining.value {
          diesel::sql_query(
            "INSERT INTO media_deletion_queue (media_id) VALUES ($1) ON CONFLICT DO NOTHING",
          )
          .bind::<BigInt, _>(media.id)
          .execute(conn)
          .await?;
        }
        Ok(!remaining.value)
      })
      .await
  }

  pub async fn exists_for_site(
    pool: &mut DbPool<'_>,
    local_site_id: lemmy_db_schema_file::newtypes::LocalSiteId,
    media_id: i64,
  ) -> LemmyResult<bool> {
    let conn = &mut get_conn(pool).await?;
    Ok(
      diesel::sql_query(
        "SELECT (EXISTS(SELECT 1 FROM media_reference WHERE local_site_id = $1 AND media_id = $2) \
         OR EXISTS(SELECT 1 FROM media_content_reference WHERE serving_site_id = $1 AND media_id = $2)) AS value",
      )
      .bind::<diesel::sql_types::Integer, _>(local_site_id.0)
      .bind::<diesel::sql_types::BigInt, _>(media_id)
      .get_result::<BooleanResult>(conn)
      .await?
      .value,
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    source::{
      instance::Instance,
      local_site::{LocalSite, LocalSiteInsertForm},
      person::{Person, PersonInsertForm},
      site::{Site, SiteUpsertForm},
    },
    test_data::TestData,
  };
  use chrono::Utc;
  use diesel::{ExpressionMethods, QueryDsl};
  use diesel_async::RunQueryDsl;
  use lemmy_db_schema_file::schema::{media_object, media_reference};
  use lemmy_diesel_utils::{
    connection::{build_db_pool_for_tests, get_conn},
    traits::Crud,
  };
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn identical_uploads_share_media_object() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = TestData::create(pool).await?;

    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let content_hash = format!("same-content-{unique}");
    let first_alias = format!("first_alias_{unique}");
    let form = MediaObjectInsertForm {
      content_hash: content_hash.clone(),
      canonical_pictrs_alias: first_alias.clone(),
      size_bytes: 42,
      width: 10,
      height: 20,
      content_type: "image/png".to_string(),
      blurhash: None,
    };
    let (first, created_first) = MediaObject::create_or_read(pool, &form).await?;
    assert!(created_first);

    let duplicate_form = MediaObjectInsertForm {
      canonical_pictrs_alias: format!("second_alias_{unique}"),
      ..form.clone()
    };
    let (second, created_second) = MediaObject::create_or_read(pool, &duplicate_form).await?;
    assert!(!created_second);
    assert_eq!(first.id, second.id);
    assert_eq!(second.canonical_pictrs_alias, first_alias);

    let second_domain = format!("second-media-{unique}.example");
    let second_instance = Instance::read_or_create(pool, &second_domain).await?;
    let second_site = Site::create(
      pool,
      &SiteUpsertForm::new("second media site".into(), second_instance.id),
    )
    .await?;
    let second_system = Person::create(
      pool,
      &PersonInsertForm {
        local: Some(false),
        ..PersonInsertForm::test_form(second_instance.id, "second_media_system")
      },
    )
    .await?;
    let second_local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(
        second_site.id,
        second_system.id,
        second_instance.domain.clone(),
      ),
    )
    .await?;

    for (local_site_id, owner) in [
      (data.local_site.id, data.person.id),
      (second_local_site.id, second_system.id),
    ] {
      MediaReference::ensure(
        pool,
        &MediaReferenceInsertForm {
          local_site_id,
          media_id: first.id,
          owner_person_id: Some(owner),
          post_id: None,
          purpose: "other".to_string(),
        },
      )
      .await?;
    }

    let other_owner = Person::create(
      pool,
      &PersonInsertForm::test_form(data.instance.id, "other_media_owner"),
    )
    .await?;
    MediaReference::ensure(
      pool,
      &MediaReferenceInsertForm {
        local_site_id: data.local_site.id,
        media_id: first.id,
        owner_person_id: Some(other_owner.id),
        post_id: None,
        purpose: "other".to_string(),
      },
    )
    .await?;

    let conn = &mut get_conn(pool).await?;
    let reference_count = media_reference::table
      .filter(media_reference::media_id.eq(first.id))
      .count()
      .get_result::<i64>(conn)
      .await?;
    let object_count = media_object::table
      .filter(media_object::content_hash.eq(&content_hash))
      .count()
      .get_result::<i64>(conn)
      .await?;
    assert_eq!(reference_count, 3);
    assert_eq!(object_count, 1);
    assert!(MediaReference::exists_for_site(pool, data.local_site.id, first.id).await?);
    assert!(MediaReference::exists_for_site(pool, second_local_site.id, first.id).await?);

    MediaReference::replace_for_content(pool, data.local_site.id, "post", "42", &[first.id])
      .await?;
    MediaReference::replace_for_content(pool, second_local_site.id, "post", "42", &[first.id])
      .await?;
    MediaReference::replace_for_content(pool, data.local_site.id, "post", "42", &[]).await?;
    #[derive(diesel::QueryableByName)]
    struct ReferenceCount {
      #[diesel(sql_type = diesel::sql_types::BigInt)]
      count: i64,
    }
    let conn = &mut get_conn(pool).await?;
    let content_count = diesel::sql_query(
      "SELECT count(*) AS count FROM media_content_reference WHERE media_id = $1",
    )
    .bind::<diesel::sql_types::BigInt, _>(first.id)
    .get_result::<ReferenceCount>(conn)
    .await?;
    assert_eq!(content_count.count, 1);
    MediaReference::replace_for_content(pool, second_local_site.id, "post", "42", &[]).await?;

    let conn = &mut get_conn(pool).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = false")
      .execute(conn)
      .await?;
    assert!(
      MediaReference::delete_for_site_and_user(
        pool,
        &first.canonical_pictrs_alias,
        data.local_site.id,
        data.person.id,
      )
      .await
      .is_err()
    );
    let conn = &mut get_conn(pool).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = true")
      .execute(conn)
      .await?;

    let remove_last = MediaReference::delete_for_site_and_user(
      pool,
      &first.canonical_pictrs_alias,
      data.local_site.id,
      data.person.id,
    )
    .await?;
    assert!(!remove_last);
    let conn = &mut get_conn(pool).await?;
    let remaining = media_reference::table
      .filter(media_reference::media_id.eq(first.id))
      .count()
      .get_result::<i64>(conn)
      .await?;
    assert_eq!(remaining, 2);
    assert!(MediaReference::exists_for_site(pool, data.local_site.id, first.id).await?);
    assert!(MediaReference::exists_for_site(pool, second_local_site.id, first.id).await?);

    let remove_last = MediaReference::delete_for_site_and_user(
      pool,
      &first.canonical_pictrs_alias,
      data.local_site.id,
      other_owner.id,
    )
    .await?;
    assert!(!remove_last);
    assert!(!MediaReference::exists_for_site(pool, data.local_site.id, first.id).await?);

    MediaReference::replace_for_content(pool, second_local_site.id, "post", "42", &[first.id])
      .await?;
    let remove_last = MediaReference::delete_for_site_and_user(
      pool,
      &first.canonical_pictrs_alias,
      second_local_site.id,
      second_system.id,
    )
    .await?;
    assert!(!remove_last);
    assert!(!MediaObject::deletion_queued(pool, first.id).await?);
    MediaReference::replace_for_content(pool, second_local_site.id, "post", "42", &[]).await?;
    assert!(MediaObject::deletion_queued(pool, first.id).await?);
    MediaObject::delete_queued_by_alias(pool, &first.canonical_pictrs_alias).await?;
    let conn = &mut get_conn(pool).await?;
    let remaining = media_object::table
      .filter(media_object::content_hash.eq(&content_hash))
      .count()
      .get_result::<i64>(conn)
      .await?;
    assert_eq!(remaining, 0);
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = false")
      .execute(conn)
      .await?;

    Person::delete(pool, other_owner.id).await?;
    Instance::delete(pool, second_instance.id).await?;
    data.delete(pool).await
  }

  #[tokio::test]
  #[serial]
  async fn post_media_references_follow_content_and_presence() -> LemmyResult<()> {
    use crate::{
      impls::presence::{
        ensure_comment_site, ensure_community_site, ensure_person_site, ensure_post_site,
        ensure_private_message_site,
      },
      source::{
        comment::{Comment, CommentInsertForm, CommentUpdateForm},
        community::{Community, CommunityInsertForm, CommunityUpdateForm},
        custom_emoji::{CustomEmoji, CustomEmojiInsertForm},
        local_user::{LocalUser, LocalUserInsertForm},
        person::{PersonActions, PersonNoteForm, PersonUpdateForm},
        post::{Post, PostInsertForm, PostUpdateForm},
        private_message::{PrivateMessage, PrivateMessageInsertForm, PrivateMessageUpdateForm},
        site::SiteUpdateForm,
        tagline::{Tagline, TaglineInsertForm},
      },
    };
    #[derive(diesel::QueryableByName)]
    struct ReferenceCount {
      #[diesel(sql_type = diesel::sql_types::BigInt)]
      count: i64,
    }
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = TestData::create(pool).await?;
    LocalUser::create(
      pool,
      &LocalUserInsertForm {
        local_site_id: data.local_site.id,
        ..LocalUserInsertForm::test_form(data.person.id)
      },
      vec![],
    )
    .await?;
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let other_domain = format!("post-media-{unique}.example");
    let other_instance = Instance::read_or_create(pool, &other_domain).await?;
    let other_site = Site::create(
      pool,
      &SiteUpsertForm::new("post media site".into(), other_instance.id),
    )
    .await?;
    let other_system = Person::create(
      pool,
      &PersonInsertForm::test_form(other_instance.id, "post_media_system"),
    )
    .await?;
    let other_local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(other_site.id, other_system.id, other_domain),
    )
    .await?;
    let media = MediaObject::create_or_read(
      pool,
      &MediaObjectInsertForm {
        content_hash: format!("post-media-{unique}"),
        canonical_pictrs_alias: format!("post-media-{unique}.png"),
        size_bytes: 1,
        width: 1,
        height: 1,
        content_type: "image/png".into(),
        blurhash: None,
      },
    )
    .await?
    .0;
    let orphan_alias = format!("orphan-media-{unique}.png");
    let orphan = MediaObject::create_or_read(
      pool,
      &MediaObjectInsertForm {
        content_hash: format!("orphan-media-{unique}"),
        canonical_pictrs_alias: orphan_alias.clone(),
        size_bytes: 1,
        width: 1,
        height: 1,
        content_type: "image/png".into(),
        blurhash: None,
      },
    )
    .await?
    .0;
    MediaReference::ensure(
      pool,
      &MediaReferenceInsertForm {
        local_site_id: data.local_site.id,
        media_id: media.id,
        owner_person_id: Some(data.person.id),
        post_id: None,
        purpose: "other".into(),
      },
    )
    .await?;
    let community = Community::create(
      pool,
      &CommunityInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        ..CommunityInsertForm::new(data.instance.id, "post_media".into(), "key".into())
      },
    )
    .await?;
    let site_ap_id = Url::parse(data.site.ap_id.as_str())?;
    let origin = format!("{}://{}", site_ap_id.scheme(), data.local_site.hostname);
    let post = Post::create(
      pool,
      &PostInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        body: Some(format!(
          "![image]({origin}/api/v4/image/{})",
          media.canonical_pictrs_alias
        )),
        ..PostInsertForm::new("media".into(), data.person.id, community.id)
      },
    )
    .await?;
    ensure_post_site(pool, data.local_site.id, post.id).await?;
    let comment = Comment::create(
      pool,
      &CommentInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        ..CommentInsertForm::new(
          data.person.id,
          post.id,
          community.id,
          format!(
            "![image]({origin}/api/v4/image/{})",
            media.canonical_pictrs_alias
          ),
        )
      },
      None,
    )
    .await?;
    ensure_post_site(pool, other_local_site.id, post.id).await?;
    ensure_comment_site(pool, other_local_site.id, comment.id).await?;
    ensure_person_site(pool, other_local_site.id, data.person.id).await?;
    ensure_community_site(pool, other_local_site.id, community.id).await?;
    let image_markdown = format!(
      "![image]({origin}/api/v4/image/{})",
      media.canonical_pictrs_alias
    );
    Person::update(
      pool,
      data.person.id,
      &PersonUpdateForm {
        bio: Some(Some(image_markdown.clone())),
        ..Default::default()
      },
    )
    .await?;
    Community::update(
      pool,
      community.id,
      &CommunityUpdateForm {
        sidebar: Some(Some(image_markdown.clone())),
        ..Default::default()
      },
    )
    .await?;
    let message = PrivateMessage::create(
      pool,
      &PrivateMessageInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        ..PrivateMessageInsertForm::new(data.person.id, other_system.id, image_markdown.clone())
      },
    )
    .await?;
    ensure_private_message_site(pool, other_local_site.id, message.id, "recipient").await?;
    ensure_private_message_site(pool, other_local_site.id, message.id, "sender").await?;
    let conn = &mut get_conn(pool).await?;
    diesel::sql_query(
      "DELETE FROM private_message_site WHERE local_site_id = $1 AND private_message_id = $2 AND role = 'sender'",
    )
    .bind::<diesel::sql_types::Integer, _>(other_local_site.id.0)
    .bind::<diesel::sql_types::Integer, _>(message.id.0)
    .execute(conn)
    .await?;
    diesel::sql_query("DELETE FROM post_site WHERE local_site_id = $1 AND post_id = $2")
      .bind::<diesel::sql_types::Integer, _>(other_local_site.id.0)
      .bind::<diesel::sql_types::Integer, _>(post.id.0)
      .execute(conn)
      .await?;
    let count = diesel::sql_query(
      "SELECT count(*) AS count FROM media_content_reference WHERE media_id = $1 AND content_kind = 'post'",
    )
    .bind::<diesel::sql_types::BigInt, _>(media.id)
    .get_result::<ReferenceCount>(conn)
    .await?;
    assert_eq!(count.count, 1);
    ensure_post_site(pool, other_local_site.id, post.id).await?;
    Site::update(
      pool,
      data.site.id,
      &SiteUpdateForm {
        sidebar: Some(Some(image_markdown.clone())),
        ..Default::default()
      },
    )
    .await?;
    let tagline = Tagline::create(
      pool,
      &TaglineInsertForm {
        local_site_id: data.local_site.id,
        content: image_markdown.clone(),
      },
    )
    .await?;
    PersonActions::note(
      pool,
      &PersonNoteForm::new(data.person.id, other_system.id, image_markdown),
    )
    .await?;
    let emoji = CustomEmoji::create(
      pool,
      &CustomEmojiInsertForm {
        local_site_id: data.local_site.id,
        shortcode: "media_test".into(),
        image_url: Url::parse(&format!(
          "{origin}/api/v4/image/{}",
          media.canonical_pictrs_alias
        ))?
        .into(),
        alt_text: "image".into(),
        category: "test".into(),
      },
    )
    .await?;
    let conn = &mut get_conn(pool).await?;
    let count = diesel::sql_query(
      "SELECT count(*) AS count FROM media_content_reference WHERE media_id = $1",
    )
    .bind::<diesel::sql_types::BigInt, _>(media.id)
    .get_result::<ReferenceCount>(conn)
    .await?;
    assert_eq!(count.count, 14);
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = false")
      .execute(conn)
      .await?;
    let alternate_origin = if origin.starts_with("https://") {
      origin.replacen("https://", "http://", 1)
    } else {
      origin.replacen("http://", "https://", 1)
    };
    let ambiguous = format!(
      "![image]({alternate_origin}/api/v4/image/{})",
      media.canonical_pictrs_alias
    );
    diesel::sql_query("UPDATE post SET body = $1 WHERE id = $2")
      .bind::<diesel::sql_types::Text, _>(&ambiguous)
      .bind::<diesel::sql_types::Integer, _>(post.id.0)
      .execute(conn)
      .await?;
    assert!(
      crate::impls::media_backfill::backfill_media_content(pool)
        .await
        .is_err()
    );
    let conn = &mut get_conn(pool).await?;
    // Exercise historical direct video URLs independently of Markdown content.
    let original = format!("{origin}/api/v4/image/{}", media.canonical_pictrs_alias);
    diesel::sql_query("UPDATE post SET body = NULL, embed_video_url = $1 WHERE id = $2")
      .bind::<diesel::sql_types::Text, _>(&original)
      .bind::<diesel::sql_types::Integer, _>(post.id.0)
      .execute(conn)
      .await?;
    diesel::sql_query("DELETE FROM media_content_reference WHERE media_id = $1")
      .bind::<diesel::sql_types::BigInt, _>(media.id)
      .execute(conn)
      .await?;
    assert!(crate::impls::media_backfill::backfill_media_content(pool).await?);
    assert!(!crate::impls::media_backfill::backfill_media_content(pool).await?);
    assert!(MediaObject::deletion_queued(pool, orphan.id).await?);
    let conn = &mut get_conn(pool).await?;
    let restored = diesel::sql_query(
      "SELECT count(*) AS count FROM media_content_reference WHERE media_id = $1",
    )
    .bind::<diesel::sql_types::BigInt, _>(media.id)
    .get_result::<ReferenceCount>(conn)
    .await?;
    assert_eq!(restored.count, 14);
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = true")
      .execute(conn)
      .await?;
    assert!(
      !MediaReference::delete_for_site_and_user(
        pool,
        &media.canonical_pictrs_alias,
        data.local_site.id,
        data.person.id,
      )
      .await?
    );
    assert!(MediaReference::exists_for_site(pool, data.local_site.id, media.id).await?);
    assert!(!MediaReference::exists_for_site(pool, other_local_site.id, media.id).await?);
    assert!(
      MediaReference::replace_for_content(
        pool,
        other_local_site.id,
        "post",
        "guessed",
        &[media.id]
      )
      .await
      .is_err()
    );
    Tagline::delete_for_site(pool, tagline.id, data.local_site.id).await?;
    CustomEmoji::delete_for_site(pool, emoji.id, data.local_site.id).await?;
    PersonActions::delete_note(pool, data.person.id, other_system.id).await?;
    Site::update(
      pool,
      data.site.id,
      &SiteUpdateForm {
        sidebar: Some(None),
        ..Default::default()
      },
    )
    .await?;
    PrivateMessage::update(
      pool,
      message.id,
      &PrivateMessageUpdateForm {
        content: Some("removed image".into()),
        ..Default::default()
      },
    )
    .await?;
    Person::update(
      pool,
      data.person.id,
      &PersonUpdateForm {
        bio: Some(None),
        ..Default::default()
      },
    )
    .await?;
    Community::update(
      pool,
      community.id,
      &CommunityUpdateForm {
        sidebar: Some(None),
        ..Default::default()
      },
    )
    .await?;
    Post::update(
      pool,
      post.id,
      &PostUpdateForm {
        body: Some(None),
        embed_video_url: Some(None),
        ..Default::default()
      },
    )
    .await?;
    let conn = &mut get_conn(pool).await?;
    let count = diesel::sql_query(
      "SELECT count(*) AS count FROM media_content_reference WHERE media_id = $1",
    )
    .bind::<diesel::sql_types::BigInt, _>(media.id)
    .get_result::<ReferenceCount>(conn)
    .await?;
    assert_eq!(count.count, 2);
    Comment::update(
      pool,
      comment.id,
      &CommentUpdateForm {
        content: Some("removed image".into()),
        ..Default::default()
      },
    )
    .await?;
    let conn = &mut get_conn(pool).await?;
    let count = diesel::sql_query(
      "SELECT count(*) AS count FROM media_content_reference WHERE media_id = $1",
    )
    .bind::<diesel::sql_types::BigInt, _>(media.id)
    .get_result::<ReferenceCount>(conn)
    .await?;
    assert_eq!(count.count, 0);
    assert!(MediaObject::deletion_queued(pool, media.id).await?);
    let retry_form = MediaObjectInsertForm {
      content_hash: media.content_hash.clone(),
      canonical_pictrs_alias: format!("retry-{unique}.png"),
      size_bytes: 1,
      width: 1,
      height: 1,
      content_type: "image/png".into(),
      blurhash: None,
    };
    let pending = MediaReference::register_upload(
      pool,
      &retry_form,
      data.local_site.id,
      Some(data.person.id),
      None,
      "other",
    )
    .await?;
    assert!(
      matches!(pending, UploadRegistration::Retry(Some(ref alias)) if alias == &media.canonical_pictrs_alias)
    );
    MediaObject::delete_queued_by_alias(pool, &media.canonical_pictrs_alias).await?;
    let registered = MediaReference::register_upload(
      pool,
      &retry_form,
      data.local_site.id,
      Some(data.person.id),
      None,
      "other",
    )
    .await?;
    let UploadRegistration::Ready(replacement, true) = registered else {
      panic!("upload did not create a replacement after queued deletion");
    };
    assert_eq!(
      replacement.canonical_pictrs_alias,
      retry_form.canonical_pictrs_alias
    );
    assert!(
      MediaReference::delete_for_site_and_user(
        pool,
        &replacement.canonical_pictrs_alias,
        data.local_site.id,
        data.person.id
      )
      .await?
    );
    MediaObject::delete_queued_by_alias(pool, &replacement.canonical_pictrs_alias).await?;
    MediaObject::delete_queued_by_alias(pool, &orphan_alias).await?;
    let conn = &mut get_conn(pool).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = false")
      .execute(conn)
      .await?;
    Instance::delete(pool, other_instance.id).await?;
    data.delete(pool).await
  }

  #[tokio::test]
  #[serial]
  async fn site_scoped_deletion_preserves_other_tenants() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = TestData::create(pool).await?;
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let conn = &mut get_conn(pool).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = true")
      .execute(conn)
      .await?;

    let second_domain = format!("site-delete-{unique}.example");
    let second_instance = Instance::read_or_create(pool, &second_domain).await?;
    let second_site = Site::create(
      pool,
      &SiteUpsertForm::new("second delete site".into(), second_instance.id),
    )
    .await?;
    let second_system = Person::create(
      pool,
      &PersonInsertForm::test_form(second_instance.id, "site_delete_system"),
    )
    .await?;
    let second_local_site = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(second_site.id, second_system.id, second_domain),
    )
    .await?;
    let alias = format!("site-delete-{unique}.png");
    let media = MediaObject::create_or_read(
      pool,
      &MediaObjectInsertForm {
        content_hash: format!("site-delete-{unique}"),
        canonical_pictrs_alias: alias.clone(),
        size_bytes: 1,
        width: 1,
        height: 1,
        content_type: "image/png".into(),
        blurhash: None,
      },
    )
    .await?
    .0;

    MediaReference::ensure(
      pool,
      &MediaReferenceInsertForm {
        local_site_id: second_local_site.id,
        media_id: media.id,
        owner_person_id: Some(second_system.id),
        post_id: None,
        purpose: "other".into(),
      },
    )
    .await?;
    assert!(
      MediaReference::delete_for_site(pool, &alias, data.local_site.id, None)
        .await
        .is_err()
    );
    assert!(MediaReference::exists_for_site(pool, second_local_site.id, media.id).await?);
    assert!(!MediaObject::deletion_queued(pool, media.id).await?);

    MediaReference::ensure(
      pool,
      &MediaReferenceInsertForm {
        local_site_id: data.local_site.id,
        media_id: media.id,
        owner_person_id: Some(data.person.id),
        post_id: None,
        purpose: "other".into(),
      },
    )
    .await?;
    assert!(
      !MediaReference::delete_for_site(pool, &alias, data.local_site.id, Some(data.person.id),)
        .await?
    );
    assert!(!MediaReference::exists_for_site(pool, data.local_site.id, media.id).await?);
    assert!(MediaReference::exists_for_site(pool, second_local_site.id, media.id).await?);
    assert!(!MediaObject::deletion_queued(pool, media.id).await?);

    assert!(MediaReference::delete_for_site(pool, &alias, second_local_site.id, None).await?);
    assert!(MediaObject::deletion_queued(pool, media.id).await?);
    MediaObject::delete_queued_by_alias(pool, &alias).await?;
    let conn = &mut get_conn(pool).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = false")
      .execute(conn)
      .await?;
    Person::delete(pool, second_system.id).await?;
    Instance::delete(pool, second_instance.id).await?;
    data.delete(pool).await
  }

  #[tokio::test]
  #[serial]
  async fn concurrent_upload_and_last_delete_are_serialized() -> LemmyResult<()> {
    let connection_pool = build_db_pool_for_tests();
    let mut test_pool = (&connection_pool).into();
    let data = TestData::create(&mut test_pool).await?;
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let conn = &mut get_conn(&mut test_pool).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = true")
      .execute(conn)
      .await?;

    let second_domain = format!("concurrent-media-{unique}.example");
    let second_instance = Instance::read_or_create(&mut test_pool, &second_domain).await?;
    let second_site = Site::create(
      &mut test_pool,
      &SiteUpsertForm::new("concurrent site".into(), second_instance.id),
    )
    .await?;
    let second_system = Person::create(
      &mut test_pool,
      &PersonInsertForm::test_form(second_instance.id, "concurrent_media_system"),
    )
    .await?;
    let second_local_site = LocalSite::create(
      &mut test_pool,
      &LocalSiteInsertForm::new(second_site.id, second_system.id, second_domain),
    )
    .await?;
    let alias = format!("concurrent-delete-{unique}.png");
    let media = MediaObject::create_or_read(
      &mut test_pool,
      &MediaObjectInsertForm {
        content_hash: format!("concurrent-delete-{unique}"),
        canonical_pictrs_alias: alias.clone(),
        size_bytes: 1,
        width: 1,
        height: 1,
        content_type: "image/png".into(),
        blurhash: None,
      },
    )
    .await?
    .0;
    MediaReference::ensure(
      &mut test_pool,
      &MediaReferenceInsertForm {
        local_site_id: data.local_site.id,
        media_id: media.id,
        owner_person_id: Some(data.person.id),
        post_id: None,
        purpose: "other".into(),
      },
    )
    .await?;

    let delete_pool = connection_pool.clone();
    let upload_pool = connection_pool.clone();
    let delete_alias = alias.clone();
    let first_site = data.local_site.id;
    let upload_form = MediaObjectInsertForm {
      content_hash: format!("concurrent-delete-{unique}"),
      canonical_pictrs_alias: format!("concurrent-upload-{unique}.png"),
      size_bytes: 1,
      width: 1,
      height: 1,
      content_type: "image/png".into(),
      blurhash: None,
    };
    let second_site_id = second_local_site.id;
    let second_person_id = second_system.id;
    let media_id = media.id;
    let delete_task = tokio::spawn(async move {
      let mut pool = (&delete_pool).into();
      MediaReference::delete_for_site(&mut pool, &delete_alias, first_site, None).await
    });
    let upload_task = tokio::spawn(async move {
      let mut pool = (&upload_pool).into();
      MediaReference::register_upload(
        &mut pool,
        &upload_form,
        second_site_id,
        Some(second_person_id),
        None,
        "other",
      )
      .await
    });
    let (delete_result, upload_result) = tokio::join!(delete_task, upload_task);
    delete_result.expect("delete task panicked")?;
    match upload_result.expect("upload task panicked")? {
      UploadRegistration::Ready(ready, created) => {
        assert!(!created);
        assert_eq!(ready.id, media_id);
        assert!(!MediaObject::deletion_queued(&mut test_pool, media_id).await?);
        assert!(MediaReference::exists_for_site(&mut test_pool, second_site_id, media_id).await?);
      }
      UploadRegistration::Retry(Some(retry_alias)) => {
        assert_eq!(retry_alias, alias);
        assert!(MediaObject::deletion_queued(&mut test_pool, media_id).await?);
      }
      UploadRegistration::Retry(None) => panic!("upload retried without a queued alias"),
    }

    if MediaObject::deletion_queued(&mut test_pool, media_id).await? {
      MediaObject::delete_queued_by_alias(&mut test_pool, &alias).await?;
    } else {
      MediaReference::delete_for_site(&mut test_pool, &alias, second_local_site.id, None).await?;
      MediaObject::delete_queued_by_alias(&mut test_pool, &alias).await?;
    }
    let conn = &mut get_conn(&mut test_pool).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = false")
      .execute(conn)
      .await?;
    Person::delete(&mut test_pool, second_system.id).await?;
    Instance::delete(&mut test_pool, second_instance.id).await?;
    data.delete(&mut test_pool).await
  }

  #[tokio::test]
  #[serial]
  async fn concurrent_post_edits_leave_matching_media_references() -> LemmyResult<()> {
    use crate::source::{
      community::{Community, CommunityInsertForm},
      post::{Post, PostInsertForm, PostUpdateForm},
    };
    #[derive(diesel::QueryableByName)]
    struct ReferenceCount {
      #[diesel(sql_type = diesel::sql_types::BigInt)]
      count: i64,
    }

    let connection_pool = build_db_pool_for_tests();
    let mut test_pool = (&connection_pool).into();
    let data = TestData::create(&mut test_pool).await?;
    let unique = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let conn = &mut get_conn(&mut test_pool).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = true")
      .execute(conn)
      .await?;
    let community = Community::create(
      &mut test_pool,
      &CommunityInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        ..CommunityInsertForm::new(data.instance.id, "edit_media".into(), "key".into())
      },
    )
    .await?;
    let alias = format!("edit-media-{unique}.png");
    let media = MediaObject::create_or_read(
      &mut test_pool,
      &MediaObjectInsertForm {
        content_hash: format!("edit-media-{unique}"),
        canonical_pictrs_alias: alias.clone(),
        size_bytes: 1,
        width: 1,
        height: 1,
        content_type: "image/png".into(),
        blurhash: None,
      },
    )
    .await?
    .0;
    MediaReference::ensure(
      &mut test_pool,
      &MediaReferenceInsertForm {
        local_site_id: data.local_site.id,
        media_id: media.id,
        owner_person_id: Some(data.person.id),
        post_id: None,
        purpose: "other".into(),
      },
    )
    .await?;
    let site_ap_id = Url::parse(data.site.ap_id.as_str())?;
    let origin = format!("{}://{}", site_ap_id.scheme(), data.local_site.hostname);
    let image_body = format!("![image]({origin}/api/v4/image/{alias})");
    let post = Post::create(
      &mut test_pool,
      &PostInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        body: Some(image_body.clone()),
        ..PostInsertForm::new("concurrent edit".into(), data.person.id, community.id)
      },
    )
    .await?;

    let image_pool = connection_pool.clone();
    let text_pool = connection_pool.clone();
    let post_id = post.id;
    let image_update_body = image_body.clone();
    let image_update = tokio::spawn(async move {
      let mut pool = (&image_pool).into();
      Post::update(
        &mut pool,
        post_id,
        &PostUpdateForm {
          body: Some(Some(image_update_body)),
          ..Default::default()
        },
      )
      .await
    });
    let text_update = tokio::spawn(async move {
      let mut pool = (&text_pool).into();
      Post::update(
        &mut pool,
        post_id,
        &PostUpdateForm {
          body: Some(Some("no image".into())),
          ..Default::default()
        },
      )
      .await
    });
    image_update.await.expect("image update panicked")?;
    text_update.await.expect("text update panicked")?;

    let stored = Post::read(&mut test_pool, post.id).await?;
    let expected = usize::from(stored.body.as_deref() == Some(image_body.as_str()));
    let conn = &mut get_conn(&mut test_pool).await?;
    let count = diesel::sql_query(
      "SELECT count(*) AS count FROM media_content_reference WHERE media_id = $1 \
       AND content_kind = 'post' AND content_key = $2",
    )
    .bind::<diesel::sql_types::BigInt, _>(media.id)
    .bind::<diesel::sql_types::Text, _>(post.id.0.to_string())
    .get_result::<ReferenceCount>(conn)
    .await?;
    assert_eq!(count.count as usize, expected);

    MediaReference::delete_for_site(
      &mut test_pool,
      &alias,
      data.local_site.id,
      Some(data.person.id),
    )
    .await?;
    MediaObject::delete_queued_by_alias(&mut test_pool, &alias).await?;
    let conn = &mut get_conn(&mut test_pool).await?;
    diesel::sql_query("UPDATE media_reference_backfill_state SET completed = false")
      .execute(conn)
      .await?;
    Community::delete(&mut test_pool, community.id).await?;
    data.delete(&mut test_pool).await
  }
}
