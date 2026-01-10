//! Conda/Mamba provider for installing packages from conda-forge.
//!
//! Uses mamba (preferred) or conda to install packages into the schalentier environment.

use super::{InstallResult, Installer, SearchResult};
use crate::config::Provider;
use crate::error::{Result, SchalentierError};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{debug, info};

/// Response from conda/mamba search --json
#[derive(Debug, Deserialize)]
struct CondaSearchResponse {
    #[serde(flatten)]
    packages: HashMap<String, Vec<CondaPackageInfo>>,
}

#[derive(Debug, Clone, Deserialize)]
struct CondaPackageInfo {
    name: String,
    version: String,
    #[serde(default)]
    build: String,
    channel: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    subdir: String,
}

/// Conda/Mamba provider for installing packages
pub struct CondaProvider {
    /// Path to schalentier data directory
    data_dir: PathBuf,
    /// Name of the environment to use
    env_name: String,
    /// Default channel
    channel: String,
}

impl CondaProvider {
    /// Create a new Conda provider
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            env_name: "base".to_string(), // Use base env for simplicity
            channel: "conda-forge".to_string(),
        }
    }

    /// Set the environment name
    pub fn with_env_name(mut self, name: String) -> Self {
        self.env_name = name;
        self
    }

    /// Set the default channel
    pub fn with_channel(mut self, channel: String) -> Self {
        self.channel = channel;
        self
    }

    /// Get the path to mamba or conda binary
    fn conda_bin(&self) -> Option<PathBuf> {
        // Prefer mamba over conda
        let mamba_path = self.data_dir.join("conda/bin/mamba");
        if mamba_path.exists() {
            return Some(mamba_path);
        }

        let conda_path = self.data_dir.join("conda/bin/conda");
        if conda_path.exists() {
            return Some(conda_path);
        }

        // Try system mamba/conda
        if let Ok(path) = which::which("mamba") {
            return Some(path);
        }
        if let Ok(path) = which::which("conda") {
            return Some(path);
        }

        None
    }

    /// Get the bin directory for installed packages
    fn bin_dir(&self) -> PathBuf {
        self.data_dir.join("conda/bin")
    }

    /// Run a conda/mamba command and capture JSON output
    fn run_json_command(&self, args: &[&str]) -> Result<String> {
        let conda = self.conda_bin().ok_or_else(|| {
            SchalentierError::ProviderNotAvailable("conda/mamba not found".to_string())
        })?;

        debug!("Running: {:?} {:?}", conda, args);

        let output = Command::new(&conda)
            .args(args)
            .env("CONDA_JSON", "1")
            .output()
            .map_err(|e| SchalentierError::CommandFailed(format!("Failed to run conda: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SchalentierError::CommandFailed(format!(
                "conda command failed: {}",
                stderr
            ))
            .into());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Search for packages using conda/mamba
    async fn search_packages(&self, query: &str) -> Result<Vec<CondaPackageInfo>> {
        // mamba search returns JSON with package name as key
        let output = self.run_json_command(&[
            "search",
            "--json",
            "-c",
            &self.channel,
            &format!("*{}*", query),
        ])?;

        // Parse the JSON response
        let response: CondaSearchResponse = serde_json::from_str(&output).map_err(|e| {
            debug!("Failed to parse conda search output: {}", output);
            SchalentierError::ParseError(format!("Failed to parse conda search output: {}", e))
        })?;

        // Flatten the results and deduplicate by name (keeping latest version)
        let mut packages: HashMap<String, CondaPackageInfo> = HashMap::new();
        for (_, versions) in response.packages {
            for pkg in versions {
                // Keep the latest version of each package
                packages
                    .entry(pkg.name.clone())
                    .and_modify(|existing| {
                        if pkg.version > existing.version {
                            *existing = pkg.clone();
                        }
                    })
                    .or_insert(pkg);
            }
        }

        Ok(packages.into_values().collect())
    }
}

impl Default for CondaProvider {
    fn default() -> Self {
        let data_dir = dirs::home_dir().unwrap_or_default().join(".schalentier");
        Self::new(data_dir)
    }
}

#[async_trait]
impl Installer for CondaProvider {
    fn provider(&self) -> Provider {
        Provider::Conda
    }

    fn is_available(&self) -> bool {
        self.conda_bin().is_some()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let packages = self.search_packages(query).await?;

        let results: Vec<SearchResult> = packages
            .into_iter()
            .take(limit)
            .map(|pkg| {
                let mut metadata = HashMap::new();
                if let Some(channel) = pkg.channel {
                    metadata.insert("channel".to_string(), channel);
                }
                metadata.insert("build".to_string(), pkg.build);

                SearchResult {
                    name: pkg.name,
                    description: None, // Conda search doesn't return descriptions
                    version: Some(pkg.version),
                    provider: Provider::Conda,
                    metadata,
                }
            })
            .collect();

        Ok(results)
    }

    async fn install(&self, name: &str, version: Option<&str>) -> Result<InstallResult> {
        info!("Installing {} via conda/mamba...", name);

        let conda = self.conda_bin().ok_or_else(|| {
            SchalentierError::ProviderNotAvailable("conda/mamba not found".to_string())
        })?;

        let mut cmd = Command::new(&conda);
        cmd.arg("install");
        cmd.arg("-y"); // Non-interactive
        cmd.arg("-c").arg(&self.channel);

        // Specify version if provided
        let pkg_spec = if let Some(v) = version {
            format!("{}={}", name, v)
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
                reason: format!("Failed to run conda: {}", e),
            })?;

        if status.success() {
            // Try to find the installed binary
            let binary_path = self.bin_dir().join(name);
            let path = if binary_path.exists() {
                Some(binary_path)
            } else {
                // Try which to find it
                which::which(name).ok()
            };

            Ok(InstallResult {
                path,
                version: version.map(|s| s.to_string()),
                success: true,
                message: Some(format!("Installed {} via conda", name)),
            })
        } else {
            Ok(InstallResult {
                path: None,
                version: None,
                success: false,
                message: Some(format!(
                    "conda install failed with exit code: {:?}",
                    status.code()
                )),
            })
        }
    }

    async fn uninstall(&self, name: &str) -> Result<()> {
        info!("Uninstalling {} via conda/mamba...", name);

        let conda = self.conda_bin().ok_or_else(|| {
            SchalentierError::ProviderNotAvailable("conda/mamba not found".to_string())
        })?;

        let status = Command::new(&conda)
            .arg("remove")
            .arg("-y")
            .arg(name)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| SchalentierError::CommandFailed(format!("Failed to run conda: {}", e)))?;

        if status.success() {
            info!("{} uninstalled successfully", name);
            Ok(())
        } else {
            Err(SchalentierError::CommandFailed(format!(
                "conda remove failed with exit code: {:?}",
                status.code()
            ))
            .into())
        }
    }

    async fn is_installed(&self, name: &str) -> Result<bool> {
        // Check if the binary exists in conda bin dir
        let binary_path = self.bin_dir().join(name);
        if binary_path.exists() {
            return Ok(true);
        }

        // Also try conda list to check if package is installed
        let conda = match self.conda_bin() {
            Some(c) => c,
            None => return Ok(false),
        };

        let output = Command::new(&conda)
            .args(["list", "--json", name])
            .output()
            .ok();

        if let Some(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // If the JSON array is non-empty, package is installed
                return Ok(stdout.contains(&format!("\"name\": \"{}\"", name)));
            }
        }

        Ok(false)
    }

    async fn installed_version(&self, name: &str) -> Result<Option<String>> {
        let conda = match self.conda_bin() {
            Some(c) => c,
            None => return Ok(None),
        };

        let output = Command::new(&conda)
            .args(["list", "--json", name])
            .output()
            .ok();

        if let Some(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse JSON to get version
                if let Ok(packages) = serde_json::from_str::<Vec<CondaPackageInfo>>(&stdout) {
                    if let Some(pkg) = packages.into_iter().find(|p| p.name == name) {
                        return Ok(Some(pkg.version));
                    }
                }
            }
        }

        // Fallback: try running the binary with --version
        let binary_path = self.bin_dir().join(name);
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
    fn test_conda_provider_creation() {
        let provider = CondaProvider::default();
        assert_eq!(provider.channel, "conda-forge");
        assert_eq!(provider.env_name, "base");
    }

    #[test]
    fn test_conda_bin_dir() {
        let provider = CondaProvider::new(PathBuf::from("/home/user/.schalentier"));
        assert_eq!(
            provider.bin_dir(),
            PathBuf::from("/home/user/.schalentier/conda/bin")
        );
    }

    #[test]
    fn test_builder_pattern() {
        let provider = CondaProvider::new(PathBuf::from("/tmp"))
            .with_env_name("test".to_string())
            .with_channel("bioconda".to_string());

        assert_eq!(provider.env_name, "test");
        assert_eq!(provider.channel, "bioconda");
    }
}
