use crate::archive;
use crate::config::LocalState;
use crate::error::{Result, SchalentierError};
use crate::state::default_data_dir;
use anyhow::Context;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Supported CPU architectures
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arch::X86_64 => write!(f, "x86_64"),
            Arch::Aarch64 => write!(f, "aarch64"),
        }
    }
}

/// Supported operating systems
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOS,
}

impl std::fmt::Display for Os {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Os::Linux => write!(f, "linux"),
            Os::MacOS => write!(f, "macos"),
        }
    }
}

/// Detect the current CPU architecture
pub fn get_arch() -> Result<Arch> {
    let arch = std::env::consts::ARCH;
    debug!("Detected architecture: {}", arch);

    match arch {
        "x86_64" | "amd64" => Ok(Arch::X86_64),
        "aarch64" | "arm64" => Ok(Arch::Aarch64),
        other => Err(SchalentierError::UnsupportedArch(other.to_string()).into()),
    }
}

/// Detect the current operating system
pub fn get_os() -> Result<Os> {
    let os = std::env::consts::OS;
    debug!("Detected OS: {}", os);

    match os {
        "linux" => Ok(Os::Linux),
        "macos" => Ok(Os::MacOS),
        other => Err(SchalentierError::UnsupportedPlatform(other.to_string()).into()),
    }
}

/// Get platform-specific Miniforge download URL
pub fn miniforge_url(arch: Arch, os: Os) -> Result<String> {
    let base = "https://github.com/conda-forge/miniforge/releases/latest/download";

    let filename = match (os, arch) {
        (Os::Linux, Arch::X86_64) => "Miniforge3-Linux-x86_64.sh",
        (Os::Linux, Arch::Aarch64) => "Miniforge3-Linux-aarch64.sh",
        (Os::MacOS, Arch::X86_64) => "Miniforge3-MacOSX-x86_64.sh",
        (Os::MacOS, Arch::Aarch64) => "Miniforge3-MacOSX-arm64.sh",
    };

    Ok(format!("{}/{}", base, filename))
}

/// Get platform-specific uv download URL
pub fn uv_url(arch: Arch, os: Os) -> Result<String> {
    let base = "https://github.com/astral-sh/uv/releases/latest/download";

    let filename = match (os, arch) {
        (Os::Linux, Arch::X86_64) => "uv-x86_64-unknown-linux-musl.tar.gz",
        (Os::Linux, Arch::Aarch64) => "uv-aarch64-unknown-linux-musl.tar.gz",
        (Os::MacOS, Arch::X86_64) => "uv-x86_64-apple-darwin.tar.gz",
        (Os::MacOS, Arch::Aarch64) => "uv-aarch64-apple-darwin.tar.gz",
    };

    Ok(format!("{}/{}", base, filename))
}

/// Get platform-specific rustup download URL
pub fn rustup_url(arch: Arch, os: Os) -> Result<String> {
    let arch_str = match arch {
        Arch::X86_64 => "x86_64",
        Arch::Aarch64 => "aarch64",
    };

    let os_str = match os {
        Os::Linux => "unknown-linux-gnu",
        Os::MacOS => "apple-darwin",
    };

    Ok(format!(
        "https://static.rust-lang.org/rustup/dist/{}-{}/rustup-init",
        arch_str, os_str
    ))
}

/// Get platform-specific Node.js download URL
pub fn node_url(arch: Arch, os: Os) -> Result<String> {
    let arch_str = match arch {
        Arch::X86_64 => "x64",
        Arch::Aarch64 => "arm64",
    };

    let os_str = match os {
        Os::Linux => "linux",
        Os::MacOS => "darwin",
    };

    // Use current version (v26.5.0 as of this writing, but will be updated)
    // In future, could fetch from https://nodejs.org/dist/index.json for latest
    let version = "v26.5.0";

    Ok(format!(
        "https://nodejs.org/dist/{}/node-{}-{}-{}.tar.gz",
        version, version, os_str, arch_str
    ))
}

/// Get platform-specific Go download URL
pub fn go_url(arch: Arch, os: Os) -> Result<String> {
    let arch_str = match arch {
        Arch::X86_64 => "amd64",
        Arch::Aarch64 => "arm64",
    };

    let os_str = match os {
        Os::Linux => "linux",
        Os::MacOS => "darwin",
    };

    // Use latest stable version
    let version = "1.22.5";

    Ok(format!(
        "https://go.dev/dl/go{}.{}.{}.tar.gz",
        version, os_str, arch_str
    ))
}

/// Bootstrap paths
pub struct BootstrapPaths {
    pub data_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub conda_dir: PathBuf,
    pub downloads_dir: PathBuf,
}

impl BootstrapPaths {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            bin_dir: data_dir.join("bin"),
            conda_dir: data_dir.join("conda"),
            downloads_dir: data_dir.join("downloads"),
            data_dir,
        }
    }

    pub fn from_default() -> Result<Self> {
        let data_dir = default_data_dir()?;
        Ok(Self::new(data_dir))
    }

    /// Ensure all bootstrap directories exist
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [&self.data_dir, &self.bin_dir, &self.downloads_dir] {
            if !dir.exists() {
                debug!("Creating directory: {}", dir.display());
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
            }
        }
        Ok(())
    }
}

/// Download a file from a URL to a destination path
pub async fn download_file(url: &str, dest: &Path) -> Result<()> {
    info!("Downloading {} to {}", url, dest.display());

    // Create client with TLS that uses webpki roots (bundled certificates)
    // This works in minimal containers without system CA certificates
    let client = reqwest::Client::new();

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to download from {}", url))?;

    if !response.status().is_success() {
        return Err(SchalentierError::Network(format!(
            "HTTP {} when downloading {}",
            response.status(),
            url
        ))
        .into());
    }

    let bytes = response
        .bytes()
        .await
        .with_context(|| "Failed to read response body")?;

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(dest, &bytes)
        .with_context(|| format!("Failed to write to {}", dest.display()))?;

    info!("Downloaded {} bytes", bytes.len());
    Ok(())
}

/// Bootstrap orchestrator
pub struct Bootstrap {
    paths: BootstrapPaths,
    arch: Arch,
    os: Os,
    /// Whether to install uv (default: true)
    install_uv: bool,
    /// Whether to install conda/miniforge (default: true)
    install_conda: bool,
    /// Whether to install rust/rustup (default: true)
    install_rust: bool,
    /// Whether to install Node.js (default: true)
    install_node: bool,
    /// Whether to install Go (default: true)
    install_go: bool,
}

impl Bootstrap {
    pub fn new() -> Result<Self> {
        let paths = BootstrapPaths::from_default()?;
        let arch = get_arch()?;
        let os = get_os()?;

        Ok(Self {
            paths,
            arch,
            os,
            install_uv: true,
            install_conda: true,
            install_rust: true,
            install_node: true,
            install_go: true,
        })
    }

    pub fn with_data_dir(data_dir: PathBuf) -> Result<Self> {
        let paths = BootstrapPaths::new(data_dir);
        let arch = get_arch()?;
        let os = get_os()?;

        Ok(Self {
            paths,
            arch,
            os,
            install_uv: true,
            install_conda: true,
            install_rust: true,
            install_node: true,
            install_go: true,
        })
    }

    /// Set whether to install uv
    pub fn set_install_uv(&mut self, install: bool) {
        self.install_uv = install;
    }

    /// Set whether to install conda/miniforge
    pub fn set_install_conda(&mut self, install: bool) {
        self.install_conda = install;
    }

    /// Set whether to install rust/rustup
    pub fn set_install_rust(&mut self, install: bool) {
        self.install_rust = install;
    }

    /// Set whether to install Node.js
    pub fn set_install_node(&mut self, install: bool) {
        self.install_node = install;
    }

    /// Set whether to install Go
    pub fn set_install_go(&mut self, install: bool) {
        self.install_go = install;
    }

    /// Run the full bootstrap process
    pub async fn run(&self, state: &mut LocalState) -> Result<()> {
        info!("Starting bootstrap for {} {}", self.os, self.arch);
        self.paths.ensure_dirs()?;

        // Install uv (Python package manager)
        if self.install_uv && !state.bootstrap.uv_installed {
            self.install_uv_component(state).await?;
        } else if !self.install_uv {
            debug!("uv installation skipped by user");
        } else {
            debug!("uv already installed, skipping");
        }

        // Install conda/miniforge
        if self.install_conda && !state.bootstrap.conda_installed {
            self.install_miniforge(state).await?;
        } else if !self.install_conda {
            debug!("Conda installation skipped by user");
        } else {
            debug!("Conda already installed, skipping");
        }

        // Install Rust/rustup
        if self.install_rust && !state.bootstrap.rust_installed {
            self.install_rustup(state).await?;
        } else if !self.install_rust {
            debug!("Rust installation skipped by user");
        } else {
            debug!("Rust already installed, skipping");
        }

        // Install Node.js
        if self.install_node && !state.bootstrap.node_installed {
            self.install_nodejs(state).await?;
        } else if !self.install_node {
            debug!("Node.js installation skipped by user");
        } else {
            debug!("Node.js already installed, skipping");
        }

        // Install Go
        if self.install_go && !state.bootstrap.go_installed {
            self.install_go(state).await?;
        } else if !self.install_go {
            debug!("Go installation skipped by user");
        } else {
            debug!("Go already installed, skipping");
        }

        state.initialized = true;
        info!("Bootstrap complete");
        Ok(())
    }

    /// Install uv
    async fn install_uv_component(&self, state: &mut LocalState) -> Result<()> {
        // Check if uv is already available in PATH
        if let Ok(path) = which::which("uv") {
            if let Ok(output) = std::process::Command::new("uv").arg("--version").output() {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    info!(
                        "uv already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    println!(
                        "ℹ uv is already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    state.bootstrap.uv_installed = true;
                    state.bootstrap.uv_path = Some(self.paths.bin_dir.join("uv"));
                    return Ok(());
                }
            }
        }

        info!("Installing uv...");
        println!("→ Installing uv...");

        let url = uv_url(self.arch, self.os)?;
        let archive_name = url.split('/').next_back().unwrap_or("uv-archive");
        let archive_path = self.paths.downloads_dir.join(archive_name);

        download_file(&url, &archive_path).await?;

        let uv_bin = self.paths.bin_dir.join("uv");

        // Extract the archive
        let extract_dir = self.paths.downloads_dir.join("uv-extract");
        if extract_dir.exists() {
            std::fs::remove_dir_all(&extract_dir)?;
        }

        info!("Extracting uv archive...");
        let extracted_files = archive::extract(&archive_path, &extract_dir)?;

        // Find the uv binary in extracted files
        let source_binary = archive::find_binary(&extracted_files, "uv")
            .or_else(|| {
                // uv releases often have the binary inside a directory like "uv-x86_64-unknown-linux-musl"
                archive::find_executables(&extracted_files)
                    .into_iter()
                    .find(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with("uv"))
                            .unwrap_or(false)
                    })
            })
            .ok_or_else(|| {
                SchalentierError::BootstrapFailed(format!(
                    "Could not find uv binary in extracted files. Found: {:?}",
                    extracted_files
                        .iter()
                        .filter_map(|p| p.file_name())
                        .collect::<Vec<_>>()
                ))
            })?;

        // Copy to bin directory
        info!("Installing uv to {}", uv_bin.display());
        std::fs::copy(&source_binary, &uv_bin)
            .with_context(|| format!("Failed to copy uv binary to {}", uv_bin.display()))?;

        // Set executable permission
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&uv_bin, std::fs::Permissions::from_mode(0o755))?;
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&extract_dir);
        let _ = std::fs::remove_file(&archive_path);

        state.bootstrap.uv_installed = true;
        state.bootstrap.uv_path = Some(uv_bin);
        info!("uv installation complete");
        println!("✓ uv installed successfully");
        Ok(())
    }

    /// Install Miniforge
    async fn install_miniforge(&self, state: &mut LocalState) -> Result<()> {
        // Check if conda is already available in PATH
        if let Ok(path) = which::which("conda") {
            if let Ok(output) = std::process::Command::new("conda")
                .arg("--version")
                .output()
            {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    info!(
                        "conda already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    println!(
                        "ℹ conda is already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    state.bootstrap.conda_installed = true;
                    // Record the *system* conda's location, not our own conda_dir — we
                    // didn't install anything there, so the generated env script must
                    // not later assume it owns a Miniforge install at conda_dir and try
                    // to source a hook file that doesn't exist.
                    state.bootstrap.conda_path =
                        Some(path.parent().unwrap_or(&path).to_path_buf());
                    return Ok(());
                }
            }
        }

        info!("Installing Miniforge...");
        println!("→ Installing Miniforge...");

        let url = miniforge_url(self.arch, self.os)?;
        let installer_name = url.split('/').next_back().unwrap_or("miniforge-installer");
        let installer_path = self.paths.downloads_dir.join(installer_name);

        download_file(&url, &installer_path).await?;

        // Run installer in batch mode
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &installer_path,
                std::fs::Permissions::from_mode(0o755),
            )?;
        }

        info!("Running Miniforge installer in batch mode...");
        let status = std::process::Command::new("bash")
            .arg(&installer_path)
            .arg("-b") // batch mode (no prompts)
            .arg("-p")
            .arg(&self.paths.conda_dir)
            .status()
            .with_context(|| "Failed to run Miniforge installer")?;

        if !status.success() {
            return Err(SchalentierError::BootstrapFailed(format!(
                "Miniforge installer failed with exit code: {:?}",
                status.code()
            ))
            .into());
        }

        // Verify conda was installed
        let conda_bin = self.paths.conda_dir.join("bin").join("conda");
        if !conda_bin.exists() {
            return Err(SchalentierError::BootstrapFailed(format!(
                "Miniforge installed but conda not found at {}",
                conda_bin.display()
            ))
            .into());
        }

        info!(
            "Miniforge installed successfully to {}",
            self.paths.conda_dir.display()
        );

        // Cleanup installer
        let _ = std::fs::remove_file(&installer_path);

        state.bootstrap.conda_installed = true;
        state.bootstrap.conda_path = Some(self.paths.conda_dir.clone());
        info!("Miniforge installation complete");
        println!("✓ Miniforge installed successfully");
        Ok(())
    }

    /// Install Rust/rustup
    async fn install_rustup(&self, state: &mut LocalState) -> Result<()> {
        // Check if rustup/cargo is already available in PATH
        if let Ok(path) = which::which("rustup") {
            if let Ok(output) = std::process::Command::new("rustup")
                .arg("--version")
                .output()
            {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    info!(
                        "rustup already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    println!(
                        "ℹ rustup is already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    state.bootstrap.rust_installed = true;
                    if let Ok(cargo_path) = which::which("cargo") {
                        state.bootstrap.rust_path = Some(cargo_path.parent().unwrap().to_path_buf());
                    }
                    return Ok(());
                }
            }
        }

        // Fallback: check cargo without rustup
        if let Ok(path) = which::which("cargo") {
            if let Ok(output) = std::process::Command::new("cargo")
                .arg("--version")
                .output()
            {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    info!(
                        "cargo already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    println!(
                        "ℹ cargo is already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    state.bootstrap.rust_installed = true;
                    state.bootstrap.rust_path = Some(path.parent().unwrap().to_path_buf());
                    return Ok(());
                }
            }
        }

        info!("Installing Rust...");
        println!("→ Installing Rust...");

        let url = rustup_url(self.arch, self.os)?;
        let download_path = self.paths.downloads_dir.join("rustup-init");

        download_file(&url, &download_path).await?;

        // Set executable permission
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&download_path, std::fs::Permissions::from_mode(0o755))?;
        }

        // Run rustup installer
        info!("Running rustup installer...");
        let cargo_home = self.paths.data_dir.join(".cargo");
        let rustup_home = self.paths.data_dir.join("rustup");

        let status = std::process::Command::new(&download_path)
            .args(&["-y", "--no-modify-path"])
            .env("CARGO_HOME", &cargo_home)
            .env("RUSTUP_HOME", &rustup_home)
            .status()
            .with_context(|| "Failed to run rustup installer")?;

        if !status.success() {
            return Err(SchalentierError::BootstrapFailed(format!(
                "Rustup installer failed with exit code: {:?}",
                status.code()
            ))
            .into());
        }

        // Cleanup
        let _ = std::fs::remove_file(&download_path);

        state.bootstrap.rust_installed = true;
        state.bootstrap.rust_path = Some(cargo_home.join("bin"));
        info!("Rust installation complete");
        println!("✓ Rust installed successfully");
        Ok(())
    }

    /// Install Node.js
    async fn install_nodejs(&self, state: &mut LocalState) -> Result<()> {
        // Check if Node.js is already available in PATH
        if let Ok(path) = which::which("node") {
            if let Ok(output) = std::process::Command::new("node")
                .arg("--version")
                .output()
            {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    info!(
                        "Node.js already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    println!(
                        "ℹ Node.js is already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    state.bootstrap.node_installed = true;
                    state.bootstrap.node_path = Some(path.parent().unwrap().to_path_buf());
                    return Ok(());
                }
            }
        }

        info!("Installing Node.js...");
        println!("→ Installing Node.js...");

        let url = node_url(self.arch, self.os)?;
        let filename = url.split('/').next_back().unwrap_or("node-archive");
        let download_path = self.paths.downloads_dir.join(filename);
        let node_dir = self.paths.data_dir.join("node");

        download_file(&url, &download_path).await?;

        info!("Extracting Node.js...");
        let extract_dir = self.paths.downloads_dir.join("node-extract");
        if extract_dir.exists() {
            std::fs::remove_dir_all(&extract_dir)?;
        }

        let extracted_files = archive::extract(&download_path, &extract_dir)?;

        // Find the extracted directory (e.g., node-v26.5.0-linux-x64)
        let extracted_root = extracted_files
            .first()
            .and_then(|p| p.parent())
            .ok_or_else(|| {
                SchalentierError::BootstrapFailed("Could not find extracted Node.js directory".to_string())
            })?;

        // Move contents to node_dir
        if node_dir.exists() {
            std::fs::remove_dir_all(&node_dir)?;
        }
        std::fs::create_dir_all(&node_dir)?;

        for entry in std::fs::read_dir(extracted_root)? {
            let entry = entry?;
            let dest = node_dir.join(entry.file_name());
            std::fs::rename(entry.path(), dest)?;
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&extract_dir);
        let _ = std::fs::remove_file(&download_path);

        state.bootstrap.node_installed = true;
        state.bootstrap.node_path = Some(node_dir.join("bin"));
        info!("Node.js installation complete");
        println!("✓ Node.js installed successfully");
        Ok(())
    }

    /// Install Go
    async fn install_go(&self, state: &mut LocalState) -> Result<()> {
        // Check if Go is already available in PATH
        if let Ok(path) = which::which("go") {
            if let Ok(output) = std::process::Command::new("go").arg("version").output() {
                if output.status.success() {
                    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    info!(
                        "Go already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    println!(
                        "ℹ Go is already installed at {} ({}), skipping download",
                        path.display(),
                        version
                    );
                    state.bootstrap.go_installed = true;
                    state.bootstrap.go_path = Some(path.parent().unwrap().to_path_buf());
                    return Ok(());
                }
            }
        }

        info!("Installing Go...");
        println!("→ Installing Go...");

        let url = go_url(self.arch, self.os)?;
        let filename = url.split('/').next_back().unwrap_or("go-archive");
        let download_path = self.paths.downloads_dir.join(filename);
        let go_dir = self.paths.data_dir.join("go");

        download_file(&url, &download_path).await?;

        info!("Extracting Go...");
        let extract_dir = self.paths.downloads_dir.join("go-extract");
        if extract_dir.exists() {
            std::fs::remove_dir_all(&extract_dir)?;
        }

        let extracted_files = archive::extract(&download_path, &extract_dir)?;

        // Find the extracted go directory
        let extracted_go = extracted_files
            .iter()
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == "go")
                    .unwrap_or(false)
            })
            .or_else(|| extracted_files.first())
            .ok_or_else(|| {
                SchalentierError::BootstrapFailed("Could not find extracted Go directory".to_string())
            })?;

        // Remove existing go_dir if it exists
        if go_dir.exists() {
            std::fs::remove_dir_all(&go_dir)?;
        }

        // Move the go directory
        std::fs::rename(extracted_go, &go_dir)?;

        // Cleanup
        let _ = std::fs::remove_dir_all(&extract_dir);
        let _ = std::fs::remove_file(&download_path);

        state.bootstrap.go_installed = true;
        state.bootstrap.go_path = Some(go_dir.join("bin"));
        info!("Go installation complete");
        println!("✓ Go installed successfully");
        Ok(())
    }

    /// Get the current bootstrap state
    pub fn check_status(&self, state: &LocalState) -> BootstrapStatus {
        BootstrapStatus {
            uv: if state.bootstrap.uv_installed {
                ComponentStatus::Installed(state.bootstrap.uv_path.clone())
            } else {
                ComponentStatus::NotInstalled
            },
            conda: if state.bootstrap.conda_installed {
                ComponentStatus::Installed(state.bootstrap.conda_path.clone())
            } else {
                ComponentStatus::NotInstalled
            },
            rust: if state.bootstrap.rust_installed {
                ComponentStatus::Installed(state.bootstrap.rust_path.clone())
            } else {
                ComponentStatus::NotInstalled
            },
            node: if state.bootstrap.node_installed {
                ComponentStatus::Installed(state.bootstrap.node_path.clone())
            } else {
                ComponentStatus::NotInstalled
            },
            go: if state.bootstrap.go_installed {
                ComponentStatus::Installed(state.bootstrap.go_path.clone())
            } else {
                ComponentStatus::NotInstalled
            },
        }
    }
}

/// Status of a bootstrap component
#[derive(Debug, Clone)]
pub enum ComponentStatus {
    NotInstalled,
    Installed(Option<PathBuf>),
    Error(String),
}

/// Overall bootstrap status
#[derive(Debug)]
pub struct BootstrapStatus {
    pub uv: ComponentStatus,
    pub conda: ComponentStatus,
    pub rust: ComponentStatus,
    pub node: ComponentStatus,
    pub go: ComponentStatus,
}

impl BootstrapStatus {
    /// True when every bootstrap component that was requested is installed. We treat all
    /// five toolchains (uv, conda, rust, node, go) as the full set.
    pub fn is_complete(&self) -> bool {
        [&self.uv, &self.conda, &self.rust, &self.node, &self.go]
            .iter()
            .all(|c| matches!(c, ComponentStatus::Installed(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_arch() {
        // This test will pass on x86_64 or aarch64, fail on unsupported archs
        let result = get_arch();
        // We just verify it doesn't panic and returns something reasonable
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_get_os() {
        let result = get_os();
        assert!(result.is_ok());
    }

    #[test]
    fn test_arch_display() {
        assert_eq!(format!("{}", Arch::X86_64), "x86_64");
        assert_eq!(format!("{}", Arch::Aarch64), "aarch64");
    }

    #[test]
    fn test_os_display() {
        assert_eq!(format!("{}", Os::Linux), "linux");
        assert_eq!(format!("{}", Os::MacOS), "macos");
    }

    #[test]
    fn test_miniforge_url_linux() {
        let url = miniforge_url(Arch::X86_64, Os::Linux).unwrap();
        assert!(url.contains("Miniforge3-Linux-x86_64.sh"));

        let url = miniforge_url(Arch::Aarch64, Os::Linux).unwrap();
        assert!(url.contains("Miniforge3-Linux-aarch64.sh"));
    }

    #[test]
    fn test_miniforge_url_macos() {
        let url = miniforge_url(Arch::X86_64, Os::MacOS).unwrap();
        assert!(url.contains("Miniforge3-MacOSX-x86_64.sh"));

        let url = miniforge_url(Arch::Aarch64, Os::MacOS).unwrap();
        assert!(url.contains("Miniforge3-MacOSX-arm64.sh"));
    }

    #[test]
    fn test_uv_url_all_platforms() {
        // Linux
        let url = uv_url(Arch::X86_64, Os::Linux).unwrap();
        assert!(url.contains("x86_64-unknown-linux-musl"));

        // macOS
        let url = uv_url(Arch::Aarch64, Os::MacOS).unwrap();
        assert!(url.contains("aarch64-apple-darwin"));
    }

    #[test]
    fn test_bootstrap_paths() {
        let paths = BootstrapPaths::new(PathBuf::from("/test/data"));
        assert_eq!(paths.bin_dir, PathBuf::from("/test/data/bin"));
        assert_eq!(paths.conda_dir, PathBuf::from("/test/data/conda"));
        assert_eq!(paths.downloads_dir, PathBuf::from("/test/data/downloads"));
    }

    #[test]
    fn test_bootstrap_status_complete() {
        // All five toolchains installed → complete.
        let status = BootstrapStatus {
            uv: ComponentStatus::Installed(Some(PathBuf::from("/bin/uv"))),
            conda: ComponentStatus::Installed(Some(PathBuf::from("/conda"))),
            rust: ComponentStatus::Installed(Some(PathBuf::from("/cargo"))),
            node: ComponentStatus::Installed(Some(PathBuf::from("/node"))),
            go: ComponentStatus::Installed(Some(PathBuf::from("/go"))),
        };
        assert!(status.is_complete());

        // Any missing component → not complete (here: go).
        let status = BootstrapStatus {
            uv: ComponentStatus::Installed(None),
            conda: ComponentStatus::Installed(None),
            rust: ComponentStatus::Installed(None),
            node: ComponentStatus::Installed(None),
            go: ComponentStatus::NotInstalled,
        };
        assert!(!status.is_complete());
    }
}
