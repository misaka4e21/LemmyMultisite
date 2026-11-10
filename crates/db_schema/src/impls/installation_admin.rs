use crate::source::installation_admin::{InstallationAdmin, InstallationAdminInsertForm};
use bcrypt::{DEFAULT_COST, hash, verify};
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, dsl::insert_into};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::schema::installation_admin;
use lemmy_diesel_utils::connection::{DbPool, get_conn};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

impl InstallationAdmin {
  pub async fn create(pool: &mut DbPool<'_>, username: &str, password: &str) -> LemmyResult<Self> {
    let password_encrypted = hash(password, DEFAULT_COST)
      .map_err(|e| LemmyErrorType::Unknown(format!("password hash failed: {e}")))?;
    let conn = &mut get_conn(pool).await?;
    insert_into(installation_admin::table)
      .values(&InstallationAdminInsertForm {
        username: username.to_string(),
        password_encrypted,
      })
      .get_result::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntCreate)
  }

  pub async fn read_by_username(
    pool: &mut DbPool<'_>,
    username: &str,
  ) -> LemmyResult<Option<Self>> {
    let conn = &mut get_conn(pool).await?;
    installation_admin::table
      .filter(installation_admin::username.eq(username))
      .first(conn)
      .await
      .optional()
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  pub async fn read(pool: &mut DbPool<'_>, id: i64) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    installation_admin::table
      .find(id)
      .first(conn)
      .await
      .with_lemmy_type(LemmyErrorType::NotFound)
  }

  pub fn verify_password(&self, password: &str) -> bool {
    verify(password, &self.password_encrypted).unwrap_or(false)
  }

  pub async fn bootstrap(pool: &mut DbPool<'_>, username: &str, password: &str) -> LemmyResult<()> {
    if Self::read_by_username(pool, username).await?.is_none() {
      Self::create(pool, username, password).await?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;
  use lemmy_diesel_utils::connection::build_db_pool_for_tests;
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn installation_admin_credentials() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let username = format!(
      "installation_admin_{}",
      Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let admin = InstallationAdmin::create(pool, &username, "secret-password").await?;
    assert!(admin.verify_password("secret-password"));
    assert!(!admin.verify_password("wrong-password"));
    assert_eq!(
      InstallationAdmin::read_by_username(pool, &username)
        .await?
        .map(|a| a.id),
      Some(admin.id)
    );
    Ok(())
  }
}
