//! Homebrew/Linuxbrew provider for installing packages.
//!
//! Wraps `brew` commands to search and install packages from Homebrew.

use super::{InstallResult, Installer, SearchResult};
use crate::config::Provider;
use crate::error::{Result, SchalentierError};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{debug, info};

/// Response from brew info --json
#[derive(Debug, Deserialize)]
struct BrewInfoResponse {
    #[allow(dead_code)]
    name: String,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    versions: BrewVersions,
    #[serde(default)]
    installed: Vec<BrewInstalledVersion>,
}

#[derive(Debug, Default, Deserialize)]
struct BrewVersions {
    stable: Option<String>,
    #[allow(dead_code)]
    head: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrewInstalledVersion {
    version: String,
}

/// Homebrew provider for installing packages
pub struct BrewProvider {
    /// Path to brew binary (auto-detected if None)
    brew_path: Option<PathBuf>,
}

impl BrewProvider {
    /// Create a new Brew provider
    pub fn new() -> Self {
        Self { brew_path: None }
    }

    /// Set a custom brew binary path
    pub fn with_brew_path(mut self, path: PathBuf) -> Self {
        self.brew_path = Some(path);
        self
    }

    /// Get the path to the brew binary
    fn brew_bin(&self) -> Option<PathBuf> {
        if let Some(ref path) = self.brew_path {
            if path.exists() {
                return Some(path.clone());
            }
        }

        // Check common Linuxbrew locations
        let linuxbrew_paths = [
            PathBuf::from("/home/linuxbrew/.linuxbrew/bin/brew"),
            PathBuf::from("/opt/homebrew/bin/brew"), // macOS ARM
            PathBuf::from("/usr/local/bin/brew"),    // macOS Intel
        ];

        for path in &linuxbrew_paths {
            if path.exists() {
                return Some(path.clone());
            }
        }

        // Try PATH
        which::which("brew").ok()
    }

    /// Run a brew command and capture output
    fn run_brew_command(&self, args: &[&str]) -> Result<String> {
        let brew = self
            .brew_bin()
            .ok_or_else(|| SchalentierError::ProviderNotAvailable("brew not found".to_string()))?;

        debug!("Running: {:?} {:?}", brew, args);

        let output = Command::new(&brew)
            .args(args)
            .output()
            .map_err(|e| SchalentierError::CommandFailed(format!("Failed to run brew: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SchalentierError::CommandFailed(format!(
                "brew command failed: {}",
                stderr
            ))
            .into());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get info about a formula
    fn get_formula_info(&self, name: &str) -> Result<Option<BrewInfoResponse>> {
        let output = self.run_brew_command(&["info", "--json=v2", name]);

        match output {
            Ok(json) => {
                // Parse the JSON - brew returns { formulae: [...], casks: [...] }
                #[derive(Deserialize)]
                struct InfoWrapper {
                    #[serde(default)]
                    formulae: Vec<BrewInfoResponse>,
                    #[serde(default)]
                    casks: Vec<BrewInfoResponse>,
                }

                let wrapper: InfoWrapper = serde_json::from_str(&json).map_err(|e| {
                    SchalentierError::ParseError(format!("Failed to parse brew info: {}", e))
                })?;

                // Prefer formulae over casks
                if let Some(formula) = wrapper.formulae.into_iter().next() {
                    return Ok(Some(formula));
                }
                if let Some(cask) = wrapper.casks.into_iter().next() {
                    return Ok(Some(cask));
                }
                Ok(None)
            }
            Err(_) => Ok(None),
        }
    }
}

impl Default for BrewProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Installer for BrewProvider {
    fn provider(&self) -> Provider {
        Provider::Brew
    }

    fn is_available(&self) -> bool {
        self.brew_bin().is_some()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Use brew search --json (if available) or parse text output. `brew search`
        // exits non-zero when nothing matches ("No formulae or casks found") — that's
        // an empty result, not a search failure, so don't propagate it as an error.
        let output = match self.run_brew_command(&["search", query]) {
            Ok(output) => output,
            Err(_) => return Ok(Vec::new()),
        };

        // Parse the output - one package per line
        let mut results: Vec<SearchResult> = output
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with("==>"))
            .take(limit)
            .map(|name| {
                let name = name.trim().to_string();
                SearchResult {
                    name,
                    description: None, // We could fetch with brew info but it's slow
                    version: None,
                    provider: Provider::Brew,
                    metadata: HashMap::new(),
                }
            })
            .collect();

        // Try to get version info for the first few results
        for result in results.iter_mut().take(3) {
            if let Ok(Some(info)) = self.get_formula_info(&result.name) {
                result.description = info.desc;
                result.version = info.versions.stable;
            }
        }

        Ok(results)
    }

    async fn install(&self, name: &str, version: Option<&str>) -> Result<InstallResult> {
        info!("Installing {} via brew...", name);

        let brew = self
            .brew_bin()
            .ok_or_else(|| SchalentierError::ProviderNotAvailable("brew not found".to_string()))?;

        let mut cmd = Command::new(&brew);
        cmd.arg("install");

        // Specify version if provided (using @ syntax)
        if let Some(v) = version {
            cmd.arg(format!("{}@{}", name, v));
        } else {
            cmd.arg(name);
        }

        debug!("Running: {:?}", cmd);

        let status = cmd
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| SchalentierError::InstallFailed {
                package: name.to_string(),
                reason: format!("Failed to run brew: {}", e),
            })?;

        if status.success() {
            // Get the installed version
            let installed_version = if let Ok(Some(info)) = self.get_formula_info(name) {
                info.versions.stable
            } else {
                version.map(|s| s.to_string())
            };

            Ok(InstallResult {
                path: which::which(name).ok(),
                version: installed_version,
                success: true,
                message: Some(format!("Installed {} via brew", name)),
            })
        } else {
            Ok(InstallResult {
                path: None,
                version: None,
                success: false,
                message: Some(format!(
                    "brew install failed with exit code: {:?}",
                    status.code()
                )),
            })
        }
    }

    async fn uninstall(&self, name: &str) -> Result<()> {
        info!("Uninstalling {} via brew...", name);

        let brew = self
            .brew_bin()
            .ok_or_else(|| SchalentierError::ProviderNotAvailable("brew not found".to_string()))?;

        let status = Command::new(&brew)
            .args(["uninstall", name])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| SchalentierError::CommandFailed(format!("Failed to run brew: {}", e)))?;

        if status.success() {
            info!("{} uninstalled successfully", name);
            Ok(())
        } else {
            Err(SchalentierError::CommandFailed(format!(
                "brew uninstall failed with exit code: {:?}",
                status.code()
            ))
            .into())
        }
    }

    async fn is_installed(&self, name: &str) -> Result<bool> {
        if let Ok(Some(info)) = self.get_formula_info(name) {
            return Ok(!info.installed.is_empty());
        }

        // Also check if binary is available
        Ok(which::which(name).is_ok())
    }

    async fn installed_version(&self, name: &str) -> Result<Option<String>> {
        if let Ok(Some(info)) = self.get_formula_info(name) {
            if let Some(installed) = info.installed.first() {
                return Ok(Some(installed.version.clone()));
            }
        }

        // Fallback: try running the binary with --version
        if let Ok(path) = which::which(name) {
            let output = Command::new(&path).arg("--version").output().ok();

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
    fn test_brew_provider_creation() {
        let provider = BrewProvider::new();
        assert!(provider.brew_path.is_none());
    }

    #[test]
    fn test_with_brew_path() {
        let provider = BrewProvider::new().with_brew_path(PathBuf::from("/custom/brew"));
        assert_eq!(provider.brew_path, Some(PathBuf::from("/custom/brew")));
    }

    #[test]
    fn test_is_available() {
        let provider = BrewProvider::new();
        // May or may not be available depending on system
        let _ = provider.is_available();
    }
}
