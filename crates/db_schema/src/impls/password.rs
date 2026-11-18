use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use subtle::ConstantTimeEq;

const DISCOURSE_PBKDF2_PREFIX: &str = "$discourse-pbkdf2-sha256$";
const MAX_PBKDF2_ITERATIONS: u32 = 5_000_000;
const MAX_BCRYPT_COST: u32 = 16;
const MAX_ARGON2_MEMORY_KIB: u32 = 65_536;
const MAX_ARGON2_ITERATIONS: u32 = 4;
const MAX_ARGON2_PARALLELISM: u32 = 4;

/// Encode the separate fields stored by Discourse's `user_passwords` table in
/// the self-identifying form consumed by `verify_password`.
pub fn encode_discourse_pbkdf2_verifier(
  algorithm: &str,
  salt_hex: &str,
  digest_hex: &str,
) -> Option<String> {
  let algorithm = algorithm.strip_prefix('$')?.strip_suffix('$')?;
  let (name, params) = algorithm.split_once('$')?;
  if name != "pbkdf2-sha256"
    || salt_hex.len() != 32
    || digest_hex.len() != 64
    || !salt_hex.bytes().all(|b| b.is_ascii_hexdigit())
    || !digest_hex.bytes().all(|b| b.is_ascii_hexdigit())
  {
    return None;
  }
  let mut iterations = None;
  let mut output_len = None;
  for param in params.split(',') {
    let (key, value) = param.split_once('=')?;
    match key {
      "i" => iterations = value.parse::<u32>().ok(),
      "l" => output_len = value.parse::<usize>().ok(),
      _ => return None,
    }
  }
  let iterations = iterations.filter(|i| (1..=MAX_PBKDF2_ITERATIONS).contains(i))?;
  if output_len != Some(32) {
    return None;
  }
  Some(format!(
    "{DISCOURSE_PBKDF2_PREFIX}i={iterations},l=32${salt_hex}${digest_hex}"
  ))
}

/// Hash new local-user passwords with Argon2id.
pub fn hash_password(password: &str) -> LemmyResult<String> {
  let salt = SaltString::generate(&mut OsRng);
  Argon2::default()
    .hash_password(password.as_bytes(), &salt)
    .map(|hash| hash.to_string())
    .map_err(|_| LemmyErrorType::CouldntCreate.into())
}

/// Verify the supported local-user formats. Discourse hashes are imported as
/// `$discourse-pbkdf2-sha256$i=<iterations>,l=32$<hex salt>$<hex digest>`.
pub fn verify_password(password: &str, stored: &str) -> bool {
  if is_supported_bcrypt_verifier(stored) {
    return bcrypt::verify(password, stored).unwrap_or(false);
  }

  if stored.starts_with("$argon2id$") {
    return PasswordHash::new(stored)
      .ok()
      .filter(is_supported_argon2id_verifier)
      .is_some_and(|parsed| {
        Argon2::default()
          .verify_password(password.as_bytes(), &parsed)
          .is_ok()
      });
  }

  verify_discourse_pbkdf2(password, stored)
}

pub fn is_legacy_password_hash(stored: &str) -> bool {
  stored.starts_with("$2") || stored.starts_with(DISCOURSE_PBKDF2_PREFIX)
}

/// Recognize only supported encoded password verifiers before migration inserts them.
pub fn is_supported_password_verifier(stored: &str) -> bool {
  if stored.starts_with("$2") {
    return is_supported_bcrypt_verifier(stored);
  }
  if stored.starts_with("$argon2id$") {
    return PasswordHash::new(stored)
      .ok()
      .is_some_and(|parsed| is_supported_argon2id_verifier(&parsed));
  }
  let Some(encoded) = stored.strip_prefix(DISCOURSE_PBKDF2_PREFIX) else {
    return false;
  };
  let mut fields = encoded.split('$');
  let (Some(params), Some(salt), Some(digest), None) =
    (fields.next(), fields.next(), fields.next(), fields.next())
  else {
    return false;
  };
  encode_discourse_pbkdf2_verifier(&format!("$pbkdf2-sha256${params}$"), salt, digest).as_deref()
    == Some(stored)
}

fn is_supported_bcrypt_verifier(stored: &str) -> bool {
  let bytes = stored.as_bytes();
  if bytes.len() != 60
    || ![b"$2a$", b"$2b$", b"$2x$", b"$2y$"]
      .iter()
      .any(|prefix| bytes.starts_with(*prefix))
    || bytes[6] != b'$'
  {
    return false;
  }
  let Ok(cost_text) = std::str::from_utf8(&bytes[4..6]) else {
    return false;
  };
  let Ok(cost) = cost_text.parse::<u32>() else {
    return false;
  };
  (4..=MAX_BCRYPT_COST).contains(&cost)
    && bytes[7..]
      .iter()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/'))
}

fn is_supported_argon2id_verifier(parsed: &PasswordHash<'_>) -> bool {
  parsed.algorithm.as_str() == "argon2id"
    && parsed.version == Some(19)
    && parsed.salt.is_some()
    && parsed.hash.is_some()
    && parsed
      .params
      .get_decimal("m")
      .is_some_and(|memory| (8..=MAX_ARGON2_MEMORY_KIB).contains(&memory))
    && parsed
      .params
      .get_decimal("t")
      .is_some_and(|iterations| (1..=MAX_ARGON2_ITERATIONS).contains(&iterations))
    && parsed
      .params
      .get_decimal("p")
      .is_some_and(|parallelism| (1..=MAX_ARGON2_PARALLELISM).contains(&parallelism))
}

fn verify_discourse_pbkdf2(password: &str, stored: &str) -> bool {
  let Some(encoded) = stored.strip_prefix(DISCOURSE_PBKDF2_PREFIX) else {
    return false;
  };
  let mut fields = encoded.split('$');
  let Some(params) = fields.next() else {
    return false;
  };
  let Some(salt) = fields.next() else {
    return false;
  };
  let Some(expected_hex) = fields.next() else {
    return false;
  };
  if fields.next().is_some() || salt.len() != 32 || expected_hex.len() != 64 {
    return false;
  }

  let mut iterations = None;
  let mut output_len = None;
  for param in params.split(',') {
    let Some((key, value)) = param.split_once('=') else {
      return false;
    };
    match key {
      "i" => iterations = value.parse::<u32>().ok(),
      "l" => output_len = value.parse::<usize>().ok(),
      _ => return false,
    }
  }
  let Some(iterations) = iterations.filter(|i| (1..=MAX_PBKDF2_ITERATIONS).contains(i)) else {
    return false;
  };
  if output_len != Some(32) || !salt.bytes().all(|b| b.is_ascii_hexdigit()) {
    return false;
  }
  let Ok(expected) = hex::decode(expected_hex) else {
    return false;
  };

  let mut actual = [0_u8; 32];
  pbkdf2_hmac::<Sha256>(
    password.as_bytes(),
    salt.as_bytes(),
    iterations,
    &mut actual,
  );
  bool::from(actual.as_slice().ct_eq(expected.as_slice()))
}

use lemmy_utils::error::{LemmyErrorType, LemmyResult};

#[cfg(test)]
mod tests {
  use super::{encode_discourse_pbkdf2_verifier, hash_password, verify_password};

  #[test]
  fn verifies_discourse_pbkdf2_fixture() {
    let verifier = encode_discourse_pbkdf2_verifier(
      "$pbkdf2-sha256$i=3,l=32$",
      "abcdabcdabcdabcdabcdabcdabcdabcd",
      "7625aab55e5836fbc64f5eadcee14863ec11e2b3cf139c95dccc87bf043d8ac7",
    );
    assert!(verifier.is_some());
    if let Some(verifier) = verifier {
      assert!(verify_password("mypass", &verifier));
      assert!(!verify_password("wrong", &verifier));
    }
    assert!(!verify_password(
      "mypass",
      "$discourse-pbkdf2-sha256$i=999999999,l=32$x$y"
    ));
  }

  #[test]
  fn new_hashes_use_argon2id() {
    let verifier = hash_password("a long enough password");
    assert!(verifier.is_ok());
    if let Ok(verifier) = verifier {
      assert!(verifier.starts_with("$argon2id$"));
      assert!(verify_password("a long enough password", &verifier));
      assert!(!verify_password("incorrect", &verifier));
    }
  }
}
