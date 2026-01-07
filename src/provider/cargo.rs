//! Cargo provider for installing Rust crates.
//!
//! Wraps `cargo install` to install crates from crates.io.

use super::{InstallResult, Installer, SearchResult};
use crate::config::Provider;
use crate::error::{Result, SchalentierError};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{debug, info};

/// Response from crates.io API search
#[derive(Debug, Deserialize)]
struct CratesSearchResponse {
    crates: Vec<CrateInfo>,
}

#[derive(Debug, Deserialize)]
struct CrateInfo {
    name: String,
    description: Option<String>,
    max_version: String,
    downloads: u64,
    repository: Option<String>,
}

/// Cargo provider for installing Rust crates
pub struct CargoProvider {
    /// Path to cargo binary (None = use PATH)
    cargo_path: Option<PathBuf>,
    /// Installation root (--root flag)
    install_root: Option<PathBuf>,
    /// HTTP client for crates.io API
    client: reqwest::Client,
}

impl CargoProvider {
    /// Create a new Cargo provider
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("schalentier/0.1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            cargo_path: None,
            install_root: None,
            client,
        }
    }

    /// Set a custom cargo binary path
    pub fn with_cargo_path(mut self, path: PathBuf) -> Self {
        self.cargo_path = Some(path);
        self
    }

    /// Set a custom installation root
    pub fn with_install_root(mut self, path: PathBuf) -> Self {
        self.install_root = Some(path);
        self
    }

    /// Get the cargo binary to use
    fn cargo_bin(&self) -> &str {
        self.cargo_path
            .as_ref()
            .and_then(|p| p.to_str())
            .unwrap_or("cargo")
    }

    /// Search crates.io API
    async fn search_crates(&self, query: &str, limit: usize) -> Result<Vec<CrateInfo>> {
        let url = format!(
            "https://crates.io/api/v1/crates?q={}&per_page={}",
            urlencoding::encode(query),
            limit
        );

        debug!("Searching crates.io: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SchalentierError::Network(format!("Failed to search crates.io: {}", e)))?;

        if !response.status().is_success() {
            return Err(SchalentierError::Network(format!(
                "crates.io returned {}",
                response.status()
            ))
            .into());
        }

        let search_response: CratesSearchResponse = response
            .json()
            .await
            .map_err(|e| SchalentierError::Network(format!("Failed to parse response: {}", e)))?;

        Ok(search_response.crates)
    }

    /// Get the installation directory for cargo binaries
    fn bin_dir(&self) -> PathBuf {
        if let Some(ref root) = self.install_root {
            root.join("bin")
        } else {
            // Default cargo bin location
            dirs::home_dir()
                .unwrap_or_default()
                .join(".cargo")
                .join("bin")
        }
    }
}

impl Default for CargoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Installer for CargoProvider {
    fn provider(&self) -> Provider {
        Provider::Cargo
    }

    fn is_available(&self) -> bool {
        // Check if cargo is available
        which::which(self.cargo_bin()).is_ok()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let crates = self.search_crates(query, limit).await?;

        let results = crates
            .into_iter()
            .map(|c| {
                let mut metadata = HashMap::new();
                metadata.insert("downloads".to_string(), c.downloads.to_string());
                if let Some(repo) = c.repository {
                    metadata.insert("repository".to_string(), repo);
                }

                SearchResult {
                    name: c.name,
                    description: c.description,
                    version: Some(c.max_version),
                    provider: Provider::Cargo,
                    metadata,
                }
            })
            .collect();

        Ok(results)
    }

    async fn install(&self, name: &str, version: Option<&str>) -> Result<InstallResult> {
        info!("Installing {} via cargo...", name);

        let mut cmd = Command::new(self.cargo_bin());
        cmd.arg("install");
        cmd.arg(name);

        if let Some(v) = version {
            cmd.arg("--version").arg(v);
        }

        if let Some(ref root) = self.install_root {
            cmd.arg("--root").arg(root);
        }

        // Use inherited IO for interactive feedback
        debug!("Running: {:?}", cmd);
        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| SchalentierError::InstallFailed {
                package: name.to_string(),
                reason: format!("Failed to run cargo: {}", e),
            })?;

        if status.success() {
            // Try to find the installed binary
            let binary_path = self.bin_dir().join(name);
            let binary_path_exe = self.bin_dir().join(format!("{}.exe", name));

            let path = if binary_path.exists() {
                Some(binary_path)
            } else if binary_path_exe.exists() {
                Some(binary_path_exe)
            } else {
                // The binary might have a different name
                which::which(name).ok()
            };

            Ok(InstallResult {
                path,
                version: version.map(|s| s.to_string()),
                success: true,
                message: Some(format!("Installed {} via cargo", name)),
            })
        } else {
            Ok(InstallResult {
                path: None,
                version: None,
                success: false,
                message: Some(format!(
                    "cargo install failed with exit code: {:?}",
                    status.code()
                )),
            })
        }
    }

    async fn uninstall(&self, name: &str) -> Result<()> {
        info!("Uninstalling {} via cargo...", name);

        let mut cmd = Command::new(self.cargo_bin());
        cmd.arg("uninstall");
        cmd.arg(name);

        if let Some(ref root) = self.install_root {
            cmd.arg("--root").arg(root);
        }

        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| SchalentierError::CommandFailed(format!("Failed to run cargo: {}", e)))?;

        if status.success() {
            info!("{} uninstalled successfully", name);
            Ok(())
        } else {
            Err(SchalentierError::CommandFailed(format!(
                "cargo uninstall failed with exit code: {:?}",
                status.code()
            ))
            .into())
        }
    }

    async fn is_installed(&self, name: &str) -> Result<bool> {
        let binary_path = self.bin_dir().join(name);
        let binary_path_exe = self.bin_dir().join(format!("{}.exe", name));

        Ok(binary_path.exists() || binary_path_exe.exists())
    }

    async fn installed_version(&self, name: &str) -> Result<Option<String>> {
        // Try to get version from the binary itself
        let binary_path = self.bin_dir().join(name);
        let binary_path_exe = self.bin_dir().join(format!("{}.exe", name));

        let binary = if binary_path.exists() {
            binary_path
        } else if binary_path_exe.exists() {
            binary_path_exe
        } else {
            return Ok(None);
        };

        // Try running with --version
        let output = Command::new(&binary)
            .arg("--version")
            .output()
            .ok();

        if let Some(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse version from output like "tool 1.2.3" or "tool version 1.2.3"
                let version = stdout
                    .split_whitespace()
                    .find(|s| s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
                    .map(|s| s.to_string());
                return Ok(version);
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cargo_provider_creation() {
        let provider = CargoProvider::new();
        // Just verify it doesn't panic
        let _ = provider.cargo_bin();
    }

    #[test]
    fn test_bin_dir_default() {
        let provider = CargoProvider::new();
        let bin_dir = provider.bin_dir();
        assert!(bin_dir.ends_with("bin"));
    }

    #[test]
    fn test_bin_dir_custom() {
        let provider = CargoProvider::new()
            .with_install_root(PathBuf::from("/custom/root"));
        let bin_dir = provider.bin_dir();
        assert_eq!(bin_dir, PathBuf::from("/custom/root/bin"));
    }

    #[test]
    fn test_is_available() {
        let provider = CargoProvider::new();
        // cargo may or may not be available
        let _ = provider.is_available();
    }

    #[tokio::test]
    async fn test_search_returns_results() {
        // This test requires network access
        let provider = CargoProvider::new();

        // Skip if no network
        if provider.search_crates("ripgrep", 1).await.is_err() {
            return;
        }

        let results = provider.search("ripgrep", 5).await;
        if let Ok(results) = results {
            assert!(!results.is_empty());
            assert!(results.iter().any(|r| r.name.contains("ripgrep")));
        }
    }
}
