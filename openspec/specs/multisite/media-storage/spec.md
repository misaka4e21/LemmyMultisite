# Media Storage Specification

## Purpose

Store uploaded files once by content hash while letting each site use its own hostname, access policy, and lifecycle.

## Requirements

### Requirement: Content-addressed media object
The system SHALL store one media object per content hash. The media object SHALL hold the canonical pict-rs alias, size, dimensions, content type, and blurhash. Uploading identical bytes SHALL reuse the existing media object.

#### Scenario: Same image uploaded twice
- **WHEN** a user on site A uploads an image
- **AND** a user on site B uploads the same bytes
- **THEN** the system stores one media object
- **AND** the two sites hold separate references

### Requirement: Site media reference
Each use of media SHALL have a reference row with `local_site_id`, media object, owner, and purpose. A site's media list SHALL return only its own references.

#### Scenario: List media
- **WHEN** site A lists uploaded media
- **THEN** the response contains site A's references
- **AND** it does not contain site B's references

### Requirement: Per-site media URL
The system SHALL render media URLs as `https://{site_hostname}/api/v4/image/{alias}`. The pict-rs backend SHALL be shared. Stored canonical object rows MUST NOT contain a site-specific media URL.

#### Scenario: Two sites serve the same alias
- **WHEN** site A and site B both reference the same media object
- **THEN** site A serves it at `https://a.example/api/v4/image/{alias}`
- **AND** site B serves it at `https://b.example/api/v4/image/{alias}`

### Requirement: Deduplicated upload
The upload path SHALL hash only the file bytes while streaming them to pict-rs. When the hash already exists, the system SHALL reuse the canonical alias, remove any non-canonical alias created by the upload, and end with one stored object.

#### Scenario: Upload duplicate
- **WHEN** a user uploads bytes whose hash already exists
- **THEN** the system returns the canonical alias
- **AND** it creates only a new site reference
- **AND** it removes the temporary duplicate alias

### Requirement: Reference-counted deletion
Deleting media SHALL remove only the current site's reference. The system SHALL delete the pict-rs alias only when no reference remains. Installation administrator purge MAY remove all references and the media object.

#### Scenario: Delete one site's reference
- **WHEN** site A deletes an image that site B still uses
- **THEN** site B's image remains available
- **AND** the pict-rs alias remains

#### Scenario: Last reference removed
- **WHEN** the last reference to a media object is deleted
- **THEN** the system removes the pict-rs alias and the media object

### Requirement: Remote image cache
Remote images SHALL be tracked by source URL at installation level. Image details SHALL be shared by source or media hash. Proxy URLs SHALL be rendered per site.

#### Scenario: Same remote image
- **WHEN** site A and site B proxy the same remote image
- **THEN** the system stores one remote image record
- **AND** each site renders its own proxy URL

### Requirement: Media access control
The image endpoint SHALL apply the current site's private instance policy. A site MUST NOT enumerate another site's media references.

#### Scenario: Private site
- **WHEN** an anonymous client requests an image from a private site
- **THEN** the system returns 401

### Requirement: S3-compatible blob storage
pict-rs SHALL store media blobs in an S3-compatible bucket. The installation MUST configure the endpoint, bucket name, region, access key, secret key, and path-style addressing through environment variables or secret files. The backend MUST work with any S3-compatible service and MUST NOT depend on RustFS-specific APIs.

#### Scenario: Custom S3-compatible endpoint
- **WHEN** the installation sets an S3 endpoint that is not AWS
- **THEN** pict-rs stores new media in the configured bucket
- **AND** Lemmy serves the uploaded media through its existing image endpoints

#### Scenario: Path-style addressing required
- **WHEN** the S3-compatible provider requires path-style bucket addressing
- **THEN** enabling the path-style setting makes uploads and reads succeed

#### Scenario: RustFS development backend
- **WHEN** the Docker development stack starts with its default settings
- **THEN** pict-rs uses the RustFS service as its S3-compatible backend

### Requirement: Bucket readiness
The media bucket SHALL exist before pict-rs starts. The deployment SHALL create the bucket idempotently during initialization and MUST fail with a clear error when the bucket cannot be created or reached.

#### Scenario: First start creates the bucket
- **WHEN** the deployment starts with an empty S3 service
- **THEN** initialization creates the configured media bucket
- **AND** pict-rs starts only after the bucket is available

#### Scenario: Existing bucket is reused
- **WHEN** the configured bucket already exists
- **THEN** initialization succeeds without deleting or recreating it

#### Scenario: Bucket unavailable
- **WHEN** the bucket cannot be created or reached
- **THEN** pict-rs does not start serving uploads
- **AND** the failure identifies the bucket or endpoint

### Requirement: Persistent alias metadata
pict-rs alias metadata SHALL remain on a persistent local volume while blob bytes live in S3. Recreating the pict-rs container MUST NOT lose the mapping between media aliases and stored objects when the metadata volume is preserved.

#### Scenario: Container restart
- **WHEN** the pict-rs container is recreated with the same metadata volume and bucket
- **THEN** existing aliases still resolve to their objects
- **AND** previously uploaded images remain readable

### Requirement: Existing media migration
The deployment SHALL provide a documented, offline migration from the existing filesystem store to S3-compatible storage. The migration MUST back up the existing metadata and files, preserve aliases, and support rollback to the filesystem store if it fails.

#### Scenario: Migrate existing filesystem media
- **WHEN** an operator stops pict-rs and runs the documented migration with the existing metadata directory and target bucket
- **THEN** previously uploaded aliases resolve from the bucket
- **AND** their Lemmy URLs remain unchanged

#### Scenario: Migration failure
- **WHEN** the migration fails before completion
- **THEN** the operator can restore the backed-up metadata and filesystem store
- **AND** the previous deployment serves media again

#### Scenario: Upload after migration
- **WHEN** a user uploads a new image after migration
- **THEN** the blob is written to S3
- **AND** the alias is recorded in the persistent metadata store

### Requirement: Startup and failure ordering
The deployment SHALL order startup so the S3 service is reachable, the bucket exists, pict-rs is healthy, and only then Lemmy starts. When S3 is unavailable, pict-rs MUST report an unhealthy state and MUST NOT silently fall back to local blob storage.

#### Scenario: S3 unavailable during startup
- **WHEN** the S3 service is not reachable
- **THEN** pict-rs does not report healthy
- **AND** Lemmy does not start serving image uploads that would be written to an unconfigured store

#### Scenario: S3 becomes unavailable at runtime
- **WHEN** an upload is attempted while the S3 service is down
- **THEN** the upload returns a storage error
- **AND** no local blob is silently created as a fallback

### Requirement: Image API compatibility
Moving blobs to S3 SHALL NOT change Lemmy's image API or per-site media URL behavior. Avatar, banner, site icon, community icon, and post image uploads SHALL continue to return the same response shape, and stored media SHALL remain accessible through the current site hostname.

#### Scenario: Avatar upload
- **WHEN** a user uploads an avatar through the existing endpoint
- **THEN** the response contains the same image URL and filename fields as before
- **AND** the image is served through the current site's `/api/v4/image/{alias}` route

#### Scenario: Credentials not exposed
- **WHEN** a client reads an image or an upload response
- **THEN** the response does not contain S3 access keys, secret keys, or bucket credentials
