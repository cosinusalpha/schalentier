//! Go provider for installing Go CLI tools.
//!
//! Uses `go install` to install tools from Go modules.

use super::{InstallResult, Installer, SearchResult};
use crate::config::Provider;
use crate::error::{Result, SchalentierError};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info};

#[derive(Debug, Deserialize)]
struct GoSearchResponse {
    results: Vec<GoPackage>,
}

#[derive(Debug, Deserialize)]
struct GoPackage {
    path: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    synopsis: Option<String>,
}

/// Go provider for installing CLI tools
pub struct GoProvider {
    go_path: Option<PathBuf>,
    gobin: Option<PathBuf>,
    client: Client,
}

impl GoProvider {
    pub fn new() -> Self {
        Self {
            go_path: None,
            gobin: None,
            client: Client::new(),
        }
    }

    /// Set a custom go binary path
    pub fn with_go_path(mut self, path: PathBuf) -> Self {
        self.go_path = Some(path);
        self
    }

    /// Override where `go install` places tool binaries (GOBIN). Used when schalentier
    /// bootstrapped its own Go toolchain, so tools land under its data dir instead of
    /// the user's `~/go/bin` convention.
    pub fn with_gobin(mut self, path: PathBuf) -> Self {
        self.gobin = Some(path);
        self
    }

    /// Get the go binary path
    fn go_bin(&self) -> &str {
        self.go_path
            .as_ref()
            .and_then(|p| p.to_str())
            .unwrap_or("go")
    }

    /// Get GOBIN directory where binaries are installed
    fn gobin_dir(&self) -> Option<PathBuf> {
        if let Some(ref gobin) = self.gobin {
            return Some(gobin.clone());
        }

        // Check GOBIN env var
        if let Ok(gobin) = std::env::var("GOBIN") {
            return Some(PathBuf::from(gobin));
        }

        // Check GOPATH/bin
        if let Ok(gopath) = std::env::var("GOPATH") {
            return Some(PathBuf::from(gopath).join("bin"));
        }

        // Default: ~/go/bin
        dirs::home_dir().map(|h| h.join("go").join("bin"))
    }
}

impl Default for GoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Installer for GoProvider {
    fn provider(&self) -> Provider {
        Provider::Go
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Use pkg.go.dev API
        let url = format!(
            "https://pkg.go.dev/v1beta/search?q={}&limit={}",
            urlencoding::encode(query),
            limit
        );

        debug!("Searching Go packages: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SchalentierError::Network(format!("Go search failed: {}", e)))?;

        if !response.status().is_success() {
            debug!("Go search returned status: {}", response.status());
            return Ok(Vec::new());
        }

        // pkg.go.dev's search API intermittently returns malformed JSON (or an HTML
        // rate-limit page) — an expected upstream quirk, not something actionable by
        // the user, so treat it as "no results" instead of surfacing a warning.
        let data: GoSearchResponse = match response.json().await {
            Ok(data) => data,
            Err(e) => {
                debug!("Failed to parse Go search response: {}", e);
                return Ok(Vec::new());
            }
        };

        Ok(data
            .results
            .into_iter()
            .map(|pkg| SearchResult {
                name: pkg.path.clone(),
                description: pkg.synopsis,
                version: pkg.version,
                provider: Provider::Go,
                metadata: HashMap::new(),
            })
            .collect())
    }

    async fn install(&self, name: &str, version: Option<&str>) -> Result<InstallResult> {
        info!("Installing {} via go install...", name);

        // Format: go install module/path@version
        let version_str = version.unwrap_or("latest");
        let package_spec = format!("{}@{}", name, version_str);

        let mut cmd = Command::new(self.go_bin());
        cmd.args(&["install", &package_spec]);

        // Set GOBIN if we have a custom path
        if let Some(ref gobin) = self.gobin_dir() {
            cmd.env("GOBIN", gobin);
        }

        debug!("Running: go install {}", package_spec);

        let status = cmd.status().map_err(|e| {
            SchalentierError::CommandFailed(format!("Failed to run go install: {}", e))
        })?;

        if !status.success() {
            return Ok(InstallResult {
                path: None,
                version: version.map(|v| v.to_string()),
                success: false,
                message: Some(format!("go install failed with exit code: {:?}", status.code())),
            });
        }

        // Find installed binary (last component of module path)
        let binary_name = name.split('/').last().unwrap_or(name);
        let binary_path = self.gobin_dir().map(|p| p.join(binary_name));

        Ok(InstallResult {
            path: binary_path,
            version: version.map(|v| v.to_string()),
            success: true,
            message: Some(format!("Installed {} via go", name)),
        })
    }

    async fn uninstall(&self, name: &str) -> Result<()> {
        // Go has no uninstall command, delete binary manually
        let binary_name = name.split('/').last().unwrap_or(name);

        if let Some(gobin) = self.gobin_dir() {
            let binary_path = gobin.join(binary_name);
            if binary_path.exists() {
                std::fs::remove_file(&binary_path).map_err(|e| {
                    SchalentierError::CommandFailed(format!(
                        "Failed to remove {}: {}",
                        binary_path.display(),
                        e
                    ))
                })?;
                info!("Removed {}", binary_path.display());
            }
        }

        Ok(())
    }

    async fn is_installed(&self, name: &str) -> Result<bool> {
        let binary_name = name.split('/').last().unwrap_or(name);

        if let Some(gobin) = self.gobin_dir() {
            return Ok(gobin.join(binary_name).exists());
        }

        Ok(false)
    }

    async fn installed_version(&self, name: &str) -> Result<Option<String>> {
        // Go embeds module build info in the binary. `go version -m <bin>` prints it;
        // the `mod` line carries the main module's version:
        //   <bin>: go1.22.5
        //       path  github.com/owner/tool
        //       mod   github.com/owner/tool  v0.44.1  h1:...
        let binary_name = name.split('/').last().unwrap_or(name);
        let Some(gobin) = self.gobin_dir() else {
            return Ok(None);
        };
        let binary_path = gobin.join(binary_name);
        if !binary_path.exists() {
            return Ok(None);
        }

        let output = Command::new(self.go_bin())
            .arg("version")
            .arg("-m")
            .arg(&binary_path)
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => return Ok(None),
        };

        Ok(parse_go_mod_version(&String::from_utf8_lossy(&output.stdout)))
    }

    fn is_available(&self) -> bool {
        which::which(self.go_bin()).is_ok()
    }
}

/// Parse the module version from `go version -m <bin>` output.
///
/// Looks for the `mod` line (whitespace/tab separated: `mod <path> <version> <hash>`)
/// and returns the version field. Falls back to `None` if no `mod` line is present.
fn parse_go_mod_version(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // fields: ["mod", "<module-path>", "<version>", "<hash>"]
        if fields.first() == Some(&"mod") && fields.len() >= 3 {
            return Some(fields[2].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_go_provider_creation() {
        let provider = GoProvider::new();
        assert_eq!(provider.provider(), Provider::Go);
    }

    #[test]
    fn test_gobin_default() {
        let provider = GoProvider::new();
        let gobin = provider.gobin_dir();
        // Should default to ~/go/bin if GOPATH not set
        assert!(gobin.is_some());
    }

    #[test]
    fn test_parse_go_mod_version() {
        let output = "\
/home/user/go/bin/lazygit: go1.22.5
\tpath\tgithub.com/jesseduffield/lazygit
\tmod\tgithub.com/jesseduffield/lazygit\tv0.44.1\th1:abc123=
\tdep\tgithub.com/some/dep\tv1.0.0\th1:def=
";
        assert_eq!(parse_go_mod_version(output), Some("v0.44.1".to_string()));
    }

    #[test]
    fn test_parse_go_mod_version_no_mod_line() {
        assert_eq!(parse_go_mod_version("some binary: go1.22.5\n"), None);
        assert_eq!(parse_go_mod_version(""), None);
    }
}
