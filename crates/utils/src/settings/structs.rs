use super::pictrs_placeholder_url;
use doku::Document;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::{
  collections::BTreeMap,
  env,
  net::{IpAddr, Ipv4Addr},
};
use url::Url;

#[derive(Debug, Deserialize, Serialize, Clone, SmartDefault, Document)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
  /// settings related to the postgresql database
  pub database: DatabaseConfig,
  /// Pictrs image server configuration.
  #[default(Some(Default::default()))]
  pub(crate) pictrs: Option<PictrsConfig>,
  /// Optional S3-compatible storage for generic file uploads. Each value can also be set with the
  /// matching `LEMMY_S3_*` environment variable.
  #[doku(example = "Some(Default::default())")]
  pub s3: Option<S3Config>,
  /// Email sending configuration. All options except login/password are mandatory
  #[doku(example = "Some(Default::default())")]
  pub email: Option<EmailConfig>,
  /// Parameters for automatic configuration of new instance (only used at first start)
  #[doku(example = "Some(Default::default())")]
  pub setup: Option<SetupConfig>,
  /// Address where lemmy should listen for incoming requests
  #[default(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)))]
  #[doku(as = "String")]
  pub bind: IpAddr,
  /// Port where lemmy should listen for incoming requests
  #[default(8536)]
  pub port: u16,
  /// Whether the site is available over TLS. Needs to be true for federation to work.
  #[default(true)]
  pub tls_enabled: bool,
  /// Set the URL for opentelemetry exports. If you do not have an opentelemetry collector, do not
  /// set this option
  #[doku(skip)]
  pub opentelemetry_url: Option<Url>,
  pub federation: FederationWorkerConfig,
  // Prometheus configuration.
  #[doku(example = "Some(Default::default())")]
  pub prometheus: Option<PrometheusConfig>,
  /// Sets a response Access-Control-Allow-Origin CORS header. Can also be set via environment:
  /// `LEMMY_CORS_ORIGIN=example.org,site.com`
  /// https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Origin
  #[doku(example = "lemmy.tld")]
  cors_origin: Vec<String>,
  /// Print logs in JSON format. You can also disable ANSI colors in logs with env var `NO_COLOR`.
  pub json_logging: bool,
  /// Data for loading Lemmy plugins
  pub plugins: Vec<PluginSettings>,
}

impl Settings {
  pub fn cors_origin(&self) -> Vec<String> {
    env::var("LEMMY_CORS_ORIGIN")
      .ok()
      .map(|e| e.split(',').map(ToString::to_string).collect())
      .unwrap_or(self.cors_origin.clone())
  }

  /// Resolve S3 settings from the config file and environment. Incomplete configuration disables
  /// generic file uploads while allowing Lemmy to start normally.
  pub fn s3(&self) -> Option<ResolvedS3Config> {
    resolve_s3_config(
      self.s3.clone().unwrap_or_default(),
      S3Environment::from_process(),
    )
  }
}

#[derive(Default)]
struct S3Environment {
  endpoint: Option<String>,
  bucket: Option<String>,
  region: Option<String>,
  access_key: Option<String>,
  secret_key: Option<String>,
  use_path_style: Option<bool>,
}

impl S3Environment {
  fn from_process() -> Self {
    Self {
      endpoint: env::var("LEMMY_S3_ENDPOINT").ok(),
      bucket: env::var("LEMMY_S3_BUCKET").ok(),
      region: env::var("LEMMY_S3_REGION").ok(),
      access_key: env::var("LEMMY_S3_ACCESS_KEY").ok(),
      secret_key: env::var("LEMMY_S3_SECRET_KEY").ok(),
      use_path_style: env::var("LEMMY_S3_USE_PATH_STYLE")
        .ok()
        .and_then(|value| value.parse().ok()),
    }
  }
}

fn resolve_s3_config(mut config: S3Config, environment: S3Environment) -> Option<ResolvedS3Config> {
  config.endpoint = environment.endpoint.or(config.endpoint);
  config.bucket = environment.bucket.or(config.bucket);
  config.region = environment.region.or(config.region);
  config.access_key = environment.access_key.or(config.access_key);
  config.secret_key = environment.secret_key.or(config.secret_key);
  config.use_path_style = environment.use_path_style.or(config.use_path_style);
  Some(ResolvedS3Config {
    endpoint: required_nonempty(config.endpoint)?,
    bucket: required_nonempty(config.bucket)?,
    region: required_nonempty(config.region)?,
    access_key: required_nonempty(config.access_key)?,
    secret_key: required_nonempty(config.secret_key)?,
    use_path_style: config.use_path_style?,
  })
}

fn required_nonempty(value: Option<String>) -> Option<String> {
  value.filter(|value| !value.trim().is_empty())
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, Document)]
#[serde(default, deny_unknown_fields)]
pub struct S3Config {
  /// S3-compatible API endpoint. Environment override: `LEMMY_S3_ENDPOINT`.
  #[doku(example = "http://localhost:9000")]
  pub endpoint: Option<String>,
  /// Dedicated document bucket. Environment override: `LEMMY_S3_BUCKET`.
  #[doku(example = "lemmy-files")]
  pub bucket: Option<String>,
  /// SigV4 region. Environment override: `LEMMY_S3_REGION`.
  #[doku(example = "us-east-1")]
  pub region: Option<String>,
  /// Static access key. Environment override: `LEMMY_S3_ACCESS_KEY`.
  #[doku(example = "replace-me")]
  pub access_key: Option<String>,
  /// Static secret key. Environment override: `LEMMY_S3_SECRET_KEY`.
  #[doku(example = "replace-me")]
  pub secret_key: Option<String>,
  /// Use path-style bucket addressing. Environment override: `LEMMY_S3_USE_PATH_STYLE`.
  #[doku(example = "true")]
  pub use_path_style: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ResolvedS3Config {
  pub endpoint: String,
  pub bucket: String,
  pub region: String,
  pub access_key: String,
  pub secret_key: String,
  pub use_path_style: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, SmartDefault, Document)]
#[serde(default, deny_unknown_fields)]
pub struct PictrsConfig {
  /// Address where pictrs is available (for image hosting)
  #[default(pictrs_placeholder_url())]
  #[doku(example = "http://localhost:8080")]
  pub url: Url,

  /// Set a custom pictrs API key. ( Required for deleting images )
  pub api_key: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, SmartDefault, Document)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseConfig {
  /// Configure the database by specifying URI pointing to a postgres instance. This parameter can
  /// also be set by environment variable `LEMMY_DATABASE_URL`.
  ///
  /// For an explanation of how to use connection URIs, see PostgreSQL's documentation:
  /// https://www.postgresql.org/docs/current/libpq-connect.html#id-1.7.3.8.3.6
  #[default("postgres://lemmy:password@localhost:5432/lemmy")]
  #[doku(example = "postgresql:///lemmy?user=lemmy&host=/var/run/postgresql")]
  pub(crate) connection: String,

  /// Maximum number of active sql connections
  ///
  /// A high value here can result in errors "could not resize shared memory segment". In this case
  /// it is necessary to increase shared memory size in Docker: https://stackoverflow.com/a/56754077
  #[default(30)]
  pub pool_size: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone, Document, SmartDefault)]
#[serde(default, deny_unknown_fields)]
pub struct EmailConfig {
  /// https://docs.rs/lettre/0.11.14/lettre/transport/smtp/struct.AsyncSmtpTransport.html#method.from_url
  #[default("smtp://localhost:25")]
  #[doku(example = "smtps://user:pass@hostname:port")]
  pub connection: String,
  /// Address to send emails from, eg "noreply@your-instance.com"
  #[doku(example = "noreply@example.com")]
  pub smtp_from_address: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, SmartDefault, Document)]
#[serde(default, deny_unknown_fields)]
pub struct SetupConfig {
  /// Hostname used to create the first site (mandatory on first start)
  #[default("unset")]
  #[doku(example = "example.com")]
  pub hostname: String,
  /// Username for the admin user
  #[doku(example = "admin")]
  pub admin_username: Option<String>,
  /// Password for the admin user. It must be between 10 and 60 characters.
  #[doku(example = "tf6HHDS4RolWfFhk4Rq9")]
  pub admin_password: Option<String>,
  /// Name of the site, can be changed later. Maximum 20 characters. Only used if `admin_username`
  /// and `admin_password` are set.
  #[doku(example = "My Lemmy Instance")]
  pub site_name: Option<String>,
  /// Email for the admin user (optional, can be omitted and set later through the website). Only
  /// used if `admin_username` and `admin_password` are set.
  #[doku(example = "user@example.com")]
  pub admin_email: Option<String>,
  /// On first start Lemmy fetches the 50 most active communities from one of these instances,
  /// to provide some initial data. It tries the first list entry, and if it fails uses subsequent
  /// instances as fallback.
  /// Leave this empty to disable community bootstrap.
  /// TODO: remove voyager.lemmy.ml from defaults once Lemmy 1.0 is deployed to production
  /// instances.
  #[default(vec!["lemmy.ml".to_string(),"lemmy.world".to_string(),"lemmy.zip".to_string(),"voyager.lemmy.ml".to_string()])]
  pub bootstrap_instances: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, SmartDefault, Document)]
#[serde(default, deny_unknown_fields)]
pub struct PrometheusConfig {
  // Address that the Prometheus metrics will be served on.
  #[default(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)))]
  #[doku(example = "127.0.0.1")]
  pub bind: IpAddr,
  // Port that the Prometheus metrics will be served on.
  #[default(10002)]
  #[doku(example = "10002")]
  pub port: u16,
}

#[derive(Debug, Deserialize, Serialize, Clone, SmartDefault, Document)]
#[serde(default, deny_unknown_fields)]
// named federation"worker"config to disambiguate from the activitypub library configuration
pub struct FederationWorkerConfig {
  /// Limit to the number of concurrent outgoing federation requests per target instance.
  /// Set this to a higher value than 1 (e.g. 6) only if you have a huge instance (>10 activities
  /// per second) and if a receiving instance is not keeping up.
  #[default(1)]
  pub concurrent_sends_per_instance: i8,
}

/// See the extism docs for more details: https://extism.org/docs/concepts/manifest
#[derive(Debug, Deserialize, Serialize, Clone, SmartDefault, Document)]
#[serde(default, deny_unknown_fields)]
pub struct PluginSettings {
  /// Where to load the .wasm file from, can be a local file path or URL
  #[doku(
    example = "https://github.com/LemmyNet/lemmy-plugins/releases/download/0.1.1/go_replace_words.wasm"
  )]
  pub file: String,
  /// SHA256 hash of the .wasm file
  #[doku(example = "37cdc01a3ff26eef578b668c6cc57fc06649deddb3a92cb6bae8e79b4e60fe12")]
  pub hash: Option<String>,
  /// Which websites the plugin may connect to
  #[serde(default)]
  #[doku(example = "lemmy.ml")]
  pub allowed_hosts: Option<Vec<String>>,
  /// Configuration options for the plugin
  #[serde(default)]
  pub config: BTreeMap<String, String>,
}

#[cfg(test)]
mod s3_settings_tests {
  use super::{S3Config, S3Environment, resolve_s3_config};

  fn complete_config() -> S3Config {
    S3Config {
      endpoint: Some("http://localhost:9000".to_string()),
      bucket: Some("lemmy-files".to_string()),
      region: Some("us-east-1".to_string()),
      access_key: Some("access-key".to_string()),
      secret_key: Some("secret-key".to_string()),
      use_path_style: Some(true),
    }
  }

  #[test]
  fn complete_s3_block_resolves() {
    let resolved = resolve_s3_config(complete_config(), S3Environment::default()).unwrap();

    assert_eq!(resolved.endpoint, "http://localhost:9000");
    assert_eq!(resolved.bucket, "lemmy-files");
    assert_eq!(resolved.region, "us-east-1");
    assert_eq!(resolved.access_key, "access-key");
    assert_eq!(resolved.secret_key, "secret-key");
    assert!(resolved.use_path_style);
  }

  #[test]
  fn partial_s3_block_stays_disabled() {
    let partial = S3Config {
      endpoint: Some("http://localhost:9000".to_string()),
      ..Default::default()
    };

    assert!(resolve_s3_config(partial, S3Environment::default()).is_none());
  }

  #[test]
  fn missing_s3_block_stays_disabled() {
    assert!(resolve_s3_config(S3Config::default(), S3Environment::default()).is_none());
  }

  #[test]
  fn environment_values_override_the_s3_block() {
    let environment = S3Environment {
      endpoint: Some("http://s3.example.test".to_string()),
      ..Default::default()
    };
    let resolved = resolve_s3_config(complete_config(), environment).unwrap();

    assert_eq!(resolved.endpoint, "http://s3.example.test");
  }
}
