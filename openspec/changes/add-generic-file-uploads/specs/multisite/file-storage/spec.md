## Purpose

Store arbitrary user documents in S3-compatible object storage and serve them through per-site URLs with tenant isolation and safe download behavior.

## ADDED Requirements

### Requirement: Generic document upload
Authenticated users SHALL upload a single non-image, non-video document through `POST /api/v4/file` using multipart form data. The API MUST preserve the original filename as metadata and return a site-scoped URL, detected content type, size, and public identifier.

#### Scenario: Upload a PDF
- **WHEN** an authenticated user uploads a PDF through `POST /api/v4/file`
- **THEN** the response contains a public identifier, URL, filename, detected content type, and size
- **AND** the file is retrievable through the current site

#### Scenario: Image or video submitted to the document endpoint
- **WHEN** the uploaded bytes are detected as an image or video
- **THEN** the request is rejected with an error that points to the existing image upload endpoints
- **AND** no document object is created

#### Scenario: Unauthenticated upload
- **WHEN** a client without authentication calls `POST /api/v4/file`
- **THEN** the request is rejected with 401

#### Scenario: Empty file
- **WHEN** the multipart body contains no bytes
- **THEN** the request is rejected without creating a reference or object

### Requirement: S3-compatible document storage
Document bytes SHALL be stored in an S3-compatible bucket configured at installation level. The configuration MUST accept endpoint, bucket, region, access key, secret key, and path-style addressing. The implementation MUST NOT depend on a provider-specific API and MUST use a bucket separate from the pict-rs image store.

#### Scenario: RustFS development backend
- **WHEN** the Docker development stack starts with its default document-storage settings
- **THEN** document uploads are written to the RustFS document bucket
- **AND** downloads are served through Lemmy

#### Scenario: Custom S3-compatible provider
- **WHEN** an operator configures a non-AWS endpoint, bucket, region, and credentials
- **THEN** uploads and downloads work without code changes
- **AND** path-style mode can be enabled when the provider requires it

#### Scenario: S3 credentials are not exposed
- **WHEN** a client reads an upload response or downloads a file
- **THEN** the response does not contain access keys, secret keys, bucket names, or endpoint credentials

### Requirement: Content-addressed deduplication
The system SHALL hash document bytes with SHA-256 and store one object per unique byte sequence per installation. Uploading identical bytes SHALL reuse the stored object and create a new site reference.

#### Scenario: Same document uploaded twice
- **WHEN** the same bytes are uploaded twice on one site
- **THEN** the storage layer holds one object
- **AND** the site has two references with their own public identifiers and filenames

#### Scenario: Same document uploaded on two sites
- **WHEN** the same bytes are uploaded on site A and site B
- **THEN** the storage layer holds one object
- **AND** each site has its own reference

#### Scenario: Different filename for identical bytes
- **WHEN** identical bytes are uploaded with different filenames
- **THEN** the object is reused
- **AND** each reference preserves its own original filename

### Requirement: Per-site file references and access
Every document use SHALL have a `file_reference` scoped to a `local_site_id`, with an unguessable public identifier, owner, original filename, and detected content type. A file MUST NOT be readable through a site that has no reference to it.

#### Scenario: Cross-site access
- **WHEN** a client requests site B's hostname for a public identifier that exists only on site A
- **THEN** the request returns 404
- **AND** the bytes are not served

#### Scenario: Shared object with separate references
- **WHEN** site A and site B both reference the same object
- **THEN** each site's URL resolves through its own reference
- **AND** deleting one reference does not break the other site

#### Scenario: Private site access
- **WHEN** an anonymous client requests a file on a private site
- **THEN** the request returns 401

### Requirement: Safe download response
Document downloads SHALL stream through Lemmy from S3. Responses MUST use `Content-Disposition: attachment`, `X-Content-Type-Options: nosniff`, and a safely encoded filename. HTML, SVG, JavaScript, and other active content MUST NOT be rendered inline.

#### Scenario: Download a PDF
- **WHEN** a client requests a valid document URL
- **THEN** the response streams the bytes from S3
- **AND** the response is an attachment with the original filename encoded safely

#### Scenario: Upload HTML with a document extension
- **WHEN** a user uploads HTML bytes named `notes.pdf`
- **THEN** the detected content type is not trusted as a safe inline type
- **AND** the download is forced as an attachment

#### Scenario: Filename injection attempt
- **WHEN** a filename contains CR, LF, path separators, or control characters
- **THEN** the stored filename is sanitized
- **AND** the response header cannot be split or redirected by the filename

### Requirement: Deletion and reference counting
Deleting a file SHALL remove only the current site's reference. The stored object and object row SHALL be deleted only when no reference remains. Only the owner or a site administrator SHALL delete a reference.

#### Scenario: Owner deletes a document
- **WHEN** the owner deletes a file reference
- **THEN** the current site's reference is removed
- **AND** the public identifier no longer resolves on that site

#### Scenario: Other user attempts deletion
- **WHEN** a user who does not own the file and is not a site administrator deletes it
- **THEN** the request returns 403
- **AND** the reference remains

#### Scenario: Last reference removed
- **WHEN** the last reference to an object is deleted
- **THEN** the S3 object and object row are deleted

#### Scenario: Shared reference remains
- **WHEN** a site deletes a reference while another site still references the object
- **THEN** the S3 object remains
- **AND** the other site's URL still resolves

### Requirement: File listing
`GET /api/v4/file/list` SHALL return only the authenticated user's file references for the current site. It MUST NOT return another site's references.

#### Scenario: List current site files
- **WHEN** an authenticated user lists files on site A
- **THEN** the response contains site A references owned by that user
- **AND** it does not contain site B references or other users' files

### Requirement: Limits, configuration, and failure behavior
Per-site settings SHALL control whether document uploads are enabled and the maximum document size. Exceeding the limit MUST reject the request without storing a partial object. When S3 is unavailable or unconfigured, uploads MUST fail with a storage or disabled error and MUST NOT create a local fallback.

#### Scenario: Upload exceeds the size limit
- **WHEN** a user uploads a document larger than the current site's limit
- **THEN** the request returns 413
- **AND** no object or reference is created

#### Scenario: Uploads disabled on the site
- **WHEN** a site administrator disables document uploads
- **THEN** `POST /api/v4/file` returns a disabled error
- **AND** existing documents remain readable

#### Scenario: S3 unavailable
- **WHEN** the S3 service is down during upload
- **THEN** the request returns a storage error
- **AND** no local file is created as a fallback

#### Scenario: No S3 configuration
- **WHEN** the installation has no S3 document configuration
- **THEN** document uploads are disabled
- **AND** the server still starts and serves existing image APIs
