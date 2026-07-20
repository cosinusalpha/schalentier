use crate::config::BootstrapState;
use crate::error::Result;
use anyhow::Context;
use std::path::Path;
use tracing::debug;

/// Shell types supported for environment script generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
}

impl ShellType {
    /// Get the appropriate file extension for this shell's env file
    pub fn env_file_name(&self) -> &'static str {
        match self {
            ShellType::Bash | ShellType::Zsh => "env.sh",
            ShellType::Fish => "env.fish",
        }
    }

    /// Detect the current shell from environment
    pub fn detect() -> Option<Self> {
        // Check SHELL environment variable
        if let Ok(shell) = std::env::var("SHELL") {
            if shell.contains("bash") {
                return Some(ShellType::Bash);
            } else if shell.contains("zsh") {
                return Some(ShellType::Zsh);
            } else if shell.contains("fish") {
                return Some(ShellType::Fish);
            }
        }

        Some(ShellType::Bash)
    }
}

/// Extra PATH entries schalentier's own bootstrap installed (rust/node/go toolchains),
/// beyond the always-present `bin_dir`. Only includes a toolchain's path if schalentier
/// itself bootstrapped it — a user's pre-existing Rust/Node/Go setup is left alone.
fn extra_bin_dirs(bootstrap: &BootstrapState) -> Vec<&Path> {
    [&bootstrap.rust_path, &bootstrap.node_path, &bootstrap.go_path]
        .into_iter()
        .filter_map(|p| p.as_deref())
        .collect()
}

/// Whether schalentier installed its own Miniforge (as opposed to a pre-existing system
/// conda) — only then should the generated env script source our conda hook.
fn owns_conda(data_dir: &Path, bootstrap: &BootstrapState) -> bool {
    bootstrap.conda_path.as_deref() == Some(data_dir.join("conda").as_path())
}

/// Generate environment script content for Bash/Zsh
pub fn generate_bash_env(data_dir: &Path, bootstrap: &BootstrapState) -> String {
    let bin_dir = data_dir.join("bin");
    let conda_dir = data_dir.join("conda");

    let path_entries: Vec<String> = std::iter::once(bin_dir.display().to_string())
        .chain(extra_bin_dirs(bootstrap).iter().map(|p| p.display().to_string()))
        .collect();
    let path_export = format!("export PATH=\"{}:$PATH\"", path_entries.join(":"));

    let conda_init = if owns_conda(data_dir, bootstrap) {
        format!(
            r#"
# Initialize conda if installed
if [ -f "{conda_dir}/etc/profile.d/conda.sh" ]; then
    . "{conda_dir}/etc/profile.d/conda.sh"
fi

# Initialize mamba if available
if [ -f "{conda_dir}/etc/profile.d/mamba.sh" ]; then
    . "{conda_dir}/etc/profile.d/mamba.sh"
fi
"#,
            conda_dir = conda_dir.display()
        )
    } else {
        String::new()
    };

    format!(
        r#"# Schalentier environment setup (Bash/Zsh)
# Source this file in your shell config: source {data_dir}/env.sh

# Add schalentier bin directories to PATH
{path_export}
{conda_init}
# Schalentier environment marker
export SCHALENTIER_DATA_DIR="{data_dir}"
"#,
        data_dir = data_dir.display(),
        path_export = path_export,
        conda_init = conda_init,
    )
}

/// Generate environment script content for Fish
pub fn generate_fish_env(data_dir: &Path, bootstrap: &BootstrapState) -> String {
    let bin_dir = data_dir.join("bin");
    let conda_dir = data_dir.join("conda");

    let path_lines: String = std::iter::once(bin_dir.display().to_string())
        .chain(extra_bin_dirs(bootstrap).iter().map(|p| p.display().to_string()))
        .map(|dir| format!("fish_add_path \"{}\"\n", dir))
        .collect();

    let conda_init = if owns_conda(data_dir, bootstrap) {
        format!(
            r#"
# Initialize conda if installed
if test -f "{conda_dir}/etc/fish/conf.d/conda.fish"
    source "{conda_dir}/etc/fish/conf.d/conda.fish"
end

# Initialize mamba if available
if test -f "{conda_dir}/etc/fish/conf.d/mamba.fish"
    source "{conda_dir}/etc/fish/conf.d/mamba.fish"
end
"#,
            conda_dir = conda_dir.display()
        )
    } else {
        String::new()
    };

    format!(
        r#"# Schalentier environment setup (Fish)
# Source this file in your config.fish: source {data_dir}/env.fish

# Add schalentier bin directories to PATH
{path_lines}
{conda_init}
# Schalentier environment marker
set -gx SCHALENTIER_DATA_DIR "{data_dir}"
"#,
        data_dir = data_dir.display(),
        path_lines = path_lines,
        conda_init = conda_init,
    )
}

/// Generate the appropriate environment script for a shell type
pub fn generate_env_script(shell: ShellType, data_dir: &Path, bootstrap: &BootstrapState) -> String {
    match shell {
        ShellType::Bash | ShellType::Zsh => generate_bash_env(data_dir, bootstrap),
        ShellType::Fish => generate_fish_env(data_dir, bootstrap),
    }
}

/// Write environment scripts to the data directory
pub fn write_env_scripts(data_dir: &Path, bootstrap: &BootstrapState) -> Result<()> {
    debug!("Writing environment scripts to {}", data_dir.display());

    // Ensure data directory exists
    if !data_dir.exists() {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create directory: {}", data_dir.display()))?;
    }

    // Write all shell scripts
    for shell in [ShellType::Bash, ShellType::Fish] {
        let content = generate_env_script(shell, data_dir, bootstrap);
        let path = data_dir.join(shell.env_file_name());

        std::fs::write(&path, &content)
            .with_context(|| format!("Failed to write {}", path.display()))?;

        debug!("Wrote {}", path.display());
    }

    Ok(())
}

/// Generate shell initialization snippet that users can add to their config
pub fn shell_init_snippet(shell: ShellType, data_dir: &Path) -> String {
    let env_file = data_dir.join(shell.env_file_name());

    match shell {
        ShellType::Bash => format!(
            r#"# Add to your ~/.bashrc or ~/.bash_profile
if [ -f "{}" ]; then
    source "{}"
fi"#,
            env_file.display(),
            env_file.display()
        ),
        ShellType::Zsh => format!(
            r#"# Add to your ~/.zshrc
if [ -f "{}" ]; then
    source "{}"
fi"#,
            env_file.display(),
            env_file.display()
        ),
        ShellType::Fish => format!(
            r#"# Add to your ~/.config/fish/config.fish
if test -f "{}"
    source "{}"
end"#,
            env_file.display(),
            env_file.display()
        ),
    }
}

/// Conventional rc file path for a shell, e.g. `~/.bashrc`.
pub fn rc_file_path(shell: ShellType) -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    Some(match shell {
        ShellType::Bash => home.join(".bashrc"),
        ShellType::Zsh => home.join(".zshrc"),
        ShellType::Fish => home.join(".config/fish/config.fish"),
    })
}

/// Whether `rc_path` already sources schalentier's env file for `shell`. Used both to
/// skip re-prompting on repeat `init` runs and to make [`ensure_sourced`] idempotent.
pub fn is_sourced(rc_path: &Path, data_dir: &Path, shell: ShellType) -> bool {
    let env_file = data_dir.join(shell.env_file_name());
    let needle = env_file.display().to_string();
    std::fs::read_to_string(rc_path)
        .map(|content| content.contains(&needle))
        .unwrap_or(false)
}

/// Append the one-line-guarded source snippet for `shell` to `rc_path`, creating the file
/// if it doesn't exist. No-op if already sourced (checked via [`is_sourced`]).
pub fn ensure_sourced(rc_path: &Path, data_dir: &Path, shell: ShellType) -> Result<()> {
    if is_sourced(rc_path, data_dir, shell) {
        debug!("{} already sources schalentier's env file", rc_path.display());
        return Ok(());
    }

    if let Some(parent) = rc_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
    }

    let existing = std::fs::read_to_string(rc_path).unwrap_or_default();
    let snippet = shell_init_snippet(shell, data_dir);
    let separator = if existing.is_empty() || existing.ends_with('\n') { "" } else { "\n" };
    let updated = format!("{existing}{separator}\n{snippet}\n");

    std::fs::write(rc_path, updated)
        .with_context(|| format!("Failed to write {}", rc_path.display()))?;

    debug!("Appended schalentier source line to {}", rc_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_shell_type_env_file_name() {
        assert_eq!(ShellType::Bash.env_file_name(), "env.sh");
        assert_eq!(ShellType::Zsh.env_file_name(), "env.sh");
        assert_eq!(ShellType::Fish.env_file_name(), "env.fish");
    }

    #[test]
    fn test_bash_env_script_contains_paths() {
        let data_dir = PathBuf::from("/home/user/.schalentier");
        let mut bootstrap = BootstrapState::default();
        bootstrap.conda_path = Some(data_dir.join("conda"));
        let script = generate_bash_env(&data_dir, &bootstrap);

        // Check path contains the data_dir (platform-independent)
        assert!(script.contains(&data_dir.display().to_string()));
        assert!(script.contains("bin"));
        assert!(script.contains("conda.sh"));
        assert!(script.contains("mamba.sh"));
        assert!(script.contains("SCHALENTIER_DATA_DIR"));
    }

    #[test]
    fn test_bash_env_script_includes_bootstrapped_toolchain_paths() {
        let data_dir = PathBuf::from("/home/user/.schalentier");
        let mut bootstrap = BootstrapState::default();
        bootstrap.rust_path = Some(PathBuf::from("/home/user/.schalentier/.cargo/bin"));
        bootstrap.node_path = Some(PathBuf::from("/home/user/.schalentier/node/bin"));
        bootstrap.go_path = Some(PathBuf::from("/home/user/.schalentier/go/bin"));
        let script = generate_bash_env(&data_dir, &bootstrap);

        assert!(script.contains("/home/user/.schalentier/.cargo/bin"));
        assert!(script.contains("/home/user/.schalentier/node/bin"));
        assert!(script.contains("/home/user/.schalentier/go/bin"));
    }

    #[test]
    fn test_bash_env_script_skips_conda_hook_when_system_conda() {
        let data_dir = PathBuf::from("/home/user/.schalentier");
        let mut bootstrap = BootstrapState::default();
        // System conda, not our own Miniforge install.
        bootstrap.conda_path = Some(PathBuf::from("/usr/bin"));
        let script = generate_bash_env(&data_dir, &bootstrap);

        assert!(!script.contains("conda.sh"));
        assert!(!script.contains("mamba.sh"));
    }

    #[test]
    fn test_bash_env_script_skips_conda_hook_when_no_conda() {
        let data_dir = PathBuf::from("/home/user/.schalentier");
        let bootstrap = BootstrapState::default();
        let script = generate_bash_env(&data_dir, &bootstrap);

        assert!(!script.contains("conda.sh"));
    }

    #[test]
    fn test_fish_env_script_contains_paths() {
        let data_dir = PathBuf::from("/home/user/.schalentier");
        let mut bootstrap = BootstrapState::default();
        bootstrap.conda_path = Some(data_dir.join("conda"));
        let script = generate_fish_env(&data_dir, &bootstrap);

        assert!(script.contains("fish_add_path"));
        // Check path contains the data_dir (platform-independent)
        assert!(script.contains(&data_dir.display().to_string()));
        assert!(script.contains("bin"));
        assert!(script.contains("conda.fish"));
        assert!(script.contains("set -gx SCHALENTIER_DATA_DIR"));
    }

    #[test]
    fn test_write_env_scripts() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        write_env_scripts(data_dir, &BootstrapState::default()).unwrap();

        assert!(data_dir.join("env.sh").exists());
        assert!(data_dir.join("env.fish").exists());
    }

    #[test]
    fn test_write_env_scripts_creates_dir() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("subdir");

        assert!(!data_dir.exists());
        write_env_scripts(&data_dir, &BootstrapState::default()).unwrap();
        assert!(data_dir.exists());

        assert!(data_dir.join("env.sh").exists());
        assert!(data_dir.join("env.fish").exists());
    }

    #[test]
    fn test_shell_init_snippet_bash() {
        let data_dir = PathBuf::from("/home/user/.schalentier");
        let snippet = shell_init_snippet(ShellType::Bash, &data_dir);

        assert!(snippet.contains("~/.bashrc"));
        assert!(snippet.contains("source"));
        assert!(snippet.contains("env.sh"));
    }

    #[test]
    fn test_shell_init_snippet_fish() {
        let data_dir = PathBuf::from("/home/user/.schalentier");
        let snippet = shell_init_snippet(ShellType::Fish, &data_dir);

        assert!(snippet.contains("config.fish"));
        assert!(snippet.contains("test -f"));
    }

    #[test]
    fn test_shell_detect() {
        // Should return Some shell type (platform-dependent default if no env vars)
        let shell = ShellType::detect();
        assert!(shell.is_some());
    }

    #[test]
    fn test_rc_file_path() {
        assert_eq!(
            rc_file_path(ShellType::Bash).unwrap().file_name().unwrap(),
            ".bashrc"
        );
        assert_eq!(
            rc_file_path(ShellType::Zsh).unwrap().file_name().unwrap(),
            ".zshrc"
        );
        assert!(rc_file_path(ShellType::Fish)
            .unwrap()
            .ends_with("fish/config.fish"));
    }

    #[test]
    fn test_is_sourced_false_when_rc_missing() {
        let temp_dir = TempDir::new().unwrap();
        let rc_path = temp_dir.path().join(".bashrc");
        let data_dir = temp_dir.path().join(".schalentier");

        assert!(!is_sourced(&rc_path, &data_dir, ShellType::Bash));
    }

    #[test]
    fn test_ensure_sourced_appends_and_is_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let rc_path = temp_dir.path().join(".bashrc");
        let data_dir = temp_dir.path().join(".schalentier");
        std::fs::write(&rc_path, "# existing content\n").unwrap();

        ensure_sourced(&rc_path, &data_dir, ShellType::Bash).unwrap();
        assert!(is_sourced(&rc_path, &data_dir, ShellType::Bash));

        let after_first = std::fs::read_to_string(&rc_path).unwrap();
        assert!(after_first.contains("# existing content"));
        assert!(after_first.contains("env.sh"));

        // Re-running must not duplicate the source line.
        ensure_sourced(&rc_path, &data_dir, ShellType::Bash).unwrap();
        let after_second = std::fs::read_to_string(&rc_path).unwrap();
        assert_eq!(after_first, after_second);
    }

    #[test]
    fn test_ensure_sourced_creates_missing_rc_file() {
        let temp_dir = TempDir::new().unwrap();
        let rc_path = temp_dir.path().join(".bashrc");
        let data_dir = temp_dir.path().join(".schalentier");

        assert!(!rc_path.exists());
        ensure_sourced(&rc_path, &data_dir, ShellType::Bash).unwrap();
        assert!(rc_path.exists());
        assert!(is_sourced(&rc_path, &data_dir, ShellType::Bash));
    }
}
