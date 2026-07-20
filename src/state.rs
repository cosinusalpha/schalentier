use crate::config::{LocalState, SchalentierConfig};
use crate::error::{Result, SchalentierError};
use anyhow::Context;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const STATE_FILE_NAME: &str = "local_state.json";
const CONFIG_FILE_NAME: &str = "schalentier.toml";
const DATA_DIR_NAME: &str = ".schalentier";
const CONFIG_DIR_NAME: &str = ".config/schalentier";
const PROJECT_DIR_NAME: &str = ".schalentier";
const PROJECT_CONFIG_FILE_NAME: &str = "config.toml";

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

/// Find a project-local config by walking up from `start` looking for
/// `.schalentier/config.toml`. Stops at the user's home directory or the
/// filesystem root, whichever comes first.
pub fn find_project_config_from(start: &Path) -> Option<PathBuf> {
    let home = dirs::home_dir();
    let mut dir = start.to_path_buf();

    loop {
        let candidate = dir.join(PROJECT_DIR_NAME).join(PROJECT_CONFIG_FILE_NAME);
        if candidate.exists() {
            return Some(candidate);
        }

        if home.as_deref() == Some(dir.as_path()) {
            return None;
        }

        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return None,
        }
    }
}

/// Find a project-local config by walking up from the current working directory.
pub fn find_project_config() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| find_project_config_from(&cwd))
}

/// Directory containing the discovered project config, if any (i.e. the
/// `.schalentier/` directory itself — used to locate project-local secrets.enc).
pub fn project_dir_from(project_config_path: &Path) -> Option<PathBuf> {
    project_config_path.parent().map(Path::to_path_buf)
}

/// Ensure the data directory exists with proper permissions
pub fn ensure_data_dir(data_dir: &Path) -> Result<()> {
    if !data_dir.exists() {
        info!("Creating data directory: {}", data_dir.display());
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data directory: {}", data_dir.display()))?;

        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(data_dir, perms)
            .with_context(|| "Failed to set directory permissions")?;
    }
    Ok(())
}

/// Set restrictive permissions on a file
fn set_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
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

    /// Load the global configuration, then merge a project-local
    /// `.schalentier/config.toml` on top if one is found by walking up from the
    /// current directory (project values win, per [`Self::merge_settings`]).
    pub fn load_with_project() -> Result<Self> {
        let mut config = Self::load()?;

        if let Some(project_path) = find_project_config() {
            debug!("Found project config: {}", project_path.display());
            let project_config = Self::load_from(&project_path)?;
            config.merge_settings(&project_config);
        }

        Ok(config)
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

        // Merge dotfiles (other's entries override on path conflict)
        for (path, entry) in &other.dotfiles {
            self.dotfiles.insert(path.clone(), entry.clone());
        }

        // Sync config: use other's if remote is specified
        if other.sync.remote.is_some() {
            self.sync = other.sync.clone();
        }

        // Variables: deep merge (other's values override on key conflict)
        crate::dotfiles::deep_merge_toml(&mut self.variables, &other.variables);
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

    #[test]
    fn test_find_project_config_in_current_dir() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join(".schalentier");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("config.toml"), "").unwrap();

        let found = find_project_config_from(temp_dir.path()).unwrap();
        assert_eq!(found, project_dir.join("config.toml"));
    }

    #[test]
    fn test_find_project_config_walks_up_from_subdirectory() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path().join(".schalentier");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("config.toml"), "").unwrap();

        let nested = temp_dir.path().join("src").join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        let found = find_project_config_from(&nested).unwrap();
        assert_eq!(found, project_dir.join("config.toml"));
    }

    #[test]
    fn test_find_project_config_none_when_absent() {
        let temp_dir = TempDir::new().unwrap();
        let nested = temp_dir.path().join("no").join("project").join("here");
        std::fs::create_dir_all(&nested).unwrap();

        assert!(find_project_config_from(&nested).is_none());
    }

    #[test]
    fn test_project_dir_from() {
        let path = Path::new("/home/user/project/.schalentier/config.toml");
        assert_eq!(
            project_dir_from(path),
            Some(PathBuf::from("/home/user/project/.schalentier"))
        );
    }

    #[test]
    fn test_merge_settings_merges_dotfiles_and_variables() {
        let mut base = SchalentierConfig::default();
        base.dotfiles.insert(
            "~/.gitconfig".to_string(),
            toml::Value::Table(toml::map::Map::new()),
        );

        let mut override_config = SchalentierConfig::default();
        override_config.dotfiles.insert(
            "~/.npmrc".to_string(),
            toml::Value::Table(toml::map::Map::new()),
        );
        let mut vars = toml::map::Map::new();
        vars.insert(
            "project_name".to_string(),
            toml::Value::String("my-app".to_string()),
        );
        override_config.variables = toml::Value::Table(vars);

        base.merge_settings(&override_config);

        assert!(base.dotfiles.contains_key("~/.gitconfig"));
        assert!(base.dotfiles.contains_key("~/.npmrc"));
        assert_eq!(
            base.variables.get("project_name").and_then(|v| v.as_str()),
            Some("my-app")
        );
    }
}
