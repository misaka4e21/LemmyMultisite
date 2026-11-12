## ADDED Requirements

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

## MODIFIED Requirements

### Requirement: Deduplicated upload
The upload path SHALL hash only the file bytes while streaming them to pict-rs. When the hash already exists, the system SHALL reuse the canonical alias, remove any non-canonical alias created by the upload, and end with one stored object.

#### Scenario: Upload duplicate
- **WHEN** a user uploads bytes whose hash already exists
- **THEN** the system returns the canonical alias
- **AND** it creates only a new site reference
- **AND** it removes the temporary duplicate alias
