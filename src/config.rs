use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Provider types available in Schalentier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    System,
    Conda,
    Cargo,
    Binary,
    Uv,
    Brew,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::System => write!(f, "system"),
            Provider::Conda => write!(f, "conda"),
            Provider::Cargo => write!(f, "cargo"),
            Provider::Binary => write!(f, "binary"),
            Provider::Uv => write!(f, "uv"),
            Provider::Brew => write!(f, "brew"),
        }
    }
}

/// A tool/package entry in the configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    /// The provider to use for this tool (optional, will use priority list if not specified)
    pub provider: Option<Provider>,
    /// Version constraint (e.g., ">=1.0", "~1.2.3", "latest")
    pub version: Option<String>,
    /// Additional provider-specific options
    #[serde(default)]
    pub options: HashMap<String, toml::Value>,
}

/// Main configuration file (schalentier.toml)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchalentierConfig {
    /// Global settings
    #[serde(default)]
    pub settings: Settings,

    /// Tools to manage (key = tool name, value = tool config)
    #[serde(default)]
    pub tools: HashMap<String, ToolEntry>,

    /// Sync configuration
    #[serde(default)]
    pub sync: SyncConfig,

    /// Dotfiles to manage (key = file path, value = settings)
    #[serde(default)]
    pub dotfiles: HashMap<String, toml::Value>,
}

/// Global settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Provider priority order (first match wins)
    #[serde(default = "default_provider_priority")]
    pub provider_priority: Vec<Provider>,

    /// Path to schalentier data directory
    pub data_dir: Option<PathBuf>,

    /// Whether to auto-update tools
    #[serde(default)]
    pub auto_update: bool,
}

fn default_provider_priority() -> Vec<Provider> {
    vec![
        Provider::Binary, // Fastest, no dependencies
        Provider::Cargo,  // Rust ecosystem
        Provider::Brew,   // Cross-platform package manager
        Provider::Conda,  // Python/scientific packages
        Provider::Uv,     // Python tools
        Provider::System, // OS package manager (requires sudo)
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider_priority: default_provider_priority(),
            data_dir: None,
            auto_update: false,
        }
    }
}

/// Sync configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Remote URL (git SSH, HTTPS, or gist URL)
    pub remote: Option<String>,

    /// Sync mode
    #[serde(default)]
    pub mode: SyncMode,

    /// Auto-sync on startup
    #[serde(default)]
    pub auto_sync: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncMode {
    #[default]
    Manual,
    Pull,
    Push,
    Bidirectional,
}

/// Status of a managed tool
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    /// Installed and managed by schalentier
    Installed,
    /// Tool exists but was not installed by schalentier (adopted)
    Adopted,
    /// In configuration but not yet installed
    Pending,
    /// Installation or update failed
    Failed,
}

/// Information about an installed tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledTool {
    /// The provider that installed this tool
    pub provider: Provider,
    /// Installed version
    pub version: Option<String>,
    /// Installation path (if applicable)
    pub path: Option<PathBuf>,
    /// Current status
    pub status: ToolStatus,
    /// Whether this tool is managed by schalentier (false = adopted)
    pub managed: bool,
    /// Installation timestamp
    pub installed_at: Option<String>,
    /// Last update check timestamp
    pub last_checked: Option<String>,
}

/// Local state file (~/.schalentier/local_state.json)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalState {
    /// Version of the state file format
    #[serde(default = "default_state_version")]
    pub version: u32,

    /// Whether schalentier has been initialized
    #[serde(default)]
    pub initialized: bool,

    /// Installed tools (key = tool name)
    #[serde(default)]
    pub tools: HashMap<String, InstalledTool>,

    /// Bootstrap status
    #[serde(default)]
    pub bootstrap: BootstrapState,

    /// Last sync timestamp
    pub last_sync: Option<String>,
}

fn default_state_version() -> u32 {
    1
}

/// Bootstrap component status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BootstrapState {
    /// Miniforge/conda installation status
    pub conda_installed: bool,
    pub conda_path: Option<PathBuf>,

    /// uv installation status
    pub uv_installed: bool,
    pub uv_path: Option<PathBuf>,

    /// Rust/cargo installation status
    pub rust_installed: bool,
    pub rust_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialize_deserialize() {
        let mut config = SchalentierConfig::default();
        config.tools.insert(
            "ripgrep".to_string(),
            ToolEntry {
                provider: Some(Provider::Cargo),
                version: Some(">=14.0".to_string()),
                options: HashMap::new(),
            },
        );

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: SchalentierConfig = toml::from_str(&toml_str).unwrap();

        assert!(parsed.tools.contains_key("ripgrep"));
        assert_eq!(parsed.tools["ripgrep"].provider, Some(Provider::Cargo));
    }

    #[test]
    fn test_local_state_serialize_deserialize() {
        let mut state = LocalState {
            initialized: true,
            ..Default::default()
        };
        state.tools.insert(
            "ripgrep".to_string(),
            InstalledTool {
                provider: Provider::Cargo,
                version: Some("14.1.0".to_string()),
                path: Some(PathBuf::from("/home/user/.cargo/bin/rg")),
                status: ToolStatus::Installed,
                managed: true,
                installed_at: Some("2024-01-15T10:30:00Z".to_string()),
                last_checked: None,
            },
        );

        let json_str = serde_json::to_string_pretty(&state).unwrap();
        let parsed: LocalState = serde_json::from_str(&json_str).unwrap();

        assert!(parsed.initialized);
        assert!(parsed.tools.contains_key("ripgrep"));
        assert_eq!(parsed.tools["ripgrep"].status, ToolStatus::Installed);
    }

    #[test]
    fn test_default_provider_priority() {
        let settings = Settings::default();
        assert_eq!(settings.provider_priority[0], Provider::Binary);
        assert_eq!(settings.provider_priority.len(), 6);
        // Binary first (fastest), System last (requires sudo)
        assert_eq!(
            settings.provider_priority.last().unwrap(),
            &Provider::System
        );
    }

    #[test]
    fn test_config_from_toml_string() {
        let toml_str = r#"
[settings]
provider_priority = ["cargo", "binary", "system"]
auto_update = true

[tools.ripgrep]
provider = "cargo"
version = ">=14.0"

[tools.fd]
provider = "binary"

[sync]
remote = "https://gist.github.com/user/abc123"
mode = "pull"
auto_sync = true
"#;

        let config: SchalentierConfig = toml::from_str(toml_str).unwrap();

        assert_eq!(config.settings.provider_priority.len(), 3);
        assert_eq!(config.settings.provider_priority[0], Provider::Cargo);
        assert!(config.settings.auto_update);
        assert!(config.tools.contains_key("ripgrep"));
        assert!(config.tools.contains_key("fd"));
        assert_eq!(config.sync.mode, SyncMode::Pull);
        assert!(config.sync.auto_sync);
    }

    #[test]
    fn test_minimal_config() {
        let toml_str = r#"
[tools.git]
"#;
        let config: SchalentierConfig = toml::from_str(toml_str).unwrap();
        assert!(config.tools.contains_key("git"));
        assert!(config.tools["git"].provider.is_none());
        assert!(config.tools["git"].version.is_none());
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(format!("{}", Provider::System), "system");
        assert_eq!(format!("{}", Provider::Conda), "conda");
        assert_eq!(format!("{}", Provider::Cargo), "cargo");
        assert_eq!(format!("{}", Provider::Binary), "binary");
        assert_eq!(format!("{}", Provider::Uv), "uv");
    }
}
