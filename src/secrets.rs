//! Secrets management: age-encrypted, passphrase-protected secret storage.
//!
//! Secrets are stored in a single encrypted file (`secrets.enc`) that can be synced
//! via git alongside `schalentier.toml`. The passphrase that decrypts it is typed once
//! per machine and then cached in the OS keyring, so every subsequent command reads the
//! keyring silently instead of prompting again.

use crate::error::{Result, SchalentierError};
use crate::state::{default_data_dir, ensure_data_dir};
use age::secrecy::SecretString;
use anyhow::Context;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::debug;

const KEYRING_SERVICE: &str = "schalentier";
const KEYRING_USERNAME: &str = "master-password";
const SECRETS_FILE_NAME: &str = "secrets.enc";
const FALLBACK_FILE_NAME: &str = "keystore.enc";
const FALLBACK_KEY_NAME: &str = "keystore.key";
const FALLBACK_PASSWORD_KEY: &str = "master-password";

/// A single stored secret: its value and the tags used to scope it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEntry {
    pub value: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// The decrypted contents of a secrets file: name -> entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretStore {
    #[serde(default)]
    pub secrets: HashMap<String, SecretEntry>,
}

impl SecretStore {
    /// Get the secrets matching any of the given tags (OR semantics).
    /// `None` (no filter) returns all secrets.
    pub fn filter_by_tags(&self, tags: Option<&[String]>) -> Vec<(&str, &SecretEntry)> {
        match tags {
            None => self
                .secrets
                .iter()
                .map(|(name, entry)| (name.as_str(), entry))
                .collect(),
            Some(tags) => self
                .secrets
                .iter()
                .filter(|(_, entry)| entry.tags.iter().any(|t| tags.contains(t)))
                .map(|(name, entry)| (name.as_str(), entry))
                .collect(),
        }
    }
}

/// Path to the secrets file in a given directory (typically the config dir).
pub fn secrets_file_path(dir: &Path) -> std::path::PathBuf {
    dir.join(SECRETS_FILE_NAME)
}

/// A place to persist the master password: the platform-native keyring
/// (Keychain/Credential Manager/Secret Service), or a local encrypted file store when
/// no native store is available — e.g. headless Linux with no D-Bus Secret Service
/// running (CI, containers, minimal servers).
enum PasswordStore {
    Native(keyring::Entry),
    Fallback(FallbackStore),
}

impl PasswordStore {
    fn get_password(&self) -> Result<Option<String>> {
        match self {
            PasswordStore::Native(entry) => match entry.get_password() {
                Ok(password) => Ok(Some(password)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(anyhow::anyhow!(SchalentierError::NoMasterPassword)).context(e),
            },
            PasswordStore::Fallback(store) => store.get_password(),
        }
    }

    fn set_password(&self, password: &str) -> Result<()> {
        match self {
            PasswordStore::Native(entry) => entry
                .set_password(password)
                .context("Failed to store master password in keyring"),
            PasswordStore::Fallback(store) => store.set_password(password),
        }
    }
}

/// Open the platform-native keyring, falling back to a local encrypted file store when
/// no native store is available.
fn open_password_store() -> Result<PasswordStore> {
    match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME) {
        Ok(entry) => Ok(PasswordStore::Native(entry)),
        Err(_) => {
            debug!("Native OS keyring unavailable, falling back to local encrypted file store");
            Ok(PasswordStore::Fallback(FallbackStore::open_at(&default_data_dir()?)?))
        }
    }
}

/// A tiny `age`-encrypted flat file holding the master password, used when no native
/// OS keyring is available. Encrypted with a random, machine-local key generated on
/// first use and stored alongside it (0600 perms). This key never leaves the machine
/// and is unrelated to the user's master password — it just protects this fallback
/// store at rest, the way a real OS keyring would.
struct FallbackStore {
    file_path: PathBuf,
    passphrase: SecretString,
}

impl FallbackStore {
    fn open_at(data_dir: &Path) -> Result<Self> {
        ensure_data_dir(data_dir)?;

        let key_path = data_dir.join(FALLBACK_KEY_NAME);
        let hexkey = if key_path.exists() {
            std::fs::read_to_string(&key_path)
                .with_context(|| format!("Failed to read {}", key_path.display()))?
                .trim()
                .to_string()
        } else {
            let mut bytes = [0u8; 32];
            rand::rng().fill(&mut bytes);
            let hexkey = hex::encode(bytes);
            std::fs::write(&key_path, &hexkey)
                .with_context(|| format!("Failed to write {}", key_path.display()))?;
            set_secrets_file_permissions(&key_path)?;
            hexkey
        };

        Ok(Self {
            file_path: data_dir.join(FALLBACK_FILE_NAME),
            passphrase: SecretString::from(hexkey),
        })
    }

    fn get_password(&self) -> Result<Option<String>> {
        if !self.file_path.exists() {
            return Ok(None);
        }

        let armored = std::fs::read(&self.file_path)
            .with_context(|| format!("Failed to read {}", self.file_path.display()))?;
        let identity = age::scrypt::Identity::new(self.passphrase.clone());
        let plaintext = age::decrypt(&identity, &armored)
            .map_err(|e| SchalentierError::DecryptionFailed(e.to_string()))?;
        let map: HashMap<String, String> =
            serde_json::from_slice(&plaintext).context("Failed to parse fallback store")?;
        Ok(map.get(FALLBACK_PASSWORD_KEY).cloned())
    }

    fn set_password(&self, password: &str) -> Result<()> {
        let mut map = HashMap::new();
        map.insert(FALLBACK_PASSWORD_KEY.to_string(), password.to_string());
        let plaintext = serde_json::to_vec(&map).context("Failed to serialize fallback store")?;

        let recipient = age::scrypt::Recipient::new(self.passphrase.clone());
        let armored = age::encrypt_and_armor(&recipient, &plaintext)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {e}"))?;

        std::fs::write(&self.file_path, &armored)
            .with_context(|| format!("Failed to write {}", self.file_path.display()))?;
        set_secrets_file_permissions(&self.file_path)?;

        Ok(())
    }
}

/// Environment variable that, when set, supplies the master password directly and
/// bypasses the OS keyring entirely. Intended for non-interactive use (CI, containers,
/// smoke tests) where no keyring or TTY is available.
pub const MASTER_PASSWORD_ENV: &str = "SCHALENTIER_MASTER_PASSWORD";

/// Get (or interactively create) the master password for this machine.
///
/// Resolution order:
/// 1. `SCHALENTIER_MASTER_PASSWORD` env var, if set (never touches the keyring).
/// 2. The OS keyring (or its local fallback store, see [`open_keyring_entry`]).
/// 3. On first use with neither, prompt via `inquire::Password` and cache in the keyring.
pub fn get_or_create_master_password() -> Result<SecretString> {
    if let Ok(password) = std::env::var(MASTER_PASSWORD_ENV) {
        if !password.is_empty() {
            debug!("Master password loaded from {} env var", MASTER_PASSWORD_ENV);
            return Ok(SecretString::from(password));
        }
    }

    let store = open_password_store()?;

    match store.get_password()? {
        Some(password) => {
            debug!("Master password loaded from keyring");
            Ok(SecretString::from(password))
        }
        None => {
            print_first_time_banner();

            let password = inquire::Password::new("Create master password:")
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .with_validator(inquire::validator::MinLengthValidator::new(8))
                .with_help_message("Used to encrypt secrets.enc. Stored in this machine's OS keyring afterward.")
                .prompt()
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Password prompt failed: {e}\n\nNo TTY available? Set the {MASTER_PASSWORD_ENV} \
                         env var instead (used for CI/non-interactive runs)."
                    )
                })?;

            store.set_password(&password)?;

            Ok(SecretString::from(password))
        }
    }
}

fn print_first_time_banner() {
    println!("No master password found for this machine.");
    println!("This is a one-time setup: it will be cached in your OS keyring afterward.");
    println!();
}

/// Prompt for a password without touching the keyring (used by `change-password`).
pub fn prompt_password(message: &str) -> Result<SecretString> {
    let password = inquire::Password::new(message)
        .with_display_mode(inquire::PasswordDisplayMode::Masked)
        .without_confirmation()
        .prompt()
        .map_err(|e| anyhow::anyhow!("Password prompt failed: {e}"))?;
    Ok(SecretString::from(password))
}

/// Replace the cached master password in the keyring (used by `change-password`).
pub fn set_master_password(password: &str) -> Result<()> {
    open_password_store()?.set_password(password)
}

/// Load and decrypt the secret store from `path`. Returns an empty store if the file
/// doesn't exist yet.
pub fn load_store(path: &Path, password: &SecretString) -> Result<SecretStore> {
    if !path.exists() {
        debug!("Secrets file not found at {}, using empty store", path.display());
        return Ok(SecretStore::default());
    }

    let armored = std::fs::read(path)
        .with_context(|| format!("Failed to read secrets file: {}", path.display()))?;

    let identity = age::scrypt::Identity::new(password.clone());
    let plaintext = age::decrypt(&identity, &armored)
        .map_err(|e| SchalentierError::DecryptionFailed(e.to_string()))?;

    let store: SecretStore =
        serde_json::from_slice(&plaintext).context("Failed to parse decrypted secrets")?;

    Ok(store)
}

/// Encrypt and save the secret store to `path`, ASCII-armored so it stays diff-friendly
/// in git. Sets restrictive (0600) permissions on Unix.
pub fn save_store(path: &Path, store: &SecretStore, password: &SecretString) -> Result<()> {
    let plaintext = serde_json::to_vec(store).context("Failed to serialize secrets")?;

    let recipient = age::scrypt::Recipient::new(password.clone());
    let armored = age::encrypt_and_armor(&recipient, &plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {e}"))?;

    std::fs::write(path, &armored)
        .with_context(|| format!("Failed to write secrets file: {}", path.display()))?;

    set_secrets_file_permissions(path)?;

    Ok(())
}

fn set_secrets_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
    Ok(())
}

/// Resolve the environment variables for a scoped set of secrets (used by `secret shell`
/// and `secret run`). `None` (no tag filter) returns every secret.
pub fn resolve_scoped_env(
    store: &SecretStore,
    tags: Option<&[String]>,
) -> Vec<(String, String)> {
    store
        .filter_by_tags(tags)
        .into_iter()
        .map(|(name, entry)| (name.to_string(), entry.value.clone()))
        .collect()
}

/// Render a `KEY="value"` shell export line, escaping embedded quotes/backslashes.
pub fn shell_export_line(shell: &str, name: &str, value: &str) -> String {
    match shell {
        "fish" => format!("set -gx {} \"{}\";", name, escape_double_quotes(value)),
        _ => format!("export {}=\"{}\"", name, escape_double_quotes(value)),
    }
}

fn escape_double_quotes(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_password() -> SecretString {
        SecretString::from("test-passphrase-123".to_string())
    }

    #[test]
    fn env_var_overrides_master_password() {
        use age::secrecy::ExposeSecret;
        // SAFETY: single-threaded within this test; no other test reads this env var.
        unsafe { std::env::set_var(MASTER_PASSWORD_ENV, "from-env-123") };
        let pw = get_or_create_master_password().expect("env override should succeed");
        assert_eq!(pw.expose_secret(), "from-env-123");
        unsafe { std::env::remove_var(MASTER_PASSWORD_ENV) };
    }

    fn sample_store() -> SecretStore {
        let mut secrets = HashMap::new();
        secrets.insert(
            "GITHUB_TOKEN".to_string(),
            SecretEntry {
                value: "ghp_xxx".to_string(),
                tags: vec!["work".to_string(), "ci".to_string()],
            },
        );
        secrets.insert(
            "AWS_KEY".to_string(),
            SecretEntry {
                value: "aws_yyy".to_string(),
                tags: vec!["personal".to_string()],
            },
        );
        SecretStore { secrets }
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let path = secrets_file_path(temp_dir.path());
        let password = test_password();
        let store = sample_store();

        save_store(&path, &store, &password).unwrap();
        assert!(path.exists());

        let loaded = load_store(&path, &password).unwrap();
        assert_eq!(loaded.secrets.len(), 2);
        assert_eq!(loaded.secrets["GITHUB_TOKEN"].value, "ghp_xxx");
        assert_eq!(
            loaded.secrets["GITHUB_TOKEN"].tags,
            vec!["work".to_string(), "ci".to_string()]
        );
    }

    #[test]
    fn test_load_nonexistent_returns_empty_store() {
        let temp_dir = TempDir::new().unwrap();
        let path = secrets_file_path(temp_dir.path());
        let store = load_store(&path, &test_password()).unwrap();
        assert!(store.secrets.is_empty());
    }

    #[test]
    fn test_wrong_password_fails_to_decrypt() {
        let temp_dir = TempDir::new().unwrap();
        let path = secrets_file_path(temp_dir.path());
        let store = sample_store();

        save_store(&path, &store, &test_password()).unwrap();

        let wrong_password = SecretString::from("wrong-passphrase".to_string());
        let result = load_store(&path, &wrong_password);
        assert!(result.is_err());
    }

    #[test]
    fn test_filter_by_tags_or_semantics() {
        let store = sample_store();

        let work_or_personal = store.filter_by_tags(Some(&["work".to_string(), "personal".to_string()]));
        assert_eq!(work_or_personal.len(), 2);

        let ci_only = store.filter_by_tags(Some(&["ci".to_string()]));
        assert_eq!(ci_only.len(), 1);
        assert_eq!(ci_only[0].0, "GITHUB_TOKEN");

        let no_match = store.filter_by_tags(Some(&["nonexistent".to_string()]));
        assert!(no_match.is_empty());
    }

    #[test]
    fn test_filter_by_tags_none_returns_all() {
        let store = sample_store();
        let all = store.filter_by_tags(None);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_resolve_scoped_env() {
        let store = sample_store();
        let mut env = resolve_scoped_env(&store, Some(&["work".to_string()]));
        env.sort();
        assert_eq!(env, vec![("GITHUB_TOKEN".to_string(), "ghp_xxx".to_string())]);
    }

    #[test]
    fn test_shell_export_line_escaping() {
        let bash = shell_export_line("bash", "FOO", "it's a \"test\"");
        assert_eq!(bash, "export FOO=\"it's a \\\"test\\\"\"");

        let fish = shell_export_line("fish", "FOO", "bar");
        assert_eq!(fish, "set -gx FOO \"bar\";");
    }

    #[test]
    fn test_secrets_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let path = secrets_file_path(temp_dir.path());
        save_store(&path, &sample_store(), &test_password()).unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Secrets file should have 0600 permissions");
    }

    #[test]
    fn test_fallback_store_set_get_roundtrip() {
        // Exercises the file-based fallback directly (bypassing native keyring
        // detection), which is what actually engages on headless machines with no
        // OS keyring / D-Bus Secret Service (CI runners, containers).
        let temp_dir = TempDir::new().unwrap();
        let store = FallbackStore::open_at(temp_dir.path()).unwrap();

        assert!(store.get_password().unwrap().is_none());

        store.set_password("super-secret-password").unwrap();
        assert_eq!(store.get_password().unwrap().unwrap(), "super-secret-password");
    }

    #[test]
    fn test_fallback_store_key_persists_across_opens() {
        // The encryption key must be reused, not regenerated, on every open —
        // otherwise a second `FallbackStore::open_at` call (e.g. the next CLI
        // invocation) would be unable to decrypt data written by the first.
        let temp_dir = TempDir::new().unwrap();

        let store1 = FallbackStore::open_at(temp_dir.path()).unwrap();
        store1.set_password("persisted-value").unwrap();
        drop(store1);

        let store2 = FallbackStore::open_at(temp_dir.path()).unwrap();
        assert_eq!(store2.get_password().unwrap().unwrap(), "persisted-value");
    }

    #[test]
    fn test_fallback_key_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let _store = FallbackStore::open_at(temp_dir.path()).unwrap();

        let key_path = temp_dir.path().join(FALLBACK_KEY_NAME);
        assert!(key_path.exists());

        let metadata = std::fs::metadata(&key_path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Fallback key file should have 0600 permissions");
    }
}
