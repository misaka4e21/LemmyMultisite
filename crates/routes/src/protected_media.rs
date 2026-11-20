use actix_web::web::Data;
use diesel::sql_query;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use lemmy_api_utils::{context::LemmyContext, tenant::TenantContext, utils::is_mod_or_admin_opt};
use lemmy_db_schema::source::{post::Post, protected_media_reference::ProtectedMediaReference};
use lemmy_db_schema_file::newtypes::{CommunityId, LocalSiteId};
use lemmy_db_schema_file::schema::media_reference;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_post::PostView;
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  traits::Crud,
};
use lemmy_utils::error::LemmyResult;

#[derive(diesel::QueryableByName)]
struct BooleanResult {
  #[diesel(sql_type = diesel::sql_types::Bool)]
  value: bool,
}

pub async fn file_reference_has_access(
  context: &Data<LemmyContext>,
  tenant: &TenantContext,
  user: Option<&LocalUserView>,
  file_reference_id: i32,
  is_public: bool,
) -> LemmyResult<bool> {
  let mut pool = context.pool();
  let bindings = ProtectedMediaReference::list_for_file_reference(
    &mut pool,
    tenant.local_site().id,
    file_reference_id,
  )
  .await?;
  if bindings.is_empty() {
    return Ok(is_public);
  }
  for binding in bindings {
    if target_resource_is_readable(&mut pool, tenant, user, &binding).await? {
      return Ok(true);
    }
  }
  Ok(false)
}

pub async fn media_reference_has_access(
  context: &Data<LemmyContext>,
  tenant: &TenantContext,
  user: Option<&LocalUserView>,
  media_reference_id: i64,
) -> LemmyResult<bool> {
  let mut pool = context.pool();
  let bindings = ProtectedMediaReference::list_for_media_reference(
    &mut pool,
    tenant.local_site().id,
    media_reference_id,
  )
  .await?;
  if bindings.is_empty() {
    let conn = &mut get_conn(&mut pool).await?;
    let purpose = media_reference::table
      .filter(media_reference::id.eq(media_reference_id))
      .filter(media_reference::local_site_id.eq(tenant.local_site().id))
      .select(media_reference::purpose)
      .first::<String>(conn)
      .await?;
    return Ok(purpose != "protected");
  }
  for binding in bindings {
    if target_resource_is_readable(&mut pool, tenant, user, &binding).await? {
      return Ok(true);
    }
  }
  Ok(false)
}

pub async fn has_unprotected_media_reference(
  pool: &mut DbPool<'_>,
  local_site_id: LocalSiteId,
  media_id: i64,
) -> LemmyResult<bool> {
  let conn = &mut get_conn(pool).await?;
  Ok(
    sql_query(
      "SELECT EXISTS (SELECT 1 FROM media_reference mr \
       WHERE mr.local_site_id = $1 AND mr.media_id = $2 \
       AND mr.purpose <> 'protected' \
       AND NOT EXISTS (SELECT 1 FROM protected_media_reference pmr \
         WHERE pmr.media_reference_id = mr.id)) AS value",
    )
    .bind::<diesel::sql_types::Integer, _>(local_site_id.0)
    .bind::<diesel::sql_types::BigInt, _>(media_id)
    .get_result::<BooleanResult>(conn)
    .await?
    .value,
  )
}

async fn target_resource_is_readable(
  pool: &mut DbPool<'_>,
  tenant: &TenantContext,
  user: Option<&LocalUserView>,
  binding: &ProtectedMediaReference,
) -> LemmyResult<bool> {
  if let Some(private_message_id) = binding.private_message_id {
    let Some(user) = user else {
      return Ok(false);
    };
    let conn = &mut get_conn(pool).await?;
    return Ok(
      sql_query(
        "SELECT EXISTS (SELECT 1 FROM private_message pm \
         JOIN private_message_site pms ON pms.private_message_id = pm.id \
         WHERE pm.id = $1 AND pms.local_site_id = $2 AND NOT pms.removed AND NOT pms.ban_removed \
         AND ((pm.creator_id = $3 AND NOT pm.deleted) \
           OR (pm.recipient_id = $3 AND NOT pm.deleted_by_recipient))) AS value",
      )
      .bind::<diesel::sql_types::Integer, _>(private_message_id.0)
      .bind::<diesel::sql_types::Integer, _>(tenant.local_site().id.0)
      .bind::<diesel::sql_types::Integer, _>(user.person.id.0)
      .get_result::<BooleanResult>(conn)
      .await?
      .value,
    );
  }

  let Some(post_id) = binding.post_id else {
    return Ok(false);
  };
  let post = Post::read(pool, post_id).await;
  let Ok(post) = post else {
    return Ok(false);
  };
  let mut is_mod_or_admin = false;
  if let Some(user) = user {
    let community_id = CommunityId(post.community_id.0);
    is_mod_or_admin = is_mod_or_admin_opt(pool, Some(user), Some(community_id))
      .await
      .is_ok();
  }
  Ok(
    PostView::read(
      pool,
      post_id,
      user.map(|user| &user.local_user),
      tenant.site().instance_id,
      is_mod_or_admin,
    )
    .await
    .is_ok(),
  )
}
