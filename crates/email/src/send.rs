use lemmy_db_schema::source::local_site::LocalSite;
use lemmy_diesel_utils::sensitive::SensitiveString;
use lemmy_utils::{
  error::{LemmyErrorExt, LemmyErrorType, LemmyResult},
  settings::structs::Settings,
  spawn_try_task,
};
use lettre::{
  Address,
  AsyncTransport,
  Message,
  message::{Mailbox, MultiPart},
  transport::smtp::extension::ClientId,
};
use std::str::FromStr;
use uuid::Uuid;

type AsyncSmtpTransport = lettre::AsyncSmtpTransport<lettre::Tokio1Executor>;

pub(crate) fn send_email(
  subject: String,
  to_email: SensitiveString,
  to_username: String,
  html: String,
  local_site: LocalSite,
  settings: &'static Settings,
) {
  spawn_try_task(async move {
    let (connection, smtp_from_address) = resolve_smtp_settings(&local_site, settings)?;
    let hostname = local_site.hostname.clone();

    #[expect(clippy::expect_used)]
    let mailer = AsyncSmtpTransport::from_url(&connection)
      .expect("init email transport")
      .hello_name(ClientId::Domain(hostname.clone()))
      .build();

    // use usize::MAX as the line wrap length, since lettre handles the wrapping for us
    let plain_text = html2text::from_read(html.as_bytes(), usize::MAX)?;

    let email = Message::builder()
      .from(
        smtp_from_address
          .parse()
          .with_lemmy_type(LemmyErrorType::InvalidEmailAddress(
            smtp_from_address.clone(),
          ))?,
      )
      .to(Mailbox::new(
        Some(to_username.clone()),
        Address::from_str(&to_email)
          .with_lemmy_type(LemmyErrorType::InvalidEmailAddress(to_email.into_inner()))?,
      ))
      .message_id(Some(format!("<{}@{}>", Uuid::new_v4(), hostname)))
      .subject(subject)
      .multipart(MultiPart::alternative_plain_html(plain_text, html.clone()))
      .with_lemmy_type(LemmyErrorType::EmailSendFailed)?;

    mailer
      .send(email)
      .await
      .with_lemmy_type(LemmyErrorType::EmailSendFailed)?;

    Ok(())
  })
}

fn resolve_smtp_settings(
  local_site: &LocalSite,
  settings: &Settings,
) -> LemmyResult<(String, String)> {
  let connection = local_site
    .smtp_connection
    .clone()
    .or_else(|| settings.email.as_ref().map(|e| e.connection.clone()))
    .ok_or(LemmyErrorType::NoEmailSetup)?;
  let smtp_from_address = local_site
    .smtp_from_address
    .clone()
    .or_else(|| settings.email.as_ref().map(|e| e.smtp_from_address.clone()))
    .ok_or(LemmyErrorType::NoEmailSetup)?;
  Ok((connection, smtp_from_address))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn site_smtp_settings_override_installation() {
    let local_site = LocalSite {
      smtp_connection: Some("smtps://site.example:465".to_string()),
      smtp_from_address: Some("noreply@site.example".to_string()),
      ..Default::default()
    };
    let (connection, from) = resolve_smtp_settings(&local_site, &Settings::default()).unwrap();
    assert_eq!(connection, "smtps://site.example:465");
    assert_eq!(from, "noreply@site.example");
  }
}
