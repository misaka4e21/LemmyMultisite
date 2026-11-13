use crate::source::oauth_provider::{
  AdminOAuthProvider,
  OAuthProviderInsertForm,
  OAuthProviderUpdateForm,
  PublicOAuthProvider,
};
use diesel::{ExpressionMethods, QueryDsl, dsl::insert_into};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::{
  newtypes::{LocalSiteId, OAuthProviderId},
  schema::oauth_provider,
};
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  traits::Crud,
};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

impl Crud for AdminOAuthProvider {
  type InsertForm = OAuthProviderInsertForm;
  type UpdateForm = OAuthProviderUpdateForm;
  type IdType = OAuthProviderId;

  async fn create(pool: &mut DbPool<'_>, form: &Self::InsertForm) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    insert_into(oauth_provider::table)
      .values(form)
      .get_result::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }

  async fn update(
    pool: &mut DbPool<'_>,
    oauth_provider_id: OAuthProviderId,
    form: &Self::UpdateForm,
  ) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    diesel::update(oauth_provider::table.find(oauth_provider_id))
      .set(form)
      .get_result::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }
}

impl AdminOAuthProvider {
  pub async fn get_all(pool: &mut DbPool<'_>, site_id: LocalSiteId) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    oauth_provider::table
      .filter(oauth_provider::local_site_id.eq(site_id))
      .order(oauth_provider::id)
      .select(oauth_provider::all_columns)
      .load::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  pub fn convert_providers_to_public(
    oauth_providers: Vec<AdminOAuthProvider>,
  ) -> Vec<PublicOAuthProvider> {
    oauth_providers
      .into_iter()
      .filter(|x| x.enabled)
      .map(|p| PublicOAuthProvider {
        id: p.id,
        display_name: p.display_name,
        authorization_endpoint: p.authorization_endpoint,
        client_id: p.client_id,
        scopes: p.scopes,
        use_pkce: p.use_pkce,
      })
      .collect()
  }

  pub async fn get_all_public(
    pool: &mut DbPool<'_>,
    site_id: LocalSiteId,
  ) -> LemmyResult<Vec<PublicOAuthProvider>> {
    AdminOAuthProvider::get_all(pool, site_id)
      .await
      .map(Self::convert_providers_to_public)
  }

  pub async fn read_for_site(
    pool: &mut DbPool<'_>,
    id: OAuthProviderId,
    site_id: LocalSiteId,
  ) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    oauth_provider::table
      .find(id)
      .filter(oauth_provider::local_site_id.eq(site_id))
      .first(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  pub async fn update_for_site(
    pool: &mut DbPool<'_>,
    id: OAuthProviderId,
    site_id: LocalSiteId,
    form: &OAuthProviderUpdateForm,
  ) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    diesel::update(
      oauth_provider::table
        .find(id)
        .filter(oauth_provider::local_site_id.eq(site_id)),
    )
    .set(form)
    .get_result(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn delete_for_site(
    pool: &mut DbPool<'_>,
    id: OAuthProviderId,
    site_id: LocalSiteId,
  ) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    let count = diesel::delete(
      oauth_provider::table
        .find(id)
        .filter(oauth_provider::local_site_id.eq(site_id)),
    )
    .execute(conn)
    .await
    .with_lemmy_type(LemmyErrorType::Deleted)?;
    if count == 0 {
      return Err(LemmyErrorType::NotFound.into());
    }
    Ok(count)
  }
}
