//! System package manager provider.
//!
//! Wraps the system package manager (apt, pacman, dnf, apk, etc.)
//! to install packages from official repositories.

use super::{InstallResult, Installer, SearchResult};
use crate::config::Provider;
use crate::error::{Result, SchalentierError};
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use tracing::{debug, info};

/// Detected package manager type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    /// Debian/Ubuntu apt
    Apt,
    /// Arch Linux pacman
    Pacman,
    /// Fedora/RHEL dnf
    Dnf,
    /// Alpine apk
    Apk,
    /// openSUSE zypper
    Zypper,
}

impl PackageManager {
    /// Detect the system package manager
    pub fn detect() -> Option<Self> {
        // Check for common package managers by looking for their binaries
        let checks = [
            ("/usr/bin/apt", PackageManager::Apt),
            ("/usr/bin/apt-get", PackageManager::Apt),
            ("/usr/bin/pacman", PackageManager::Pacman),
            ("/usr/bin/dnf", PackageManager::Dnf),
            ("/usr/bin/yum", PackageManager::Dnf), // yum is often symlinked to dnf
            ("/sbin/apk", PackageManager::Apk),
            ("/usr/bin/zypper", PackageManager::Zypper),
        ];

        for (path, pm) in checks {
            if std::path::Path::new(path).exists() {
                debug!("Detected package manager: {:?} at {}", pm, path);
                return Some(pm);
            }
        }

        // Also check using `which` command as fallback
        for (cmd, pm) in [
            ("apt", PackageManager::Apt),
            ("pacman", PackageManager::Pacman),
            ("dnf", PackageManager::Dnf),
            ("apk", PackageManager::Apk),
            ("zypper", PackageManager::Zypper),
        ] {
            if which::which(cmd).is_ok() {
                debug!("Detected package manager via which: {:?}", pm);
                return Some(pm);
            }
        }

        None
    }

    /// Get the command name for this package manager
    fn command(&self) -> &'static str {
        match self {
            PackageManager::Apt => "apt",
            PackageManager::Pacman => "pacman",
            PackageManager::Dnf => "dnf",
            PackageManager::Apk => "apk",
            PackageManager::Zypper => "zypper",
        }
    }

    /// Build the search command arguments
    fn search_args(&self, query: &str) -> Vec<String> {
        match self {
            PackageManager::Apt => vec!["search".to_string(), query.to_string()],
            PackageManager::Pacman => vec!["-Ss".to_string(), query.to_string()],
            PackageManager::Dnf => vec!["search".to_string(), query.to_string()],
            PackageManager::Apk => vec!["search".to_string(), query.to_string()],
            PackageManager::Zypper => vec!["search".to_string(), query.to_string()],
        }
    }

    /// Build the install command arguments
    fn install_args(&self, package: &str) -> Vec<String> {
        match self {
            PackageManager::Apt => vec!["install".to_string(), "-y".to_string(), package.to_string()],
            PackageManager::Pacman => vec!["-S".to_string(), "--noconfirm".to_string(), package.to_string()],
            PackageManager::Dnf => vec!["install".to_string(), "-y".to_string(), package.to_string()],
            PackageManager::Apk => vec!["add".to_string(), package.to_string()],
            PackageManager::Zypper => vec!["install".to_string(), "-y".to_string(), package.to_string()],
        }
    }

    /// Build the uninstall command arguments
    fn uninstall_args(&self, package: &str) -> Vec<String> {
        match self {
            PackageManager::Apt => vec!["remove".to_string(), "-y".to_string(), package.to_string()],
            PackageManager::Pacman => vec!["-R".to_string(), "--noconfirm".to_string(), package.to_string()],
            PackageManager::Dnf => vec!["remove".to_string(), "-y".to_string(), package.to_string()],
            PackageManager::Apk => vec!["del".to_string(), package.to_string()],
            PackageManager::Zypper => vec!["remove".to_string(), "-y".to_string(), package.to_string()],
        }
    }

    /// Check if a package is installed
    fn is_installed_args(&self, package: &str) -> Vec<String> {
        match self {
            PackageManager::Apt => vec!["list".to_string(), "--installed".to_string(), package.to_string()],
            PackageManager::Pacman => vec!["-Q".to_string(), package.to_string()],
            PackageManager::Dnf => vec!["list".to_string(), "--installed".to_string(), package.to_string()],
            PackageManager::Apk => vec!["info".to_string(), "-e".to_string(), package.to_string()],
            PackageManager::Zypper => vec!["search".to_string(), "-i".to_string(), package.to_string()],
        }
    }

    /// Parse search output into results
    fn parse_search_output(&self, output: &str, limit: usize) -> Vec<SearchResult> {
        let mut results = Vec::new();

        match self {
            PackageManager::Apt => {
                // apt search output format:
                // package-name/distribution version arch
                //   description
                for line in output.lines() {
                    if results.len() >= limit {
                        break;
                    }
                    // Skip lines that start with whitespace (descriptions)
                    if line.starts_with(' ') || line.is_empty() {
                        continue;
                    }
                    // Parse "package/repo version arch" format
                    if let Some(slash_idx) = line.find('/') {
                        let name = &line[..slash_idx];
                        let rest = &line[slash_idx + 1..];
                        let version = rest.split_whitespace().nth(1).map(|s| s.to_string());

                        results.push(SearchResult {
                            name: name.to_string(),
                            description: None,
                            version,
                            provider: Provider::System,
                            metadata: HashMap::new(),
                        });
                    }
                }
            }
            PackageManager::Pacman => {
                // pacman -Ss output format:
                // repo/package-name version
                //     description
                let mut current_name = None;
                let mut current_version = None;

                for line in output.lines() {
                    if results.len() >= limit {
                        break;
                    }
                    if line.starts_with(' ') {
                        // Description line
                        if let Some(name) = current_name.take() {
                            results.push(SearchResult {
                                name,
                                description: Some(line.trim().to_string()),
                                version: current_version.take(),
                                provider: Provider::System,
                                metadata: HashMap::new(),
                            });
                        }
                    } else if let Some(slash_idx) = line.find('/') {
                        // Package line
                        let name = line[slash_idx + 1..].split_whitespace().next().unwrap_or("").to_string();
                        let version = line.split_whitespace().nth(1).map(|s| s.to_string());
                        current_name = Some(name);
                        current_version = version;
                    }
                }
                // Handle last package if no description
                if let Some(name) = current_name {
                    results.push(SearchResult {
                        name,
                        description: None,
                        version: current_version,
                        provider: Provider::System,
                        metadata: HashMap::new(),
                    });
                }
            }
            PackageManager::Dnf => {
                // dnf search output format:
                // package-name.arch : description
                for line in output.lines() {
                    if results.len() >= limit {
                        break;
                    }
                    if line.contains(" : ") {
                        let parts: Vec<&str> = line.splitn(2, " : ").collect();
                        if parts.len() == 2 {
                            let name_arch = parts[0].trim();
                            let name = name_arch.split('.').next().unwrap_or(name_arch).to_string();
                            let description = parts[1].trim().to_string();

                            results.push(SearchResult {
                                name,
                                description: Some(description),
                                version: None,
                                provider: Provider::System,
                                metadata: HashMap::new(),
                            });
                        }
                    }
                }
            }
            PackageManager::Apk => {
                // apk search output format:
                // package-name-version
                for line in output.lines() {
                    if results.len() >= limit {
                        break;
                    }
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    // Try to split name and version (version usually after last -)
                    let name = line.to_string();
                    results.push(SearchResult {
                        name,
                        description: None,
                        version: None,
                        provider: Provider::System,
                        metadata: HashMap::new(),
                    });
                }
            }
            PackageManager::Zypper => {
                // zypper search output is tabular
                // S | Name | Summary | Type
                for line in output.lines() {
                    if results.len() >= limit {
                        break;
                    }
                    // Skip header lines
                    if line.starts_with('-') || line.starts_with('S') || line.is_empty() {
                        continue;
                    }
                    let parts: Vec<&str> = line.split('|').collect();
                    if parts.len() >= 3 {
                        let name = parts[1].trim().to_string();
                        let description = parts[2].trim().to_string();

                        results.push(SearchResult {
                            name,
                            description: Some(description),
                            version: None,
                            provider: Provider::System,
                            metadata: HashMap::new(),
                        });
                    }
                }
            }
        }

        results
    }
}

impl std::fmt::Display for PackageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageManager::Apt => write!(f, "apt"),
            PackageManager::Pacman => write!(f, "pacman"),
            PackageManager::Dnf => write!(f, "dnf"),
            PackageManager::Apk => write!(f, "apk"),
            PackageManager::Zypper => write!(f, "zypper"),
        }
    }
}

/// System package manager provider
pub struct SystemProvider {
    package_manager: Option<PackageManager>,
}

impl SystemProvider {
    /// Create a new system provider, detecting the package manager automatically
    pub fn new() -> Self {
        Self {
            package_manager: PackageManager::detect(),
        }
    }

    /// Create a system provider with a specific package manager
    pub fn with_package_manager(pm: PackageManager) -> Self {
        Self {
            package_manager: Some(pm),
        }
    }

    /// Get the detected package manager
    pub fn package_manager(&self) -> Option<PackageManager> {
        self.package_manager
    }
}

impl Default for SystemProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if we're running as root
fn nix_is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[async_trait]
impl Installer for SystemProvider {
    fn provider(&self) -> Provider {
        Provider::System
    }

    fn is_available(&self) -> bool {
        self.package_manager.is_some()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let pm = self.package_manager.ok_or_else(|| {
            SchalentierError::ProviderNotFound {
                provider: "system".to_string(),
            }
        })?;

        let args = pm.search_args(query);
        let output = Command::new(pm.command())
            .args(&args)
            .output()
            .map_err(|e| SchalentierError::CommandFailed(format!("Search failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(pm.parse_search_output(&stdout, limit))
    }

    async fn install(&self, name: &str, _version: Option<&str>) -> Result<InstallResult> {
        let pm = self.package_manager.ok_or_else(|| {
            SchalentierError::ProviderNotFound {
                provider: "system".to_string(),
            }
        })?;

        info!("Installing {} via {}...", name, pm);

        let args = pm.install_args(name);

        // Run with sudo and inherit stdin for password prompt
        let status = if !nix_is_root() {
            debug!("Running: sudo {} {:?}", pm.command(), args);
            Command::new("sudo")
                .arg(pm.command())
                .args(&args)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
        } else {
            debug!("Running: {} {:?}", pm.command(), args);
            Command::new(pm.command())
                .args(&args)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
        };

        let status = status.map_err(|e| {
            SchalentierError::InstallFailed {
                package: name.to_string(),
                reason: format!("Failed to run {}: {}", pm.command(), e),
            }
        })?;

        if status.success() {
            // Try to find the installed binary
            let binary_path = which::which(name).ok();

            Ok(InstallResult {
                path: binary_path,
                version: None, // Could parse from package manager output
                success: true,
                message: Some(format!("Installed {} via {}", name, pm)),
            })
        } else {
            Ok(InstallResult {
                path: None,
                version: None,
                success: false,
                message: Some(format!(
                    "Installation failed with exit code: {:?}",
                    status.code()
                )),
            })
        }
    }

    async fn uninstall(&self, name: &str) -> Result<()> {
        let pm = self.package_manager.ok_or_else(|| {
            SchalentierError::ProviderNotFound {
                provider: "system".to_string(),
            }
        })?;

        info!("Uninstalling {} via {}...", name, pm);

        let args = pm.uninstall_args(name);

        // Run with sudo and inherit stdin for password prompt
        let status = if !nix_is_root() {
            Command::new("sudo")
                .arg(pm.command())
                .args(&args)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
        } else {
            Command::new(pm.command())
                .args(&args)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
        };

        let status = status.map_err(|e| {
            SchalentierError::CommandFailed(format!("Uninstall failed: {}", e))
        })?;

        if status.success() {
            info!("{} uninstalled successfully", name);
            Ok(())
        } else {
            Err(SchalentierError::CommandFailed(format!(
                "Uninstall failed with exit code: {:?}",
                status.code()
            ))
            .into())
        }
    }

    async fn is_installed(&self, name: &str) -> Result<bool> {
        let pm = self.package_manager.ok_or_else(|| {
            SchalentierError::ProviderNotFound {
                provider: "system".to_string(),
            }
        })?;

        let args = pm.is_installed_args(name);
        let output = Command::new(pm.command())
            .args(&args)
            .output()
            .map_err(|e| SchalentierError::CommandFailed(format!("Check failed: {}", e)))?;

        Ok(output.status.success())
    }

    async fn installed_version(&self, name: &str) -> Result<Option<String>> {
        let pm = self.package_manager.ok_or_else(|| {
            SchalentierError::ProviderNotFound {
                provider: "system".to_string(),
            }
        })?;

        // Get version using package manager specific commands
        let output = match pm {
            PackageManager::Apt => {
                Command::new("dpkg")
                    .args(["-s", name])
                    .output()
            }
            PackageManager::Pacman => {
                Command::new("pacman")
                    .args(["-Q", name])
                    .output()
            }
            PackageManager::Dnf => {
                Command::new("rpm")
                    .args(["-q", name])
                    .output()
            }
            PackageManager::Apk => {
                Command::new("apk")
                    .args(["info", name])
                    .output()
            }
            PackageManager::Zypper => {
                Command::new("rpm")
                    .args(["-q", name])
                    .output()
            }
        };

        let output = output.map_err(|e| {
            SchalentierError::CommandFailed(format!("Version check failed: {}", e))
        })?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse version from output
        let version = match pm {
            PackageManager::Apt => {
                // Look for "Version: x.y.z" line
                stdout
                    .lines()
                    .find(|l| l.starts_with("Version:"))
                    .map(|l| l.trim_start_matches("Version:").trim().to_string())
            }
            PackageManager::Pacman => {
                // Output is "package version"
                stdout.split_whitespace().nth(1).map(|s| s.to_string())
            }
            PackageManager::Dnf | PackageManager::Zypper => {
                // Output is "package-version.arch"
                let name_version = stdout.trim();
                // Try to extract version after package name
                if name_version.starts_with(name) {
                    Some(name_version[name.len()..].trim_start_matches('-').to_string())
                } else {
                    Some(name_version.to_string())
                }
            }
            PackageManager::Apk => {
                // First line is usually package-version
                stdout.lines().next().map(|s| s.to_string())
            }
        };

        Ok(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_manager_detection() {
        // This test just verifies the detection logic runs without panicking
        // Actual detection depends on the system
        let _pm = PackageManager::detect();
    }

    #[test]
    fn test_apt_search_parsing() {
        let output = r#"Sorting...
Full Text Search...
git/jammy 1:2.34.1-1ubuntu1.10 amd64
  fast, scalable, distributed revision control system

git-all/jammy 1:2.34.1-1ubuntu1.10 all
  fast, scalable, distributed revision control system (all subpackages)
"#;

        let results = PackageManager::Apt.parse_search_output(output, 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "git");
    }

    #[test]
    fn test_pacman_search_parsing() {
        let output = r#"extra/git 2.44.0-1
    the fast distributed version control system
extra/git-lfs 3.5.1-1
    Git extension for versioning large files
"#;

        let results = PackageManager::Pacman.parse_search_output(output, 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "git");
        assert!(results[0].description.is_some());
    }

    #[test]
    fn test_dnf_search_parsing() {
        let output = r#"Last metadata expiration check: 0:05:23 ago
git.x86_64 : Fast Version Control System
git-all.noarch : Meta-package to pull in all git tools
"#;

        let results = PackageManager::Dnf.parse_search_output(output, 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].name, "git");
    }

    #[test]
    fn test_system_provider_creation() {
        let provider = SystemProvider::new();
        // Just verify it doesn't panic
        let _ = provider.is_available();
    }

    #[test]
    fn test_install_args() {
        assert!(PackageManager::Apt.install_args("git").contains(&"-y".to_string()));
        assert!(PackageManager::Pacman.install_args("git").contains(&"--noconfirm".to_string()));
        assert!(PackageManager::Dnf.install_args("git").contains(&"-y".to_string()));
    }
}
