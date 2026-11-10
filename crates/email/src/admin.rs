use crate::{send::send_email, user_language};
use lemmy_db_schema::source::local_site::LocalSite;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_diesel_utils::connection::DbPool;
use lemmy_utils::{error::LemmyResult, settings::structs::Settings};

/// Send a new applicant email notification to all admins
pub async fn send_new_applicant_email_to_admins(
  applicant_username: &str,
  pool: &mut DbPool<'_>,
  local_site: &LocalSite,
  settings: &'static Settings,
) -> LemmyResult<()> {
  // Collect the admins with emails
  let admins = LocalUserView::list_admins_with_emails(pool, local_site.id).await?;

  let applications_link = &format!(
    "{}://{}/registration_applications",
    settings.get_protocol_string(),
    local_site.hostname,
  );

  for admin in admins {
    let lang = user_language(&admin.local_user);
    if let Some(email) = admin.local_user.email {
      let subject = lang.new_application_subject(&local_site.hostname, applicant_username);
      let body = lang.new_application_body(applications_link);
      send_email(
        subject,
        email,
        admin.person.name,
        body,
        local_site.clone(),
        settings,
      );
    }
  }
  Ok(())
}

/// Send a report to all admins
pub async fn send_new_report_email_to_admins(
  reporter_username: &str,
  reported_username: &str,
  pool: &mut DbPool<'_>,
  local_site: &LocalSite,
  settings: &'static Settings,
) -> LemmyResult<()> {
  // Collect the admins with emails
  let admins = LocalUserView::list_admins_with_emails(pool, local_site.id).await?;

  let reports_link = &format!(
    "{}://{}/reports",
    settings.get_protocol_string(),
    local_site.hostname
  );

  for admin in admins {
    let lang = user_language(&admin.local_user);
    if let Some(email) = admin.local_user.email {
      let subject =
        lang.new_report_subject(&local_site.hostname, reported_username, reporter_username);
      let body = lang.new_report_body(reports_link);
      send_email(
        subject,
        email,
        admin.person.name,
        body,
        local_site.clone(),
        settings,
      );
    }
  }
  Ok(())
}
