//! Tool detection for schalentier init.
//!
//! Detects already-installed package managers and development tools
//! to help users make informed decisions during initialization.

use crate::provider::system::PackageManager;
use std::process::Command;
use tracing::debug;

/// Detection result for a single tool
#[derive(Debug, Clone)]
pub struct ToolDetection {
    /// Name of the tool
    pub name: String,
    /// Whether the tool is available on the system
    pub available: bool,
    /// Version string (if available)
    pub version: Option<String>,
    /// Path to the tool binary (if available)
    pub path: Option<String>,
}

impl ToolDetection {
    /// Create a new detection result
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            available: false,
            version: None,
            path: None,
        }
    }

    /// Mark as available with a path
    fn with_path(mut self, path: String) -> Self {
        self.available = true;
        self.path = Some(path);
        self
    }

    /// Add version information
    fn with_version(mut self, version: String) -> Self {
        self.version = Some(version);
        self
    }
}

/// Tool detector for common package managers and development tools
pub struct ToolDetector;

impl ToolDetector {
    /// Detect all available tools
    pub fn detect_all() -> DetectionResults {
        let mut results = DetectionResults::default();

        // Detect tools in order
        results.uv = Self::detect_uv();
        results.conda = Self::detect_conda();
        results.brew = Self::detect_brew();
        results.cargo = Self::detect_cargo();
        results.system_pm = Self::detect_system_pm();

        results
    }

    /// Detect uv (Python package installer)
    fn detect_uv() -> ToolDetection {
        let mut detection = ToolDetection::new("uv");

        if let Ok(output) = Command::new("uv").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                debug!("Detected uv: {}", version);

                // Try to get the full path
                if let Ok(path) = which::which("uv") {
                    detection = detection.with_path(path.display().to_string());
                }

                return detection.with_version(version);
            }
        }

        detection
    }

    /// Detect conda/mamba
    fn detect_conda() -> ToolDetection {
        let mut detection = ToolDetection::new("conda");

        // Try conda first
        if let Ok(output) = Command::new("conda").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                debug!("Detected conda: {}", version);

                if let Ok(path) = which::which("conda") {
                    detection = detection.with_path(path.display().to_string());
                }

                return detection.with_version(version);
            }
        }

        // Try mamba as fallback
        if let Ok(output) = Command::new("mamba").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                debug!("Detected mamba: {}", version);

                if let Ok(path) = which::which("mamba") {
                    detection = detection.with_path(path.display().to_string());
                }

                return detection.with_version(version);
            }
        }

        detection
    }

    /// Detect brew (Homebrew/Linuxbrew)
    fn detect_brew() -> ToolDetection {
        let mut detection = ToolDetection::new("brew");

        if let Ok(output) = Command::new("brew").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                debug!("Detected brew: {}", version);

                if let Ok(path) = which::which("brew") {
                    detection = detection.with_path(path.display().to_string());
                }

                return detection.with_version(version);
            }
        }

        detection
    }

    /// Detect cargo (Rust package manager)
    fn detect_cargo() -> ToolDetection {
        let mut detection = ToolDetection::new("cargo");

        if let Ok(output) = Command::new("cargo").arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                debug!("Detected cargo: {}", version);

                if let Ok(path) = which::which("cargo") {
                    detection = detection.with_path(path.display().to_string());
                }

                return detection.with_version(version);
            }
        }

        detection
    }

    /// Detect system package manager (apt, pacman, dnf, apk, zypper)
    fn detect_system_pm() -> ToolDetection {
        if let Some(pm) = PackageManager::detect() {
            let display_name = pm.display_name();
            let version = pm.get_version().ok().flatten();
            debug!("Detected system package manager: {}", display_name);

            let mut detection = ToolDetection::new(display_name);
            detection = detection.with_path(pm.binary_path().to_string());

            if let Some(v) = version {
                detection = detection.with_version(v);
            }

            return detection;
        }

        ToolDetection::new("system-pm")
    }
}

/// Results of tool detection
#[derive(Debug, Clone, Default)]
pub struct DetectionResults {
    pub uv: ToolDetection,
    pub conda: ToolDetection,
    pub brew: ToolDetection,
    pub cargo: ToolDetection,
    pub system_pm: ToolDetection,
}

impl DetectionResults {
    /// Get all detected tools as a list
    pub fn all(&self) -> Vec<&ToolDetection> {
        vec![
            &self.uv,
            &self.conda,
            &self.brew,
            &self.cargo,
            &self.system_pm,
        ]
    }

    /// Get count of available tools
    pub fn count_available(&self) -> usize {
        self.all().iter().filter(|t| t.available).count()
    }

    /// Check if any system package manager is available
    pub fn has_system_pm(&self) -> bool {
        self.system_pm.available
    }

    /// Check if any of the tools we might bootstrap are available
    pub fn has_alternative_tools(&self) -> bool {
        self.brew.available || self.cargo.available || self.has_system_pm()
    }
}

impl Default for ToolDetection {
    fn default() -> Self {
        Self {
            name: String::new(),
            available: false,
            version: None,
            path: None,
        }
    }
}

// Extend PackageManager with display and version methods
pub trait PackageManagerExt {
    fn display_name(&self) -> String;
    fn binary_path(&self) -> &'static str;
    fn get_version(&self) -> crate::error::Result<Option<String>>;
}

impl PackageManagerExt for PackageManager {
    fn display_name(&self) -> String {
        match self {
            PackageManager::Apt => "apt (Debian/Ubuntu)".to_string(),
            PackageManager::Pacman => "pacman (Arch)".to_string(),
            PackageManager::Dnf => "dnf (Fedora/RHEL)".to_string(),
            PackageManager::Apk => "apk (Alpine)".to_string(),
            PackageManager::Zypper => "zypper (openSUSE)".to_string(),
        }
    }

    fn binary_path(&self) -> &'static str {
        match self {
            PackageManager::Apt => "/usr/bin/apt",
            PackageManager::Pacman => "/usr/bin/pacman",
            PackageManager::Dnf => "/usr/bin/dnf",
            PackageManager::Apk => "/sbin/apk",
            PackageManager::Zypper => "/usr/bin/zypper",
        }
    }

    fn get_version(&self) -> crate::error::Result<Option<String>> {
        let cmd = match self {
            PackageManager::Apt => "apt",
            PackageManager::Pacman => "pacman",
            PackageManager::Dnf => "dnf",
            PackageManager::Apk => "apk",
            PackageManager::Zypper => "zypper",
        };

        match Command::new(cmd).arg("--version").output() {
            Ok(output) if output.status.success() => Ok(Some(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string(),
            )),
            _ => Ok(None),
        }
    }
}
