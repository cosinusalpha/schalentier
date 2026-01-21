use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::{Confirm, MultiSelect};
use schalentier::{
    bootstrap::{get_arch, get_os, Bootstrap},
    cli::{ConfigAction, SnippetAction},
    config::{InstalledTool, ToolEntry, ToolStatus},
    detection::ToolDetector,
    dotfiles::{ApplyAction, DotfileManager},
    error::{self, print_info, print_success, print_warning},
    provider::create_default_registry,
    shell::{shell_init_snippet, write_env_scripts, ShellType},
    state::default_data_dir,
    Cli, Commands, LocalState, Provider, SchalentierConfig, Shell,
};
use tracing::{debug, info};

/// Format version string with consistent `v` prefix
/// Handles versions that already have `v` prefix to avoid `vv1.2.3`
fn format_version(version: &str) -> String {
    let v = version.strip_prefix('v').unwrap_or(version);
    format!("v{}", v)
}

/// Create a spinner with a message
fn create_spinner(msg: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message(msg.to_string());
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    spinner
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        error::print_error(&err);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse_args();

    // Initialize logging based on verbose flag
    schalentier::logging::init(cli.verbose);

    debug!("Schalentier started");
    debug!("Command: {:?}", cli.command);

    match cli.command {
        Commands::Init {
            force,
            yes,
            skip_bootstrap,
        } => {
            cmd_init(force, yes, skip_bootstrap).await?;
        }
        Commands::Add {
            name,
            provider,
            no_install,
            dry_run,
        } => {
            cmd_add(&name, provider.as_deref(), no_install, dry_run).await?;
        }
        Commands::Sync {
            remote,
            push,
            pull,
            prune,
            dry_run,
        } => {
            cmd_sync(remote.as_deref(), push, pull, prune, dry_run).await?;
        }
        Commands::Update { name, dry_run } => {
            cmd_update(name.as_deref(), dry_run).await?;
        }
        Commands::Doctor { fix } => {
            cmd_doctor(fix).await?;
        }
        Commands::Remove {
            name,
            keep_installed,
        } => {
            cmd_remove(&name, keep_installed).await?;
        }
        Commands::List { detailed, provider } => {
            cmd_list(detailed, provider.as_deref()).await?;
        }
        Commands::Search {
            query,
            limit,
            provider,
        } => {
            cmd_search(&query, limit, provider.as_deref()).await?;
        }
        Commands::Alias {
            definition,
            list,
            remove,
        } => {
            cmd_alias(definition, list, remove)?;
        }
        Commands::Snippet { action } => {
            cmd_snippet(action)?;
        }
        Commands::Config { action } => {
            cmd_config(action)?;
        }
        Commands::Completions { shell } => {
            cmd_completions(shell);
        }
    }

    Ok(())
}

/// Initialize schalentier
async fn cmd_init(force: bool, yes: bool, skip_bootstrap: bool) -> Result<()> {
    let mut state = LocalState::load()?;

    if state.initialized && !force {
        print_warning("Already initialized. Use --force to re-initialize.");
        return Ok(());
    }

    // Determine what to install
    let (install_uv, install_conda) = if skip_bootstrap {
        (false, false) // Skip all bootstrapping
    } else if yes {
        (true, true) // Install everything by default
    } else {
        prompt_init_options()?
    };

    info!("Initializing schalentier...");

    // Run bootstrap with user preferences (skipped if both are false)
    if install_uv || install_conda {
        let mut bootstrap = Bootstrap::new()?;
        bootstrap.set_install_uv(install_uv);
        bootstrap.set_install_conda(install_conda);
        bootstrap.run(&mut state).await?;
    } else {
        // Just mark as initialized without bootstrap
        state.initialized = true;
        print_info("Skipping bootstrap (no uv or conda will be installed)");
    }

    // Save state (also ensures data_dir exists)
    state.save()?;

    // Write environment scripts
    let data_dir = default_data_dir()?;
    write_env_scripts(&data_dir)?;

    // Create default config if it doesn't exist
    let config = SchalentierConfig::load()?;
    if config.tools.is_empty() {
        config.save()?;
        print_info("Created default configuration file");
    }

    print_success("Initialization complete!");

    // Show shell setup instructions
    if let Some(shell) = ShellType::detect() {
        println!("\nTo complete setup, add the following to your shell config:\n");
        println!("{}", shell_init_snippet(shell, &data_dir));
    }

    Ok(())
}

/// Prompt user for init options interactively
fn prompt_init_options() -> Result<(bool, bool)> {
    println!("\nWelcome to schalentier!\n");
    println!("This will set up your cross-platform package manager.\n");

    // Detect installed tools
    let detection = ToolDetector::detect_all();

    // Display detection results
    println!("{}System Tools Detected", "═".repeat(35));
    println!();
    for tool in detection.all() {
        if tool.available {
            let version_str = tool
                .version
                .as_ref()
                .map(|v| format!(" ({})", v))
                .unwrap_or_default();
            println!("✓ {}{}", tool.name, version_str);
        } else {
            println!("✗ {}", tool.name);
        }
    }
    println!();

    // Show recommendations based on detections
    if detection.has_alternative_tools() {
        println!("You already have access to several package managers.");
        println!("Bootstrapping uv and conda is optional.");
        println!();
    }

    // Prepare default selections based on what's already installed
    let mut defaults = vec![];
    let mut default_uv = true;
    let mut default_conda = true;

    if detection.uv.available {
        println!("ℹ uv is already installed. Skipping by default.");
        default_uv = false;
    }

    if detection.conda.available {
        println!("ℹ conda/mamba is already installed. Skipping by default.");
        default_conda = false;
    }

    if default_uv {
        defaults.push(0);
    }
    if default_conda {
        defaults.push(1);
    }

    println!();

    // Ask about bootstrap components
    let components = [
        (
            "uv",
            "uv - Fast Python package installer (recommended for Python CLI tools)",
        ),
        (
            "conda",
            "Miniforge/Conda - Scientific packages and isolated environments",
        ),
    ];

    let selections = MultiSelect::new(
        "Which package managers should be bootstrapped?",
        components.iter().map(|(_, desc)| *desc).collect(),
    )
    .with_default(&defaults)
    .with_help_message("Use space to toggle, enter to confirm. These can be installed later.")
    .prompt();

    let selected = match selections {
        Ok(s) => s,
        Err(inquire::InquireError::OperationCanceled) => {
            println!("\nSetup cancelled. Run 'schalentier init' to try again.");
            std::process::exit(0);
        }
        Err(e) => return Err(anyhow::anyhow!("Prompt error: {}", e)),
    };

    let install_uv = selected.iter().any(|s| s.contains("uv"));
    let install_conda = selected.iter().any(|s| s.contains("Miniforge"));

    // Confirm before proceeding
    println!();
    let proceed = Confirm::new("Proceed with installation?")
        .with_default(true)
        .prompt();

    match proceed {
        Ok(true) => Ok((install_uv, install_conda)),
        Ok(false) => {
            println!("\nSetup cancelled.");
            std::process::exit(0);
        }
        Err(inquire::InquireError::OperationCanceled) => {
            println!("\nSetup cancelled.");
            std::process::exit(0);
        }
        Err(e) => Err(anyhow::anyhow!("Prompt error: {}", e)),
    }
}

/// Add a package to the configuration
async fn cmd_add(
    name: &str,
    provider: Option<&str>,
    no_install: bool,
    dry_run: bool,
) -> Result<()> {
    let mut config = SchalentierConfig::load()?;
    let mut state = LocalState::load()?;

    // Check if already in config
    if config.tools.contains_key(name) && !dry_run {
        print_warning(&format!("'{}' is already in your configuration", name));
        return Ok(());
    }

    // Parse provider if specified
    let provider_enum = provider.map(|p| match p.to_lowercase().as_str() {
        "system" => Provider::System,
        "conda" => Provider::Conda,
        "cargo" => Provider::Cargo,
        "binary" => Provider::Binary,
        "uv" => Provider::Uv,
        "brew" => Provider::Brew,
        _ => Provider::Binary, // Default
    });

    // Dry run - show what would happen
    if dry_run {
        println!("Dry run: showing what would happen for '{}'", name);
        println!();

        // Check if tool exists
        if let Ok(existing_path) = which::which(name) {
            println!("  Tool already exists at: {}", existing_path.display());
            if !state.tools.contains_key(name) {
                println!("  Action: Would ADOPT existing installation");
            } else {
                println!("  Action: Already tracked in state");
            }
        } else {
            println!("  Tool not found on system");
            println!(
                "  Action: Would INSTALL via {}",
                provider_enum
                    .as_ref()
                    .map(|p| format!("{}", p))
                    .unwrap_or_else(|| "auto-detected provider".to_string())
            );
        }

        // Search for the tool to show available versions
        let arch = get_arch()?;
        let os = get_os()?;
        let data_dir = default_data_dir()?;
        let registry = create_default_registry(arch, os, data_dir);

        println!();
        println!("  Available from:");
        let results = registry.search_all_clustered(name, 1).await;
        for result in results
            .iter()
            .filter(|r| r.name.to_lowercase() == name.to_lowercase())
        {
            for p in &result.providers {
                let ver = p
                    .version
                    .as_ref()
                    .map(|v| format_version(v))
                    .unwrap_or_else(|| "?".to_string());
                println!("    - {} {}", p.provider, ver);
            }
        }
        if results.is_empty() {
            println!("    (no packages found)");
        }

        return Ok(());
    }

    // Add to config with the requested provider
    config.tools.insert(
        name.to_string(),
        ToolEntry {
            provider: provider_enum.clone(),
            version: None,
            options: std::collections::HashMap::new(),
        },
    );

    if no_install {
        config.save()?;
        print_success(&format!(
            "Added '{}' to configuration (not installed)",
            name
        ));
        return Ok(());
    }

    // Check if tool already exists on the system (adoption logic)
    if let Ok(existing_path) = which::which(name) {
        // Tool exists - check if it's already managed by us
        if !state.tools.contains_key(name) {
            // Not in our state - adopt it instead of installing
            print_info(&format!(
                "Found existing '{}' at {}",
                name,
                existing_path.display()
            ));

            // Try to get version from the existing binary
            let version = get_binary_version(&existing_path);

            // Determine provider (if it's in common paths, guess the provider)
            let detected_provider = detect_provider_from_path(&existing_path);

            state.tools.insert(
                name.to_string(),
                InstalledTool {
                    provider: detected_provider.clone(),
                    version,
                    path: Some(existing_path),
                    status: ToolStatus::Adopted,
                    managed: false, // Not managed by us - just adopted
                    installed_at: Some(chrono_lite_now()),
                    last_checked: None,
                },
            );
            state.save()?;
            config.save()?;

            print_success(&format!(
                "Adopted '{}' (managed by {})",
                name, detected_provider
            ));
            return Ok(());
        }
    }

    // Tool doesn't exist - proceed with installation
    let arch = get_arch()?;
    let os = get_os()?;
    let data_dir = default_data_dir()?;
    let registry = create_default_registry(arch, os, data_dir);

    info!("Installing '{}'...", name);

    // Show brief spinner while preparing, then clear for install
    // (clearing allows sudo password prompts to display correctly)
    let spinner = create_spinner(&format!("Preparing to install {}...", name));
    spinner.finish_and_clear();
    print_info(&format!("Installing {}...", name));

    // Use install_with_fallback which tries preferred provider first,
    // then falls back to others in priority order
    match registry
        .install_with_fallback(name, None, provider_enum.clone())
        .await
    {
        Ok((install_result, actual_provider)) => {
            // Check if fallback was used
            let used_fallback = provider_enum
                .as_ref()
                .is_some_and(|p| p != &actual_provider);
            if used_fallback {
                print_info(&format!(
                    "Note: Preferred provider {:?} unavailable, used {:?} instead",
                    provider_enum.unwrap(),
                    actual_provider
                ));
            }

            // Update state with the ACTUAL provider used
            state.tools.insert(
                name.to_string(),
                InstalledTool {
                    provider: actual_provider.clone(),
                    version: install_result.version.clone(),
                    path: install_result.path.clone(),
                    status: ToolStatus::Installed,
                    managed: true,
                    installed_at: Some(chrono_lite_now()),
                    last_checked: None,
                },
            );
            state.save()?;

            let ver = install_result.version.as_deref().unwrap_or("unknown");
            print_success(&format!(
                "Installed '{}' {} via {}",
                name,
                format_version(ver),
                actual_provider
            ));
        }
        Err(e) => {
            print_warning(&format!(
                "Installation failed: {}. Added to config but not installed.",
                e
            ));
        }
    }

    config.save()?;
    Ok(())
}

/// Try to get version from a binary by running it with --version
fn get_binary_version(path: &std::path::Path) -> Option<String> {
    use std::process::Command;

    let output = Command::new(path).arg("--version").output().ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse version from output like "tool 1.2.3" or "tool version 1.2.3"
        stdout
            .split_whitespace()
            .find(|s| {
                s.chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            })
            .map(|s| s.trim_end_matches(',').to_string())
    } else {
        None
    }
}

/// Detect likely provider from binary path
fn detect_provider_from_path(path: &std::path::Path) -> Provider {
    let path_str = path.to_string_lossy();

    if path_str.contains(".cargo/bin") {
        Provider::Cargo
    } else if path_str.contains("linuxbrew")
        || path_str.contains("homebrew")
        || path_str.contains("Cellar")
    {
        Provider::Brew
    } else if path_str.contains("conda")
        || path_str.contains("mamba")
        || path_str.contains("miniforge")
    {
        Provider::Conda
    } else if path_str.contains(".local/bin") {
        // Could be uv or pip installed
        Provider::Uv
    } else if path_str.contains(".schalentier") {
        Provider::Binary
    } else {
        Provider::System // Default fallback for /usr, /bin, and others
    }
}

/// Sync configuration with remote
async fn cmd_sync(
    remote: Option<&str>,
    push: bool,
    pull: bool,
    prune: bool,
    dry_run: bool,
) -> Result<()> {
    use std::process::Command;

    let config = SchalentierConfig::load()?;
    let state = LocalState::load()?;
    let config_dir = schalentier::state::config_dir()?;

    // Determine remote URL
    let remote_url = remote.map(String::from).or(config.sync.remote.clone());

    // Dry run - show what would happen
    if dry_run {
        println!("Dry run: showing what would happen during sync");
        println!();

        println!("  Config directory: {}", config_dir.display());
        println!(
            "  Remote URL: {}",
            remote_url.as_deref().unwrap_or("(not configured)")
        );
        println!(
            "  Mode: {}",
            if push && pull {
                "bidirectional"
            } else if push {
                "push"
            } else if pull {
                "pull"
            } else {
                "bidirectional (default)"
            }
        );
        println!();

        // Check if git repo exists
        let is_git_repo = config_dir.join(".git").exists();
        if !is_git_repo {
            println!("  Git repo: NOT INITIALIZED");
            if remote_url.is_some() {
                println!("  Action: Would clone from remote");
            } else {
                println!("  Action: Would initialize new git repo");
            }
        } else {
            println!("  Git repo: OK");

            // Check for uncommitted changes
            let status_output = Command::new("git")
                .current_dir(&config_dir)
                .args(["status", "--porcelain"])
                .output()?;
            let uncommitted = String::from_utf8_lossy(&status_output.stdout);
            if !uncommitted.is_empty() {
                println!("  Uncommitted changes: YES");
                for line in uncommitted.lines().take(5) {
                    println!("    {}", line);
                }
                if uncommitted.lines().count() > 5 {
                    println!("    ... and {} more", uncommitted.lines().count() - 5);
                }
            } else {
                println!("  Uncommitted changes: NO");
            }
        }

        // Show tools that would be installed (in config but not state)
        let to_install: Vec<_> = config
            .tools
            .keys()
            .filter(|name| !state.tools.contains_key(*name))
            .collect();

        if !to_install.is_empty() {
            println!();
            println!("  Tools to install (in config, not installed):");
            for name in &to_install {
                println!("    - {}", name);
            }
        }

        // Show tools that would be pruned (in state but not config)
        if prune {
            let to_prune: Vec<_> = state
                .tools
                .keys()
                .filter(|name| !config.tools.contains_key(*name))
                .collect();

            if !to_prune.is_empty() {
                println!();
                println!("  Tools to prune (installed, not in config):");
                for name in &to_prune {
                    println!("    - {}", name);
                }
            }
        }

        return Ok(());
    }

    // config is already loaded above for dry_run check
    // It will be reloaded as config_after after pull
    debug!("Config directory: {}", config_dir.display());

    // Check if config dir is a git repo
    let is_git_repo = config_dir.join(".git").exists();

    if !is_git_repo {
        if let Some(ref url) = remote_url {
            // Clone the remote into config dir
            let spinner = create_spinner(&format!("Cloning {}...", url));

            // Back up existing config if present
            let config_path = config_dir.join("schalentier.toml");
            let backup_path = config_dir.join("schalentier.toml.backup");
            if config_path.exists() {
                std::fs::copy(&config_path, &backup_path)?;
            }

            // Clone into a temp dir first, then move contents
            let temp_dir = config_dir.join(".git-clone-temp");
            let status = Command::new("git")
                .args(["clone", url, &temp_dir.to_string_lossy()])
                .status()?;

            if status.success() {
                // Move .git and files from temp to config dir
                let git_dir = temp_dir.join(".git");
                if git_dir.exists() {
                    std::fs::rename(&git_dir, config_dir.join(".git"))?;
                }
                // Copy any files from cloned repo
                for entry in std::fs::read_dir(&temp_dir)? {
                    let entry = entry?;
                    let dest = config_dir.join(entry.file_name());
                    if !dest.exists() {
                        std::fs::rename(entry.path(), dest)?;
                    }
                }
                let _ = std::fs::remove_dir_all(&temp_dir);
                spinner.finish_and_clear();
                print_success("Repository cloned successfully");
            } else {
                spinner.finish_and_clear();
                // Restore backup if clone failed
                if backup_path.exists() {
                    std::fs::rename(&backup_path, &config_path)?;
                }
                return Err(anyhow::anyhow!("Failed to clone repository"));
            }
        } else {
            // Initialize new git repo
            print_info("Initializing git repository in config directory...");
            let status = Command::new("git")
                .current_dir(&config_dir)
                .args(["init"])
                .status()?;

            if !status.success() {
                return Err(anyhow::anyhow!("Failed to initialize git repository"));
            }
            print_success("Git repository initialized");
            print_info("Use 'schalentier sync --remote <url>' to set up remote");
            return Ok(());
        }
    }

    // At this point we have a git repo
    let bidirectional = !push && !pull;

    // Get current branch name (defaults to "main" if detection fails)
    let branch = get_current_branch(&config_dir).unwrap_or_else(|| "main".to_string());
    debug!("Using branch: {}", branch);

    // PULL: Get changes from remote
    if pull || bidirectional {
        let spinner = create_spinner("Pulling changes from remote...");

        // Check if we have a remote configured
        let remote_check = Command::new("git")
            .current_dir(&config_dir)
            .args(["remote", "get-url", "origin"])
            .output()?;

        if !remote_check.status.success() {
            if let Some(ref url) = remote_url {
                // Add remote
                Command::new("git")
                    .current_dir(&config_dir)
                    .args(["remote", "add", "origin", url])
                    .status()?;
            } else {
                spinner.finish_and_clear();
                print_warning("No remote configured. Use --remote to specify one.");
                if !push {
                    return Ok(());
                }
            }
        }

        let pull_status = Command::new("git")
            .current_dir(&config_dir)
            .args(["pull", "--rebase", "origin", &branch])
            .status();

        match pull_status {
            Ok(status) if status.success() => {
                spinner.finish_and_clear();
                print_success("Pull completed");
            }
            Ok(_) => {
                // Try without --rebase, maybe it's a fresh repo
                let pull_status2 = Command::new("git")
                    .current_dir(&config_dir)
                    .args(["pull", "origin", &branch, "--allow-unrelated-histories"])
                    .status()?;

                spinner.finish_and_clear();
                if pull_status2.success() {
                    print_success("Pull completed (merged histories)");
                } else {
                    print_warning("Pull failed - you may need to resolve conflicts manually");
                }
            }
            Err(e) => {
                spinner.finish_and_clear();
                print_warning(&format!("Pull failed: {}", e));
            }
        }
    }

    // Reload config after pull
    let config_after = SchalentierConfig::load()?;

    // APPLY: Install tools that were added in the pulled config
    if pull || bidirectional {
        let mut state = LocalState::load()?;
        let arch = get_arch()?;
        let os = get_os()?;
        let data_dir = default_data_dir()?;
        let registry = create_default_registry(arch, os, data_dir);

        // Find tools in config but not in state (need to install)
        let to_install: Vec<_> = config_after
            .tools
            .iter()
            .filter(|(name, _)| !state.tools.contains_key(*name))
            .collect();

        if !to_install.is_empty() {
            print_info(&format!(
                "Installing {} new tools from config...",
                to_install.len()
            ));

            for (name, entry) in to_install {
                info!("Installing {}...", name);
                match registry
                    .install_with_fallback(name, entry.version.as_deref(), entry.provider.clone())
                    .await
                {
                    Ok((result, provider)) => {
                        if result.success {
                            state.tools.insert(
                                name.clone(),
                                InstalledTool {
                                    provider,
                                    version: result.version,
                                    path: result.path,
                                    status: ToolStatus::Installed,
                                    managed: true,
                                    installed_at: Some(chrono_lite_now()),
                                    last_checked: None,
                                },
                            );
                            print_success(&format!("  Installed {}", name));
                        } else {
                            print_warning(&format!(
                                "  Failed to install {}: {}",
                                name,
                                result.message.as_deref().unwrap_or("unknown error")
                            ));
                        }
                    }
                    Err(e) => {
                        print_warning(&format!("  Failed to install {}: {}", name, e));
                    }
                }
            }
            state.save()?;
        }

        // PRUNE: Remove tools that are in state but not in config
        if prune {
            let to_remove: Vec<_> = state
                .tools
                .keys()
                .filter(|name| !config_after.tools.contains_key(*name))
                .cloned()
                .collect();

            if !to_remove.is_empty() {
                print_info(&format!("Pruning {} orphaned tools...", to_remove.len()));

                for name in to_remove {
                    if let Some(tool) = state.tools.remove(&name) {
                        if let Some(provider) = registry.get(tool.provider.clone()) {
                            match provider.uninstall(&name).await {
                                Ok(_) => print_success(&format!("  Removed {}", name)),
                                Err(e) => {
                                    print_warning(&format!("  Failed to remove {}: {}", name, e))
                                }
                            }
                        }
                    }
                }
                state.save()?;
            }
        }
    }

    // PUSH: Commit and push local changes
    if push || bidirectional {
        // Check for uncommitted changes
        let status_output = Command::new("git")
            .current_dir(&config_dir)
            .args(["status", "--porcelain"])
            .output()?;

        let has_changes = !status_output.stdout.is_empty();

        if has_changes {
            print_info("Committing local changes...");

            // Add all changes
            Command::new("git")
                .current_dir(&config_dir)
                .args(["add", "-A"])
                .status()?;

            // Commit
            let commit_status = Command::new("git")
                .current_dir(&config_dir)
                .args(["commit", "-m", "Update schalentier configuration"])
                .status()?;

            if commit_status.success() {
                print_success("Changes committed");
            }
        }

        // Push
        let spinner = create_spinner("Pushing to remote...");
        let push_status = Command::new("git")
            .current_dir(&config_dir)
            .args(["push", "-u", "origin", &branch])
            .status()?;

        if push_status.success() {
            spinner.finish_and_clear();
            print_success("Push completed");
        } else {
            // Try creating the branch first
            let push_status2 = Command::new("git")
                .current_dir(&config_dir)
                .args(["push", "--set-upstream", "origin", &branch])
                .status()?;

            spinner.finish_and_clear();
            if push_status2.success() {
                print_success("Push completed");
            } else {
                print_warning("Push failed - check your remote configuration");
            }
        }
    }

    print_success("Sync complete!");
    Ok(())
}

/// Update installed packages
async fn cmd_update(name: Option<&str>, dry_run: bool) -> Result<()> {
    let mut state = LocalState::load()?;
    let arch = get_arch()?;
    let os = get_os()?;
    let data_dir = default_data_dir()?;
    let registry = create_default_registry(arch, os, data_dir);

    if dry_run {
        print_info("Checking for updates...\n");
    } else {
        print_info("Checking and applying updates...\n");
    }

    // Determine which tools to check
    let tools_to_check: Vec<(String, _)> = if let Some(tool_name) = name {
        if let Some(tool) = state.tools.get(tool_name) {
            vec![(tool_name.to_string(), tool.clone())]
        } else {
            print_warning(&format!("'{}' is not installed", tool_name));
            return Ok(());
        }
    } else {
        state
            .tools
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };

    if tools_to_check.is_empty() {
        println!("No tools installed to update.");
        return Ok(());
    }

    println!("Checking {} tool(s) for updates...\n", tools_to_check.len());

    let mut updates_available = Vec::new();
    let mut updates_applied = 0;
    let mut up_to_date = 0;
    let mut check_failed = 0;

    for (tool_name, tool) in &tools_to_check {
        // Skip adopted tools (not managed by us)
        if !tool.managed {
            println!("  {} - skipped (adopted, not managed)", tool_name);
            continue;
        }

        let current_version = tool.version.as_deref().unwrap_or("unknown");

        // Get the provider for this tool
        if let Some(provider) = registry.get(tool.provider.clone()) {
            // Query for latest version
            match provider.latest_version(tool_name).await {
                Ok(Some(latest)) => {
                    let needs_update = version_is_newer(&latest, current_version);

                    if needs_update {
                        println!(
                            "  {} {} -> {} [{}] UPDATE AVAILABLE",
                            tool_name,
                            format_version(current_version),
                            format_version(&latest),
                            tool.provider
                        );
                        updates_available.push((tool_name.clone(), tool.clone(), latest));
                    } else {
                        println!(
                            "  {} {} [{}] up to date",
                            tool_name,
                            format_version(current_version),
                            tool.provider
                        );
                        up_to_date += 1;
                    }
                }
                Ok(None) => {
                    println!(
                        "  {} {} [{}] - could not determine latest version",
                        tool_name,
                        format_version(current_version),
                        tool.provider
                    );
                    check_failed += 1;
                }
                Err(e) => {
                    println!(
                        "  {} {} [{}] - check failed: {}",
                        tool_name,
                        format_version(current_version),
                        tool.provider,
                        e
                    );
                    check_failed += 1;
                }
            }
        } else {
            println!(
                "  {} {} [{}] - provider not available",
                tool_name,
                format_version(current_version),
                tool.provider
            );
            check_failed += 1;
        }
    }

    // Summary
    println!();

    if updates_available.is_empty() {
        print_success(&format!("All {} managed tools are up to date!", up_to_date));
        if check_failed > 0 {
            print_warning(&format!("{} tools could not be checked", check_failed));
        }
        return Ok(());
    }

    println!(
        "Found {} update(s) available, {} up to date",
        updates_available.len(),
        up_to_date
    );

    if dry_run {
        println!();
        print_info("Run without --dry-run to apply updates");
        return Ok(());
    }

    // Apply updates
    println!();
    print_info("Applying updates...\n");

    for (tool_name, tool, new_version) in updates_available {
        print!("  Updating {}...", tool_name);

        if let Some(provider) = registry.get(tool.provider.clone()) {
            match provider.install(&tool_name, Some(&new_version)).await {
                Ok(result) if result.success => {
                    let ver = result.version.as_deref().unwrap_or(&new_version);
                    println!(" done ({})", format_version(ver));

                    // Update state
                    if let Some(state_tool) = state.tools.get_mut(&tool_name) {
                        state_tool.version = result.version.or(Some(new_version));
                        state_tool.path = result.path.or(state_tool.path.clone());
                        state_tool.last_checked = Some(chrono_lite_now());
                    }
                    updates_applied += 1;
                }
                Ok(result) => {
                    println!(
                        " FAILED: {}",
                        result.message.as_deref().unwrap_or("unknown error")
                    );
                }
                Err(e) => {
                    println!(" FAILED: {}", e);
                }
            }
        }
    }

    // Save state
    state.save()?;

    println!();
    if updates_applied > 0 {
        print_success(&format!("Successfully updated {} tool(s)", updates_applied));
    }

    Ok(())
}

/// Compare versions to determine if `latest` is newer than `current`.
/// Simple semver-like comparison (handles x.y.z format).
fn version_is_newer(latest: &str, current: &str) -> bool {
    // Strip common prefixes like 'v'
    let latest = latest.trim_start_matches('v');
    let current = current.trim_start_matches('v');

    // If versions are equal, no update needed
    if latest == current {
        return false;
    }

    // Parse as semver-like components
    let parse_version = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+'])
            .filter_map(|part| part.parse::<u64>().ok())
            .collect()
    };

    let latest_parts = parse_version(latest);
    let current_parts = parse_version(current);

    // Compare component by component
    for (l, c) in latest_parts.iter().zip(current_parts.iter()) {
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }

    // If all compared parts are equal, longer version is newer
    // (e.g., 1.0.1 > 1.0)
    latest_parts.len() > current_parts.len()
}

/// Run diagnostics
async fn cmd_doctor(fix: bool) -> Result<()> {
    use std::process::Command;

    print_info("Running diagnostics...\n");

    let data_dir = default_data_dir()?;
    let state = LocalState::load()?;
    let config = SchalentierConfig::load()?;
    let config_dir = schalentier::state::config_dir()?;

    let mut issues_found = 0;
    let mut issues_fixed = 0;

    println!("=== Core Status ===\n");

    // Check data directory
    print!("Data directory: ");
    if data_dir.exists() {
        println!("OK ({})", data_dir.display());
    } else {
        println!("MISSING");
        issues_found += 1;
        if fix {
            std::fs::create_dir_all(&data_dir)?;
            println!("  -> Created");
            issues_fixed += 1;
        }
    }

    // Check config directory
    print!("Config directory: ");
    if config_dir.exists() {
        println!("OK ({})", config_dir.display());
    } else {
        println!("MISSING");
        issues_found += 1;
        if fix {
            std::fs::create_dir_all(&config_dir)?;
            println!("  -> Created");
            issues_fixed += 1;
        }
    }

    // Check initialization
    print!("Initialized: ");
    if state.initialized {
        println!("OK");
    } else {
        println!("NO - run 'schalentier init'");
        issues_found += 1;
    }

    // Check bootstrap components
    print!("Bootstrap - uv: ");
    if state.bootstrap.uv_installed {
        println!(
            "OK ({})",
            state
                .bootstrap
                .uv_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "path unknown".to_string())
        );
    } else {
        println!("NOT INSTALLED");
    }

    print!("Bootstrap - conda: ");
    if state.bootstrap.conda_installed {
        println!(
            "OK ({})",
            state
                .bootstrap
                .conda_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "path unknown".to_string())
        );
    } else {
        println!("NOT INSTALLED");
    }

    // Check environment scripts
    print!("Environment scripts: ");
    let env_sh = data_dir.join("env.sh");
    let env_fish = data_dir.join("env.fish");
    let env_ps1 = data_dir.join("env.ps1");
    if env_sh.exists() && env_fish.exists() && env_ps1.exists() {
        println!("OK");
    } else {
        println!("MISSING");
        issues_found += 1;
        if fix {
            write_env_scripts(&data_dir)?;
            println!("  -> Generated");
            issues_fixed += 1;
        }
    }

    println!("\n=== Available Providers ===\n");

    // Check available providers
    let arch = get_arch()?;
    let os = get_os()?;
    let registry = create_default_registry(arch, os, data_dir.clone());

    let provider_checks = [
        ("Binary (GitHub)", Provider::Binary),
        ("Cargo (crates.io)", Provider::Cargo),
        ("Brew (Homebrew)", Provider::Brew),
        ("Conda (conda-forge)", Provider::Conda),
        ("System (apt/dnf/pacman)", Provider::System),
        ("UV (PyPI)", Provider::Uv),
    ];

    for (name, provider_type) in provider_checks {
        print!("{}: ", name);
        if let Some(provider) = registry.get(provider_type.clone()) {
            if provider.is_available() {
                println!("Available");
            } else {
                println!("Registered but not available");
            }
        } else {
            println!("Not registered");
        }
    }

    println!("\n=== Sync Status ===\n");

    // Check sync/git status
    let is_git_repo = config_dir.join(".git").exists();
    print!("Git repository: ");
    if is_git_repo {
        println!("OK");

        // Check remote
        let remote_output = Command::new("git")
            .current_dir(&config_dir)
            .args(["remote", "get-url", "origin"])
            .output();

        print!("Remote configured: ");
        if let Ok(output) = remote_output {
            if output.status.success() {
                let url = String::from_utf8_lossy(&output.stdout);
                println!("OK ({})", url.trim());
            } else {
                println!("NO - run 'schalentier sync --remote <url>'");
            }
        } else {
            println!("Could not check");
        }

        // Check for uncommitted changes
        let status_output = Command::new("git")
            .current_dir(&config_dir)
            .args(["status", "--porcelain"])
            .output();

        print!("Uncommitted changes: ");
        if let Ok(output) = status_output {
            if output.stdout.is_empty() {
                println!("None");
            } else {
                let changes = String::from_utf8_lossy(&output.stdout).lines().count();
                println!("{} files modified", changes);
            }
        } else {
            println!("Could not check");
        }
    } else {
        println!("NOT INITIALIZED - run 'schalentier sync' to set up");
    }

    println!("\n=== Tool Status ===\n");

    println!("Configuration: {} tools defined", config.tools.len());
    println!("State: {} tools tracked", state.tools.len());

    // Count by status
    let managed_count = state.tools.values().filter(|t| t.managed).count();
    let adopted_count = state.tools.values().filter(|t| !t.managed).count();
    println!("  - Managed by schalentier: {}", managed_count);
    println!("  - Adopted (external): {}", adopted_count);

    // Check for orphaned tools (in state but not in config)
    let orphaned: Vec<_> = state
        .tools
        .keys()
        .filter(|k| !config.tools.contains_key(*k))
        .collect();

    if !orphaned.is_empty() {
        println!();
        print_warning(&format!(
            "{} orphaned tools (installed but not in config):",
            orphaned.len()
        ));
        for name in &orphaned {
            println!("  - {}", name);
        }
        issues_found += orphaned.len();
    }

    // Check for missing tools (in config but not in state)
    let missing: Vec<_> = config
        .tools
        .keys()
        .filter(|k| !state.tools.contains_key(*k))
        .collect();

    if !missing.is_empty() {
        println!();
        print_warning(&format!(
            "{} missing tools (in config but not installed):",
            missing.len()
        ));
        for name in &missing {
            println!("  - {}", name);
        }
        issues_found += missing.len();
    }

    // Check tool accessibility (can we find them in PATH?)
    let mut inaccessible: Vec<(&String, &Provider)> = Vec::new();
    for (name, tool) in &state.tools {
        if which::which(name).is_err() {
            inaccessible.push((name, &tool.provider));
        }
    }

    if !inaccessible.is_empty() {
        println!();
        print_warning(&format!(
            "{} tools not accessible (not in PATH):",
            inaccessible.len()
        ));
        for (name, provider) in &inaccessible {
            println!("  - {} [{}]", name, provider);
        }
        issues_found += inaccessible.len();
        if !state.initialized {
            print_info("  Run 'schalentier init' and source the env script to fix PATH");
        }
    }

    // Summary
    println!("\n=== Summary ===\n");
    if issues_found == 0 && state.initialized {
        print_success("All checks passed!");
    } else {
        println!("Issues found: {}", issues_found);
        if fix && issues_fixed > 0 {
            print_success(&format!("Issues fixed: {}", issues_fixed));
        }
        if issues_found > issues_fixed {
            print_info("Some issues require manual intervention");
        }
    }

    Ok(())
}

/// Remove a package
async fn cmd_remove(name: &str, keep_installed: bool) -> Result<()> {
    let mut config = SchalentierConfig::load()?;
    let mut state = LocalState::load()?;

    if !config.tools.contains_key(name) && !state.tools.contains_key(name) {
        print_warning(&format!("'{}' is not managed by schalentier", name));
        return Ok(());
    }

    // Check if the tool is adopted (not managed by schalentier)
    if let Some(tool) = state.tools.get(name) {
        if !tool.managed {
            // Tool is adopted - refuse to uninstall
            print_warning(&format!(
                "'{}' was not installed by schalentier (managed by {})",
                name, tool.provider
            ));
            print_info("Use --keep-installed to remove from config without uninstalling");

            if keep_installed {
                // Allow removing from config only
                config.tools.remove(name);
                state.tools.remove(name);
                config.save()?;
                state.save()?;
                print_success(&format!(
                    "Removed '{}' from tracking (tool still installed)",
                    name
                ));
            }
            return Ok(());
        }
    }

    // Remove from config
    config.tools.remove(name);
    config.save()?;

    if keep_installed {
        print_success(&format!(
            "Removed '{}' from configuration (kept installed)",
            name
        ));
        return Ok(());
    }

    // Uninstall if we have it in state
    if let Some(tool) = state.tools.remove(name) {
        let arch = get_arch()?;
        let os = get_os()?;
        let data_dir = default_data_dir()?;
        let registry = create_default_registry(arch, os, data_dir);

        if let Some(installer) = registry.get(tool.provider.clone()) {
            installer.uninstall(name).await?;
        }

        state.save()?;
        print_success(&format!("Removed '{}' and uninstalled", name));
    } else {
        print_success(&format!("Removed '{}' from configuration", name));
    }

    Ok(())
}

/// List managed tools
async fn cmd_list(detailed: bool, provider_filter: Option<&str>) -> Result<()> {
    let config = SchalentierConfig::load()?;
    let state = LocalState::load()?;

    if config.tools.is_empty() && state.tools.is_empty() {
        println!("No tools managed. Use 'schalentier add <tool>' to add one.");
        return Ok(());
    }

    // Parse provider filter
    let filter = provider_filter.map(|p| match p.to_lowercase().as_str() {
        "system" => Provider::System,
        "conda" => Provider::Conda,
        "cargo" => Provider::Cargo,
        "binary" => Provider::Binary,
        "uv" => Provider::Uv,
        "brew" => Provider::Brew,
        _ => Provider::Binary,
    });

    // Collect all tools (from both config and state)
    let mut all_tools: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for name in config.tools.keys() {
        all_tools.insert(name.clone());
    }
    for name in state.tools.keys() {
        all_tools.insert(name.clone());
    }

    println!("Managed tools:\n");

    for name in &all_tools {
        let config_entry = config.tools.get(name);
        let installed = state.tools.get(name);

        // Get actual provider (from state if installed, otherwise from config)
        let actual_provider = installed
            .map(|t| t.provider.clone())
            .or_else(|| config_entry.and_then(|e| e.provider.clone()));

        // Apply filter
        if let Some(ref f) = filter {
            if actual_provider.as_ref() != Some(f) {
                continue;
            }
        }

        if detailed {
            println!("{}:", name);

            // Show ownership/management status
            let ownership = match (config_entry, installed) {
                (Some(_), Some(tool)) if tool.managed => "Installed by schalentier",
                (Some(_), Some(_)) => "Adopted (external)",
                (Some(_), None) => "In config (not installed)",
                (None, Some(tool)) if tool.managed => "Orphaned (installed but not in config)",
                (None, Some(_)) => "Orphaned adopted (not in config)",
                (None, None) => "Unknown",
            };
            println!("  Ownership: {}", ownership);

            println!(
                "  Provider: {}",
                actual_provider
                    .as_ref()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "auto".to_string())
            );

            if let Some(entry) = config_entry {
                println!(
                    "  Config version: {}",
                    entry.version.as_deref().unwrap_or("any")
                );
            }

            if let Some(tool) = installed {
                println!("  Status: {:?}", tool.status);
                println!(
                    "  Installed version: {}",
                    tool.version.as_deref().unwrap_or("unknown")
                );
                if let Some(ref path) = tool.path {
                    println!("  Path: {}", path.display());
                }

                // Check if binary is actually accessible
                let accessible = which::which(name).is_ok();
                println!(
                    "  Accessible: {}",
                    if accessible {
                        "yes"
                    } else {
                        "NO (not in PATH)"
                    }
                );
            } else {
                println!("  Status: Not installed");
            }
            println!();
        } else {
            // Compact view
            let (status_symbol, status_text) = match (config_entry, installed) {
                (Some(_), Some(tool)) if tool.managed => ("✓", "installed"),
                (Some(_), Some(_)) => ("~", "adopted"),
                (Some(_), None) => ("○", "pending"),
                (None, Some(tool)) if tool.managed => ("!", "orphaned"),
                (None, Some(_)) => ("!", "orphaned-adopted"),
                (None, None) => ("?", "unknown"),
            };

            let version = installed
                .and_then(|t| t.version.as_ref())
                .map(|v| format_version(v))
                .unwrap_or_default();

            let provider_str = actual_provider
                .as_ref()
                .map(|p| format!("[{}]", p))
                .unwrap_or_default();

            println!(
                "  {} {} {} {} {}",
                status_symbol, name, version, provider_str, status_text
            );
        }
    }

    // Show legend in compact view
    if !detailed {
        println!();
        println!("Legend: ✓=installed ~=adopted ○=pending !=orphaned");
    }

    Ok(())
}

/// Search for packages
async fn cmd_search(query: &str, limit: usize, provider_filter: Option<&str>) -> Result<()> {
    let arch = get_arch()?;
    let os = get_os()?;
    let data_dir = default_data_dir()?;
    let registry = create_default_registry(arch, os, data_dir);

    // Parse provider filter if specified
    let filter = provider_filter.and_then(|p| match p.to_lowercase().as_str() {
        "system" => Some(Provider::System),
        "conda" => Some(Provider::Conda),
        "cargo" => Some(Provider::Cargo),
        "binary" => Some(Provider::Binary),
        "uv" => Some(Provider::Uv),
        "brew" => Some(Provider::Brew),
        _ => {
            print_warning(&format!(
                "Unknown provider '{}', searching all providers",
                p
            ));
            None
        }
    });

    let results = if let Some(ref provider_type) = filter {
        // Search only the specified provider
        let spinner = create_spinner(&format!(
            "Searching for '{}' in {:?}...",
            query, provider_type
        ));

        let results = if let Some(provider) = registry.get(provider_type.clone()) {
            if provider.is_available() {
                match provider.search(query, limit).await {
                    Ok(results) => results,
                    Err(e) => {
                        spinner.finish_and_clear();
                        print_warning(&format!("Search failed: {}", e));
                        Vec::new()
                    }
                }
            } else {
                spinner.finish_and_clear();
                print_warning(&format!(
                    "Provider {:?} is not available on this system",
                    provider_type
                ));
                Vec::new()
            }
        } else {
            spinner.finish_and_clear();
            print_warning(&format!("Provider {:?} is not registered", provider_type));
            Vec::new()
        };
        spinner.finish_and_clear();
        results
    } else {
        // Search all providers with clustering
        let spinner = create_spinner(&format!("Searching for '{}'...", query));
        let clustered = registry.search_all_clustered(query, limit).await;
        spinner.finish_and_clear();

        if clustered.is_empty() {
            println!("No results found for '{}'", query);
            return Ok(());
        }

        println!("Found {} unique packages:\n", clustered.len());

        for result in clustered {
            // Format providers list
            let providers_str: Vec<String> = result
                .providers
                .iter()
                .map(|p| {
                    if let Some(ref v) = p.version {
                        format!("{} {}", p.provider, format_version(v))
                    } else {
                        format!("{}", p.provider)
                    }
                })
                .collect();

            println!("  {}", result.name);
            println!("    Available from: {}", providers_str.join(", "));

            if let Some(desc) = &result.description {
                // Truncate long descriptions
                let desc = if desc.len() > 60 {
                    format!("{}...", &desc[..57])
                } else {
                    desc.clone()
                };
                println!("    {}", desc);
            }
            if let Some(stars) = result.metadata.get("stars") {
                println!("    Stars: {}", stars);
            }
            println!();
        }

        return Ok(());
    };

    // Single provider results (when filter is specified)
    if results.is_empty() {
        println!("No results found for '{}'", query);
        return Ok(());
    }

    println!("Found {} results:\n", results.len());

    for result in results {
        let version_str = result
            .version
            .as_deref()
            .map(format_version)
            .unwrap_or_else(|| "v?".to_string());
        println!("  {} {} [{}]", result.name, version_str, result.provider);
        if let Some(desc) = &result.description {
            // Truncate long descriptions
            let desc = if desc.len() > 60 {
                format!("{}...", &desc[..57])
            } else {
                desc.clone()
            };
            println!("    {}", desc);
        }
        if let Some(stars) = result.metadata.get("stars") {
            println!("    Stars: {}", stars);
        }
        println!();
    }

    Ok(())
}

/// Simple timestamp function (avoiding chrono dependency)
fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

/// Get the current git branch name for a repository
fn get_current_branch(repo_dir: &std::path::Path) -> Option<String> {
    use std::process::Command;

    // Try to get current branch via git
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() && branch != "HEAD" {
            return Some(branch);
        }
    }

    // Fallback: try to detect default branch from remote
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        let full_ref = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // refs/remotes/origin/main -> main
        if let Some(branch) = full_ref.rsplit('/').next() {
            return Some(branch.to_string());
        }
    }

    None
}

//=============================================================================
// Alias Command Implementation (Task 6.7)
//=============================================================================

/// Marker prefix for alias scripts to identify them
const ALIAS_MARKER: &str = "# schalentier-alias";

/// Create, list, or remove shell aliases as executable scripts
fn cmd_alias(definition: Option<String>, list: bool, remove: Option<String>) -> Result<()> {
    let data_dir = default_data_dir()?;
    let bin_dir = data_dir.join("bin");

    // Ensure bin directory exists
    std::fs::create_dir_all(&bin_dir)?;

    if list {
        // List all aliases
        return list_aliases(&bin_dir);
    }

    if let Some(name) = remove {
        // Remove an alias
        return remove_alias(&bin_dir, &name);
    }

    if let Some(def) = definition {
        // Create a new alias
        return create_alias(&bin_dir, &def);
    }

    // No arguments - show help
    print_info("Usage:");
    println!("  schalentier alias NAME=\"COMMAND\"  Create an alias");
    println!("  schalentier alias --list           List all aliases");
    println!("  schalentier alias --remove NAME    Remove an alias");
    println!();
    println!("Examples:");
    println!("  schalentier alias ll=\"ls -la\"");
    println!("  schalentier alias lt=\"ls -ltrh\"");
    println!("  schalentier alias g=\"git\"");

    Ok(())
}

/// Create a new alias script
fn create_alias(bin_dir: &std::path::Path, definition: &str) -> Result<()> {
    // Parse NAME=COMMAND or NAME="COMMAND"
    let (name, command) = parse_alias_definition(definition)?;

    // Validate name (alphanumeric, underscores, hyphens only)
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        anyhow::bail!(
            "Invalid alias name '{}'. Use only letters, numbers, underscores, and hyphens.",
            name
        );
    }

    // Check if it would shadow an existing binary
    if which::which(&name).is_ok() {
        let existing = which::which(&name).unwrap();
        // Only warn if it's not our own alias
        if !existing.starts_with(bin_dir) {
            print_warning(&format!(
                "Alias '{}' will shadow existing command at {}",
                name,
                existing.display()
            ));
        }
    }

    let script_path = bin_dir.join(&name);

    // Generate script content
    let script = format!(
        r#"#!/bin/sh
{marker}
# Alias: {name}
# Command: {command}
exec {command} "$@"
"#,
        marker = ALIAS_MARKER,
        name = name,
        command = command
    );

    // Write script
    std::fs::write(&script_path, &script)?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms)?;
    }

    print_success(&format!("Created alias '{}' -> '{}'", name, command));
    debug!("Alias script created at {}", script_path.display());

    Ok(())
}

/// Parse alias definition like "ll=ls -la" or "ll=\"ls -la\""
fn parse_alias_definition(definition: &str) -> Result<(String, String)> {
    // Find the first '='
    let eq_pos = definition
        .find('=')
        .ok_or_else(|| anyhow::anyhow!("Invalid alias format. Use NAME=\"COMMAND\""))?;

    let name = definition[..eq_pos].trim().to_string();
    let mut command = definition[eq_pos + 1..].trim().to_string();

    // Remove surrounding quotes if present
    if (command.starts_with('"') && command.ends_with('"'))
        || (command.starts_with('\'') && command.ends_with('\''))
    {
        command = command[1..command.len() - 1].to_string();
    }

    if name.is_empty() || command.is_empty() {
        anyhow::bail!("Invalid alias format. Use NAME=\"COMMAND\"");
    }

    Ok((name, command))
}

/// List all defined aliases
fn list_aliases(bin_dir: &std::path::Path) -> Result<()> {
    let mut aliases = Vec::new();

    if bin_dir.exists() {
        for entry in std::fs::read_dir(bin_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                // Check if it's an alias script (contains our marker)
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains(ALIAS_MARKER) {
                        // Extract alias info from comments
                        let name = path.file_name().unwrap().to_string_lossy().to_string();
                        let command = content
                            .lines()
                            .find(|l| l.starts_with("# Command:"))
                            .map(|l| l.trim_start_matches("# Command:").trim().to_string())
                            .unwrap_or_else(|| "?".to_string());

                        aliases.push((name, command));
                    }
                }
            }
        }
    }

    if aliases.is_empty() {
        println!("No aliases defined.");
        println!();
        println!("Create one with: schalentier alias NAME=\"COMMAND\"");
    } else {
        println!("Defined aliases:");
        println!();
        for (name, command) in &aliases {
            println!("  {} = {}", name, command);
        }
        println!();
        println!("Total: {} alias(es)", aliases.len());
    }

    Ok(())
}

/// Remove an alias
fn remove_alias(bin_dir: &std::path::Path, name: &str) -> Result<()> {
    let script_path = bin_dir.join(name);

    if !script_path.exists() {
        anyhow::bail!("Alias '{}' not found", name);
    }

    // Verify it's actually an alias script (not a real binary)
    let content = std::fs::read_to_string(&script_path)?;
    if !content.contains(ALIAS_MARKER) {
        anyhow::bail!(
            "'{}' is not an alias created by schalentier (it's a real binary)",
            name
        );
    }

    std::fs::remove_file(&script_path)?;
    print_success(&format!("Removed alias '{}'", name));

    Ok(())
}

//=============================================================================
// Snippet Command Implementation (Task 6.8)
//=============================================================================

/// Manage shell snippets
fn cmd_snippet(action: SnippetAction) -> Result<()> {
    match action {
        SnippetAction::List => snippet_list(),
        SnippetAction::Add { name, file } => snippet_add(name, file),
        SnippetAction::Remove { name } => snippet_remove(&name),
    }
}

/// List installed snippets
fn snippet_list() -> Result<()> {
    let data_dir = default_data_dir()?;
    let snippets_dir = data_dir.join("snippets.d");

    let mut snippets = Vec::new();

    if snippets_dir.exists() {
        for entry in std::fs::read_dir(&snippets_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_stem() {
                    snippets.push(name.to_string_lossy().to_string());
                }
            }
        }
    }

    if snippets.is_empty() {
        println!("No snippets installed.");
        println!();
        println!("Available built-in snippets: yazi, zoxide, fzf, direnv, starship, atuin");
        println!();
        println!("Add one with: schalentier snippet add <name>");
    } else {
        println!("Installed snippets:");
        println!();
        for name in &snippets {
            println!("  {}", name);
        }
        println!();
        println!("Total: {} snippet(s)", snippets.len());
    }

    Ok(())
}

/// Add a snippet (from registry or custom file)
fn snippet_add(name: Option<String>, file: Option<String>) -> Result<()> {
    let data_dir = default_data_dir()?;
    let snippets_dir = data_dir.join("snippets.d");
    std::fs::create_dir_all(&snippets_dir)?;

    if let Some(file_path) = file {
        // Custom snippet from file
        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read snippet file: {}", e))?;

        let snippet_name = name.unwrap_or_else(|| {
            std::path::Path::new(&file_path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

        let dest = snippets_dir.join(format!("{}.bash", snippet_name));
        std::fs::write(&dest, content)?;
        print_success(&format!("Added custom snippet '{}'", snippet_name));
        return Ok(());
    }

    let name = name.ok_or_else(|| anyhow::anyhow!("Specify a snippet name or --file"))?;

    // Check built-in registry
    let snippet = get_builtin_snippet(&name)?;

    let dest = snippets_dir.join(format!("{}.bash", name));
    std::fs::write(&dest, snippet)?;
    print_success(&format!("Added snippet '{}' from built-in registry", name));

    // Show what was added
    println!();
    println!("Snippet will be active after reloading your shell (source ~/.schalentier/env.sh)");

    Ok(())
}

/// Remove a snippet
fn snippet_remove(name: &str) -> Result<()> {
    let data_dir = default_data_dir()?;
    let snippets_dir = data_dir.join("snippets.d");

    // Try various extensions
    let extensions = ["bash", "sh", "fish", "ps1"];
    let mut removed = false;

    for ext in &extensions {
        let path = snippets_dir.join(format!("{}.{}", name, ext));
        if path.exists() {
            std::fs::remove_file(&path)?;
            removed = true;
        }
    }

    if removed {
        print_success(&format!("Removed snippet '{}'", name));
    } else {
        anyhow::bail!("Snippet '{}' not found", name);
    }

    Ok(())
}

//=============================================================================
// Config Command Implementation (Task 6.5)
//=============================================================================

/// Manage dotfile/config patching
fn cmd_config(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Apply => config_apply(),
        ConfigAction::Diff => config_diff(),
        ConfigAction::List => config_list(),
        ConfigAction::Reset { file } => config_reset(&file),
    }
}

/// Apply all dotfile patches
fn config_apply() -> Result<()> {
    let config = SchalentierConfig::load()?;

    if config.dotfiles.is_empty() {
        println!("No dotfiles configured.");
        println!();
        println!("Add dotfiles to your schalentier.toml:");
        println!();
        println!("  [dotfiles.\"~/.config/micro/settings.json\"]");
        println!("  colorscheme = \"monokai\"");
        println!("  tabsize = 4");
        return Ok(());
    }

    let manager = DotfileManager::from_config(&config.dotfiles)?;
    let results = manager.apply()?;

    let mut created = 0;
    let mut updated = 0;
    let mut unchanged = 0;

    for result in &results {
        match result.action {
            ApplyAction::Created => {
                println!("  Created: {}", result.path.display());
                created += 1;
            }
            ApplyAction::Updated => {
                println!("  Updated: {}", result.path.display());
                updated += 1;
            }
            ApplyAction::Unchanged => {
                debug!("Unchanged: {}", result.path.display());
                unchanged += 1;
            }
        }
    }

    println!();
    if created > 0 || updated > 0 {
        print_success(&format!(
            "Applied {} dotfile patch(es): {} created, {} updated, {} unchanged",
            results.len(),
            created,
            updated,
            unchanged
        ));
    } else {
        print_info("All dotfiles are already up to date");
    }

    Ok(())
}

/// Show diff of what would change
fn config_diff() -> Result<()> {
    let config = SchalentierConfig::load()?;

    if config.dotfiles.is_empty() {
        println!("No dotfiles configured.");
        return Ok(());
    }

    let manager = DotfileManager::from_config(&config.dotfiles)?;
    let diffs = manager.diff()?;

    let mut has_changes = false;

    for diff in &diffs {
        if diff.would_create {
            has_changes = true;
            println!("Would CREATE: {}", diff.path.display());
            println!("  Format: {:?}", diff.format);
            println!();
        } else if diff.would_modify {
            has_changes = true;
            println!("Would MODIFY: {}", diff.path.display());
            println!("  Format: {:?}", diff.format);

            // Show a simple diff
            if let Some(ref current) = diff.current {
                let current_lines: Vec<_> = current.lines().collect();
                let proposed_lines: Vec<_> = diff.proposed.lines().collect();

                // Simple line-by-line diff (just show changes, not full unified diff)
                let max_lines = current_lines.len().max(proposed_lines.len());
                let mut shown = 0;
                for i in 0..max_lines {
                    let curr = current_lines.get(i).copied().unwrap_or("");
                    let prop = proposed_lines.get(i).copied().unwrap_or("");
                    if curr != prop {
                        if shown < 10 {
                            if !curr.is_empty() {
                                println!("  - {}", curr);
                            }
                            if !prop.is_empty() {
                                println!("  + {}", prop);
                            }
                            shown += 1;
                        } else if shown == 10 {
                            println!("  ... ({} more changes)", max_lines - i);
                            break;
                        }
                    }
                }
            }
            println!();
        }
    }

    if !has_changes {
        print_info("No changes would be made. All dotfiles are up to date.");
    } else {
        println!();
        print_info("Run 'schalentier config apply' to apply these changes");
    }

    Ok(())
}

/// List managed dotfiles
fn config_list() -> Result<()> {
    let config = SchalentierConfig::load()?;

    if config.dotfiles.is_empty() {
        println!("No dotfiles configured.");
        println!();
        println!("Add dotfiles to your schalentier.toml:");
        println!();
        println!("  [dotfiles.\"~/.config/micro/settings.json\"]");
        println!("  colorscheme = \"monokai\"");
        return Ok(());
    }

    let manager = DotfileManager::from_config(&config.dotfiles)?;
    let patches = manager.list();

    println!("Managed dotfiles ({}):", patches.len());
    println!();

    for patch in patches {
        let status = if patch.target.exists() {
            "exists"
        } else {
            "pending"
        };

        println!(
            "  {} [{:?}] ({})",
            patch.target.display(),
            patch.format,
            status
        );
    }

    Ok(())
}

/// Reset a file from backup
fn config_reset(file: &str) -> Result<()> {
    let config = SchalentierConfig::load()?;
    let manager = DotfileManager::from_config(&config.dotfiles)?;

    manager.reset(file)?;
    print_success(&format!("Restored {} from backup", file));

    Ok(())
}

/// Get a built-in snippet by name
fn get_builtin_snippet(name: &str) -> Result<String> {
    let snippet = match name.to_lowercase().as_str() {
        "yazi" => {
            r#"# Schalentier snippet: yazi
# Wrapper function for yazi that changes directory on exit

function yy() {
    local tmp="$(mktemp -t "yazi-cwd.XXXXXX")"
    yazi "$@" --cwd-file="$tmp"
    if cwd="$(cat -- "$tmp")" && [ -n "$cwd" ] && [ "$cwd" != "$PWD" ]; then
        builtin cd -- "$cwd"
    fi
    rm -f -- "$tmp"
}
"#
        }
        "zoxide" => {
            r#"# Schalentier snippet: zoxide
# Smart cd command that learns your habits

if command -v zoxide >/dev/null 2>&1; then
    eval "$(zoxide init bash)"
fi
"#
        }
        "fzf" => {
            r#"# Schalentier snippet: fzf
# Fuzzy finder key bindings (Ctrl+R for history, Ctrl+T for files)

if command -v fzf >/dev/null 2>&1; then
    eval "$(fzf --bash)"
fi
"#
        }
        "direnv" => {
            r#"# Schalentier snippet: direnv
# Automatic environment loading from .envrc files

if command -v direnv >/dev/null 2>&1; then
    eval "$(direnv hook bash)"
fi
"#
        }
        "starship" => {
            r#"# Schalentier snippet: starship
# Cross-shell prompt customization

if command -v starship >/dev/null 2>&1; then
    eval "$(starship init bash)"
fi
"#
        }
        "atuin" => {
            r#"# Schalentier snippet: atuin
# Magical shell history with sync

if command -v atuin >/dev/null 2>&1; then
    eval "$(atuin init bash)"
fi
"#
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown snippet '{}'. Available: yazi, zoxide, fzf, direnv, starship, atuin",
                name
            ));
        }
    };

    Ok(snippet.to_string())
}

/// Generate shell completions
fn cmd_completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "schalentier", &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_newer_basic() {
        assert!(version_is_newer("2.0.0", "1.0.0"));
        assert!(version_is_newer("1.1.0", "1.0.0"));
        assert!(version_is_newer("1.0.1", "1.0.0"));
        assert!(!version_is_newer("1.0.0", "1.0.0"));
        assert!(!version_is_newer("1.0.0", "2.0.0"));
    }

    #[test]
    fn test_version_is_newer_with_prefix() {
        assert!(version_is_newer("v2.0.0", "v1.0.0"));
        assert!(version_is_newer("v2.0.0", "1.0.0"));
        assert!(version_is_newer("2.0.0", "v1.0.0"));
        assert!(!version_is_newer("v1.0.0", "v1.0.0"));
    }

    #[test]
    fn test_version_is_newer_different_lengths() {
        assert!(version_is_newer("1.0.1", "1.0"));
        assert!(version_is_newer("1.1", "1.0.0"));
        assert!(!version_is_newer("1.0", "1.0.1"));
    }

    #[test]
    fn test_version_is_newer_with_prerelease() {
        // 1.0.0-beta should be treated as 1.0.0 for basic comparison
        assert!(version_is_newer("1.0.1", "1.0.0-beta"));
        assert!(version_is_newer("1.0.0", "0.9.9-rc1"));
    }
}
