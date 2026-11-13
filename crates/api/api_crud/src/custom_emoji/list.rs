use actix_web::web::{Data, Json, Query};
use lemmy_api_utils::{context::LemmyContext, tenant::TenantContext};
use lemmy_db_views_custom_emoji::{
  CustomEmojiView,
  api::{ListCustomEmojis, ListCustomEmojisResponse},
};
use lemmy_utils::error::LemmyError;

pub async fn list_custom_emojis(
  Query(data): Query<ListCustomEmojis>,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> Result<Json<ListCustomEmojisResponse>, LemmyError> {
  let custom_emojis =
    CustomEmojiView::list(&mut context.pool(), tenant.local_site().id, &data.category).await?;

  Ok(Json(ListCustomEmojisResponse { custom_emojis }))
}
