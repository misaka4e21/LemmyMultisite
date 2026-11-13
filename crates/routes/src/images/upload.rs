use super::utils::delete_old_image;
use UploadType::*;
use actix_multipart::Multipart;
use actix_web::{self, HttpRequest, web::*};
use futures::StreamExt;
use lemmy_api_utils::{
  context::LemmyContext,
  request::{PictrsResponse, delete_image_alias},
  tenant::TenantContext,
  utils::{is_admin, is_mod_or_admin},
};
use lemmy_db_schema::source::{
  community::{Community, CommunityUpdateForm},
  images::ImageDetails,
  local_site::LocalSite,
  media::{MediaObject, MediaObjectInsertForm, MediaReference, MediaReferenceInsertForm},
  person::{Person, PersonUpdateForm},
  site::{Site, SiteUpdateForm},
};
use lemmy_db_views_community::api::CommunityIdQuery;
use lemmy_db_views_local_image::api::UploadImageResponse;
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::SiteView;
use lemmy_diesel_utils::traits::Crud;
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};
use reqwest::{
  Body,
  multipart::{Form as MultipartForm, Part},
};
use sha2::{Digest, Sha256};
use std::{
  io,
  sync::{Arc, Mutex},
  time::Duration,
};
use url::Url;

#[derive(Clone, Copy)]
pub enum UploadType {
  Avatar,
  Banner,
  Other,
}

impl UploadType {
  fn purpose(&self) -> &'static str {
    match self {
      Avatar => "avatar",
      Banner => "banner",
      Other => "other",
    }
  }
}

pub async fn upload_image(
  req: HttpRequest,
  body: Payload,
  local_user_view: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<UploadImageResponse>> {
  let local_site = tenant.local_site().clone();

  if local_site.image_upload_disabled {
    return Err(LemmyErrorType::ImageUploadDisabled.into());
  }

  Ok(Json(
    do_upload_image(req, body, Other, &local_user_view, &local_site, &context).await?,
  ))
}

pub async fn upload_user_avatar(
  req: HttpRequest,
  body: Payload,
  local_user_view: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<UploadImageResponse>> {
  let local_site = tenant.local_site().clone();

  let image = do_upload_image(req, body, Avatar, &local_user_view, &local_site, &context).await?;
  delete_old_image(
    &local_user_view.person.avatar,
    Some(&image.image_url),
    local_site.id,
    local_user_view.person.id,
    &context,
  )
  .await?;

  let form = PersonUpdateForm {
    avatar: Some(Some(image.image_url.clone().into())),
    ..Default::default()
  };
  Person::update(&mut context.pool(), local_user_view.person.id, &form).await?;

  Ok(Json(image))
}

pub async fn upload_user_banner(
  req: HttpRequest,
  body: Payload,
  local_user_view: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<UploadImageResponse>> {
  let local_site = tenant.local_site().clone();

  let image = do_upload_image(req, body, Banner, &local_user_view, &local_site, &context).await?;
  delete_old_image(
    &local_user_view.person.banner,
    Some(&image.image_url),
    local_site.id,
    local_user_view.person.id,
    &context,
  )
  .await?;

  let form = PersonUpdateForm {
    banner: Some(Some(image.image_url.clone().into())),
    ..Default::default()
  };
  Person::update(&mut context.pool(), local_user_view.person.id, &form).await?;

  Ok(Json(image))
}

pub async fn upload_community_icon(
  req: HttpRequest,
  query: Query<CommunityIdQuery>,
  body: Payload,
  local_user_view: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<UploadImageResponse>> {
  let local_site = tenant.local_site().clone();

  let community: Community = Community::read(&mut context.pool(), query.id).await?;
  is_mod_or_admin(&mut context.pool(), &local_user_view, community.id).await?;

  let image = do_upload_image(req, body, Avatar, &local_user_view, &local_site, &context).await?;
  delete_old_image(
    &community.icon,
    Some(&image.image_url),
    local_site.id,
    local_user_view.person.id,
    &context,
  )
  .await?;

  let form = CommunityUpdateForm {
    icon: Some(Some(image.image_url.clone().into())),
    ..Default::default()
  };
  Community::update(&mut context.pool(), community.id, &form).await?;

  Ok(Json(image))
}

pub async fn upload_community_banner(
  req: HttpRequest,
  query: Query<CommunityIdQuery>,
  body: Payload,
  local_user_view: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<UploadImageResponse>> {
  let local_site = tenant.local_site().clone();

  let community: Community = Community::read(&mut context.pool(), query.id).await?;
  is_mod_or_admin(&mut context.pool(), &local_user_view, community.id).await?;

  let image = do_upload_image(req, body, Banner, &local_user_view, &local_site, &context).await?;
  delete_old_image(
    &community.banner,
    Some(&image.image_url),
    local_site.id,
    local_user_view.person.id,
    &context,
  )
  .await?;

  let form = CommunityUpdateForm {
    banner: Some(Some(image.image_url.clone().into())),
    ..Default::default()
  };
  Community::update(&mut context.pool(), community.id, &form).await?;

  Ok(Json(image))
}

pub async fn upload_site_icon(
  req: HttpRequest,
  body: Payload,
  local_user_view: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<UploadImageResponse>> {
  is_admin(&local_user_view)?;

  let SiteView {
    site, local_site, ..
  } = tenant.site_view.clone();

  let image = do_upload_image(req, body, Avatar, &local_user_view, &local_site, &context).await?;
  delete_old_image(
    &site.icon,
    Some(&image.image_url),
    local_site.id,
    local_user_view.person.id,
    &context,
  )
  .await?;

  let form = SiteUpdateForm {
    icon: Some(Some(image.image_url.clone().into())),
    ..Default::default()
  };
  Site::update(&mut context.pool(), site.id, &form).await?;

  Ok(Json(image))
}

pub async fn upload_site_banner(
  req: HttpRequest,
  body: Payload,
  local_user_view: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<UploadImageResponse>> {
  is_admin(&local_user_view)?;

  let SiteView {
    site, local_site, ..
  } = tenant.site_view.clone();

  let image = do_upload_image(req, body, Banner, &local_user_view, &local_site, &context).await?;
  delete_old_image(
    &site.banner,
    Some(&image.image_url),
    local_site.id,
    local_user_view.person.id,
    &context,
  )
  .await?;

  let form = SiteUpdateForm {
    banner: Some(Some(image.image_url.clone().into())),
    ..Default::default()
  };
  Site::update(&mut context.pool(), site.id, &form).await?;

  Ok(Json(image))
}

async fn do_upload_image(
  req: HttpRequest,
  body: Payload,
  upload_type: UploadType,
  local_user_view: &LocalUserView,
  local_site: &LocalSite,
  context: &Data<LemmyContext>,
) -> LemmyResult<UploadImageResponse> {
  let pictrs_url = context.settings().pictrs()?.url;
  let image_url = format!("{}image", pictrs_url);
  let hash_prefix = match upload_type {
    Avatar => format!("avatar:{}", local_site.image_max_avatar_size),
    Banner => format!("banner:{}", local_site.image_max_banner_size),
    Other => format!(
      "other:{}:{}",
      local_site.image_max_upload_size, local_site.image_allow_video_uploads
    ),
  };
  let resize = match upload_type {
    Avatar => local_site.image_max_avatar_size.to_string(),
    Banner => local_site.image_max_banner_size.to_string(),
    Other => local_site.image_max_upload_size.to_string(),
  };
  let allow_video = local_site.image_allow_video_uploads.to_string();
  let timeout = Duration::from_secs(local_site.image_upload_timeout_seconds.try_into()?);
  let peer_addr = req.head().peer_addr;
  let mut multipart = Multipart::new(req.headers(), body);
  let mut uploaded = None;
  let mut pictrs_msg = String::new();

  while let Some(field) = multipart.next().await {
    let mut field = field
      .map_err(|e| LemmyErrorType::PictrsInvalidImageUpload(format!("multipart error: {e}")))?;
    let (field_name, filename) = match field.content_disposition() {
      Some(disposition) => (
        disposition.get_name().unwrap_or_default().to_string(),
        disposition.get_filename().unwrap_or("image").to_string(),
      ),
      None => (String::new(), "image".to_string()),
    };
    let content_type = field.content_type().map(ToString::to_string);

    if !field_name.starts_with("images") {
      while let Some(chunk) = field.next().await {
        chunk
          .map_err(|e| LemmyErrorType::PictrsInvalidImageUpload(format!("multipart error: {e}")))?;
      }
      continue;
    }

    // Field is !Send, so hash and forward it from a local task while reqwest consumes the
    // Send channel. This keeps the file out of memory and hashes only the file bytes.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, io::Error>>(8);
    let hash_state = Arc::new(Mutex::new((Sha256::new(), 0_u64)));
    {
      let mut state = hash_state
        .lock()
        .map_err(|_| LemmyErrorType::CouldntCreate)?;
      state.0.update(hash_prefix.as_bytes());
    }
    let hash_state_for_task = Arc::clone(&hash_state);
    actix_web::rt::spawn(async move {
      while let Some(chunk) = field.next().await {
        match chunk {
          Ok(bytes) => {
            if let Ok(mut state) = hash_state_for_task.lock() {
              state.0.update(&bytes);
              state.1 += bytes.len() as u64;
            }
            if tx.send(Ok(bytes)).await.is_err() {
              break;
            }
          }
          Err(error) => {
            let _ = tx.send(Err(io::Error::other(error.to_string()))).await;
            break;
          }
        }
      }
    });
    let body_stream = futures::stream::unfold(rx, |mut rx| async move {
      rx.recv().await.map(|item| (item, rx))
    });

    let mut part = Part::stream(Body::wrap_stream(body_stream)).file_name(filename);
    if let Some(content_type) = content_type {
      part =
        part
          .mime_str(&content_type)
          .with_lemmy_type(LemmyErrorType::PictrsInvalidImageUpload(
            "invalid image content type".to_string(),
          ))?;
    }
    let form = MultipartForm::new().part("images[]", part);
    let mut client_req = context
      .pictrs_client()
      .post(image_url.clone())
      .multipart(form);

    // Set pictrs parameters to downscale images and restrict file types.
    // https://git.asonix.dog/asonix/pict-rs/#api
    client_req = match upload_type {
      Avatar => client_req.query(&[
        ("resize", resize.as_str()),
        ("allow_animation", "false"),
        ("allow_video", "false"),
      ]),
      Banner => client_req.query(&[
        ("resize", resize.as_str()),
        ("allow_animation", "false"),
        ("allow_video", "false"),
      ]),
      Other => client_req.query(&[
        ("allow_video", allow_video.as_str()),
        ("resize", resize.as_str()),
      ]),
    };
    if let Some(addr) = peer_addr {
      client_req = client_req.header("X-Forwarded-For", addr.to_string());
    }

    let res = client_req
      .timeout(timeout)
      .send()
      .await
      // Dont check for status code here and dont call error_for_status(). If the upload failed,
      // this is handled below as images.files is empty.
      .with_lemmy_type(LemmyErrorType::PictrsInvalidImageUpload(
        "HTTP request to pict-rs failed".to_string(),
      ))?;

    let (content_hash, size_bytes) = {
      let state = hash_state
        .lock()
        .map_err(|_| LemmyErrorType::CouldntCreate)?;
      let digest = state.0.clone().finalize();
      let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
      (hash, state.1 as i64)
    };
    let images = res.json::<PictrsResponse>().await?;
    pictrs_msg = images.msg.clone();
    let protocol_and_hostname = context.protocol_and_hostname();
    for image in images.files {
      let media_form = MediaObjectInsertForm {
        content_hash: content_hash.clone(),
        canonical_pictrs_alias: image.file.clone(),
        size_bytes,
        width: image.details.width.into(),
        height: image.details.height.into(),
        content_type: image.details.content_type.clone(),
        blurhash: image.details.blurhash.clone(),
      };
      let (media, created) = MediaObject::create_or_read(&mut context.pool(), &media_form).await?;
      if !created && media.canonical_pictrs_alias != image.file {
        // The uploaded bytes already exist. Drop the duplicate pict-rs alias and keep using the
        // canonical one.
        delete_image_alias(&image.file, context).await.ok();
      }

      let alias = media.canonical_pictrs_alias.clone();
      let thumbnail_url = Url::parse(&format!("{protocol_and_hostname}/api/v4/image/{alias}"))?;

      MediaReference::ensure(
        &mut context.pool(),
        &MediaReferenceInsertForm {
          local_site_id: local_site.id,
          media_id: media.id,
          owner_person_id: Some(local_user_view.person.id),
          post_id: None,
          purpose: upload_type.purpose().to_string(),
        },
      )
      .await?;

      if created {
        let details_form = image
          .details
          .build_image_details_form(&thumbnail_url, Some(media.content_hash.clone()));
        ImageDetails::create(&mut context.pool(), &details_form).await?;
      }
      uploaded = Some((media.canonical_pictrs_alias, thumbnail_url));
    }
  }
  let (filename, image_url) =
    uploaded.ok_or(LemmyErrorType::PictrsInvalidImageUpload(pictrs_msg))?;
  Ok(UploadImageResponse {
    image_url,
    filename,
  })
}
