# Spec Delta

## ADDED Requirements

### Requirement: Multiple password verifier formats
Local account authentication SHALL verify supported legacy PBKDF2 and bcrypt password hash formats as well as Argon2id. New passwords SHALL use Argon2id, and a successful login using a legacy verifier SHALL replace it with an Argon2id verifier without changing the account identity or site scope.

#### Scenario: Authenticate with imported PBKDF2 verifier
- **WHEN** a site-scoped local user presents a password matching a supported Discourse PBKDF2 verifier
- **THEN** login succeeds and the stored verifier is upgraded to Argon2id

#### Scenario: Authenticate with Argon2id verifier
- **WHEN** a local user presents a password matching an Argon2id verifier
- **THEN** login succeeds without changing the verifier unless its parameters require rehashing

#### Scenario: Reject unknown verifier format
- **WHEN** the stored verifier uses an unsupported algorithm or malformed encoding
- **THEN** login fails safely without revealing verifier details or changing the stored value
