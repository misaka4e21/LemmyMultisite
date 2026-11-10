use super::utils::{adapt_request, delete_old_image, make_send};
use UploadType::*;
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
use reqwest::Body;
use sha2::{Digest, Sha256};
use std::{
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
  delete_old_image(&local_user_view.person.avatar, &context).await?;

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
  delete_old_image(&local_user_view.person.banner, &context).await?;

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
  delete_old_image(&community.icon, &context).await?;

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
  delete_old_image(&community.banner, &context).await?;

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
  delete_old_image(&site.icon, &context).await?;

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
  delete_old_image(&site.banner, &context).await?;

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
  let max_upload_size = local_site.image_max_upload_size.to_string();
  let image_url = format!("{}image", pictrs_url);

  let mut client_req = adapt_request(&req, image_url, context);

  // Set pictrs parameters to downscale images and restrict file types.
  // https://git.asonix.dog/asonix/pict-rs/#api
  client_req = match upload_type {
    Avatar => {
      let max_size = local_site.image_max_avatar_size.to_string();
      client_req.query(&[
        ("resize", max_size.as_ref()),
        ("allow_animation", "false"),
        ("allow_video", "false"),
      ])
    }
    Banner => {
      let max_size = local_site.image_max_banner_size.to_string();
      client_req.query(&[
        ("resize", max_size.as_ref()),
        ("allow_animation", "false"),
        ("allow_video", "false"),
      ])
    }
    Other => {
      let mut query = vec![(
        "allow_video",
        local_site.image_allow_video_uploads.to_string(),
      )];
      query.push(("resize", max_upload_size));
      client_req.query(&query)
    }
  };
  if let Some(addr) = req.head().peer_addr {
    client_req = client_req.header("X-Forwarded-For", addr.to_string())
  };

  let hash_prefix = match upload_type {
    Avatar => format!("avatar:{}", local_site.image_max_avatar_size),
    Banner => format!("banner:{}", local_site.image_max_banner_size),
    Other => format!(
      "other:{}:{}",
      local_site.image_max_upload_size, local_site.image_allow_video_uploads
    ),
  };
  let mut hasher = Sha256::new();
  hasher.update(hash_prefix.as_bytes());
  let hash_state = Arc::new(Mutex::new((hasher, 0_u64)));
  let hash_state_for_stream = Arc::clone(&hash_state);
  let body = body.map(move |chunk| {
    if let Ok(bytes) = &chunk {
      if let Ok(mut state) = hash_state_for_stream.lock() {
        state.0.update(bytes.as_ref());
        state.1 += bytes.len() as u64;
      }
    }
    chunk
  });

  // Make HTTP request to pict-rs with the user provided image data.
  let res = client_req
    .timeout(Duration::from_secs(
      local_site.image_upload_timeout_seconds.try_into()?,
    ))
    .body(Body::wrap_stream(make_send(body)))
    .send()
    .await
    // Dont check for status code here and dont call `error_for_status()`. If the upload failed,
    // this is handled below as `images.files` is empty.
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
  let pictrs_msg = images.msg.clone();
  let protocol_and_hostname = context.protocol_and_hostname();
  let mut uploaded = None;
  let multiple_files = images.files.len() > 1;
  for (index, image) in images.files.iter().enumerate() {
    // Pictrs allows uploading multiple images in a single request. Lemmy doesnt need this,
    // but still a user may upload multiple and so we need to store all links in db for
    // to allow deletion via web ui.
    let media_form = MediaObjectInsertForm {
      content_hash: if multiple_files {
        format!("{content_hash}:{index}")
      } else {
        content_hash.clone()
      },
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
  let (filename, image_url) =
    uploaded.ok_or(LemmyErrorType::PictrsInvalidImageUpload(pictrs_msg))?;
  Ok(UploadImageResponse {
    image_url,
    filename,
  })
}
