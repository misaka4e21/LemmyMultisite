use activitypub_federation::config::Data;
use actix_web::web::Json;
use lemmy_api_utils::{
  build_response::build_comment_response,
  context::LemmyContext,
  notify::notify_mod_action,
  send_activity::{ActivityChannel, SendActivityData},
  utils::check_community_mod_action,
};
use lemmy_db_schema::{
  impls::presence::{SiteReportObject, resolve_reports_for_site, set_comment_site_removed},
  source::{
    comment::{Comment, CommentUpdateForm},
    local_user::LocalUser,
    modlog::{Modlog, ModlogInsertForm},
  },
};
use lemmy_db_views_comment::{
  CommentView,
  api::{CommentResponse, RemoveComment},
};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_diesel_utils::traits::Crud;
use lemmy_utils::error::{LemmyErrorType, LemmyResult};

pub async fn remove_comment(
  Json(data): Json<RemoveComment>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<CommentResponse>> {
  let comment_id = data.comment_id;
  let local_instance_id = local_user_view.person.instance_id;
  let active_site_id = local_user_view.local_user.local_site_id;
  let orig_comment = CommentView::read(
    &mut context.pool(),
    comment_id,
    Some(&local_user_view.local_user),
    local_instance_id,
  )
  .await?;

  check_community_mod_action(
    &local_user_view,
    &orig_comment.community,
    false,
    &mut context.pool(),
  )
  .await?;

  LocalUser::is_higher_mod_or_admin_check(
    &mut context.pool(),
    orig_comment.community.id,
    local_user_view.person.id,
    vec![orig_comment.creator.id],
  )
  .await?;

  // Don't allow removing or restoring comment which was deleted by user, as it would reveal
  // the comment text in mod log.
  if orig_comment.comment.deleted {
    return Err(LemmyErrorType::CouldntUpdate.into());
  }
  let is_origin = orig_comment.comment.origin_local_site_id == Some(active_site_id);

  let (updated_comment, forms) = if let Some(remove_children) = data.remove_children {
    let updated_comments: Vec<Comment> = Comment::update_removed_for_comment_and_children_split(
      &mut context.pool(),
      &orig_comment.comment.path,
      active_site_id,
      remove_children,
    )
    .await?;

    let updated_comment = updated_comments
      .iter()
      .find(|c| c.id == comment_id)
      .ok_or(LemmyErrorType::CouldntUpdate)?
      .clone();

    let forms: Vec<_> = updated_comments
      .iter()
      // Filter out deleted comments here so their content doesn't show up in the modlog.
      .filter(|c| !c.deleted)
      .map(|comment| {
        ModlogInsertForm::mod_remove_comment(
          local_user_view.person.id,
          comment,
          orig_comment.community.id,
          remove_children,
          &data.reason,
          None,
        )
      })
      .collect();

    for comment in &updated_comments {
      resolve_reports_for_site(
        &mut context.pool(),
        active_site_id,
        SiteReportObject::Comment(comment.id),
        None,
        local_user_view.person.id,
      )
      .await?;
    }

    (updated_comment, forms)
  } else {
    // Do the remove
    let removed = data.removed;
    let updated_comment = if is_origin {
      Comment::update(
        &mut context.pool(),
        comment_id,
        &CommentUpdateForm {
          removed: Some(removed),
          ..Default::default()
        },
      )
      .await?
    } else {
      set_comment_site_removed(&mut context.pool(), active_site_id, comment_id, removed).await?;
      orig_comment.comment.clone()
    };

    resolve_reports_for_site(
      &mut context.pool(),
      active_site_id,
      SiteReportObject::Comment(comment_id),
      None,
      local_user_view.person.id,
    )
    .await?;

    // Mod tables
    let form = ModlogInsertForm::mod_remove_comment(
      local_user_view.person.id,
      &orig_comment.comment,
      orig_comment.community.id,
      removed,
      &data.reason,
      None,
    );

    (updated_comment, vec![form])
  };

  let actions = Modlog::create(&mut context.pool(), &forms).await?;
  notify_mod_action(actions, &context);

  let updated_comment_id = updated_comment.id;

  if is_origin {
    ActivityChannel::submit_activity(
      SendActivityData::RemoveComment {
        comment: updated_comment,
        moderator: local_user_view.person.clone(),
        community: orig_comment.community,
        reason: data.reason.clone(),
        with_replies: data.remove_children.unwrap_or_default(),
      },
      &context,
    )?;
  }

  Ok(Json(
    build_comment_response(
      &context,
      updated_comment_id,
      Some(local_user_view),
      local_instance_id,
    )
    .await?,
  ))
}
