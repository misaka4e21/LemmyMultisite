use crate::protected_media::file_reference_has_access;
use actix_multipart::Multipart;
use actix_web::{
  HttpResponse,
  http::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE},
  web::{Data, Json, Path},
};
use aws_sdk_s3::{
  Client,
  config::{Credentials, Region},
  primitives::ByteStream,
};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::{AsyncConnection, RunQueryDsl};
use futures::StreamExt;
use lemmy_api_utils::{context::LemmyContext, tenant::TenantContext, utils::is_admin};
use lemmy_db_schema::source::file::{
  FileObject, FileObjectInsertForm, FileReference, FileReferenceInsertForm,
};
use lemmy_db_schema_file::schema::{file_object, file_reference};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_site::api::{FileListResponse, FileUploadResponse, SuccessResponse};
use lemmy_diesel_utils::connection::get_conn;
use lemmy_utils::{
  error::{LemmyError, LemmyErrorType, LemmyResult},
  settings::SETTINGS,
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use sha2::{Digest, Sha256};
use std::{path::Path as FilePath, sync::LazyLock, time::Duration};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

struct FileStorage {
  client: Client,
  bucket: String,
}

static STORAGE: LazyLock<Option<FileStorage>> = LazyLock::new(|| {
  let config = SETTINGS.s3()?;
  let sdk_config = aws_sdk_s3::Config::builder()
    .behavior_version_latest()
    .endpoint_url(config.endpoint)
    .region(Region::new(config.region))
    .credentials_provider(Credentials::new(
      config.access_key,
      config.secret_key,
      None,
      None,
      "lemmy-static-s3",
    ))
    .force_path_style(config.use_path_style)
    .retry_config(aws_smithy_types::retry::RetryConfig::standard().with_max_attempts(3))
    .timeout_config(
      aws_smithy_types::timeout::TimeoutConfig::builder()
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(30))
        .operation_attempt_timeout(Duration::from_secs(60))
        .operation_timeout(Duration::from_secs(120))
        .build(),
    )
    .build();
  Some(FileStorage {
    client: Client::from_conf(sdk_config),
    bucket: config.bucket,
  })
});

pub async fn upload_file(
  mut payload: Multipart,
  user: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<FileUploadResponse>> {
  let site = tenant.local_site();
  if site.file_upload_disabled {
    return Err(LemmyErrorType::FileUploadDisabled.into());
  }
  let storage = STORAGE.as_ref().ok_or(LemmyErrorType::FileUploadDisabled)?;

  let temporary_file = tempfile::NamedTempFile::new()?;
  let temporary_path = temporary_file.path().to_path_buf();
  let std_file = temporary_file.reopen()?;
  let mut file = tokio::fs::File::from_std(std_file);
  let mut digest = Sha256::new();
  let mut size_bytes = 0_u64;
  let max_size_bytes = u64::try_from(site.file_max_upload_size_bytes)
    .map_err(|_| LemmyErrorType::InvalidFileUploadPolicy)?;
  let mut original_filename = None;

  while let Some(field_result) = payload.next().await {
    let mut field = field_result.map_err(|_| LemmyErrorType::InvalidBodyField)?;
    if field.name() != Some("file") || original_filename.is_some() {
      return Err(LemmyErrorType::InvalidBodyField.into());
    }
    original_filename = field
      .content_disposition()
      .and_then(|disposition| disposition.get_filename())
      .map(sanitize_filename);
    while let Some(chunk_result) = field.next().await {
      let chunk = chunk_result.map_err(|_| LemmyErrorType::InvalidBodyField)?;
      let chunk_size = u64::try_from(chunk.len()).map_err(|_| LemmyErrorType::FileTooLarge)?;
      size_bytes = size_bytes.saturating_add(chunk_size);
      if size_bytes > max_size_bytes {
        return Err(LemmyErrorType::FileTooLarge.into());
      }
      digest.update(&chunk);
      file.write_all(&chunk).await?;
    }
  }
  file.flush().await?;
  drop(file);
  let original_filename = original_filename.ok_or(LemmyErrorType::InvalidBodyField)?;
  if size_bytes == 0 {
    return Err(LemmyErrorType::InvalidBodyField.into());
  }

  let content_type = detect_content_type(&temporary_path)?;
  let parsed_type = content_type
    .parse::<mime::Mime>()
    .map_err(|_| LemmyErrorType::FileTypeNotAllowed)?;
  if parsed_type.type_() == mime::IMAGE || parsed_type.type_() == mime::VIDEO {
    return Err(LemmyErrorType::FileTypeMustUseImageEndpoint("/api/v4/image".into()).into());
  }
  if !site.file_allowed_mime_types.is_empty()
    && !site
      .file_allowed_mime_types
      .iter()
      .any(|allowed| allowed == parsed_type.essence_str())
  {
    return Err(LemmyErrorType::FileTypeNotAllowed.into());
  }

  let content_hash = finalize_content_hash(digest);
  let storage_key = format!("files/{content_hash}");
  let existing = FileObject::read_by_hash(&mut context.pool(), &content_hash).await?;
  let object_present = if existing.is_some() {
    match storage
      .client
      .head_object()
      .bucket(&storage.bucket)
      .key(&storage_key)
      .send()
      .await
    {
      Ok(_) => true,
      Err(error)
        if error
          .raw_response()
          .is_some_and(|response| response.status().as_u16() == 404) =>
      {
        false
      }
      Err(error) => {
        let status = error
          .raw_response()
          .map(|response| response.status().as_u16());
        return Err(map_storage_error(status).into());
      }
    }
  } else {
    false
  };
  if !object_present {
    let body = ByteStream::from_path(&temporary_path)
      .await
      .map_err(|_| LemmyErrorType::FileStorageUnavailable)?;
    storage
      .client
      .put_object()
      .bucket(&storage.bucket)
      .key(&storage_key)
      .content_type(&content_type)
      .body(body)
      .send()
      .await
      .map_err(|error| {
        map_storage_error(
          error
            .raw_response()
            .map(|response| response.status().as_u16()),
        )
      })?;
  }

  let (object, _) = FileObject::create_or_read(
    &mut context.pool(),
    &FileObjectInsertForm {
      content_hash,
      storage_key,
      size_bytes: i64::try_from(size_bytes).map_err(|_| LemmyErrorType::FileTooLarge)?,
      content_type: content_type.clone(),
    },
  )
  .await?;
  let reference = FileReference::create(
    &mut context.pool(),
    &FileReferenceInsertForm {
      public_id: Uuid::new_v4(),
      local_site_id: site.id,
      file_id: object.id,
      owner_person_id: user.person.id,
      original_filename: original_filename.clone(),
      is_public: true,
    },
  )
  .await?;
  let protocol = context.settings().get_protocol_string();
  let public_url = format!(
    "{protocol}://{}/api/v4/file/{}/{}",
    tenant.hostname,
    reference.public_id,
    urlencoding::encode(&original_filename),
  );
  drop(temporary_file);

  Ok(Json(FileUploadResponse {
    public_id: reference.public_id.to_string(),
    url: public_url,
    filename: original_filename,
    content_type,
    size_bytes,
  }))
}

fn detect_content_type(path: &FilePath) -> LemmyResult<String> {
  let detected = infer::get_from_path(path)
    .map_err(|_| LemmyErrorType::FileTypeNotAllowed)?
    .map(|kind| kind.mime_type().to_string());
  Ok(detected.unwrap_or_else(|| "application/octet-stream".to_string()))
}

fn sanitize_filename(filename: &str) -> String {
  let filename = filename.replace(['/', '\\'], "_");
  let filename: String = filename
    .chars()
    .filter(|character| !character.is_control())
    .collect();
  if filename.trim().is_empty() {
    "download".to_string()
  } else {
    filename
  }
}

pub async fn download_file(
  path: Path<(Uuid, String)>,
  user: Option<LocalUserView>,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<HttpResponse> {
  let (public_id, _filename) = path.into_inner();
  let site = tenant.local_site();
  if user.is_none() && site.private_instance {
    return Ok(HttpResponse::Unauthorized().finish());
  }
  let reference = FileReference::read_by_public_id(&mut context.pool(), site.id, public_id)
    .await?
    .ok_or(LemmyErrorType::FileNotFound)?;
  if !file_reference_has_access(
    &context,
    &tenant,
    user.as_ref(),
    reference.id,
    reference.is_public,
  )
  .await?
  {
    return Ok(HttpResponse::NotFound().finish());
  }
  let object = FileObject::read(reference.file_id, &mut context.pool()).await?;
  let storage = STORAGE
    .as_ref()
    .ok_or(LemmyErrorType::FileStorageUnavailable)?;
  let result = storage
    .client
    .get_object()
    .bucket(&storage.bucket)
    .key(&object.storage_key)
    .send()
    .await
    .map_err(|error| {
      map_storage_error(
        error
          .raw_response()
          .map(|response| response.status().as_u16()),
      )
    })?;
  let filename = utf8_percent_encode(&reference.original_filename, NON_ALPHANUMERIC);
  let stream = ReaderStream::new(result.body.into_async_read());
  Ok(
    HttpResponse::Ok()
      .insert_header((CONTENT_TYPE, object.content_type))
      .insert_header((CONTENT_LENGTH, object.size_bytes.to_string()))
      .insert_header((
        CONTENT_DISPOSITION,
        format!("attachment; filename*=UTF-8''{filename}"),
      ))
      .insert_header(("X-Content-Type-Options", "nosniff"))
      .streaming(stream),
  )
}

pub async fn list_files(
  user: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<FileListResponse>> {
  let site = tenant.local_site();
  let references =
    FileReference::list_by_owner(&mut context.pool(), site.id, user.person.id).await?;
  let mut files = Vec::with_capacity(references.len());
  for reference in references {
    let object = FileObject::read(reference.file_id, &mut context.pool()).await?;
    let url = format!(
      "{}://{}/api/v4/file/{}/{}",
      context.settings().get_protocol_string(),
      tenant.hostname,
      reference.public_id,
      urlencoding::encode(&reference.original_filename),
    );
    files.push(FileUploadResponse {
      public_id: reference.public_id.to_string(),
      url,
      filename: reference.original_filename,
      content_type: object.content_type,
      size_bytes: u64::try_from(object.size_bytes)
        .map_err(|_| LemmyErrorType::FileStorageUnavailable)?,
    });
  }
  Ok(Json(FileListResponse { files }))
}

pub async fn delete_file(
  path: Path<Uuid>,
  user: LocalUserView,
  context: Data<LemmyContext>,
  tenant: TenantContext,
) -> LemmyResult<Json<SuccessResponse>> {
  let public_id = path.into_inner();
  let site = tenant.local_site();
  let storage = STORAGE
    .as_ref()
    .ok_or(LemmyErrorType::FileStorageUnavailable)?;
  let mut pool = context.pool();
  let conn = &mut get_conn(&mut pool).await?;
  conn
    .transaction::<(), LemmyError, _>(async |conn| {
      let reference = file_reference::table
        .filter(file_reference::local_site_id.eq(site.id))
        .filter(file_reference::public_id.eq(public_id))
        .first::<FileReference>(conn)
        .await?;
      if reference.owner_person_id != user.person.id {
        is_admin(&user)?;
      }
      let object = file_object::table
        .find(reference.file_id)
        .for_update()
        .first::<FileObject>(conn)
        .await?;
      let reference_count: i64 = file_reference::table
        .filter(file_reference::file_id.eq(object.id))
        .count()
        .get_result(conn)
        .await?;
      if is_last_file_reference(reference_count) {
        storage
          .client
          .delete_object()
          .bucket(&storage.bucket)
          .key(&object.storage_key)
          .send()
          .await
          .map_err(|error| {
            map_storage_error(
              error
                .raw_response()
                .map(|response| response.status().as_u16()),
            )
          })?;
      }
      diesel::delete(file_reference::table.find(reference.id))
        .execute(conn)
        .await?;
      if is_last_file_reference(reference_count) {
        diesel::delete(file_object::table.find(object.id))
          .execute(conn)
          .await?;
      }
      Ok(())
    })
    .await?;
  Ok(Json(SuccessResponse::default()))
}

fn map_storage_error(status: Option<u16>) -> LemmyErrorType {
  match status {
    Some(404) => LemmyErrorType::FileNotFound,
    Some(401 | 403) => LemmyErrorType::FileStorageUnauthorized,
    _ => LemmyErrorType::FileStorageUnavailable,
  }
}

fn finalize_content_hash(digest: Sha256) -> String {
  hex::encode(digest.finalize())
}

fn is_last_file_reference(reference_count: i64) -> bool {
  reference_count == 1
}

#[cfg(test)]
mod tests {
  use super::{
    detect_content_type, finalize_content_hash, is_last_file_reference, sanitize_filename,
  };
  use sha2::{Digest, Sha256};
  use std::io::Write;

  #[test]
  fn hashes_streamed_file_contents() {
    let mut digest = Sha256::new();
    digest.update(b"hello world");

    assert_eq!(
      finalize_content_hash(digest),
      "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
  }

  #[test]
  fn detects_content_type_from_file_bytes() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"%PDF-1.7\nfile content").unwrap();

    assert_eq!(detect_content_type(file.path()).unwrap(), "application/pdf");
  }

  #[test]
  fn sanitizes_paths_and_control_characters_in_filenames() {
    assert_eq!(
      sanitize_filename("../folder\\name\r\n\0.pdf"),
      ".._folder_name.pdf"
    );
    assert_eq!(sanitize_filename(" \r\n "), "download");
  }

  #[test]
  fn detects_when_a_reference_is_the_last_one() {
    assert!(is_last_file_reference(1));
    assert!(!is_last_file_reference(2));
  }
}
