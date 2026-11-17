use crate::source::{
  local_site::{LocalSite, LocalSiteInsertForm, LocalSiteUpdateForm},
  media::MediaReference,
};
use diesel::{QueryDsl, dsl::insert_into, sql_query, sql_types::Integer};
use diesel_async::{AsyncConnection, RunQueryDsl, pg::AsyncPgConnection};
use lemmy_db_schema_file::{
  newtypes::{LocalSiteId, SiteId},
  schema::{local_site, site},
};
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyError, LemmyErrorExt, LemmyErrorType, LemmyResult};

impl LocalSite {
  pub async fn lock_actor_name_changes(
    conn: &mut AsyncPgConnection,
    local_site_id: LocalSiteId,
  ) -> LemmyResult<()> {
    sql_query("SELECT pg_advisory_xact_lock(1987001, $1)")
      .bind::<Integer, _>(local_site_id.0)
      .execute(conn)
      .await?;
    Ok(())
  }

  pub async fn create(pool: &mut DbPool<'_>, form: &LocalSiteInsertForm) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    conn
      .transaction::<Self, LemmyError, _>(async |conn| {
        diesel::sql_query(
          "SELECT pg_advisory_xact_lock(hashtextextended('media-site-catalog', 0))",
        )
        .execute(conn)
        .await?;
        site::table
          .find(form.site_id)
          .for_update()
          .select(site::id)
          .first::<SiteId>(conn)
          .await?;
        let local = insert_into(local_site::table)
          .values(form)
          .get_result::<Self>(conn)
          .await
          .with_lemmy_type(LemmyErrorType::CouldntCreate)?;
        MediaReference::sync_owned_site_content(&mut conn.into(), local.site_id).await?;
        #[derive(diesel::QueryableByName)]
        struct RemoteSite {
          #[diesel(sql_type = diesel::sql_types::Integer)]
          id: i32,
        }
        let remote_sites = diesel::sql_query(
          "SELECT site.id FROM site LEFT JOIN local_site ON local_site.site_id = site.id \
         WHERE local_site.id IS NULL ORDER BY site.id FOR UPDATE OF site",
        )
        .load::<RemoteSite>(conn)
        .await?;
        for remote in remote_sites {
          MediaReference::sync_local_site_content(&mut conn.into(), local.id, SiteId(remote.id))
            .await?;
        }
        Ok(local)
      })
      .await
  }

  pub async fn update(
    pool: &mut DbPool<'_>,
    local_site_id: LocalSiteId,
    form: &LocalSiteUpdateForm,
  ) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    conn
      .transaction::<Self, LemmyError, _>(async |conn| {
        if form.federation_enabled == Some(true) {
          Self::lock_actor_name_changes(conn, local_site_id).await?;
          let was_enabled = local_site::table
            .find(local_site_id)
            .select(local_site::federation_enabled)
            .first::<bool>(conn)
            .await?;
          if !was_enabled {
            let usernames = crate::source::person::Person::list_names_owned_by_site(
              &mut conn.into(),
              local_site_id,
            )
            .await?;
            let community_names = crate::source::community::Community::list_names_owned_by_site(
              &mut conn.into(),
              local_site_id,
            )
            .await?;
            let incompatible_names = usernames
              .into_iter()
              .filter(|name| lemmy_utils::utils::validation::is_valid_actor_name(name).is_err())
              .map(|name| format!("username: {name}"))
              .chain(
                community_names
                  .into_iter()
                  .filter(|name| {
                    lemmy_utils::utils::validation::is_valid_actor_name(name).is_err()
                  })
                  .map(|name| format!("community: {name}")),
              )
              .collect::<Vec<_>>();
            if !incompatible_names.is_empty() {
              return Err(LemmyErrorType::Unknown(format!(
                "Cannot enable federation until these local names are changed: {}",
                incompatible_names.join(", ")
              ))
              .into());
            }
          }
        }
        diesel::update(local_site::table.find(local_site_id))
          .set(form)
          .get_result::<Self>(conn)
          .await
          .with_lemmy_type(LemmyErrorType::CouldntUpdate)
      })
      .await
  }

  pub async fn delete(pool: &mut DbPool<'_>) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    diesel::delete(local_site::table)
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }

  pub async fn list_hostnames(pool: &mut DbPool<'_>) -> LemmyResult<Vec<String>> {
    let conn = &mut get_conn(pool).await?;
    local_site::table
      .select(local_site::hostname)
      .load(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)
  }
}

#[cfg(test)]
mod tests {

  use super::*;
  use crate::{
    source::{
      comment::{Comment, CommentInsertForm},
      community::{Community, CommunityInsertForm, CommunityUpdateForm},
      person::{Person, PersonInsertForm, PersonUpdateForm},
      post::{Post, PostInsertForm},
      site::Site,
    },
    test_data::TestData,
  };
  use lemmy_diesel_utils::{
    connection::{DbPool, build_db_pool_for_tests},
    traits::Crud,
  };
  use lemmy_utils::error::LemmyResult;
  use pretty_assertions::assert_eq;
  use serial_test::serial;

  async fn read_local_site(pool: &mut DbPool<'_>) -> LemmyResult<LocalSite> {
    let conn = &mut get_conn(pool).await?;
    local_site::table
      .first(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  async fn prepare_site_with_community(
    pool: &mut DbPool<'_>,
  ) -> LemmyResult<(TestData, Person, Community)> {
    let data = TestData::create(pool).await?;

    let new_person = PersonInsertForm::test_form(data.instance.id, "thommy_site_agg");
    let inserted_person = Person::create(pool, &new_person).await?;

    let new_community = CommunityInsertForm::new(
      data.instance.id,
      "TIL_site_agg".into(),
      "pubkey".to_string(),
    );

    let inserted_community = Community::create(pool, &new_community).await?;

    Ok((data, inserted_person, inserted_community))
  }

  #[tokio::test]
  #[serial]
  async fn federation_cannot_be_enabled_with_incompatible_local_actor_names() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = TestData::create(pool).await?;
    LocalSite::update(
      pool,
      data.local_site.id,
      &LocalSiteUpdateForm {
        federation_enabled: Some(false),
        ..Default::default()
      },
    )
    .await?;

    let person = Person::create(
      pool,
      &PersonInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        ..PersonInsertForm::test_form(data.instance.id, "中文用户名")
      },
    )
    .await?;
    let community = Community::create(
      pool,
      &CommunityInsertForm {
        origin_local_site_id: Some(data.local_site.id),
        ..CommunityInsertForm::new(data.instance.id, "中文社区".into(), "pubkey".into())
      },
    )
    .await?;

    let enable = LocalSite::update(
      pool,
      data.local_site.id,
      &LocalSiteUpdateForm {
        federation_enabled: Some(true),
        ..Default::default()
      },
    )
    .await;
    let error = enable.expect_err("incompatible local names must block federation");
    let LemmyErrorType::Unknown(message) = error.error_type else {
      panic!("expected an actionable federation validation error");
    };
    assert!(message.contains("username: 中文用户名"));
    assert!(message.contains("community: 中文社区"));
    assert!(!read_local_site(pool).await?.federation_enabled);

    Person::update(
      pool,
      person.id,
      &PersonUpdateForm {
        name: Some("compatible_user".into()),
        ..Default::default()
      },
    )
    .await?;
    Community::update(
      pool,
      community.id,
      &CommunityUpdateForm {
        name: Some("compatible_community".into()),
        ..Default::default()
      },
    )
    .await?;
    LocalSite::update(
      pool,
      data.local_site.id,
      &LocalSiteUpdateForm {
        federation_enabled: Some(true),
        ..Default::default()
      },
    )
    .await?;
    assert!(read_local_site(pool).await?.federation_enabled);

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_aggregates() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();

    let (data, inserted_person, inserted_community) = prepare_site_with_community(pool).await?;

    let new_post = PostInsertForm::new(
      "A test post".into(),
      inserted_person.id,
      inserted_community.id,
    );

    // Insert two of those posts
    let inserted_post = Post::create(pool, &new_post).await?;
    let _inserted_post_again = Post::create(pool, &new_post).await?;

    let comment_form = CommentInsertForm::new(
      inserted_person.id,
      inserted_post.id,
      inserted_community.id,
      "A test comment".into(),
    );

    // Insert two of those comments
    let inserted_comment = Comment::create(pool, &comment_form, None).await?;

    let child_comment_form = CommentInsertForm::new(
      inserted_person.id,
      inserted_post.id,
      inserted_community.id,
      "A test comment".into(),
    );
    let _inserted_child_comment =
      Comment::create(pool, &child_comment_form, Some(&inserted_comment.path)).await?;

    let site_aggregates_before_delete = read_local_site(pool).await?;

    // TODO: this is unstable, sometimes it returns 0 users, sometimes 1
    //assert_eq!(0, site_aggregates_before_delete.users);
    assert_eq!(1, site_aggregates_before_delete.local_communities);
    assert_eq!(2, site_aggregates_before_delete.local_posts);
    assert_eq!(2, site_aggregates_before_delete.local_comments);

    // Try a post delete
    Post::delete(pool, inserted_post.id).await?;
    let site_aggregates_after_post_delete = read_local_site(pool).await?;
    assert_eq!(1, site_aggregates_after_post_delete.local_posts);
    assert_eq!(0, site_aggregates_after_post_delete.local_comments);

    // This should delete all the associated rows, and fire triggers
    let person_num_deleted = Person::delete(pool, inserted_person.id).await?;
    assert_eq!(1, person_num_deleted);

    // Delete the community
    let community_num_deleted = Community::delete(pool, inserted_community.id).await?;
    assert_eq!(1, community_num_deleted);

    // Site should still exist, it can without a site creator.
    let after_delete_creator = read_local_site(pool).await;
    assert!(after_delete_creator.is_ok());

    Site::delete(pool, data.site.id).await?;
    let after_delete_site = read_local_site(pool).await;
    assert!(after_delete_site.is_err());

    data.delete(pool).await?;

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_soft_delete() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();

    let (data, inserted_person, inserted_community) = prepare_site_with_community(pool).await?;

    let site_aggregates_before = read_local_site(pool).await?;
    assert_eq!(1, site_aggregates_before.local_communities);

    Community::update(
      pool,
      inserted_community.id,
      &CommunityUpdateForm {
        deleted: Some(true),
        ..Default::default()
      },
    )
    .await?;

    let site_aggregates_after_delete = read_local_site(pool).await?;
    assert_eq!(0, site_aggregates_after_delete.local_communities);

    Community::update(
      pool,
      inserted_community.id,
      &CommunityUpdateForm {
        deleted: Some(false),
        ..Default::default()
      },
    )
    .await?;

    Community::update(
      pool,
      inserted_community.id,
      &CommunityUpdateForm {
        removed: Some(true),
        ..Default::default()
      },
    )
    .await?;

    let site_aggregates_after_remove = read_local_site(pool).await?;
    assert_eq!(0, site_aggregates_after_remove.local_communities);

    Community::update(
      pool,
      inserted_community.id,
      &CommunityUpdateForm {
        deleted: Some(true),
        ..Default::default()
      },
    )
    .await?;

    let site_aggregates_after_remove_delete = read_local_site(pool).await?;
    assert_eq!(0, site_aggregates_after_remove_delete.local_communities);

    Community::delete(pool, inserted_community.id).await?;
    Person::delete(pool, inserted_person.id).await?;
    data.delete(pool).await?;

    Ok(())
  }
}
