use super::{InstallResult, Installer, SearchResult};
use crate::archive::{self, ArchiveFormat};
use crate::bootstrap::{Arch, Os};
use crate::config::Provider;
use crate::error::{Result, SchalentierError};
use anyhow::Context;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// GitHub API response for repository search
#[derive(Debug, Deserialize)]
struct GitHubSearchResponse {
    items: Vec<GitHubRepo>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubRepo {
    name: String,
    full_name: String,
    description: Option<String>,
    html_url: String,
    stargazers_count: u64,
}

/// GitHub API response for releases
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    assets: Vec<GitHubAsset>,
    prerelease: bool,
    draft: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    download_count: u64,
}

/// Binary provider - downloads pre-built binaries from GitHub releases
pub struct BinaryProvider {
    arch: Arch,
    os: Os,
    client: reqwest::Client,
    bin_dir: Option<PathBuf>,
}

impl BinaryProvider {
    pub fn new(arch: Arch, os: Os) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("schalentier/0.1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            arch,
            os,
            client,
            bin_dir: None,
        }
    }

    pub fn with_bin_dir(mut self, bin_dir: PathBuf) -> Self {
        self.bin_dir = Some(bin_dir);
        self
    }

    /// Search GitHub for repositories with releases
    async fn search_github(&self, query: &str, limit: usize) -> Result<Vec<GitHubRepo>> {
        let url = format!(
            "https://api.github.com/search/repositories?q={}+in:name&sort=stars&per_page={}",
            urlencoding::encode(query),
            limit
        );

        debug!("Searching GitHub: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| "Failed to search GitHub")?;

        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::FORBIDDEN {
                warn!("GitHub API rate limit reached");
                return Ok(Vec::new());
            }
            return Err(SchalentierError::Network(format!(
                "GitHub API returned {}",
                response.status()
            ))
            .into());
        }

        let search_response: GitHubSearchResponse = response
            .json()
            .await
            .with_context(|| "Failed to parse GitHub search response")?;

        Ok(search_response.items)
    }

    /// Get the latest release for a repository
    async fn get_latest_release(&self, owner: &str, repo: &str) -> Result<Option<GitHubRelease>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            owner, repo
        );

        debug!("Fetching latest release: {}", url);

        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            debug!("No releases found for {}/{}", owner, repo);
            return Ok(None);
        }

        if !response.status().is_success() {
            return Err(SchalentierError::Network(format!(
                "GitHub API returned {}",
                response.status()
            ))
            .into());
        }

        let release: GitHubRelease = response.json().await?;
        Ok(Some(release))
    }

    /// Find the best matching asset for the current platform
    fn find_best_asset<'a>(&self, assets: &'a [GitHubAsset]) -> Option<&'a GitHubAsset> {
        // Keywords that indicate platform compatibility
        let arch_keywords = match self.arch {
            Arch::X86_64 => vec!["x86_64", "amd64", "x64", "64bit"],
            Arch::Aarch64 => vec!["aarch64", "arm64", "armv8"],
        };

        let os_keywords = match self.os {
            Os::Linux => vec!["linux", "Linux"],
            Os::MacOS => vec!["darwin", "macos", "osx", "apple", "Darwin", "MacOS"],
            Os::Windows => vec!["windows", "win", "Windows"],
        };

        let extension = match self.os {
            Os::Windows => vec![".zip", ".exe", ".msi"],
            _ => vec![".tar.gz", ".tgz", ".zip"],
        };

        // Score each asset
        let mut scored_assets: Vec<(&GitHubAsset, u32)> = assets
            .iter()
            .filter_map(|asset| {
                let name = asset.name.to_lowercase();

                // Must have correct extension
                if !extension.iter().any(|ext| asset.name.ends_with(ext)) {
                    return None;
                }

                let mut score = 0u32;

                // Check OS match
                if os_keywords
                    .iter()
                    .any(|kw| name.contains(&kw.to_lowercase()))
                {
                    score += 10;
                } else {
                    // No OS indicator - might be platform-agnostic or wrong
                    return None;
                }

                // Check arch match
                if arch_keywords
                    .iter()
                    .any(|kw| name.contains(&kw.to_lowercase()))
                {
                    score += 10;
                }

                // Prefer musl over glibc for Linux (better compatibility)
                if self.os == Os::Linux && name.contains("musl") {
                    score += 2;
                }

                // Prefer static builds
                if name.contains("static") {
                    score += 1;
                }

                // Penalize debug builds
                if name.contains("debug") {
                    score = score.saturating_sub(5);
                }

                Some((asset, score))
            })
            .collect();

        scored_assets.sort_by(|a, b| b.1.cmp(&a.1));
        scored_assets.first().map(|(asset, _)| *asset)
    }

    /// Guess the binary name based on the package name
    fn guess_binary_name(&self, query: &str, repo_name: &str) -> String {
        // Common patterns:
        // ripgrep -> rg
        // fd-find -> fd
        // bat -> bat
        let known_mappings = [
            ("ripgrep", "rg"),
            ("fd-find", "fd"),
            ("delta", "delta"),
            ("bat", "bat"),
            ("exa", "exa"),
            ("eza", "eza"),
            ("zoxide", "zoxide"),
            ("starship", "starship"),
            ("tokei", "tokei"),
            ("hyperfine", "hyperfine"),
            ("procs", "procs"),
            ("dust", "dust"),
            ("bottom", "btm"),
            ("gitui", "gitui"),
            ("lazygit", "lazygit"),
        ];

        for (pkg, bin) in known_mappings {
            if query.to_lowercase() == pkg || repo_name.to_lowercase() == pkg {
                return bin.to_string();
            }
        }

        // Default: use the query name
        query.to_lowercase()
    }

    /// Find the binary in extracted files
    fn find_binary_in_extracted(
        &self,
        files: &[PathBuf],
        binary_name: &str,
        fallback_name: &str,
    ) -> Option<PathBuf> {
        // Try the archive's find_binary function first
        if let Some(path) = archive::find_binary(files, binary_name) {
            return Some(path);
        }

        // Try fallback name
        if binary_name != fallback_name {
            if let Some(path) = archive::find_binary(files, fallback_name) {
                return Some(path);
            }
        }

        // Try to find any executable
        let executables = archive::find_executables(files);
        if executables.len() == 1 {
            // If there's only one executable, use it
            return Some(executables[0].clone());
        }

        // Try to find an executable that matches the name pattern
        for exe in &executables {
            if let Some(name) = exe.file_name().and_then(|s| s.to_str()) {
                let name_lower = name.to_lowercase();
                if name_lower.contains(&binary_name.to_lowercase())
                    || name_lower.contains(&fallback_name.to_lowercase())
                {
                    return Some(exe.clone());
                }
            }
        }

        None
    }
}

#[async_trait]
impl Installer for BinaryProvider {
    fn provider(&self) -> Provider {
        Provider::Binary
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let repos = self.search_github(query, limit * 2).await?;

        let mut results = Vec::new();

        for repo in repos.into_iter().take(limit) {
            // Check if repo has releases with compatible assets
            let parts: Vec<&str> = repo.full_name.split('/').collect();
            if parts.len() != 2 {
                continue;
            }

            let (owner, repo_name) = (parts[0], parts[1]);

            if let Ok(Some(release)) = self.get_latest_release(owner, repo_name).await {
                if self.find_best_asset(&release.assets).is_some() {
                    let mut metadata = HashMap::new();
                    metadata.insert("stars".to_string(), repo.stargazers_count.to_string());
                    metadata.insert("repo".to_string(), repo.full_name.clone());

                    results.push(SearchResult {
                        name: repo.name.clone(),
                        description: repo.description.clone(),
                        version: Some(release.tag_name),
                        provider: Provider::Binary,
                        metadata,
                    });
                }
            }
        }

        Ok(results)
    }

    async fn install(&self, name: &str, _version: Option<&str>) -> Result<InstallResult> {
        // First, search for the repo
        let repos = self.search_github(name, 5).await?;

        let repo = repos
            .into_iter()
            .find(|r| r.name.to_lowercase() == name.to_lowercase())
            .or(None)
            .ok_or_else(|| SchalentierError::PackageNotFound {
                package: name.to_string(),
            })?;

        let parts: Vec<&str> = repo.full_name.split('/').collect();
        let (owner, repo_name) = (parts[0], parts[1]);

        // Get the release
        let release = self
            .get_latest_release(owner, repo_name)
            .await?
            .ok_or_else(|| SchalentierError::PackageNotFound {
                package: name.to_string(),
            })?;

        // Find the best asset
        let asset = self.find_best_asset(&release.assets).ok_or_else(|| {
            SchalentierError::InstallFailed {
                package: name.to_string(),
                reason: format!("No compatible binary found for {} {}", self.os, self.arch),
            }
        })?;

        info!("Found asset: {} ({} bytes)", asset.name, asset.size);

        // Setup directories
        let data_dir = dirs::home_dir().unwrap_or_default().join(".schalentier");

        let bin_dir = self.bin_dir.clone().unwrap_or_else(|| data_dir.join("bin"));
        let downloads_dir = data_dir.join("downloads");

        std::fs::create_dir_all(&bin_dir)?;
        std::fs::create_dir_all(&downloads_dir)?;

        let download_path = downloads_dir.join(&asset.name);
        info!("Downloading to {}", download_path.display());

        let response = self.client.get(&asset.browser_download_url).send().await?;

        let bytes = response.bytes().await?;
        std::fs::write(&download_path, &bytes)?;

        // Determine the binary name (could be different from repo name)
        let binary_name = self.guess_binary_name(name, &repo.name);

        // Check if this is an archive or a direct binary
        let final_binary_path = if let Some(format) = ArchiveFormat::from_path(&download_path) {
            // It's an archive, extract it
            let extract_dir = downloads_dir.join(format!("{}-extract", name));
            if extract_dir.exists() {
                std::fs::remove_dir_all(&extract_dir)?;
            }

            info!("Extracting {:?} archive...", format);
            let extracted_files = archive::extract(&download_path, &extract_dir)?;

            // Find the binary in extracted files
            let binary_path = self
                .find_binary_in_extracted(&extracted_files, &binary_name, name)
                .ok_or_else(|| SchalentierError::InstallFailed {
                    package: name.to_string(),
                    reason: format!(
                        "Could not find binary '{}' in extracted files. Found: {:?}",
                        binary_name,
                        extracted_files
                            .iter()
                            .filter_map(|p| p.file_name())
                            .collect::<Vec<_>>()
                    ),
                })?;

            // Copy to bin directory
            let dest_name = if self.os == Os::Windows && !binary_name.ends_with(".exe") {
                format!("{}.exe", binary_name)
            } else {
                binary_name.clone()
            };
            let dest_path = bin_dir.join(&dest_name);

            info!(
                "Installing {} to {}",
                binary_path.display(),
                dest_path.display()
            );
            std::fs::copy(&binary_path, &dest_path)?;

            // Set executable permission on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest_path, std::fs::Permissions::from_mode(0o755))?;
            }

            // Cleanup extract directory
            let _ = std::fs::remove_dir_all(&extract_dir);

            dest_path
        } else {
            // Direct binary download (e.g., .exe file)
            let dest_name = if self.os == Os::Windows {
                if asset.name.ends_with(".exe") {
                    binary_name.clone()
                } else {
                    format!("{}.exe", binary_name)
                }
            } else {
                binary_name.clone()
            };
            let dest_path = bin_dir.join(&dest_name);

            info!(
                "Installing {} to {}",
                download_path.display(),
                dest_path.display()
            );
            std::fs::copy(&download_path, &dest_path)?;

            // Set executable permission on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest_path, std::fs::Permissions::from_mode(0o755))?;
            }

            dest_path
        };

        // Cleanup download
        let _ = std::fs::remove_file(&download_path);

        Ok(InstallResult {
            path: Some(final_binary_path.clone()),
            version: Some(release.tag_name),
            success: true,
            message: Some(format!(
                "Installed {} from {} to {}",
                name,
                repo.full_name,
                final_binary_path.display()
            )),
        })
    }

    async fn uninstall(&self, name: &str) -> Result<()> {
        let bin_dir = self.bin_dir.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".schalentier")
                .join("bin")
        });

        let binary_path = bin_dir.join(name);
        let binary_path_exe = bin_dir.join(format!("{}.exe", name));

        if binary_path.exists() {
            std::fs::remove_file(&binary_path)?;
            info!("Removed {}", binary_path.display());
        } else if binary_path_exe.exists() {
            std::fs::remove_file(&binary_path_exe)?;
            info!("Removed {}", binary_path_exe.display());
        } else {
            warn!("Binary {} not found in {}", name, bin_dir.display());
        }

        Ok(())
    }

    async fn is_installed(&self, name: &str) -> Result<bool> {
        let bin_dir = self.bin_dir.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".schalentier")
                .join("bin")
        });

        let binary_path = bin_dir.join(name);
        let binary_path_exe = bin_dir.join(format!("{}.exe", name));

        Ok(binary_path.exists() || binary_path_exe.exists())
    }

    async fn installed_version(&self, _name: &str) -> Result<Option<String>> {
        // Would need to run the binary with --version and parse output
        // For now, return None
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_best_asset_linux_x64() {
        let provider = BinaryProvider::new(Arch::X86_64, Os::Linux);

        let assets = vec![
            GitHubAsset {
                name: "tool-linux-x86_64.tar.gz".to_string(),
                browser_download_url: "https://example.com/a".to_string(),
                size: 1000,
                download_count: 100,
            },
            GitHubAsset {
                name: "tool-windows-x86_64.zip".to_string(),
                browser_download_url: "https://example.com/b".to_string(),
                size: 1000,
                download_count: 50,
            },
            GitHubAsset {
                name: "tool-darwin-arm64.tar.gz".to_string(),
                browser_download_url: "https://example.com/c".to_string(),
                size: 1000,
                download_count: 30,
            },
        ];

        let best = provider.find_best_asset(&assets);
        assert!(best.is_some());
        assert!(best.unwrap().name.contains("linux"));
    }

    #[test]
    fn test_find_best_asset_macos_arm() {
        let provider = BinaryProvider::new(Arch::Aarch64, Os::MacOS);

        let assets = vec![
            GitHubAsset {
                name: "tool-linux-x86_64.tar.gz".to_string(),
                browser_download_url: "https://example.com/a".to_string(),
                size: 1000,
                download_count: 100,
            },
            GitHubAsset {
                name: "tool-darwin-arm64.tar.gz".to_string(),
                browser_download_url: "https://example.com/b".to_string(),
                size: 1000,
                download_count: 50,
            },
            GitHubAsset {
                name: "tool-darwin-x86_64.tar.gz".to_string(),
                browser_download_url: "https://example.com/c".to_string(),
                size: 1000,
                download_count: 30,
            },
        ];

        let best = provider.find_best_asset(&assets);
        assert!(best.is_some());
        assert!(best.unwrap().name.contains("darwin"));
        assert!(best.unwrap().name.contains("arm64"));
    }

    #[test]
    fn test_find_best_asset_windows() {
        let provider = BinaryProvider::new(Arch::X86_64, Os::Windows);

        let assets = vec![
            GitHubAsset {
                name: "tool-linux-x86_64.tar.gz".to_string(),
                browser_download_url: "https://example.com/a".to_string(),
                size: 1000,
                download_count: 100,
            },
            GitHubAsset {
                name: "tool-windows-x64.zip".to_string(),
                browser_download_url: "https://example.com/b".to_string(),
                size: 1000,
                download_count: 50,
            },
        ];

        let best = provider.find_best_asset(&assets);
        assert!(best.is_some());
        assert!(best.unwrap().name.contains("windows"));
    }

    #[test]
    fn test_find_best_asset_prefers_musl() {
        let provider = BinaryProvider::new(Arch::X86_64, Os::Linux);

        let assets = vec![
            GitHubAsset {
                name: "tool-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                browser_download_url: "https://example.com/a".to_string(),
                size: 1000,
                download_count: 100,
            },
            GitHubAsset {
                name: "tool-x86_64-unknown-linux-musl.tar.gz".to_string(),
                browser_download_url: "https://example.com/b".to_string(),
                size: 1000,
                download_count: 50,
            },
        ];

        let best = provider.find_best_asset(&assets);
        assert!(best.is_some());
        assert!(best.unwrap().name.contains("musl"));
    }

    #[test]
    fn test_find_best_asset_no_match() {
        let provider = BinaryProvider::new(Arch::X86_64, Os::Linux);

        let assets = vec![GitHubAsset {
            name: "tool-source.tar.gz".to_string(), // No platform indicator
            browser_download_url: "https://example.com/a".to_string(),
            size: 1000,
            download_count: 100,
        }];

        let best = provider.find_best_asset(&assets);
        assert!(best.is_none());
    }
}
