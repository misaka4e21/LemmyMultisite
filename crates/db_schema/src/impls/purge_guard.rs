use diesel::sql_types::{Bool, Integer, Text};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::newtypes::LocalSiteId;
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyErrorType, LemmyResult};

#[derive(Clone, Copy)]
pub enum PurgeTarget {
  Community(i32),
  Post(i32),
  Comment(i32),
}

#[derive(diesel::QueryableByName)]
struct Protected {
  #[diesel(sql_type = Bool)]
  value: bool,
}

/// Locks the deletion graph and rejects operations which would affect another site's objects or
/// state. The caller must keep this connection in the same transaction through the DELETE.
pub async fn lock_and_check_purge(
  pool: &mut DbPool<'_>,
  site_id: LocalSiteId,
  target: PurgeTarget,
) -> LemmyResult<()> {
  let (kind, id) = match target {
    PurgeTarget::Community(id) => ("community", id),
    PurgeTarget::Post(id) => ("post", id),
    PurgeTarget::Comment(id) => ("comment", id),
  };
  let conn = &mut get_conn(pool).await?;
  // Lock parents first, followed by descendants and reports. Foreign-key inserts into the
  // protected graph must wait until the decision and deletion have committed.
  for statement in [
    "SELECT id FROM community WHERE id IN (SELECT id FROM community WHERE $2 = 'community' AND id = $1 UNION SELECT community_id FROM post WHERE $2 = 'post' AND id = $1 UNION SELECT community_id FROM comment WHERE $2 = 'comment' AND id = $1) ORDER BY id FOR UPDATE",
    "SELECT id FROM community WHERE id IN (SELECT CASE WHEN community_id = $1 THEN target_id ELSE community_id END FROM community_community_follow WHERE $2 = 'community' AND (community_id = $1 OR target_id = $1) UNION SELECT ct.community_id FROM post_community_tag pct JOIN community_tag ct ON ct.id = pct.community_tag_id WHERE pct.post_id IN (SELECT id FROM post WHERE ($2 = 'community' AND community_id = $1) OR ($2 = 'post' AND id = $1))) ORDER BY id FOR UPDATE",
    "SELECT id FROM multi_community WHERE id IN (SELECT multi_community_id FROM multi_community_entry WHERE $2 = 'community' AND community_id = $1) ORDER BY id FOR UPDATE",
    "SELECT id FROM post WHERE ($2 = 'community' AND community_id = $1) OR ($2 = 'post' AND id = $1) OR ($2 = 'comment' AND id = (SELECT post_id FROM comment WHERE id = $1)) ORDER BY id FOR UPDATE",
    "SELECT id FROM comment WHERE ($2 = 'community' AND community_id = $1) OR ($2 = 'post' AND post_id = $1) OR ($2 = 'comment' AND path <@ (SELECT path FROM comment WHERE id = $1)) ORDER BY id FOR UPDATE",
    "SELECT id FROM report_combined WHERE ($2 = 'community' AND community_id = $1) OR ($2 = 'post' AND post_id = $1) OR ($2 = 'comment' AND comment_id IN (SELECT id FROM comment WHERE path <@ (SELECT path FROM comment WHERE id = $1))) ORDER BY id FOR UPDATE",
    "SELECT id FROM modlog WHERE id IN (WITH RECURSIVE logs AS (SELECT id FROM modlog WHERE ($2 = 'community' AND target_community_id = $1) OR target_post_id IN (SELECT id FROM post WHERE ($2 = 'community' AND community_id = $1) OR ($2 = 'post' AND id = $1)) OR target_comment_id IN (SELECT id FROM comment WHERE ($2 = 'community' AND community_id = $1) OR ($2 = 'post' AND post_id = $1) OR ($2 = 'comment' AND path <@ (SELECT path FROM comment WHERE id = $1))) UNION SELECT child.id FROM modlog child JOIN logs parent ON child.bulk_action_parent_id = parent.id) SELECT id FROM logs) ORDER BY id FOR UPDATE",
  ] {
    diesel::sql_query(statement)
      .bind::<Integer, _>(id)
      .bind::<Text, _>(kind)
      .execute(conn)
      .await?;
  }

  let protected = diesel::sql_query(
    "WITH RECURSIVE target_communities AS ( \
       SELECT id FROM community WHERE ($3 = 'community' AND id = $1) \
       OR id IN (SELECT community_id FROM post WHERE $3 = 'post' AND id = $1) \
       OR id IN (SELECT community_id FROM comment WHERE $3 = 'comment' AND id = $1)), \
     target_posts AS (SELECT id FROM post WHERE ($3 = 'community' AND community_id = $1) OR ($3 = 'post' AND id = $1)), \
     parent_posts AS (SELECT id FROM target_posts UNION SELECT post_id FROM comment WHERE $3 = 'comment' AND id = $1), \
     target_comments AS (SELECT id FROM comment WHERE ($3 = 'community' AND community_id = $1) OR ($3 = 'post' AND post_id = $1) OR ($3 = 'comment' AND path <@ (SELECT path FROM comment WHERE id = $1))), \
     target_reports AS (SELECT id, report_creator_id FROM report_combined WHERE ($3 = 'community' AND community_id = $1) OR post_id IN (SELECT id FROM target_posts) OR comment_id IN (SELECT id FROM target_comments)), \
     target_logs AS (SELECT id, mod_id FROM modlog WHERE ($3 = 'community' AND target_community_id = $1) OR target_post_id IN (SELECT id FROM target_posts) OR target_comment_id IN (SELECT id FROM target_comments) \
       UNION SELECT m.id, m.mod_id FROM modlog m JOIN target_logs parent ON m.bulk_action_parent_id = parent.id) \
     SELECT ( \
       EXISTS (SELECT 1 FROM community c WHERE c.id IN (SELECT id FROM target_communities) AND c.origin_local_site_id IS DISTINCT FROM $2) \
       OR EXISTS (SELECT 1 FROM community_site cs WHERE cs.community_id IN (SELECT id FROM target_communities) AND cs.local_site_id <> $2) \
       OR EXISTS (SELECT 1 FROM post p WHERE p.id IN (SELECT id FROM parent_posts) AND p.origin_local_site_id IS DISTINCT FROM $2) \
       OR EXISTS (SELECT 1 FROM post p JOIN person author ON author.id = p.creator_id WHERE p.id IN (SELECT id FROM target_posts) AND author.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = author.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM post_site ps WHERE ps.post_id IN (SELECT id FROM parent_posts) AND ps.local_site_id <> $2) \
       OR EXISTS (SELECT 1 FROM comment c WHERE c.id IN (SELECT id FROM target_comments) AND c.origin_local_site_id IS DISTINCT FROM $2) \
       OR EXISTS (SELECT 1 FROM comment c JOIN person author ON author.id = c.creator_id WHERE c.id IN (SELECT id FROM target_comments) AND author.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = author.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM comment_site cs WHERE cs.comment_id IN (SELECT id FROM target_comments) AND cs.local_site_id <> $2) \
       OR EXISTS (SELECT 1 FROM post_actions a JOIN person p ON p.id = a.person_id WHERE a.post_id IN (SELECT id FROM target_posts) AND p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM comment_actions a JOIN person p ON p.id = a.person_id WHERE a.comment_id IN (SELECT id FROM target_comments) AND p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM person_saved_combined a JOIN person p ON p.id = a.person_id WHERE (a.post_id IN (SELECT id FROM target_posts) OR a.comment_id IN (SELECT id FROM target_comments)) AND p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM person_liked_combined a JOIN person p ON p.id = a.person_id WHERE (a.post_id IN (SELECT id FROM target_posts) OR a.comment_id IN (SELECT id FROM target_comments)) AND p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM community_actions a JOIN person p ON p.id = a.person_id WHERE $3 = 'community' AND a.community_id = $1 AND p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM community_actions a JOIN person p ON p.id = a.follow_approver_id WHERE $3 = 'community' AND a.community_id = $1 AND p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM report_site rs WHERE rs.report_combined_id IN (SELECT id FROM target_reports) AND rs.local_site_id <> $2) \
       OR EXISTS (SELECT 1 FROM report_site rs JOIN person p ON p.id = rs.resolver_id WHERE rs.report_combined_id IN (SELECT id FROM target_reports) AND p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM target_reports r JOIN person p ON p.id = r.report_creator_id WHERE p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM report_combined r JOIN person p ON p.id = r.resolver_id WHERE r.id IN (SELECT id FROM target_reports) AND p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM target_logs l JOIN person p ON p.id = l.mod_id WHERE p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM notification n JOIN person p ON p.id = n.recipient_id WHERE (($3 = 'community' AND n.community_id = $1) OR n.post_id IN (SELECT id FROM target_posts) OR n.comment_id IN (SELECT id FROM target_comments) OR n.modlog_id IN (SELECT id FROM target_logs)) AND p.origin_local_site_id IS DISTINCT FROM $2 AND NOT EXISTS (SELECT 1 FROM local_user lu WHERE lu.person_id = p.id AND lu.local_site_id = $2)) \
       OR EXISTS (SELECT 1 FROM media_reference r WHERE r.post_id IN (SELECT id FROM target_posts) AND r.local_site_id <> $2) \
       OR EXISTS (SELECT 1 FROM media_content_reference r WHERE r.local_site_id <> $2 AND ((r.content_kind = 'community' AND $3 = 'community' AND r.content_key = $1::text) OR (r.content_kind = 'post' AND r.content_key IN (SELECT id::text FROM target_posts)) OR (r.content_kind = 'comment' AND r.content_key IN (SELECT id::text FROM target_comments)) OR (r.content_kind = 'report' AND r.content_key IN (SELECT id::text FROM target_reports)))) \
       OR EXISTS (SELECT 1 FROM multi_community_entry e JOIN multi_community m ON m.id = e.multi_community_id WHERE $3 = 'community' AND e.community_id = $1 AND m.origin_local_site_id IS DISTINCT FROM $2) \
       OR EXISTS (SELECT 1 FROM multi_community_entry e JOIN multi_community_site ms ON ms.multi_community_id = e.multi_community_id WHERE $3 = 'community' AND e.community_id = $1 AND ms.local_site_id <> $2) \
       OR EXISTS (SELECT 1 FROM community_community_follow f JOIN community c ON c.id = CASE WHEN f.community_id = $1 THEN f.target_id ELSE f.community_id END WHERE $3 = 'community' AND (f.community_id = $1 OR f.target_id = $1) AND c.origin_local_site_id IS DISTINCT FROM $2) \
       OR EXISTS (SELECT 1 FROM community_community_follow f JOIN community_site cs ON cs.community_id = CASE WHEN f.community_id = $1 THEN f.target_id ELSE f.community_id END WHERE $3 = 'community' AND (f.community_id = $1 OR f.target_id = $1) AND cs.local_site_id <> $2) \
       OR EXISTS (SELECT 1 FROM post_community_tag pct JOIN community_tag ct ON ct.id = pct.community_tag_id JOIN community c ON c.id = ct.community_id WHERE pct.post_id IN (SELECT id FROM target_posts) AND c.origin_local_site_id IS DISTINCT FROM $2) \
       OR EXISTS (SELECT 1 FROM post_community_tag pct JOIN community_tag ct ON ct.id = pct.community_tag_id JOIN community_site cs ON cs.community_id = ct.community_id WHERE pct.post_id IN (SELECT id FROM target_posts) AND cs.local_site_id <> $2) \
       OR EXISTS (SELECT 1 FROM post_community_tag pct JOIN community_tag ct ON ct.id = pct.community_tag_id WHERE $3 = 'community' AND ct.community_id = $1 AND pct.post_id NOT IN (SELECT id FROM target_posts)) \
     ) AS value",
  )
  .bind::<Integer, _>(id)
  .bind::<Integer, _>(site_id.0)
  .bind::<Text, _>(kind)
  .get_result::<Protected>(conn)
  .await?;
  if protected.value {
    return Err(LemmyErrorType::CouldntUpdate.into());
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use crate::{
    impls::presence::{
      SiteReportId, combined_report_id, delete_comment_for_site, delete_community_for_site,
      delete_post_for_site, ensure_comment_site, ensure_report_site,
    },
    source::{
      comment::{Comment, CommentInsertForm},
      community::{Community, CommunityInsertForm},
      instance::Instance,
      local_site::{LocalSite, LocalSiteInsertForm},
      media::{MediaObject, MediaObjectInsertForm, MediaReference, MediaReferenceInsertForm},
      person::{Person, PersonInsertForm},
      post::{Post, PostActions, PostInsertForm, PostSavedForm},
      post_report::{PostReport, PostReportForm},
      site::{Site, SiteUpsertForm},
    },
    test_data::TestData,
    traits::{Reportable, Saveable},
  };
  use diesel::QueryDsl;
  use diesel_async::{AsyncConnection, RunQueryDsl};
  use lemmy_db_schema_file::schema::{post as post_table, post_report};
  use lemmy_diesel_utils::{
    connection::{build_db_pool_for_tests, get_conn},
    traits::Crud,
  };
  use lemmy_utils::error::{LemmyError, LemmyResult};
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn purge_rejects_foreign_descendants_presence_and_actions() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let first = TestData::create(pool).await?;
    // The generic fixture predates tenant ownership; make its author explicitly local.
    diesel::sql_query("UPDATE person SET origin_local_site_id = $1 WHERE id = $2")
      .bind::<diesel::sql_types::Integer, _>(first.local_site.id.0)
      .bind::<diesel::sql_types::Integer, _>(first.person.id.0)
      .execute(&mut get_conn(pool).await?)
      .await?;
    let second_instance = Instance::read_or_create(pool, "purge-guard.example").await?;
    let second_site = Site::create(
      pool,
      &SiteUpsertForm::new("second".into(), second_instance.id),
    )
    .await?;
    let system = Person::create(
      pool,
      &PersonInsertForm::test_form(second_instance.id, "purge_guard_system"),
    )
    .await?;
    let second = LocalSite::create(
      pool,
      &LocalSiteInsertForm::new(second_site.id, system.id, "purge-guard.example".into()),
    )
    .await?;
    let foreign = Person::create(
      pool,
      &PersonInsertForm {
        origin_local_site_id: Some(second.id),
        ..PersonInsertForm::test_form(second_instance.id, "purge_guard_foreign")
      },
    )
    .await?;
    let community_name = format!("purgeguard{}", first.local_site.id.0);
    let community = Community::create(
      pool,
      &CommunityInsertForm {
        origin_local_site_id: Some(first.local_site.id),
        ..CommunityInsertForm::new(
          first.instance.id,
          community_name,
          "key".into(),
        )
      },
    )
    .await?;
    let post = Post::create(
      pool,
      &PostInsertForm {
        origin_local_site_id: Some(first.local_site.id),
        ..PostInsertForm::new("purge guard".into(), first.person.id, community.id)
      },
    )
    .await?;
    let foreign_comment = Comment::create(
      pool,
      &CommentInsertForm {
        origin_local_site_id: Some(second.id),
        ..CommentInsertForm::new(foreign.id, post.id, community.id, "foreign".into())
      },
      None,
    )
    .await?;
    assert!(
      delete_post_for_site(pool, first.local_site.id, post.id)
        .await
        .is_err()
    );
    assert!(
      delete_community_for_site(pool, first.local_site.id, community.id)
        .await
        .is_err()
    );
    Comment::read(pool, foreign_comment.id).await?;
    Comment::delete(pool, foreign_comment.id).await?;

    let own_comment = Comment::create(
      pool,
      &CommentInsertForm {
        origin_local_site_id: Some(first.local_site.id),
        ..CommentInsertForm::new(first.person.id, post.id, community.id, "own".into())
      },
      None,
    )
    .await?;
    ensure_comment_site(pool, second.id, own_comment.id).await?;
    assert!(
      delete_comment_for_site(pool, first.local_site.id, own_comment.id)
        .await
        .is_err()
    );
    let conn = &mut get_conn(pool).await?;
    diesel::sql_query("DELETE FROM comment_site WHERE local_site_id = $1 AND comment_id = $2")
      .bind::<diesel::sql_types::Integer, _>(second.id.0)
      .bind::<diesel::sql_types::Integer, _>(own_comment.id.0)
      .execute(conn)
      .await?;
    delete_comment_for_site(pool, first.local_site.id, own_comment.id).await?;

    let saved = PostSavedForm::new(post.id, foreign.id);
    PostActions::save(pool, &saved).await?;
    assert!(
      delete_post_for_site(pool, first.local_site.id, post.id)
        .await
        .is_err()
    );
    PostActions::unsave(pool, &saved).await?;
    let media = MediaObject::create_or_read(
      pool,
      &MediaObjectInsertForm {
        content_hash: "purge-guard-media".into(),
        canonical_pictrs_alias: "purge-guard-media.png".into(),
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
        local_site_id: second.id,
        media_id: media.id,
        owner_person_id: Some(foreign.id),
        post_id: Some(post.id),
        purpose: "other".into(),
      },
    )
    .await?;
    assert!(
      delete_post_for_site(pool, first.local_site.id, post.id)
        .await
        .is_err()
    );
    assert!(MediaReference::exists_for_site(pool, second.id, media.id).await?);
    MediaObject::delete_by_alias(pool, &media.canonical_pictrs_alias).await?;
    let report = PostReport::report(
      pool,
      &PostReportForm {
        creator_id: foreign.id,
        post_id: post.id,
        original_post_name: post.name.clone(),
        original_post_url: None,
        original_post_body: None,
        reason: "foreign report".into(),
        violates_instance_rules: true,
      },
    )
    .await?;
    let report_id = combined_report_id(pool, SiteReportId::Post(report.id)).await?;
    ensure_report_site(pool, second.id, report_id).await?;
    assert!(
      delete_post_for_site(pool, first.local_site.id, post.id)
        .await
        .is_err()
    );
    let conn = &mut get_conn(pool).await?;
    diesel::delete(post_report::table.find(report.id))
      .execute(conn)
      .await?;
    let connection_pool = build_db_pool_for_tests();
    let insert_pool = connection_pool.clone();
    let (locked_tx, locked_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let owner_site = first.local_site.id;
    let post_id = post.id;
    let purge = tokio::spawn(async move {
      let mut borrowed_pool = (&connection_pool).into();
      let conn = &mut get_conn(&mut borrowed_pool).await?;
      conn
        .transaction::<(), LemmyError, _>(async |conn| {
          super::lock_and_check_purge(
            &mut conn.into(),
            owner_site,
            super::PurgeTarget::Post(post_id.0),
          )
          .await?;
          let _ = locked_tx.send(());
          let _ = release_rx.await;
          diesel::delete(post_table::table.find(post_id))
            .execute(conn)
            .await?;
          Ok(())
        })
        .await
    });
    locked_rx.await.expect("purge did not lock the target");
    let foreign_id = foreign.id;
    let other_site_id = second.id;
    let community_id = community.id;
    let mut insertion = tokio::spawn(async move {
      let mut borrowed_pool = (&insert_pool).into();
      Comment::create(
        &mut borrowed_pool,
        &CommentInsertForm {
          origin_local_site_id: Some(other_site_id),
          ..CommentInsertForm::new(foreign_id, post_id, community_id, "concurrent".into())
        },
        None,
      )
      .await
    });
    assert!(
      tokio::time::timeout(std::time::Duration::from_millis(100), &mut insertion)
        .await
        .is_err()
    );
    let _ = release_tx.send(());
    purge.await.expect("purge task panicked")?;
    assert!(insertion.await.expect("insertion task panicked").is_err());
    delete_community_for_site(pool, first.local_site.id, community.id).await?;
    Instance::delete(pool, second_instance.id).await?;
    first.delete(pool).await
  }
}
