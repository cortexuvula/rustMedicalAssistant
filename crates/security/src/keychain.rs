//! Cross-platform OS-keychain wrapper for the database encryption key.
//!
//! Uses the `keyring` crate v3 with platform-native backends:
//! - macOS: Security framework (Keychain Services) via `apple-native`
//! - Windows: Credential Manager via `windows-native`
//! - Linux: libsecret / Secret Service via `sync-secret-service`
//!
//! For tests, call `keyring::set_default_credential_builder(...)` with the
//! mock builder before invoking these functions to isolate test runs.

use keyring::Entry;
use rand::RngCore;

/// Service name used in the OS keychain. Exposed for tests and manual
/// inspection (e.g. `security find-generic-password -s rustMedicalAssistant -a db-key`).
pub const KEYCHAIN_SERVICE: &str = "rustMedicalAssistant";
/// Account name used to identify the database encryption key.
pub const KEYCHAIN_DB_KEY_ACCOUNT: &str = "db-key";

/// Errors returned by the keychain wrapper.
#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    #[error("keychain access denied or unavailable: {0}")]
    Access(String),
    #[error("keychain entry malformed: {0}")]
    Malformed(String),
    #[error("entropy source failed: {0}")]
    Entropy(String),
}

pub type KeychainResult<T> = Result<T, KeychainError>;

/// Read the database key from the keychain. Returns `Ok(None)` if no entry
/// exists yet (caller may want to generate one).
pub fn get_db_key() -> KeychainResult<Option<[u8; 32]>> {
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_DB_KEY_ACCOUNT)
        .map_err(|e| KeychainError::Access(e.to_string()))?;
    match entry.get_secret() {
        Ok(bytes) => {
            if bytes.len() != 32 {
                return Err(KeychainError::Malformed(format!(
                    "expected 32 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Ok(Some(key))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(KeychainError::Access(e.to_string())),
    }
}

/// Get the existing database key from the keychain, or generate and store a
/// new random 32-byte key if none exists.
pub fn get_or_create_db_key() -> KeychainResult<[u8; 32]> {
    if let Some(key) = get_db_key()? {
        return Ok(key);
    }
    let mut key = [0u8; 32];
    rand::thread_rng()
        .try_fill_bytes(&mut key)
        .map_err(|e| KeychainError::Entropy(e.to_string()))?;
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_DB_KEY_ACCOUNT)
        .map_err(|e| KeychainError::Access(e.to_string()))?;
    entry
        .set_secret(&key)
        .map_err(|e| KeychainError::Access(e.to_string()))?;
    Ok(key)
}

/// Remove the database key from the keychain. Used by the "Wipe and start
/// fresh" recovery path.
pub fn wipe_db_key() -> KeychainResult<()> {
    let entry = Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_DB_KEY_ACCOUNT)
        .map_err(|e| KeychainError::Access(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(KeychainError::Access(e.to_string())),
    }
}

/// Encode a 32-byte key as a 64-char hex string for `PRAGMA key="x'<hex>'"`.
pub fn key_to_hex(key: &[u8; 32]) -> String {
    hex::encode(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Configure keyring to use the in-process mock backend so tests don't
    /// touch the real OS keychain. Note that the mock is `EntryOnly`
    /// persistence — every call to `Entry::new(SERVICE, ACCOUNT)` returns a
    /// fresh empty credential rather than sharing state. That makes
    /// cross-call persistence tests impossible to write at the unit level;
    /// real persistence is verified by the integration tests in Task 4 and
    /// by manual smoke testing on each platform.
    fn use_mock_backend() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
    }

    #[test]
    fn get_db_key_returns_none_when_absent() {
        use_mock_backend();
        // Each Entry::new() in the mock backend yields a fresh empty
        // credential, so any first read sees NoEntry → our wrapper maps
        // that to Ok(None).
        let result = get_db_key().expect("read");
        assert!(result.is_none(), "expected None on empty keychain, got Some");
    }

    #[test]
    fn key_to_hex_produces_64_chars() {
        let key = [0xABu8; 32];
        let hex_str = key_to_hex(&key);
        assert_eq!(hex_str.len(), 64);
        assert_eq!(
            hex_str,
            "abababababababababababababababababababababababababababababababab"
        );
    }
}
