//! Dotfile and config file patching system
//!
//! Supports intelligent merging of configuration settings into various file formats:
//! - JSON: Deep merge using serde_json
//! - TOML: Deep merge using toml crate
//! - YAML: Deep merge using serde_yaml
//! - INI: Section-aware merge
//! - KeyValue: Simple KEY=VALUE format (.env files)
//! - Unknown: Replace mode (complete file replacement)

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

use crate::error::Result;
use crate::template::{self, TemplateContext};

/// Supported configuration file formats
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigFormat {
    Json,
    Toml,
    Yaml,
    Ini,
    KeyValue,
    Unknown,
}

impl ConfigFormat {
    /// Detect format from file path
    pub fn detect(path: &Path) -> Self {
        // First check by extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_lowercase().as_str() {
                "json" => return ConfigFormat::Json,
                "toml" => return ConfigFormat::Toml,
                "yaml" | "yml" => return ConfigFormat::Yaml,
                "ini" => return ConfigFormat::Ini,
                "env" => return ConfigFormat::KeyValue,
                "conf" => {
                    // .conf could be INI or other, check filename
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.contains("git") {
                            return ConfigFormat::Ini;
                        }
                    }
                    return ConfigFormat::Ini; // Default .conf to INI
                }
                _ => {}
            }
        }

        // Check by known filename patterns
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            match name {
                ".gitconfig" | ".gitignore" => return ConfigFormat::Ini,
                ".env" => return ConfigFormat::KeyValue,
                "settings.json" | "config.json" => return ConfigFormat::Json,
                "config.toml" | "starship.toml" | "alacritty.toml" => return ConfigFormat::Toml,
                _ => {
                    // Check for .env.* pattern
                    if name.starts_with(".env.") || name.starts_with("env.") {
                        return ConfigFormat::KeyValue;
                    }
                }
            }
        }

        ConfigFormat::Unknown
    }
}

/// A single dotfile patch definition
#[derive(Debug, Clone)]
pub struct DotfilePatch {
    /// Target file path (expanded from ~)
    pub target: PathBuf,
    /// Detected or overridden format
    pub format: ConfigFormat,
    /// Settings to apply (for structured formats)
    pub settings: Option<TomlValue>,
    /// Raw content (for replace mode)
    pub content: Option<String>,
}

/// Manages dotfile patching operations
pub struct DotfileManager {
    /// Parsed dotfile patches from config
    patches: Vec<DotfilePatch>,
}

impl DotfileManager {
    /// Create a new DotfileManager from the dotfiles section of config.
    ///
    /// `ctx` is required if any entry sets `_template = true`; those entries have
    /// their string values (and `_content`) rendered through minijinja before
    /// merging. Entries without `_template` are applied unchanged, as before.
    pub fn from_config(dotfiles: &HashMap<String, TomlValue>) -> Result<Self> {
        Self::from_config_with_context(dotfiles, None)
    }

    /// Same as [`Self::from_config`], but renders `_template = true` entries
    /// using the given [`TemplateContext`].
    pub fn from_config_with_context(
        dotfiles: &HashMap<String, TomlValue>,
        ctx: Option<&TemplateContext>,
    ) -> Result<Self> {
        let mut patches = Vec::new();

        for (path_str, value) in dotfiles {
            let target = expand_path(path_str);
            let format = ConfigFormat::detect(&target);

            // Check for _content (replace mode) or _format override
            let (settings, content, final_format) = if let Some(table) = value.as_table() {
                let is_templated = table
                    .get("_template")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let content = table
                    .get("_content")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let content = match (&content, is_templated) {
                    (Some(c), true) => {
                        Some(render_templated(c, ctx, path_str)?)
                    }
                    _ => content,
                };

                let format_override = table.get("_format").and_then(|v| v.as_str());

                let final_format = if let Some(fmt) = format_override {
                    match fmt.to_lowercase().as_str() {
                        "json" => ConfigFormat::Json,
                        "toml" => ConfigFormat::Toml,
                        "yaml" | "yml" => ConfigFormat::Yaml,
                        "ini" => ConfigFormat::Ini,
                        "keyvalue" | "env" => ConfigFormat::KeyValue,
                        _ => format.clone(),
                    }
                } else if content.is_some() {
                    ConfigFormat::Unknown // Replace mode
                } else {
                    format.clone()
                };

                // Filter out special keys for settings
                let mut settings_table = toml::map::Map::new();
                for (k, v) in table {
                    if !k.starts_with('_') {
                        let v = if is_templated {
                            render_templated_toml_value(v, ctx, path_str)?
                        } else {
                            v.clone()
                        };
                        settings_table.insert(k.clone(), v);
                    }
                }

                let settings = if settings_table.is_empty() {
                    None
                } else {
                    Some(TomlValue::Table(settings_table))
                };

                (settings, content, final_format)
            } else {
                (Some(value.clone()), None, format)
            };

            patches.push(DotfilePatch {
                target,
                format: final_format,
                settings,
                content,
            });
        }

        Ok(Self { patches })
    }

    /// Get list of all managed dotfiles
    pub fn list(&self) -> Vec<&DotfilePatch> {
        self.patches.iter().collect()
    }

    /// Show diff of what would change (dry-run)
    pub fn diff(&self) -> Result<Vec<DotfileDiff>> {
        let mut diffs = Vec::new();

        for patch in &self.patches {
            let diff = self.compute_diff(patch)?;
            diffs.push(diff);
        }

        Ok(diffs)
    }

    /// Apply all patches
    pub fn apply(&self) -> Result<Vec<ApplyResult>> {
        let mut results = Vec::new();

        for patch in &self.patches {
            let result = self.apply_patch(patch)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Reset a file from its backup
    pub fn reset(&self, file_path: &str) -> Result<()> {
        let target = expand_path(file_path);
        let backup = backup_path(&target);

        if !backup.exists() {
            anyhow::bail!("No backup found for {}", target.display());
        }

        fs::copy(&backup, &target)?;
        tracing::info!("Restored {} from backup", target.display());

        Ok(())
    }

    /// Compute diff for a single patch
    fn compute_diff(&self, patch: &DotfilePatch) -> Result<DotfileDiff> {
        let exists = patch.target.exists();
        let current_content = if exists {
            fs::read_to_string(&patch.target).ok()
        } else {
            None
        };

        let new_content = self.generate_content(patch, current_content.as_deref())?;

        Ok(DotfileDiff {
            path: patch.target.clone(),
            format: patch.format.clone(),
            exists,
            would_create: !exists,
            would_modify: exists && current_content.as_deref() != Some(&new_content),
            current: current_content,
            proposed: new_content,
        })
    }

    /// Apply a single patch
    fn apply_patch(&self, patch: &DotfilePatch) -> Result<ApplyResult> {
        // Ensure parent directory exists
        if let Some(parent) = patch.target.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create backup if file exists and no backup yet
        if patch.target.exists() {
            let backup = backup_path(&patch.target);
            if !backup.exists() {
                fs::copy(&patch.target, &backup)?;
                tracing::debug!("Created backup: {}", backup.display());
            }
        }

        let current_content = if patch.target.exists() {
            fs::read_to_string(&patch.target).ok()
        } else {
            None
        };

        let new_content = self.generate_content(patch, current_content.as_deref())?;

        // Check if content would change
        if current_content.as_deref() == Some(&new_content) {
            return Ok(ApplyResult {
                path: patch.target.clone(),
                action: ApplyAction::Unchanged,
            });
        }

        // Write new content
        fs::write(&patch.target, &new_content)?;

        let action = if current_content.is_some() {
            ApplyAction::Updated
        } else {
            ApplyAction::Created
        };

        Ok(ApplyResult {
            path: patch.target.clone(),
            action,
        })
    }

    /// Generate the new content for a patch
    fn generate_content(&self, patch: &DotfilePatch, current: Option<&str>) -> Result<String> {
        // Replace mode - just use the content directly
        if let Some(ref content) = patch.content {
            return Ok(content.trim().to_string() + "\n");
        }

        let settings = match &patch.settings {
            Some(s) => s,
            None => return Ok(current.unwrap_or("").to_string()),
        };

        match patch.format {
            ConfigFormat::Json => merge_json(current, settings),
            ConfigFormat::Toml => merge_toml(current, settings),
            ConfigFormat::Yaml => merge_yaml(current, settings),
            ConfigFormat::Ini => merge_ini(current, settings),
            ConfigFormat::KeyValue => merge_keyvalue(current, settings),
            ConfigFormat::Unknown => {
                // Replace mode for unknown
                if let Some(content) = &patch.content {
                    Ok(content.clone())
                } else {
                    // Try to serialize settings as TOML
                    Ok(toml::to_string_pretty(settings)?)
                }
            }
        }
    }
}

/// Result of diff operation
#[derive(Debug)]
pub struct DotfileDiff {
    pub path: PathBuf,
    pub format: ConfigFormat,
    pub exists: bool,
    pub would_create: bool,
    pub would_modify: bool,
    pub current: Option<String>,
    pub proposed: String,
}

/// Result of apply operation
#[derive(Debug)]
pub struct ApplyResult {
    pub path: PathBuf,
    pub action: ApplyAction,
}

#[derive(Debug, PartialEq)]
pub enum ApplyAction {
    Created,
    Updated,
    Unchanged,
}

// === Format-specific merge functions ===

/// Merge JSON content with new settings
fn merge_json(current: Option<&str>, settings: &TomlValue) -> Result<String> {
    // Parse current JSON or start with empty object
    let mut current_json: JsonValue = if let Some(content) = current {
        serde_json::from_str(content).unwrap_or(JsonValue::Object(serde_json::Map::new()))
    } else {
        JsonValue::Object(serde_json::Map::new())
    };

    // Convert TOML settings to JSON
    let settings_json = toml_to_json(settings);

    // Deep merge
    deep_merge_json(&mut current_json, &settings_json);

    // Pretty print
    Ok(serde_json::to_string_pretty(&current_json)? + "\n")
}

/// Merge TOML content with new settings
fn merge_toml(current: Option<&str>, settings: &TomlValue) -> Result<String> {
    // Parse current TOML or start with empty table
    let mut current_toml: TomlValue = if let Some(content) = current {
        toml::from_str(content).unwrap_or(TomlValue::Table(toml::map::Map::new()))
    } else {
        TomlValue::Table(toml::map::Map::new())
    };

    // Deep merge
    deep_merge_toml(&mut current_toml, settings);

    // Pretty print
    Ok(toml::to_string_pretty(&current_toml)?)
}

/// Merge YAML content with new settings
fn merge_yaml(current: Option<&str>, settings: &TomlValue) -> Result<String> {
    // Convert TOML settings to JSON first (for easier manipulation)
    let settings_json = toml_to_json(settings);

    // Parse current YAML or start with empty object
    let mut current_yaml: JsonValue = if let Some(content) = current {
        serde_yaml::from_str(content).unwrap_or(JsonValue::Object(serde_json::Map::new()))
    } else {
        JsonValue::Object(serde_json::Map::new())
    };

    // Deep merge
    deep_merge_json(&mut current_yaml, &settings_json);

    // Pretty print as YAML
    Ok(serde_yaml::to_string(&current_yaml)?)
}

/// Merge INI content with new settings
fn merge_ini(current: Option<&str>, settings: &TomlValue) -> Result<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();

    // Parse existing INI
    if let Some(content) = current {
        let mut current_section = String::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                current_section = trimmed[1..trimmed.len() - 1].to_string();
                sections.entry(current_section.clone()).or_default();
            } else if let Some(eq_pos) = trimmed.find('=') {
                let key = trimmed[..eq_pos].trim().to_string();
                let value = trimmed[eq_pos + 1..].trim().to_string();
                sections
                    .entry(current_section.clone())
                    .or_default()
                    .insert(key, value);
            }
        }
    }

    // Apply new settings
    if let Some(table) = settings.as_table() {
        for (section, values) in table {
            if let Some(section_table) = values.as_table() {
                let section_map = sections.entry(section.clone()).or_default();
                for (key, value) in section_table {
                    let value_str = toml_value_to_ini_string(value);
                    section_map.insert(key.clone(), value_str);
                }
            }
        }
    }

    // Rebuild INI
    // First, write entries without a section (root level)
    if let Some(root) = sections.get("") {
        for (key, value) in root {
            lines.push(format!("{} = {}", key, value));
        }
        if !root.is_empty() {
            lines.push(String::new());
        }
    }

    // Then write each section
    for (section, entries) in &sections {
        if section.is_empty() {
            continue;
        }
        lines.push(format!("[{}]", section));
        for (key, value) in entries {
            lines.push(format!("{} = {}", key, value));
        }
        lines.push(String::new());
    }

    Ok(lines.join("\n"))
}

/// Merge KeyValue (.env) content with new settings
fn merge_keyvalue(current: Option<&str>, settings: &TomlValue) -> Result<String> {
    let mut entries: HashMap<String, String> = HashMap::new();
    let mut key_order: Vec<String> = Vec::new();

    // Parse existing content
    if let Some(content) = current {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(eq_pos) = trimmed.find('=') {
                let key = trimmed[..eq_pos].trim().to_string();
                // Handle export prefix
                let key = key.strip_prefix("export ").unwrap_or(&key).to_string();
                let value = trimmed[eq_pos + 1..].trim().to_string();
                if !entries.contains_key(&key) {
                    key_order.push(key.clone());
                }
                entries.insert(key, value);
            }
        }
    }

    // Apply new settings
    if let Some(table) = settings.as_table() {
        for (key, value) in table {
            let value_str = match value {
                TomlValue::String(s) => s.clone(),
                TomlValue::Integer(i) => i.to_string(),
                TomlValue::Float(f) => f.to_string(),
                TomlValue::Boolean(b) => b.to_string(),
                _ => toml::to_string(value).unwrap_or_default(),
            };
            if !entries.contains_key(key) {
                key_order.push(key.clone());
            }
            entries.insert(key.clone(), value_str);
        }
    }

    // Rebuild content preserving order
    let mut lines: Vec<String> = Vec::new();
    for key in &key_order {
        if let Some(value) = entries.get(key) {
            lines.push(format!("{}={}", key, value));
        }
    }

    Ok(lines.join("\n") + "\n")
}

// === Helper functions ===

/// Render a single string through the template engine, if a context is available.
/// Errors are annotated with the dotfile path they occurred in.
fn render_templated(text: &str, ctx: Option<&TemplateContext>, dotfile_path: &str) -> Result<String> {
    let Some(ctx) = ctx else {
        return Ok(text.to_string());
    };
    template::render(text, ctx)
        .map_err(|e| anyhow::anyhow!("Template error in {}\n  \u{2192} {}", dotfile_path, e))
}

/// Render every string leaf in a TOML value through the template engine.
fn render_templated_toml_value(
    value: &TomlValue,
    ctx: Option<&TemplateContext>,
    dotfile_path: &str,
) -> Result<TomlValue> {
    match value {
        TomlValue::String(s) => Ok(TomlValue::String(render_templated(s, ctx, dotfile_path)?)),
        TomlValue::Array(arr) => {
            let rendered: Result<Vec<_>> = arr
                .iter()
                .map(|v| render_templated_toml_value(v, ctx, dotfile_path))
                .collect();
            Ok(TomlValue::Array(rendered?))
        }
        TomlValue::Table(table) => {
            let mut rendered = toml::map::Map::new();
            for (k, v) in table {
                rendered.insert(k.clone(), render_templated_toml_value(v, ctx, dotfile_path)?);
            }
            Ok(TomlValue::Table(rendered))
        }
        other => Ok(other.clone()),
    }
}

/// Expand ~ to home directory
fn expand_path(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

/// Get backup path for a file
fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.to_path_buf();
    let name = backup
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    backup.set_file_name(format!("{}.schalentier-backup", name));
    backup
}

/// Convert TOML value to JSON value
fn toml_to_json(toml: &TomlValue) -> JsonValue {
    match toml {
        TomlValue::String(s) => JsonValue::String(s.clone()),
        TomlValue::Integer(i) => JsonValue::Number((*i).into()),
        TomlValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        TomlValue::Boolean(b) => JsonValue::Bool(*b),
        TomlValue::Array(arr) => JsonValue::Array(arr.iter().map(toml_to_json).collect()),
        TomlValue::Table(table) => {
            let map: serde_json::Map<String, JsonValue> = table
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_json(v)))
                .collect();
            JsonValue::Object(map)
        }
        TomlValue::Datetime(dt) => JsonValue::String(dt.to_string()),
    }
}

/// Deep merge JSON values (target is modified in place)
fn deep_merge_json(target: &mut JsonValue, source: &JsonValue) {
    match (target, source) {
        (JsonValue::Object(target_map), JsonValue::Object(source_map)) => {
            for (key, source_value) in source_map {
                if let Some(target_value) = target_map.get_mut(key) {
                    deep_merge_json(target_value, source_value);
                } else {
                    target_map.insert(key.clone(), source_value.clone());
                }
            }
        }
        (target, source) => {
            *target = source.clone();
        }
    }
}

/// Deep merge TOML values (target is modified in place)
pub(crate) fn deep_merge_toml(target: &mut TomlValue, source: &TomlValue) {
    match (target, source) {
        (TomlValue::Table(target_map), TomlValue::Table(source_map)) => {
            for (key, source_value) in source_map {
                if let Some(target_value) = target_map.get_mut(key) {
                    deep_merge_toml(target_value, source_value);
                } else {
                    target_map.insert(key.clone(), source_value.clone());
                }
            }
        }
        (target, source) => {
            *target = source.clone();
        }
    }
}

/// Convert TOML value to INI string representation
fn toml_value_to_ini_string(value: &TomlValue) -> String {
    match value {
        TomlValue::String(s) => s.clone(),
        TomlValue::Integer(i) => i.to_string(),
        TomlValue::Float(f) => f.to_string(),
        TomlValue::Boolean(b) => b.to_string(),
        _ => toml::to_string(value).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_detection() {
        assert_eq!(
            ConfigFormat::detect(Path::new("test.json")),
            ConfigFormat::Json
        );
        assert_eq!(
            ConfigFormat::detect(Path::new("test.toml")),
            ConfigFormat::Toml
        );
        assert_eq!(
            ConfigFormat::detect(Path::new("test.yaml")),
            ConfigFormat::Yaml
        );
        assert_eq!(
            ConfigFormat::detect(Path::new("test.yml")),
            ConfigFormat::Yaml
        );
        assert_eq!(
            ConfigFormat::detect(Path::new("test.ini")),
            ConfigFormat::Ini
        );
        assert_eq!(
            ConfigFormat::detect(Path::new(".env")),
            ConfigFormat::KeyValue
        );
        assert_eq!(
            ConfigFormat::detect(Path::new(".gitconfig")),
            ConfigFormat::Ini
        );
        assert_eq!(
            ConfigFormat::detect(Path::new("test.custom")),
            ConfigFormat::Unknown
        );
    }

    #[test]
    fn test_json_merge() {
        let current = r#"{"existing": "value", "nested": {"a": 1}}"#;
        let settings: TomlValue = toml::from_str(
            r#"
            new_key = "new_value"
            [nested]
            b = 2
        "#,
        )
        .unwrap();

        let result = merge_json(Some(current), &settings).unwrap();
        let parsed: JsonValue = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["existing"], "value");
        assert_eq!(parsed["new_key"], "new_value");
        assert_eq!(parsed["nested"]["a"], 1);
        assert_eq!(parsed["nested"]["b"], 2);
    }

    #[test]
    fn test_toml_merge() {
        let current = r#"
existing = "value"
[section]
a = 1
"#;
        let settings: TomlValue = toml::from_str(
            r#"
            new_key = "new_value"
            [section]
            b = 2
        "#,
        )
        .unwrap();

        let result = merge_toml(Some(current), &settings).unwrap();
        let parsed: TomlValue = toml::from_str(&result).unwrap();

        assert_eq!(parsed["existing"].as_str(), Some("value"));
        assert_eq!(parsed["new_key"].as_str(), Some("new_value"));
        assert_eq!(parsed["section"]["a"].as_integer(), Some(1));
        assert_eq!(parsed["section"]["b"].as_integer(), Some(2));
    }

    #[test]
    fn test_keyvalue_merge() {
        let current = "EXISTING=value\nOTHER=123";
        let settings: TomlValue = toml::from_str(
            r#"
            NEW_KEY = "new_value"
            EXISTING = "updated"
        "#,
        )
        .unwrap();

        let result = merge_keyvalue(Some(current), &settings).unwrap();

        assert!(result.contains("EXISTING=updated"));
        assert!(result.contains("OTHER=123"));
        assert!(result.contains("NEW_KEY=new_value"));
    }

    #[test]
    fn test_expand_path() {
        let expanded = expand_path("~/.config/test");
        assert!(!expanded.to_string_lossy().starts_with("~"));
    }

    fn test_ctx() -> TemplateContext {
        let mut secrets = HashMap::new();
        secrets.insert("GITHUB_TOKEN".to_string(), "ghp_xxx".to_string());
        TemplateContext {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            hostname: "my-laptop".to_string(),
            username: "ada".to_string(),
            home: "/home/ada".to_string(),
            env: HashMap::new(),
            secrets,
            variables: toml::Value::Table(toml::map::Map::new()),
        }
    }

    #[test]
    fn test_templated_dotfile_renders_values() {
        let mut dotfiles = HashMap::new();
        let mut entry = toml::map::Map::new();
        entry.insert("_template".to_string(), TomlValue::Boolean(true));
        entry.insert(
            "oauth_token".to_string(),
            TomlValue::String("{{ secret.GITHUB_TOKEN }}".to_string()),
        );
        entry.insert(
            "machine".to_string(),
            TomlValue::String("{{ hostname }}".to_string()),
        );
        dotfiles.insert(
            "~/.config/example.json".to_string(),
            TomlValue::Table(entry),
        );

        let manager =
            DotfileManager::from_config_with_context(&dotfiles, Some(&test_ctx())).unwrap();
        let patch = &manager.list()[0];
        let settings = patch.settings.as_ref().unwrap().as_table().unwrap();

        assert_eq!(settings["oauth_token"].as_str(), Some("ghp_xxx"));
        assert_eq!(settings["machine"].as_str(), Some("my-laptop"));
    }

    #[test]
    fn test_untemplated_dotfile_leaves_braces_literal() {
        let mut dotfiles = HashMap::new();
        let mut entry = toml::map::Map::new();
        entry.insert(
            "literal".to_string(),
            TomlValue::String("{{ hostname }}".to_string()),
        );
        dotfiles.insert("~/.config/plain.json".to_string(), TomlValue::Table(entry));

        let manager = DotfileManager::from_config(&dotfiles).unwrap();
        let patch = &manager.list()[0];
        let settings = patch.settings.as_ref().unwrap().as_table().unwrap();

        assert_eq!(settings["literal"].as_str(), Some("{{ hostname }}"));
    }

    #[test]
    fn test_templated_content_replace_mode() {
        let mut dotfiles = HashMap::new();
        let mut entry = toml::map::Map::new();
        entry.insert("_template".to_string(), TomlValue::Boolean(true));
        entry.insert(
            "_content".to_string(),
            TomlValue::String("Host {{ hostname }}\n".to_string()),
        );
        dotfiles.insert("~/.ssh/config".to_string(), TomlValue::Table(entry));

        let manager =
            DotfileManager::from_config_with_context(&dotfiles, Some(&test_ctx())).unwrap();
        let patch = &manager.list()[0];

        assert_eq!(patch.content.as_deref(), Some("Host my-laptop\n"));
    }

    #[test]
    fn test_templated_dotfile_missing_secret_errors() {
        let mut dotfiles = HashMap::new();
        let mut entry = toml::map::Map::new();
        entry.insert("_template".to_string(), TomlValue::Boolean(true));
        entry.insert(
            "token".to_string(),
            TomlValue::String("{{ secret.MISSING }}".to_string()),
        );
        dotfiles.insert(
            "~/.config/example.json".to_string(),
            TomlValue::Table(entry),
        );

        let result = DotfileManager::from_config_with_context(&dotfiles, Some(&test_ctx()));
        let err = result.err().expect("expected missing-secret error");
        assert!(err.to_string().contains("MISSING"));
    }
}
