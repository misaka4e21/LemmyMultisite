use actix_web::web::{Data, Json, Query};
use lemmy_api_utils::{context::LemmyContext, utils::check_community_global_mod_action};
use lemmy_db_schema::source::{community::Community, post::Post};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_post::api::ListPostLikes;
use lemmy_db_views_vote::VoteView;
use lemmy_diesel_utils::{pagination::PagedResponse, traits::Crud};
use lemmy_utils::error::LemmyResult;

/// Lists likes for a post
pub async fn list_post_likes(
  Query(data): Query<ListPostLikes>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<PagedResponse<VoteView>>> {
  let post = Post::read(&mut context.pool(), data.post_id).await?;
  let community = Community::read(&mut context.pool(), post.community_id).await?;
  check_community_global_mod_action(&local_user_view, &community, false, &mut context.pool())
    .await?;

  let post_likes = VoteView::list_for_post(
    &mut context.pool(),
    data.post_id,
    data.page_cursor,
    data.limit,
    local_user_view.person.instance_id,
  )
  .await?;

  Ok(Json(post_likes))
}
