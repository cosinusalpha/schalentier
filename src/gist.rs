//! GitHub Gist sync: encrypted config storage via GitHub Gists API.
//!
//! Provides an alternative sync backend to git repositories. The config file
//! is encrypted with age (reusing the master password from secrets.rs) before
//! being uploaded to a GitHub Gist. Supports both public and secret gists.
//!
//! # URL Scheme
//! - `gist://new` - Create a new gist (will return actual gist ID)
//! - `gist://<gist-id>` - Use existing gist (e.g., `gist://abc123def456`)
//!
//! # Authentication
//! Requires `GITHUB_TOKEN` stored in the encrypted secrets store:
//! ```bash
//! schalentier secret set GITHUB_TOKEN --tags github
//! ```

use crate::error::{Result, SchalentierError};
use crate::secrets::{get_or_create_master_password, load_store, secrets_file_path};
use crate::state::config_dir;
use age::secrecy::SecretString;
use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

const DEFAULT_GITHUB_API_BASE: &str = "https://api.github.com";
const DEFAULT_GIST_FILENAME: &str = "schalentier.enc";

/// Base URL for the GitHub Gists API. Overridable via `SCHALENTIER_GITHUB_API_BASE`
/// so tests (and self-hosted GitHub Enterprise) can point at a different endpoint.
fn github_api_base() -> String {
    std::env::var("SCHALENTIER_GITHUB_API_BASE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_GITHUB_API_BASE.to_string())
}

#[derive(Debug, Clone, Serialize)]
struct GistFile {
    content: String,
}

#[derive(Debug, Serialize)]
struct CreateGistRequest {
    description: String,
    public: bool,
    files: HashMap<String, GistFile>,
}

#[derive(Debug, Deserialize)]
struct GistResponse {
    id: String,
    #[allow(dead_code)]
    html_url: String,
    public: bool,
    files: HashMap<String, GistFileResponse>,
}

#[derive(Debug, Deserialize)]
struct GistFileResponse {
    content: String,
}

pub struct GistClient {
    client: Client,
    token: String,
}

impl GistClient {
    pub async fn new() -> Result<Self> {
        let password = get_or_create_master_password()?;
        let config_dir = config_dir()?;
        let store = load_store(&secrets_file_path(&config_dir), &password)?;

        let token = store
            .secrets
            .get("GITHUB_TOKEN")
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "GITHUB_TOKEN not found in secrets. Add it with: schalentier secret set GITHUB_TOKEN"
                )
            })?
            .value
            .clone();

        debug!("GistClient initialized with GitHub token");

        Ok(Self {
            client: Client::new(),
            token,
        })
    }

    pub async fn create_gist(&self, content: &str, public: bool) -> Result<String> {
        let mut files = HashMap::new();
        files.insert(
            DEFAULT_GIST_FILENAME.to_string(),
            GistFile {
                content: content.to_string(),
            },
        );

        let request = CreateGistRequest {
            description: "Schalentier encrypted configuration".to_string(),
            public,
            files,
        };

        debug!("Creating {} gist", if public { "public" } else { "secret" });

        let response = self
            .client
            .post(format!("{}/gists", github_api_base()))
            .bearer_auth(&self.token)
            .header("User-Agent", "schalentier")
            .header("Accept", "application/vnd.github+json")
            .json(&request)
            .send()
            .await
            .context("Failed to create gist")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "GitHub API error ({}): {}",
                status,
                body
            ));
        }

        let gist: GistResponse = response
            .json()
            .await
            .context("Failed to parse gist response")?;

        debug!("Created gist: {} (public={})", gist.id, gist.public);

        Ok(gist.id)
    }

    pub async fn get_gist(&self, gist_id: &str) -> Result<String> {
        debug!("Fetching gist: {}", gist_id);

        let response = self
            .client
            .get(format!("{}/gists/{}", github_api_base(), gist_id))
            .bearer_auth(&self.token)
            .header("User-Agent", "schalentier")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .context("Failed to fetch gist")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "GitHub API error ({}): {}",
                status,
                body
            ));
        }

        let gist: GistResponse = response
            .json()
            .await
            .context("Failed to parse gist response")?;

        let file = gist
            .files
            .get(DEFAULT_GIST_FILENAME)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Gist does not contain '{}' file",
                    DEFAULT_GIST_FILENAME
                )
            })?;

        debug!("Fetched gist content ({} bytes)", file.content.len());

        Ok(file.content.clone())
    }

    pub async fn update_gist(&self, gist_id: &str, content: &str) -> Result<()> {
        debug!("Updating gist: {}", gist_id);

        let mut files = HashMap::new();
        files.insert(
            DEFAULT_GIST_FILENAME.to_string(),
            GistFile {
                content: content.to_string(),
            },
        );

        let request = CreateGistRequest {
            description: "Schalentier encrypted configuration".to_string(),
            public: false,
            files,
        };

        let response = self
            .client
            .patch(format!("{}/gists/{}", github_api_base(), gist_id))
            .bearer_auth(&self.token)
            .header("User-Agent", "schalentier")
            .header("Accept", "application/vnd.github+json")
            .json(&request)
            .send()
            .await
            .context("Failed to update gist")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "GitHub API error ({}): {}",
                status,
                body
            ));
        }

        debug!("Updated gist: {}", gist_id);

        Ok(())
    }
}

pub fn encrypt_content(plaintext: &str, password: &SecretString) -> Result<String> {
    let recipient = age::scrypt::Recipient::new(password.clone());
    let encrypted = age::encrypt_and_armor(&recipient, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
    Ok(encrypted)
}

pub fn decrypt_content(armored: &str, password: &SecretString) -> Result<String> {
    let identity = age::scrypt::Identity::new(password.clone());
    let plaintext = age::decrypt(&identity, armored.as_bytes())
        .map_err(|e| SchalentierError::DecryptionFailed(e.to_string()))?;
    Ok(String::from_utf8(plaintext)?)
}

pub fn parse_gist_url(url: &str) -> Option<String> {
    url.strip_prefix("gist://").map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let password = SecretString::from("test-password-123".to_string());
        let plaintext = "[tools]\nripgrep = {}";
        
        let encrypted = encrypt_content(plaintext, &password).unwrap();
        assert!(encrypted.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"));
        
        let decrypted = decrypt_content(&encrypted, &password).unwrap();
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_wrong_password_fails() {
        let password1 = SecretString::from("password1".to_string());
        let password2 = SecretString::from("password2".to_string());
        
        let encrypted = encrypt_content("test", &password1).unwrap();
        let result = decrypt_content(&encrypted, &password2);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_gist_url() {
        assert_eq!(parse_gist_url("gist://abc123"), Some("abc123".to_string()));
        assert_eq!(parse_gist_url("gist://new"), Some("new".to_string()));
        assert_eq!(parse_gist_url("https://github.com"), None);
        assert_eq!(parse_gist_url("git@github.com:user/repo.git"), None);
    }
}
