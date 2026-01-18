use crate::config::{LocalState, SchalentierConfig};
use crate::error::{Result, SchalentierError};
use anyhow::Context;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const STATE_FILE_NAME: &str = "local_state.json";
const CONFIG_FILE_NAME: &str = "schalentier.toml";
const DATA_DIR_NAME: &str = ".schalentier";
const CONFIG_DIR_NAME: &str = ".config/schalentier";

/// Get the default data directory (~/.schalentier)
pub fn default_data_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        SchalentierError::Config("Could not determine home directory".to_string())
    })?;
    Ok(home.join(DATA_DIR_NAME))
}

/// Get the config directory (~/.config/schalentier)
/// This is the directory that can be synced via git
pub fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        SchalentierError::Config("Could not determine home directory".to_string())
    })?;
    let dir = home.join(CONFIG_DIR_NAME);

    // Create if it doesn't exist
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config directory: {}", dir.display()))?;
    }

    Ok(dir)
}

/// Get the path to the local state file
pub fn state_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(STATE_FILE_NAME)
}

/// Get the path to the config file
/// Priority: current directory > ~/.config/schalentier/ > ~/
pub fn config_file_path() -> PathBuf {
    // First check current directory
    let local_config = PathBuf::from(CONFIG_FILE_NAME);
    if local_config.exists() {
        return local_config;
    }

    // Check config directory (~/.config/schalentier/)
    if let Ok(cfg_dir) = config_dir() {
        let cfg_path = cfg_dir.join(CONFIG_FILE_NAME);
        if cfg_path.exists() {
            return cfg_path;
        }
    }

    // Fall back to home directory (legacy location)
    if let Some(home) = dirs::home_dir() {
        let home_config = home.join(CONFIG_FILE_NAME);
        if home_config.exists() {
            return home_config;
        }
    }

    // Default to config directory (will be created if needed)
    if let Ok(cfg_dir) = config_dir() {
        return cfg_dir.join(CONFIG_FILE_NAME);
    }

    // Ultimate fallback to current directory
    local_config
}

/// Ensure the data directory exists with proper permissions
pub fn ensure_data_dir(data_dir: &Path) -> Result<()> {
    if !data_dir.exists() {
        info!("Creating data directory: {}", data_dir.display());
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data directory: {}", data_dir.display()))?;

        // Set directory permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            std::fs::set_permissions(data_dir, perms)
                .with_context(|| "Failed to set directory permissions")?;
        }
    }
    Ok(())
}

/// Set restrictive permissions on a file (Unix only)
#[cfg(unix)]
fn set_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_permissions(_path: &Path) -> Result<()> {
    // On Windows, file permissions work differently (ACLs)
    // For now, we skip this - could use windows-acl crate for proper support
    Ok(())
}

impl LocalState {
    /// Load the local state from the default location
    pub fn load() -> Result<Self> {
        let data_dir = default_data_dir()?;
        Self::load_from(&data_dir)
    }

    /// Load the local state from a specific data directory
    pub fn load_from(data_dir: &Path) -> Result<Self> {
        let state_path = state_file_path(data_dir);

        if !state_path.exists() {
            debug!("State file not found, returning default state");
            return Ok(LocalState::default());
        }

        debug!("Loading state from: {}", state_path.display());
        let content = std::fs::read_to_string(&state_path)
            .with_context(|| format!("Failed to read state file: {}", state_path.display()))?;

        let state: LocalState =
            serde_json::from_str(&content).with_context(|| "Failed to parse state file")?;

        debug!("Loaded state with {} tools", state.tools.len());
        Ok(state)
    }

    /// Save the local state to the default location
    pub fn save(&self) -> Result<()> {
        let data_dir = default_data_dir()?;
        self.save_to(&data_dir)
    }

    /// Save the local state to a specific data directory
    pub fn save_to(&self, data_dir: &Path) -> Result<()> {
        ensure_data_dir(data_dir)?;

        let state_path = state_file_path(data_dir);
        debug!("Saving state to: {}", state_path.display());

        let content =
            serde_json::to_string_pretty(self).with_context(|| "Failed to serialize state")?;

        std::fs::write(&state_path, &content)
            .with_context(|| format!("Failed to write state file: {}", state_path.display()))?;

        set_file_permissions(&state_path)?;

        debug!("State saved successfully");
        Ok(())
    }

    /// Check if schalentier has been initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl SchalentierConfig {
    /// Load the configuration from the default location
    pub fn load() -> Result<Self> {
        let config_path = config_file_path();
        Self::load_from(&config_path)
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            debug!(
                "Config file not found at {}, using defaults",
                path.display()
            );
            return Ok(SchalentierConfig::default());
        }

        debug!("Loading config from: {}", path.display());
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: SchalentierConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        debug!("Loaded config with {} tools", config.tools.len());
        Ok(config)
    }

    /// Save the configuration to the default location
    pub fn save(&self) -> Result<()> {
        let config_path = config_file_path();
        self.save_to(&config_path)
    }

    /// Save configuration to a specific path
    pub fn save_to(&self, path: &Path) -> Result<()> {
        debug!("Saving config to: {}", path.display());

        let content = toml::to_string_pretty(self).with_context(|| "Failed to serialize config")?;

        std::fs::write(path, &content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        debug!("Config saved successfully");
        Ok(())
    }

    /// Merge settings from another config (user config overrides defaults)
    pub fn merge_settings(&mut self, other: &SchalentierConfig) {
        // Provider priority: use other's if specified
        if !other.settings.provider_priority.is_empty() {
            self.settings.provider_priority = other.settings.provider_priority.clone();
        }

        // Data dir: use other's if specified
        if other.settings.data_dir.is_some() {
            self.settings.data_dir = other.settings.data_dir.clone();
        }

        // Auto update: use other's value
        self.settings.auto_update = other.settings.auto_update;

        // Merge tools (other's tools override)
        for (name, entry) in &other.tools {
            self.tools.insert(name.clone(), entry.clone());
        }

        // Sync config: use other's if remote is specified
        if other.sync.remote.is_some() {
            self.sync = other.sync.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{InstalledTool, Provider, ToolEntry, ToolStatus};
    use tempfile::TempDir;

    #[test]
    fn test_state_save_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        let mut state = LocalState {
            initialized: true,
            ..Default::default()
        };
        state.tools.insert(
            "test-tool".to_string(),
            InstalledTool {
                provider: Provider::Cargo,
                version: Some("1.0.0".to_string()),
                path: None,
                status: ToolStatus::Installed,
                managed: true,
                installed_at: None,
                last_checked: None,
            },
        );

        // Save
        state.save_to(data_dir).unwrap();

        // Verify file exists
        let state_path = state_file_path(data_dir);
        assert!(state_path.exists());

        // Load
        let loaded = LocalState::load_from(data_dir).unwrap();
        assert!(loaded.initialized);
        assert!(loaded.tools.contains_key("test-tool"));
        assert_eq!(loaded.tools["test-tool"].provider, Provider::Cargo);
    }

    #[test]
    fn test_config_save_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        let mut config = SchalentierConfig::default();
        config.tools.insert(
            "ripgrep".to_string(),
            ToolEntry {
                provider: Some(Provider::Cargo),
                version: Some(">=14.0".to_string()),
                options: std::collections::HashMap::new(),
            },
        );

        // Save
        config.save_to(&config_path).unwrap();

        // Verify file exists
        assert!(config_path.exists());

        // Load
        let loaded = SchalentierConfig::load_from(&config_path).unwrap();
        assert!(loaded.tools.contains_key("ripgrep"));
    }

    #[test]
    fn test_load_nonexistent_state_returns_default() {
        let temp_dir = TempDir::new().unwrap();
        let state = LocalState::load_from(temp_dir.path()).unwrap();
        assert!(!state.initialized);
        assert!(state.tools.is_empty());
    }

    #[test]
    fn test_load_nonexistent_config_returns_default() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.toml");
        let config = SchalentierConfig::load_from(&config_path).unwrap();
        assert!(config.tools.is_empty());
        assert!(!config.settings.provider_priority.is_empty());
    }

    #[test]
    fn test_ensure_data_dir_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("new_data_dir");

        assert!(!data_dir.exists());
        ensure_data_dir(&data_dir).unwrap();
        assert!(data_dir.exists());
    }

    #[test]
    fn test_config_merge_settings() {
        let mut base = SchalentierConfig::default();

        let mut override_config = SchalentierConfig::default();
        override_config.settings.provider_priority = vec![Provider::Cargo, Provider::Binary];
        override_config.settings.auto_update = true;
        override_config.tools.insert(
            "custom-tool".to_string(),
            ToolEntry {
                provider: None,
                version: None,
                options: std::collections::HashMap::new(),
            },
        );

        base.merge_settings(&override_config);

        assert_eq!(base.settings.provider_priority.len(), 2);
        assert_eq!(base.settings.provider_priority[0], Provider::Cargo);
        assert!(base.settings.auto_update);
        assert!(base.tools.contains_key("custom-tool"));
    }

    #[cfg(unix)]
    #[test]
    fn test_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        let state = LocalState::default();
        state.save_to(data_dir).unwrap();

        let state_path = state_file_path(data_dir);
        let metadata = std::fs::metadata(&state_path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "State file should have 0600 permissions");
    }
}
