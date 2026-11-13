use crate::source::{
  custom_emoji::{CustomEmoji, CustomEmojiInsertForm, CustomEmojiUpdateForm},
  custom_emoji_keyword::{CustomEmojiKeyword, CustomEmojiKeywordInsertForm},
};
use diesel::{ExpressionMethods, QueryDsl, dsl::insert_into};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::{
  newtypes::{CustomEmojiId, LocalSiteId},
  schema::{
    custom_emoji::{self as emoji_table, dsl::custom_emoji},
    custom_emoji_keyword::dsl::{custom_emoji_id, custom_emoji_keyword},
  },
};
use lemmy_diesel_utils::{
  connection::{DbPool, get_conn},
  traits::Crud,
};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

impl Crud for CustomEmoji {
  type InsertForm = CustomEmojiInsertForm;
  type UpdateForm = CustomEmojiUpdateForm;
  type IdType = CustomEmojiId;

  async fn create(pool: &mut DbPool<'_>, form: &Self::InsertForm) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    insert_into(custom_emoji)
      .values(form)
      .get_result::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }

  async fn update(
    pool: &mut DbPool<'_>,
    emoji_id: Self::IdType,
    new_custom_emoji: &Self::UpdateForm,
  ) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    diesel::update(custom_emoji.find(emoji_id))
      .set(new_custom_emoji)
      .get_result::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }
}

impl CustomEmoji {
  pub async fn update_for_site(
    pool: &mut DbPool<'_>,
    emoji_id: CustomEmojiId,
    site_id: LocalSiteId,
    form: &CustomEmojiUpdateForm,
  ) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    diesel::update(
      custom_emoji
        .find(emoji_id)
        .filter(emoji_table::local_site_id.eq(site_id)),
    )
    .set(form)
    .get_result(conn)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn delete_for_site(
    pool: &mut DbPool<'_>,
    emoji_id: CustomEmojiId,
    site_id: LocalSiteId,
  ) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    let count = diesel::delete(
      custom_emoji
        .find(emoji_id)
        .filter(emoji_table::local_site_id.eq(site_id)),
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

impl CustomEmojiKeyword {
  pub async fn create_from_keywords(
    pool: &mut DbPool<'_>,
    for_custom_emoji_id: CustomEmojiId,
    keywords: &[String],
  ) -> LemmyResult<Vec<Self>> {
    let forms = keywords
      .iter()
      .map(|k| CustomEmojiKeywordInsertForm {
        custom_emoji_id: for_custom_emoji_id,
        keyword: k.to_lowercase().trim().to_string(),
      })
      .collect();

    Self::create(pool, &forms).await
  }

  pub async fn create(
    pool: &mut DbPool<'_>,
    form: &Vec<CustomEmojiKeywordInsertForm>,
  ) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    insert_into(custom_emoji_keyword)
      .values(form)
      .get_results::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }
  pub async fn delete(pool: &mut DbPool<'_>, emoji_id: CustomEmojiId) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    diesel::delete(custom_emoji_keyword.filter(custom_emoji_id.eq(emoji_id)))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }
}
