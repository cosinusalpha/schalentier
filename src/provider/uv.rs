//! UV provider for installing Python CLI tools.
//!
//! Uses `uv tool install` to install Python command-line tools from PyPI.

use super::{InstallResult, Installer, SearchResult};
use crate::config::Provider;
use crate::error::{Result, SchalentierError};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{debug, info};

/// Response from PyPI search API
#[derive(Debug, Deserialize)]
struct PyPISearchResult {
    name: String,
    version: String,
    #[serde(default)]
    summary: Option<String>,
}

/// Response from PyPI package info API
#[derive(Debug, Deserialize)]
struct PyPIPackageInfo {
    info: PyPIInfo,
}

#[derive(Debug, Deserialize)]
struct PyPIInfo {
    name: String,
    version: String,
    summary: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    home_page: Option<String>,
}

/// UV provider for installing Python CLI tools
pub struct UvProvider {
    /// Path to schalentier data directory
    data_dir: PathBuf,
    /// HTTP client for PyPI API
    client: reqwest::Client,
}

impl UvProvider {
    /// Create a new UV provider
    pub fn new(data_dir: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("schalentier/0.1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { data_dir, client }
    }

    /// Get the path to the uv binary
    fn uv_bin(&self) -> Option<PathBuf> {
        // Check schalentier's own uv first
        let schalentier_uv = self.data_dir.join("bin/uv");
        if schalentier_uv.exists() {
            return Some(schalentier_uv);
        }

        // Try PATH
        which::which("uv").ok()
    }

    /// Get the bin directory for UV-installed tools
    fn tools_bin_dir(&self) -> PathBuf {
        // UV installs tools to ~/.local/bin by default
        dirs::home_dir().unwrap_or_default().join(".local/bin")
    }

    /// Search PyPI for packages
    async fn search_pypi(&self, query: &str, _limit: usize) -> Result<Vec<PyPISearchResult>> {
        // PyPI doesn't have a great search API, so we use the simple approach
        // of checking if a package exists by name
        let url = format!("https://pypi.org/pypi/{}/json", query);

        debug!("Checking PyPI: {}", url);

        match self.client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                let info: PyPIPackageInfo = response.json().await.map_err(|e| {
                    SchalentierError::ParseError(format!("Failed to parse PyPI response: {}", e))
                })?;

                Ok(vec![PyPISearchResult {
                    name: info.info.name,
                    version: info.info.version,
                    summary: info.info.summary,
                }])
            }
            _ => {
                // Try a fuzzy search by checking common tool packages
                // This is a simplified approach - a real implementation would use
                // a search endpoint or index
                Ok(Vec::new())
            }
        }
    }

    /// Get package info from PyPI
    async fn get_package_info(&self, name: &str) -> Result<Option<PyPIPackageInfo>> {
        let url = format!("https://pypi.org/pypi/{}/json", name);

        debug!("Getting PyPI info: {}", url);

        match self.client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                let info: PyPIPackageInfo = response.json().await.map_err(|e| {
                    SchalentierError::ParseError(format!("Failed to parse PyPI response: {}", e))
                })?;
                Ok(Some(info))
            }
            _ => Ok(None),
        }
    }
}

impl Default for UvProvider {
    fn default() -> Self {
        let data_dir = dirs::home_dir().unwrap_or_default().join(".schalentier");
        Self::new(data_dir)
    }
}

#[async_trait]
impl Installer for UvProvider {
    fn provider(&self) -> Provider {
        Provider::Uv
    }

    fn is_available(&self) -> bool {
        self.uv_bin().is_some()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let packages = self.search_pypi(query, limit).await?;

        let results: Vec<SearchResult> = packages
            .into_iter()
            .take(limit)
            .map(|pkg| SearchResult {
                name: pkg.name,
                description: pkg.summary,
                version: Some(pkg.version),
                provider: Provider::Uv,
                metadata: HashMap::new(),
            })
            .collect();

        Ok(results)
    }

    async fn install(&self, name: &str, version: Option<&str>) -> Result<InstallResult> {
        info!("Installing {} via uv tool...", name);

        let uv = self
            .uv_bin()
            .ok_or_else(|| SchalentierError::ProviderNotAvailable("uv not found".to_string()))?;

        let mut cmd = Command::new(&uv);
        cmd.arg("tool");
        cmd.arg("install");

        // Specify version if provided
        let pkg_spec = if let Some(v) = version {
            format!("{}=={}", name, v)
        } else {
            name.to_string()
        };
        cmd.arg(&pkg_spec);

        debug!("Running: {:?}", cmd);

        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| SchalentierError::InstallFailed {
                package: name.to_string(),
                reason: format!("Failed to run uv: {}", e),
            })?;

        if status.success() {
            // Try to find the installed binary
            let binary_path = self.tools_bin_dir().join(name);
            let path = if binary_path.exists() {
                Some(binary_path)
            } else {
                which::which(name).ok()
            };

            // Get installed version from PyPI info
            let installed_version = if let Ok(Some(info)) = self.get_package_info(name).await {
                Some(info.info.version)
            } else {
                version.map(|s| s.to_string())
            };

            Ok(InstallResult {
                path,
                version: installed_version,
                success: true,
                message: Some(format!("Installed {} via uv tool", name)),
            })
        } else {
            Ok(InstallResult {
                path: None,
                version: None,
                success: false,
                message: Some(format!(
                    "uv tool install failed with exit code: {:?}",
                    status.code()
                )),
            })
        }
    }

    async fn uninstall(&self, name: &str) -> Result<()> {
        info!("Uninstalling {} via uv tool...", name);

        let uv = self
            .uv_bin()
            .ok_or_else(|| SchalentierError::ProviderNotAvailable("uv not found".to_string()))?;

        let status = Command::new(&uv)
            .args(["tool", "uninstall", name])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| SchalentierError::CommandFailed(format!("Failed to run uv: {}", e)))?;

        if status.success() {
            info!("{} uninstalled successfully", name);
            Ok(())
        } else {
            Err(SchalentierError::CommandFailed(format!(
                "uv tool uninstall failed with exit code: {:?}",
                status.code()
            ))
            .into())
        }
    }

    async fn is_installed(&self, name: &str) -> Result<bool> {
        // Check if the binary exists in the tools bin dir
        let binary_path = self.tools_bin_dir().join(name);
        if binary_path.exists() {
            return Ok(true);
        }

        // Also check uv tool list
        let uv = match self.uv_bin() {
            Some(uv) => uv,
            None => return Ok(false),
        };

        let output = Command::new(&uv).args(["tool", "list"]).output().ok();

        if let Some(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Check if the package name appears in the output
                return Ok(stdout
                    .lines()
                    .any(|line| line.split_whitespace().next() == Some(name)));
            }
        }

        Ok(false)
    }

    async fn installed_version(&self, name: &str) -> Result<Option<String>> {
        // Check uv tool list for version
        let uv = match self.uv_bin() {
            Some(uv) => uv,
            None => return Ok(None),
        };

        let output = Command::new(&uv).args(["tool", "list"]).output().ok();

        if let Some(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse lines like "httpie v3.2.2" or "ruff 0.1.0"
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.first() == Some(&name) {
                        if let Some(version) = parts.get(1) {
                            // Remove 'v' prefix if present
                            let v = version.trim_start_matches('v');
                            return Ok(Some(v.to_string()));
                        }
                    }
                }
            }
        }

        // Fallback: try running the binary with --version
        let binary_path = self.tools_bin_dir().join(name);
        if binary_path.exists() {
            let output = Command::new(&binary_path).arg("--version").output().ok();

            if let Some(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let version = stdout
                        .split_whitespace()
                        .find(|s| {
                            s.chars()
                                .next()
                                .map(|c| c.is_ascii_digit())
                                .unwrap_or(false)
                        })
                        .map(|s| s.to_string());
                    return Ok(version);
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uv_provider_creation() {
        let provider = UvProvider::new(PathBuf::from("/home/user/.schalentier"));
        assert_eq!(provider.data_dir, PathBuf::from("/home/user/.schalentier"));
    }

    #[test]
    fn test_tools_bin_dir() {
        let provider = UvProvider::new(PathBuf::from("/tmp"));
        let bin_dir = provider.tools_bin_dir();
        assert!(bin_dir.ends_with(".local/bin"));
    }

    #[test]
    fn test_is_available() {
        let provider = UvProvider::default();
        // May or may not be available depending on system
        let _ = provider.is_available();
    }
}
