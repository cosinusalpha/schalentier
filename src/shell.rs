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
    PowerShell,
}

impl ShellType {
    /// Get the appropriate file extension for this shell's env file
    pub fn env_file_name(&self) -> &'static str {
        match self {
            ShellType::Bash | ShellType::Zsh => "env.sh",
            ShellType::Fish => "env.fish",
            ShellType::PowerShell => "env.ps1",
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

        // Check PSModulePath for PowerShell
        if std::env::var("PSModulePath").is_ok() {
            return Some(ShellType::PowerShell);
        }

        // Default based on OS
        #[cfg(windows)]
        return Some(ShellType::PowerShell);

        #[cfg(not(windows))]
        return Some(ShellType::Bash);
    }
}

/// Generate environment script content for Bash/Zsh
pub fn generate_bash_env(data_dir: &Path) -> String {
    let bin_dir = data_dir.join("bin");
    let conda_dir = data_dir.join("conda");

    format!(
        r#"# Schalentier environment setup (Bash/Zsh)
# Source this file in your shell config: source {data_dir}/env.sh

# Add schalentier bin directory to PATH
export PATH="{bin_dir}:$PATH"

# Initialize conda if installed
if [ -f "{conda_dir}/etc/profile.d/conda.sh" ]; then
    . "{conda_dir}/etc/profile.d/conda.sh"
fi

# Initialize mamba if available
if [ -f "{conda_dir}/etc/profile.d/mamba.sh" ]; then
    . "{conda_dir}/etc/profile.d/mamba.sh"
fi

# Schalentier environment marker
export SCHALENTIER_DATA_DIR="{data_dir}"
"#,
        data_dir = data_dir.display(),
        bin_dir = bin_dir.display(),
        conda_dir = conda_dir.display(),
    )
}

/// Generate environment script content for Fish
pub fn generate_fish_env(data_dir: &Path) -> String {
    let bin_dir = data_dir.join("bin");
    let conda_dir = data_dir.join("conda");

    format!(
        r#"# Schalentier environment setup (Fish)
# Source this file in your config.fish: source {data_dir}/env.fish

# Add schalentier bin directory to PATH
fish_add_path "{bin_dir}"

# Initialize conda if installed
if test -f "{conda_dir}/etc/fish/conf.d/conda.fish"
    source "{conda_dir}/etc/fish/conf.d/conda.fish"
end

# Initialize mamba if available
if test -f "{conda_dir}/etc/fish/conf.d/mamba.fish"
    source "{conda_dir}/etc/fish/conf.d/mamba.fish"
end

# Schalentier environment marker
set -gx SCHALENTIER_DATA_DIR "{data_dir}"
"#,
        data_dir = data_dir.display(),
        bin_dir = bin_dir.display(),
        conda_dir = conda_dir.display(),
    )
}

/// Generate environment script content for PowerShell
pub fn generate_powershell_env(data_dir: &Path) -> String {
    let bin_dir = data_dir.join("bin");
    let conda_dir = data_dir.join("conda");

    format!(
        r#"# Schalentier environment setup (PowerShell)
# Dot-source this file in your profile: . "{data_dir}\env.ps1"

# Add schalentier bin directory to PATH
$env:PATH = "{bin_dir};$env:PATH"

# Initialize conda if installed
$condaHook = "{conda_dir}\shell\condabin\conda-hook.ps1"
if (Test-Path $condaHook) {{
    . $condaHook
}}

# Schalentier environment marker
$env:SCHALENTIER_DATA_DIR = "{data_dir}"
"#,
        data_dir = data_dir.display(),
        bin_dir = bin_dir.display(),
        conda_dir = conda_dir.display(),
    )
}

/// Generate the appropriate environment script for a shell type
pub fn generate_env_script(shell: ShellType, data_dir: &Path) -> String {
    match shell {
        ShellType::Bash | ShellType::Zsh => generate_bash_env(data_dir),
        ShellType::Fish => generate_fish_env(data_dir),
        ShellType::PowerShell => generate_powershell_env(data_dir),
    }
}

/// Write environment scripts to the data directory
pub fn write_env_scripts(data_dir: &Path) -> Result<()> {
    debug!("Writing environment scripts to {}", data_dir.display());

    // Ensure data directory exists
    if !data_dir.exists() {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create directory: {}", data_dir.display()))?;
    }

    // Write all shell scripts
    for shell in [ShellType::Bash, ShellType::Fish, ShellType::PowerShell] {
        let content = generate_env_script(shell, data_dir);
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
        ShellType::PowerShell => format!(
            r#"# Add to your PowerShell profile ($PROFILE)
if (Test-Path "{}") {{
    . "{}"
}}"#,
            env_file.display(),
            env_file.display()
        ),
    }
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
        assert_eq!(ShellType::PowerShell.env_file_name(), "env.ps1");
    }

    #[test]
    fn test_bash_env_script_contains_paths() {
        let data_dir = PathBuf::from("/home/user/.schalentier");
        let script = generate_bash_env(&data_dir);

        // Check path contains the data_dir (platform-independent)
        assert!(script.contains(&data_dir.display().to_string()));
        assert!(script.contains("bin"));
        assert!(script.contains("conda.sh"));
        assert!(script.contains("mamba.sh"));
        assert!(script.contains("SCHALENTIER_DATA_DIR"));
    }

    #[test]
    fn test_fish_env_script_contains_paths() {
        let data_dir = PathBuf::from("/home/user/.schalentier");
        let script = generate_fish_env(&data_dir);

        assert!(script.contains("fish_add_path"));
        // Check path contains the data_dir (platform-independent)
        assert!(script.contains(&data_dir.display().to_string()));
        assert!(script.contains("bin"));
        assert!(script.contains("conda.fish"));
        assert!(script.contains("set -gx SCHALENTIER_DATA_DIR"));
    }

    #[test]
    fn test_powershell_env_script_contains_paths() {
        let data_dir = PathBuf::from("C:\\Users\\test\\.schalentier");
        let script = generate_powershell_env(&data_dir);

        assert!(script.contains("$env:PATH"));
        assert!(script.contains("conda-hook.ps1"));
        assert!(script.contains("SCHALENTIER_DATA_DIR"));
    }

    #[test]
    fn test_write_env_scripts() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        write_env_scripts(data_dir).unwrap();

        assert!(data_dir.join("env.sh").exists());
        assert!(data_dir.join("env.fish").exists());
        assert!(data_dir.join("env.ps1").exists());
    }

    #[test]
    fn test_write_env_scripts_creates_dir() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().join("subdir");

        assert!(!data_dir.exists());
        write_env_scripts(&data_dir).unwrap();
        assert!(data_dir.exists());

        assert!(data_dir.join("env.sh").exists());
        assert!(data_dir.join("env.fish").exists());
        assert!(data_dir.join("env.ps1").exists());
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
}
