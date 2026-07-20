use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::{Confirm, MultiSelect};
use schalentier::{
    bootstrap::{get_arch, get_os, Bootstrap},
    cli::{ConfigAction, SecretAction, SnippetAction},
    config::{InstalledTool, ToolEntry, ToolStatus},
    detection::ToolDetector,
    dotfiles::{ApplyAction, DotfileManager},
    error::{self, print_info, print_success, print_warning},
    gist,
    provider::create_default_registry,
    secrets,
    shell::{ensure_sourced, is_sourced, rc_file_path, shell_init_snippet, write_env_scripts, ShellType},
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
            setup_shell,
        } => {
            cmd_init(force, yes, skip_bootstrap, setup_shell).await?;
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
            public,
            secret,
        } => {
            cmd_sync(remote.as_deref(), push, pull, prune, dry_run, public, secret).await?;
        }
        Commands::Update { name, dry_run, force } => {
            cmd_update(name.as_deref(), dry_run, force).await?;
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
        Commands::List { detailed, provider, security } => {
            cmd_list(detailed, provider.as_deref(), security).await?;
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
        Commands::Secret { action } => {
            cmd_secret(action)?;
        }
        Commands::Registry { action } => {
            cmd_registry(action)?;
        }
        Commands::Audit { package, refresh } => {
            cmd_audit(package, refresh).await?;
        }
    }

    Ok(())
}

/// Initialize schalentier
async fn cmd_init(force: bool, yes: bool, skip_bootstrap: bool, setup_shell: bool) -> Result<()> {
    let mut state = LocalState::load()?;

    if state.initialized && !force {
        print_warning("Already initialized. Use --force to re-initialize.");
        return Ok(());
    }

    // Determine what to install
    let (install_uv, install_conda, install_rust, install_node, install_go) = if skip_bootstrap {
        (false, false, false, false, false) // Skip all bootstrapping
    } else if yes {
        (true, true, true, true, true) // Install everything by default
    } else {
        prompt_init_options()?
    };

    info!("Initializing schalentier...");

    // Run bootstrap with user preferences (skipped if all are false)
    if install_uv || install_conda || install_rust || install_node || install_go {
        let mut bootstrap = Bootstrap::new()?;
        bootstrap.set_install_uv(install_uv);
        bootstrap.set_install_conda(install_conda);
        bootstrap.set_install_rust(install_rust);
        bootstrap.set_install_node(install_node);
        bootstrap.set_install_go(install_go);
        bootstrap.run(&mut state).await?;
    } else {
        // Just mark as initialized without bootstrap
        state.initialized = true;
        print_info("Skipping bootstrap (no tools will be installed)");
    }

    // Save state (also ensures data_dir exists)
    state.save()?;

    // Write environment scripts
    let data_dir = default_data_dir()?;
    write_env_scripts(&data_dir, &state.bootstrap)?;

    // Create default config if it doesn't exist
    let config = SchalentierConfig::load()?;
    if config.tools.is_empty() {
        config.save()?;
        print_info("Created default configuration file");
    }

    print_success("Initialization complete!");

    // Offer to wire schalentier's env file into the user's actual shell config.
    if let Some(shell) = ShellType::detect() {
        setup_shell_integration(shell, &data_dir, yes, setup_shell)?;
    }

    Ok(())
}

/// Offer (or, with `setup_shell`, directly apply) sourcing schalentier's generated env
/// file from the user's shell rc file. Falls back to printing copy-paste instructions
/// when declined, non-interactive without `--setup-shell`, or already set up.
fn setup_shell_integration(
    shell: ShellType,
    data_dir: &std::path::Path,
    yes: bool,
    setup_shell: bool,
) -> Result<()> {
    let default_rc = rc_file_path(shell);

    // Already wired up (e.g. a repeat `init --force`) — nothing to do or ask.
    if let Some(ref rc) = default_rc {
        if is_sourced(rc, data_dir, shell) {
            return Ok(());
        }
    }

    if setup_shell {
        if let Some(rc) = default_rc {
            ensure_sourced(&rc, data_dir, shell)?;
            print_success(&format!("Added schalentier setup to {}", rc.display()));
        } else {
            print_warning("Could not determine home directory; printing instructions instead:");
            println!("\n{}", shell_init_snippet(shell, data_dir));
        }
        return Ok(());
    }

    if yes {
        // Non-interactive without --setup-shell: keep today's print-only behavior.
        println!("\nTo complete setup, add the following to your shell config:\n");
        println!("{}", shell_init_snippet(shell, data_dir));
        return Ok(());
    }

    let proceed = match inquire::Confirm::new(
        "Add schalentier's environment setup to your shell config now?",
    )
    .with_default(true)
    .prompt()
    {
        Ok(answer) => answer,
        Err(inquire::InquireError::OperationCanceled) => {
            println!("\nSkipped. To complete setup manually, add the following to your shell config:\n");
            println!("{}", shell_init_snippet(shell, data_dir));
            return Ok(());
        }
        Err(e) => return Err(anyhow::anyhow!("Prompt failed: {e}")),
    };

    if !proceed {
        println!("\nTo complete setup later, add the following to your shell config:\n");
        println!("{}", shell_init_snippet(shell, data_dir));
        return Ok(());
    }

    let default_rc_str = default_rc
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    let chosen = inquire::Text::new("Shell config file to update:")
        .with_default(&default_rc_str)
        .prompt()
        .map_err(|e| anyhow::anyhow!("Prompt failed: {e}"))?;

    let rc_path = std::path::PathBuf::from(chosen);
    ensure_sourced(&rc_path, data_dir, shell)?;
    print_success(&format!("Added schalentier setup to {}", rc_path.display()));

    Ok(())
}

/// Prompt user for init options interactively
fn prompt_init_options() -> Result<(bool, bool, bool, bool, bool)> {
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
        println!("Bootstrapping additional tools is optional.");
        println!();
    }

    // Prepare default selections based on what's already installed
    let mut defaults = vec![];
    let mut default_uv = true;
    let mut default_conda = true;
    let mut default_rust = true;
    let mut default_node = true;
    let mut default_go = true;

    if detection.uv.available {
        println!("ℹ uv is already installed. Skipping by default.");
        default_uv = false;
    }

    if detection.conda.available {
        println!("ℹ conda/mamba is already installed. Skipping by default.");
        default_conda = false;
    }

    if detection.rust.available {
        println!("ℹ Rust is already installed. Skipping by default.");
        default_rust = false;
    }

    if detection.node.available {
        println!("ℹ Node.js is already installed. Skipping by default.");
        default_node = false;
    }

    if detection.go.available {
        println!("ℹ Go is already installed. Skipping by default.");
        default_go = false;
    }

    if default_uv {
        defaults.push(0);
    }
    if default_conda {
        defaults.push(1);
    }
    if default_rust {
        defaults.push(2);
    }
    if default_node {
        defaults.push(3);
    }
    if default_go {
        defaults.push(4);
    }

    println!();

    // Ask about bootstrap components
    let components = [
        ("uv", "uv - Fast Python package installer (recommended for Python CLI tools)"),
        ("conda", "Miniforge/Conda - Scientific packages and isolated environments"),
        ("rust", "Rust (rustup) - Rust toolchain and cargo package manager"),
        ("node", "Node.js - JavaScript runtime and npm package manager"),
        ("go", "Go - Go toolchain for building and installing Go CLI tools"),
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

    let install_uv = selected.iter().any(|s| s.contains("uv -"));
    let install_conda = selected.iter().any(|s| s.contains("Miniforge"));
    let install_rust = selected.iter().any(|s| s.contains("Rust (rustup)"));
    let install_node = selected.iter().any(|s| s.contains("Node.js"));
    let install_go = selected.iter().any(|s| s.contains("Go -"));

    // Confirm before proceeding
    println!();
    let proceed = Confirm::new("Proceed with installation?")
        .with_default(true)
        .prompt();

    match proceed {
        Ok(true) => Ok((install_uv, install_conda, install_rust, install_node, install_go)),
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

    // ==========================================
    // STEP 1: Resolve ALL providers
    // ==========================================

    let pkg_registry = schalentier::registry::PackageRegistry::load()?;
    
    let resolution = match pkg_registry.resolve_all_providers(name) {
        Ok(r) => r,
        Err(_) => {
            // Not in the curated registry. Query the live providers to help the user:
            // confirm an exact match, or surface close matches for a likely typo.
            print_info(&format!("'{}' not in registry, searching providers...", name));
            search_providers_for(name).await;
            return cmd_add_legacy(name, provider, no_install, dry_run, config, state).await;
        }
    };

    // Display resolution
    println!();
    println!("Package: {}", resolution.canonical_name);
    println!("Description: {}", resolution.description);
    println!();

    // Show available providers
    let mut providers: Vec<_> = resolution.available_providers.iter().collect();
    providers.sort_by(|a, b| a.0.cmp(b.0));

    if !providers.is_empty() {
        println!("Available in {} provider(s):", providers.len());
        for (provider_name, provider_info) in &providers {
            let pkg_name = &provider_info.package_name;
            let note = if pkg_name != &resolution.canonical_name {
                format!(" (as {})", pkg_name)
            } else {
                String::new()
            };
            println!("  ✓ {}{}", provider_name, note);
        }
    } else {
        print_warning("No providers available for this package");
        return Ok(());
    }

    // Show unavailable providers
    if !dry_run && !resolution.unavailable_providers.is_empty() {
        println!();
        println!("Not available in {} provider(s):", resolution.unavailable_providers.len());
        let mut unavail: Vec<_> = resolution.unavailable_providers.iter().collect();
        unavail.sort_by(|a, b| a.0.cmp(b.0));
        for (provider_name, reason) in &unavail {
            println!("  ✗ {}: {}", provider_name, reason);
        }
    }

    // ==========================================
    // STEP 2: Security Audit (if installing)
    // ==========================================

    if !dry_run
        && !no_install
        && !perform_security_audit(&resolution, config.settings.audit_cache_ttl_hours).await?
    {
        // User declined to install a package with known vulnerabilities.
        println!("Installation cancelled.");
        return Ok(());
    }

    // ==========================================
    // STEP 3: Select Provider
    // ==========================================

    let selected_provider = if let Some(p) = provider {
        // User specified provider
        if !resolution.available_providers.contains_key(p) {
            return Err(anyhow::anyhow!(
                "Package '{}' is not available via {}. Available: {}",
                name,
                p,
                resolution.available_providers.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        p.to_string()
    } else {
        // Auto-select based on priority
        let priority = &config.settings.provider_priority;
        select_best_provider(&resolution, priority)?
    };

    println!();
    if dry_run {
        println!("Dry run - would install via {}", selected_provider);
        return Ok(());
    }

    println!("Selected provider: {}", selected_provider);

    // ==========================================
    // STEP 4: Install
    // ==========================================

    if no_install {
        config.tools.insert(
            resolution.canonical_name.clone(),
            ToolEntry {
                provider: Some(str_to_provider(&selected_provider)),
                version: None,
                options: std::collections::HashMap::new(),
            },
        );
        config.save()?;
        print_success(&format!(
            "Added '{}' to configuration (not installed)",
            resolution.canonical_name
        ));
        return Ok(());
    }

    // Install using selected provider
    let provider_info = resolution.available_providers.get(&selected_provider).unwrap();
    
    // Use legacy install logic
    cmd_install_with_provider(
        &resolution.canonical_name,
        &provider_info.package_name,
        &selected_provider,
        &mut state,
    ).await?;

    // Update config
    config.tools.insert(
        resolution.canonical_name.clone(),
        ToolEntry {
            provider: Some(str_to_provider(&selected_provider)),
            version: None,
            options: std::collections::HashMap::new(),
        },
    );
    config.save()?;

    Ok(())
}

/// Search live providers for a package name and print what was found.
///
/// Best-effort discovery for packages not in the curated registry: confirms an exact
/// name match (across providers) or lists close matches so the user can spot a typo.
/// Never fails the install — a search error or empty result is just reported.
async fn search_providers_for(name: &str) {
    let (arch, os, data_dir) = match (get_arch(), get_os(), default_data_dir()) {
        (Ok(a), Ok(o), Ok(d)) => (a, o, d),
        _ => return,
    };
    let registry = create_default_registry(arch, os, data_dir);

    let spinner = create_spinner(&format!("Searching providers for '{}'...", name));
    let results = registry.search_all_clustered(name, 10).await;
    spinner.finish_and_clear();

    if results.is_empty() {
        print_info(&format!(
            "No provider matches found for '{}'; will attempt install by name.",
            name
        ));
        return;
    }

    if let Some(exact) = results.iter().find(|r| r.name.eq_ignore_ascii_case(name)) {
        let providers: Vec<String> = exact.providers.iter().map(|p| p.provider.to_string()).collect();
        print_info(&format!(
            "Found '{}' in: {}",
            exact.name,
            providers.join(", ")
        ));
    } else {
        println!("Did you mean one of these?");
        for r in results.iter().take(5) {
            let providers: Vec<String> = r.providers.iter().map(|p| p.provider.to_string()).collect();
            println!("  {} ({})", r.name, providers.join(", "));
        }
    }
}

/// Legacy add command for packages not in registry
async fn cmd_add_legacy(
    name: &str,
    provider: Option<&str>,
    no_install: bool,
    dry_run: bool,
    mut config: SchalentierConfig,
    mut state: LocalState,
) -> Result<()> {
    // Parse provider if specified
    let provider_enum = provider.map(|p| match p.to_lowercase().as_str() {
        "system" => Provider::System,
        "conda" => Provider::Conda,
        "cargo" => Provider::Cargo,
        "binary" => Provider::Binary,
        "uv" => Provider::Uv,
        "brew" => Provider::Brew,
        _ => Provider::Binary,
    });

    if dry_run {
        match provider_enum {
            Some(p) => println!("Dry run - would install '{}' via {}", name, p),
            None => println!(
                "Dry run - would install '{}' via provider priority (not in registry)",
                name
            ),
        }
        return Ok(());
    }

    // Add to config
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
        print_success(&format!("Added '{}' to configuration (not installed)", name));
        return Ok(());
    }

    // Install using provider registry
    let arch = get_arch()?;
    let os = get_os()?;
    let data_dir = default_data_dir()?;
    let registry = create_default_registry(arch, os, data_dir);

    info!("Installing '{}'...", name);

    match registry
        .install_with_fallback(name, None, provider_enum.clone())
        .await
    {
        Ok((install_result, actual_provider)) => {
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

/// Install a package with a specific provider
async fn cmd_install_with_provider(
    canonical_name: &str,
    package_name: &str,
    provider_name: &str,
    state: &mut LocalState,
) -> Result<()> {
    let arch = get_arch()?;
    let os = get_os()?;
    let data_dir = default_data_dir()?;
    let registry = create_default_registry(arch, os, data_dir);

    println!("Installing '{}' via {}...", package_name, provider_name);

    let provider_enum = str_to_provider(provider_name);

    match registry
        .install_with_fallback(package_name, None, Some(provider_enum.clone()))
        .await
    {
        Ok((install_result, actual_provider)) => {
            state.tools.insert(
                canonical_name.to_string(),
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
                canonical_name,
                format_version(ver),
                actual_provider
            ));
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Installation failed: {}", e));
        }
    }

    Ok(())
}

/// Convert provider string to enum
fn str_to_provider(s: &str) -> Provider {
    match s.to_lowercase().as_str() {
        "system" => Provider::System,
        "conda" => Provider::Conda,
        "cargo" => Provider::Cargo,
        "binary" => Provider::Binary,
        "uv" => Provider::Uv,
        "brew" => Provider::Brew,
        _ => Provider::Binary,
    }
}

/// Select the best provider based on priority
fn select_best_provider(
    resolution: &schalentier::registry::MultiProviderResolution,
    priority: &[Provider],
) -> Result<String> {
    for provider in priority {
        let provider_str = provider.to_string();
        if resolution.available_providers.contains_key(&provider_str) {
            return Ok(provider_str);
        }
    }
    
    // No priority match, use first available
    resolution
        .available_providers
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No providers available"))
}

const OSV_CACHE_FILE_NAME: &str = "osv_cache.json";

/// Build a [`SecurityAuditor`] with on-disk caching enabled at the given TTL.
fn cached_auditor(ttl_hours: u64) -> Result<schalentier::security::SecurityAuditor> {
    use schalentier::security::SecurityAuditor;

    let cache_path = default_data_dir()?.join(OSV_CACHE_FILE_NAME);
    Ok(SecurityAuditor::new().with_cache(cache_path, ttl_hours))
}

/// Perform a security audit on a package before install.
///
/// Returns `Ok(true)` if installation should proceed (clean, or the user chose to
/// continue despite advisories) and `Ok(false)` if the user declined. Lets the caller
/// unwind normally instead of terminating the process.
async fn perform_security_audit(
    resolution: &schalentier::registry::MultiProviderResolution,
    audit_cache_ttl_hours: u64,
) -> Result<bool> {
    use inquire::Confirm;

    let mut auditor = cached_auditor(audit_cache_ttl_hours)?;
    // No installed version at add time; report all known advisories for the package.
    let report = auditor.audit(resolution, None, false).await?;

    if report.is_clean() {
        return Ok(true);
    }

    // Show warnings
    println!();
    println!("⚠  SECURITY ADVISORY DETECTED");
    println!();

    for vuln in &report.vulnerabilities {
        println!("{}", vuln.format());
        println!();
    }

    // Ask user what to do. Critical advisories default to NO; lesser ones default to YES.
    let proceed = if report.has_critical() {
        println!("🚨 CRITICAL VULNERABILITIES FOUND!");
        println!();
        Confirm::new("This package has critical security vulnerabilities. Install anyway?")
            .with_default(false)
            .prompt()?
    } else {
        Confirm::new("Install despite security warnings?")
            .with_default(true)
            .prompt()?
    };

    Ok(proceed)
}

/// Sync configuration with remote
async fn cmd_sync(
    remote: Option<&str>,
    push: bool,
    pull: bool,
    prune: bool,
    dry_run: bool,
    public_flag: bool,
    secret_flag: bool,
) -> Result<()> {
    use std::process::Command;

    // Merge a project-local .schalentier/config.toml when running inside a project, so
    // sync installs honor project tool overrides. sync does not save config, so merging
    // the project layer on top is safe here.
    let config = SchalentierConfig::load_with_project()?;
    let state = LocalState::load()?;
    let config_dir = schalentier::state::config_dir()?;

    // Determine remote URL
    let remote_url = remote.map(String::from).or(config.sync.remote.clone());

    // Check for gist:// URL scheme
    if let Some(ref url) = remote_url {
        if let Some(gist_id) = gist::parse_gist_url(url) {
            return cmd_sync_gist(
                &gist_id,
                push,
                pull,
                prune,
                dry_run,
                public_flag,
                secret_flag,
                &config,
            )
            .await;
        }
    }

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

/// Sync configuration with GitHub Gist (encrypted)
async fn cmd_sync_gist(
    gist_id: &str,
    push: bool,
    pull: bool,
    prune: bool,
    dry_run: bool,
    public_flag: bool,
    secret_flag: bool,
    config: &SchalentierConfig,
) -> Result<()> {
    let config_dir = schalentier::state::config_dir()?;

    // Determine visibility: CLI flag > config default
    let is_public = if public_flag {
        true
    } else if secret_flag {
        false
    } else {
        config.sync.gist_public
    };

    // Dry run - show what would happen
    if dry_run {
        println!("Dry run: showing what would happen during gist sync");
        println!();
        println!("  Config directory: {}", config_dir.display());
        println!("  Gist ID: {}", gist_id);
        println!("  Visibility: {}", if is_public { "public" } else { "secret" });
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
        return Ok(());
    }

    // Initialize gist client
    let gist_client = gist::GistClient::new().await?;
    let password = secrets::get_or_create_master_password()?;

    // PULL: Download and decrypt
    if pull || (!push && !pull) {
        let spinner = create_spinner("Downloading encrypted gist...");

        let gist_id_to_fetch = if gist_id == "new" {
            print_warning("Cannot pull from 'gist://new' - use push to create a new gist");
            return Ok(());
        } else {
            gist_id
        };

        match gist_client.get_gist(gist_id_to_fetch).await {
            Ok(encrypted) => {
                spinner.finish_and_clear();
                
                let decrypted = match gist::decrypt_content(&encrypted, &password) {
                    Ok(content) => content,
                    Err(e) => {
                        print_warning(&format!("Failed to decrypt gist: {}", e));
                        print_info("Make sure you're using the same master password across machines");
                        return Err(e);
                    }
                };

                // Write decrypted config
                let config_path = config_dir.join("schalentier.toml");
                std::fs::write(&config_path, &decrypted)?;
                print_success("Downloaded and decrypted configuration");

                // Reload config and install tools
                let config_after = SchalentierConfig::load()?;
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
            Err(e) => {
                spinner.finish_and_clear();
                print_warning(&format!("Failed to fetch gist: {}", e));
            }
        }
    }

    // PUSH: Encrypt and upload
    if push {
        let config_path = config_dir.join("schalentier.toml");
        
        if !config_path.exists() {
            print_warning("No schalentier.toml found. Run 'schalentier init' first.");
            return Ok(());
        }

        let plaintext = std::fs::read_to_string(&config_path)?;
        let encrypted = gist::encrypt_content(&plaintext, &password)?;

        if gist_id == "new" {
            // Create new gist
            let spinner = create_spinner("Creating encrypted gist...");
            let new_gist_id = gist_client.create_gist(&encrypted, is_public).await?;
            spinner.finish_and_clear();

            print_success(&format!(
                "Created {} gist: gist://{}",
                if is_public { "public" } else { "secret" },
                new_gist_id
            ));
            print_info("Add this to your config:");
            println!("  [sync]");
            println!("  remote = \"gist://{}\"", new_gist_id);
        } else {
            // Update existing gist
            let spinner = create_spinner("Updating encrypted gist...");
            gist_client.update_gist(gist_id, &encrypted).await?;
            spinner.finish_and_clear();
            
            print_success(&format!("Updated gist: gist://{}", gist_id));
        }
    }

    print_success("Gist sync complete!");
    Ok(())
}

/// Returns the version a tool is pinned to in config, unless the pin is `"latest"`
/// (which means "no pin, always update").
fn pinned_version<'a>(config: &'a SchalentierConfig, tool_name: &str) -> Option<&'a str> {
    config
        .tools
        .get(tool_name)
        .and_then(|entry| entry.version.as_deref())
        .filter(|v| *v != "latest")
}

/// Update installed packages
async fn cmd_update(name: Option<&str>, dry_run: bool, force: bool) -> Result<()> {
    let mut state = LocalState::load()?;
    let config = SchalentierConfig::load_with_project()?;
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

        // Respect a version pin set in schalentier.toml unless --force overrides it.
        if !force {
            if let Some(pin) = pinned_version(&config, tool_name) {
                println!(
                    "  {} {} [{}] - pinned to {}, skipping (use --force to override)",
                    tool_name,
                    format_version(current_version),
                    tool.provider,
                    pin
                );
                up_to_date += 1;
                continue;
            }
        }

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
    // Read-only diagnostics: merge project config so doctor reflects the effective setup.
    let config = SchalentierConfig::load_with_project()?;
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
    if env_sh.exists() && env_fish.exists() {
        println!("OK");
    } else {
        println!("MISSING");
        issues_found += 1;
        if fix {
            write_env_scripts(&data_dir, &state.bootstrap)?;
            println!("  -> Generated");
            issues_fixed += 1;
        }
    }

    // Informational only: whether the shell rc file already sources the env script.
    // `doctor --fix` never writes to rc files itself (an explicit `init`-time choice).
    print!("Shell config sourcing: ");
    if let Some(shell) = ShellType::detect() {
        match rc_file_path(shell) {
            Some(rc) if is_sourced(&rc, &data_dir, shell) => {
                println!("OK ({})", rc.display());
            }
            Some(rc) => {
                println!("NOT SOURCED ({})", rc.display());
                println!("  -> Run 'schalentier init --setup-shell' or add the source line manually");
            }
            None => println!("UNKNOWN (could not determine home directory)"),
        }
    } else {
        println!("UNKNOWN (could not detect shell)");
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

    // Project-local config, if run from inside a project directory
    if let Some(project_path) = schalentier::state::find_project_config() {
        println!("\n=== Project Context ===\n");
        println!("Project config: {}", project_path.display());

        if let Ok(project_config) = SchalentierConfig::load_from(&project_path) {
            if !project_config.tools.is_empty() {
                println!("  Tools overridden:");
                for (name, entry) in &project_config.tools {
                    match &entry.version {
                        Some(v) => println!("    - {} (v{})", name, v),
                        None => println!("    - {}", name),
                    }
                }
            }
            if !project_config.dotfiles.is_empty() {
                println!("  Dotfiles overridden: {}", project_config.dotfiles.len());
            }
        }

        if let Some(project_dir) = schalentier::state::project_dir_from(&project_path) {
            let project_secrets = secrets::secrets_file_path(&project_dir);
            if project_secrets.exists() {
                println!("  Project secrets: {}", project_secrets.display());
            }
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
/// One-line cached security status for `list --security`, e.g. "✓ clean" or
/// "⚠ 2 advisories". Never hits the network — reads only what `schalentier audit`
/// has already cached, so it stays fast for everyday `list` use.
fn security_status_line(
    auditor: &schalentier::security::SecurityAuditor,
    pkg_registry: &schalentier::registry::PackageRegistry,
    name: &str,
    installed: Option<&schalentier::config::InstalledTool>,
) -> String {
    let Ok(resolution) = pkg_registry.resolve_all_providers(name) else {
        return "⊘ not in registry".to_string();
    };
    let auditable = resolution
        .available_providers
        .keys()
        .any(|p| schalentier::security::osv::OsvAuditor::ecosystem_for_provider(p).is_some());
    if !auditable {
        return "⊘ no OSV-supported ecosystem".to_string();
    }

    let version = installed.and_then(|t| t.version.as_deref());
    match auditor.peek_cache(&resolution, version) {
        Some(report) if report.is_clean() => "✓ clean".to_string(),
        Some(report) => format!(
            "⚠ {} advisor{}",
            report.total_advisories(),
            if report.total_advisories() == 1 { "y" } else { "ies" }
        ),
        None => "? not checked (run `schalentier audit`)".to_string(),
    }
}

async fn cmd_list(detailed: bool, provider_filter: Option<&str>, security: bool) -> Result<()> {
    // Read-only: merge project config so list shows project tool overrides in context.
    let config = SchalentierConfig::load_with_project()?;
    let state = LocalState::load()?;

    if config.tools.is_empty() && state.tools.is_empty() {
        println!("No tools managed. Use 'schalentier add <tool>' to add one.");
        return Ok(());
    }

    let security_auditor = if security {
        Some(cached_auditor(config.settings.audit_cache_ttl_hours)?)
    } else {
        None
    };
    let pkg_registry = if security {
        Some(schalentier::registry::PackageRegistry::load()?)
    } else {
        None
    };

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
            if let (Some(auditor), Some(pkg_registry)) = (&security_auditor, &pkg_registry) {
                println!(
                    "  Security: {}",
                    security_status_line(auditor, pkg_registry, name, installed)
                );
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

            let security_str = match (&security_auditor, &pkg_registry) {
                (Some(auditor), Some(pkg_registry)) => {
                    format!(" {}", security_status_line(auditor, pkg_registry, name, installed))
                }
                _ => String::new(),
            };

            println!(
                "  {} {} {} {} {}{}",
                status_symbol, name, version, provider_str, status_text, security_str
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

    // Make executable
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

/// Build a [`schalentier::template::TemplateContext`] only if at least one dotfile
/// entry sets `_template = true` — avoids prompting for the secrets master password
/// on every `config apply`/`diff` for users who never opted into templating.
fn build_template_context_if_needed(
    config: &SchalentierConfig,
) -> Result<Option<schalentier::template::TemplateContext>> {
    let any_templated = config.dotfiles.values().any(|v| {
        v.as_table()
            .and_then(|t| t.get("_template"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    });

    if !any_templated {
        return Ok(None);
    }

    let os = get_os()?;
    let arch = get_arch()?;

    let password = secrets::get_or_create_master_password()?;
    let global_dir = schalentier::state::config_dir()?;
    let mut store = secrets::load_store(&secrets::secrets_file_path(&global_dir), &password)?;

    // Project secrets override global secrets with the same name.
    if let Some(project_path) = schalentier::state::find_project_config() {
        if let Some(project_dir) = schalentier::state::project_dir_from(&project_path) {
            let project_store =
                secrets::load_store(&secrets::secrets_file_path(&project_dir), &password)?;
            store.secrets.extend(project_store.secrets);
        }
    }

    let secret_values = store
        .secrets
        .iter()
        .map(|(name, entry)| (name.clone(), entry.value.clone()))
        .collect();

    let ctx = schalentier::template::TemplateContext::from_system(&os.to_string(), &arch.to_string())
        .with_secrets(secret_values)
        .with_variables(config.variables.clone());

    Ok(Some(ctx))
}

/// Apply all dotfile patches
fn config_apply() -> Result<()> {
    let config = SchalentierConfig::load_with_project()?;

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

    let ctx = build_template_context_if_needed(&config)?;
    let manager = DotfileManager::from_config_with_context(&config.dotfiles, ctx.as_ref())?;
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
    let config = SchalentierConfig::load_with_project()?;

    if config.dotfiles.is_empty() {
        println!("No dotfiles configured.");
        return Ok(());
    }

    let ctx = build_template_context_if_needed(&config)?;
    let manager = DotfileManager::from_config_with_context(&config.dotfiles, ctx.as_ref())?;
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

/// Manage encrypted secrets
/// Path to the global secrets store (~/.config/schalentier/secrets.enc).
fn global_secrets_path() -> Result<std::path::PathBuf> {
    let config_dir = schalentier::state::config_dir()?;
    Ok(secrets::secrets_file_path(&config_dir))
}

/// Path to the project-local secrets store, if a project config is found by
/// walking up from the current directory.
fn project_secrets_path() -> Option<std::path::PathBuf> {
    schalentier::state::find_project_config()
        .and_then(|p| schalentier::state::project_dir_from(&p))
        .map(|dir| secrets::secrets_file_path(&dir))
}

/// The secrets store path a `set`/`delete`/`edit` should write to: project-local
/// if inside a project (and `--global` wasn't passed), otherwise global.
fn effective_secrets_path(force_global: bool) -> Result<std::path::PathBuf> {
    if !force_global {
        if let Some(path) = project_secrets_path() {
            return Ok(path);
        }
    }
    global_secrets_path()
}

/// Load and merge global + project secret stores (project entries win on name
/// conflict), for read-oriented operations (`get`, `list`, `export`, `shell`, `run`).
fn load_merged_store(
    password: &age::secrecy::SecretString,
) -> Result<schalentier::secrets::SecretStore> {
    let mut store = secrets::load_store(&global_secrets_path()?, password)?;
    if let Some(project_path) = project_secrets_path() {
        let project_store = secrets::load_store(&project_path, password)?;
        store.secrets.extend(project_store.secrets);
    }
    Ok(store)
}

fn cmd_secret(action: SecretAction) -> Result<()> {
    use age::secrecy::ExposeSecret;
    use schalentier::secrets::{SecretEntry, SecretStore};

    match action {
        SecretAction::Set {
            name,
            value,
            tags,
            global,
        } => {
            let secrets_path = effective_secrets_path(global)?;
            let password = secrets::get_or_create_master_password()?;
            let mut store = secrets::load_store(&secrets_path, &password)?;

            let value = match value {
                Some(v) => v,
                None => secrets::prompt_password(&format!("Value for '{}':", name))?
                    .expose_secret()
                    .to_string(),
            };

            store
                .secrets
                .insert(name.clone(), SecretEntry { value, tags });
            secrets::save_store(&secrets_path, &store, &password)?;

            print_success(&format!("Secret '{}' saved", name));
        }

        SecretAction::Get { name } => {
            let password = secrets::get_or_create_master_password()?;
            let store = load_merged_store(&password)?;

            let entry = store.secrets.get(&name).ok_or_else(|| {
                anyhow::anyhow!(schalentier::error::SchalentierError::SecretNotFound {
                    name: name.clone()
                })
            })?;

            print!("{}", entry.value);
        }

        SecretAction::List { tags } => {
            let password = secrets::get_or_create_master_password()?;

            match project_secrets_path() {
                Some(project_path) => {
                    let project_store = secrets::load_store(&project_path, &password)?;
                    let global_store = secrets::load_store(&global_secrets_path()?, &password)?;

                    println!("Project secrets ({}):", project_path.display());
                    print_secret_list(&project_store, tags.as_deref());
                    println!();
                    println!("Global secrets ({}):", global_secrets_path()?.display());
                    print_secret_list(&global_store, tags.as_deref());
                }
                None => {
                    let store = secrets::load_store(&global_secrets_path()?, &password)?;
                    print_secret_list(&store, tags.as_deref());
                }
            }
        }

        SecretAction::Delete { name, global } => {
            let secrets_path = effective_secrets_path(global)?;
            let password = secrets::get_or_create_master_password()?;
            let mut store = secrets::load_store(&secrets_path, &password)?;

            if store.secrets.remove(&name).is_none() {
                return Err(anyhow::anyhow!(
                    schalentier::error::SchalentierError::SecretNotFound { name }
                ));
            }

            secrets::save_store(&secrets_path, &store, &password)?;
            print_success(&format!("Secret '{}' deleted", name));
        }

        SecretAction::Export { shell, tags } => {
            let password = secrets::get_or_create_master_password()?;
            let store = load_merged_store(&password)?;

            let env = secrets::resolve_scoped_env(&store, tags.as_deref());
            for (name, value) in env {
                println!("{}", secrets::shell_export_line(&shell, &name, &value));
            }
        }

        SecretAction::Edit => {
            let secrets_path = effective_secrets_path(false)?;
            let password = secrets::get_or_create_master_password()?;
            let store = secrets::load_store(&secrets_path, &password)?;

            let plaintext = serde_json::to_string_pretty(&store)?;

            let mut tmp = std::env::temp_dir();
            tmp.push(format!("schalentier-secrets-{}.json", std::process::id()));
            std::fs::write(&tmp, &plaintext)?;

            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let status = std::process::Command::new(&editor).arg(&tmp).status();

            let edited = std::fs::read_to_string(&tmp);
            let _ = std::fs::remove_file(&tmp);

            let status = status?;
            if !status.success() {
                return Err(anyhow::anyhow!("Editor exited with an error"));
            }

            let new_store: SecretStore = serde_json::from_str(&edited?)
                .map_err(|e| anyhow::anyhow!("Invalid secrets JSON: {e}"))?;
            secrets::save_store(&secrets_path, &new_store, &password)?;
            print_success("Secrets updated");
        }

        SecretAction::ChangePassword => {
            let old_password = secrets::get_or_create_master_password()?;
            let store = load_merged_store(&old_password)?;

            let new_password = secrets::prompt_password("New master password:")?;
            secrets::save_store(&global_secrets_path()?, &store, &new_password)?;
            secrets::set_master_password(new_password.expose_secret())?;

            print_success("Master password changed");
        }

        SecretAction::Shell { tags } => {
            let password = secrets::get_or_create_master_password()?;
            let store = load_merged_store(&password)?;
            let env = secrets::resolve_scoped_env(&store, tags.as_deref());

            let shell_type = ShellType::detect().unwrap_or(ShellType::Bash);
            let shell_bin = match shell_type {
                ShellType::Bash => "bash",
                ShellType::Zsh => "zsh",
                ShellType::Fish => "fish",
            };

            print_info(&format!(
                "Spawning {} with {} secret(s){}...",
                shell_bin,
                env.len(),
                tags.as_ref()
                    .map(|t| format!(" [{}]", t.join(", ")))
                    .unwrap_or_default()
            ));

            let status = std::process::Command::new(shell_bin).envs(env).status()?;
            std::process::exit(status.code().unwrap_or(1));
        }

        SecretAction::Run { tags, command } => {
            let password = secrets::get_or_create_master_password()?;
            let store = load_merged_store(&password)?;
            let env = secrets::resolve_scoped_env(&store, tags.as_deref());

            let (program, args) = command
                .split_first()
                .ok_or_else(|| anyhow::anyhow!("No command given"))?;

            let status = std::process::Command::new(program)
                .args(args)
                .envs(env)
                .status()?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }

    Ok(())
}

/// Print a sorted `NAME [tags]` list for a secret store, or a "none" message.
fn print_secret_list(store: &schalentier::secrets::SecretStore, tags: Option<&[String]>) {
    let mut matches = store.filter_by_tags(tags);
    matches.sort_by_key(|(name, _)| name.to_string());

    if matches.is_empty() {
        println!("  (none)");
    } else {
        for (name, entry) in matches {
            if entry.tags.is_empty() {
                println!("  {}", name);
            } else {
                println!("  {} [{}]", name, entry.tags.join(", "));
            }
        }
    }
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

    #[test]
    fn test_pinned_version_returns_pin() {
        let mut config = SchalentierConfig::default();
        config.tools.insert(
            "bat".to_string(),
            ToolEntry {
                provider: None,
                version: Some("0.24.0".to_string()),
                options: Default::default(),
            },
        );
        assert_eq!(pinned_version(&config, "bat"), Some("0.24.0"));
    }

    #[test]
    fn test_pinned_version_latest_means_unpinned() {
        let mut config = SchalentierConfig::default();
        config.tools.insert(
            "bat".to_string(),
            ToolEntry {
                provider: None,
                version: Some("latest".to_string()),
                options: Default::default(),
            },
        );
        assert_eq!(pinned_version(&config, "bat"), None);
    }

    #[test]
    fn test_pinned_version_no_entry_is_unpinned() {
        let config = SchalentierConfig::default();
        assert_eq!(pinned_version(&config, "bat"), None);
    }
}

fn cmd_registry(action: schalentier::cli::RegistryAction) -> anyhow::Result<()> {
    use schalentier::cli::RegistryAction;

    match action {
        RegistryAction::Validate => {
            println!("Validating registry...\n");

            let registry = schalentier::registry::PackageRegistry::load()?;
            let errors = registry.validate();

            if errors.is_empty() {
                print_success("Registry is valid");
                println!("\nPackage count: {}", registry.package_count());
            } else {
                println!("Found {} error(s):\n", errors.len());
                for error in errors {
                    println!("  ✗ {}", error);
                }
                std::process::exit(1);
            }
        }
        RegistryAction::Info => {
            let registry = schalentier::registry::PackageRegistry::load()?;
            let stats = registry.stats();

            println!("Registry Statistics\n");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Total packages:  {}", stats.total_packages);
            println!("Total aliases:   {}", stats.total_aliases);
            println!();
            println!("Packages by provider:");

            let mut providers: Vec<_> = stats.provider_counts.iter().collect();
            providers.sort_by(|a, b| b.1.cmp(a.1));

            for (provider, count) in providers {
                let percentage = (*count as f64 / stats.total_packages as f64) * 100.0;
                println!(
                    "  {:12} {:4} packages ({:.1}%)",
                    provider, count, percentage
                );
            }
        }
        RegistryAction::Update => {
            cmd_registry_update()?;
        }
    }

    Ok(())
}

fn cmd_registry_update() -> anyhow::Result<()> {
    println!("Downloading latest registry from GitHub...");

    let url = "https://raw.githubusercontent.com/cosinusalpha/schalentier/main/registry/packages.json";

    let rt = tokio::runtime::Runtime::new()?;
    let content = rt.block_on(async {
        let client = reqwest::Client::new();
        let response = client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to download registry: HTTP {}",
                response.status()
            ));
        }

        let content = response.text().await?;

        let _: schalentier::registry::Registry = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Downloaded registry is invalid: {}", e))?;

        Ok::<_, anyhow::Error>(content)
    })?;

    let path = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("No data directory"))?
        .join("schalentier/registry/packages.json");

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, content)?;

    print_success(&format!("Registry updated to {}", path.display()));
    Ok(())
}

async fn cmd_audit(package: Option<String>, refresh: bool) -> anyhow::Result<()> {
    use schalentier::registry::PackageRegistry;

    let state = LocalState::load()?;
    let config = SchalentierConfig::load_with_project()?;
    let pkg_registry = PackageRegistry::load()?;
    let mut auditor = cached_auditor(config.settings.audit_cache_ttl_hours)?;

    println!("Running security audit (via OSV.dev)...\n");

    let packages_to_check = if let Some(pkg) = package {
        vec![pkg]
    } else {
        state.tools.keys().cloned().collect()
    };

    if packages_to_check.is_empty() {
        println!("No packages to audit (none installed)");
        return Ok(());
    }

    use schalentier::security::osv::OsvAuditor;

    let mut total_advisories = 0;
    let mut vuln_packages = 0;
    let mut has_critical = false;

    for pkg_name in &packages_to_check {
        let installed = state.tools.get(pkg_name);
        print!(
            "  {} ({})...",
            pkg_name,
            installed.map(|t| t.provider.to_string()).unwrap_or_else(|| "?".to_string())
        );

        match pkg_registry.resolve_all_providers(pkg_name) {
            Ok(resolution) => {
                // OSV can only audit packages from ecosystems it covers (cargo/uv/npm/go).
                // Binary/brew/system tools have no queryable ecosystem — say so plainly
                // rather than printing a reassuring "clean".
                let auditable = resolution
                    .available_providers
                    .keys()
                    .any(|p| OsvAuditor::ecosystem_for_provider(p).is_some());
                if !auditable {
                    println!(" ⊘ skipped (no OSV-supported ecosystem)");
                    continue;
                }

                // Narrow to the installed version when known, else check the whole package.
                let version = installed.and_then(|t| t.version.as_deref());
                match auditor.audit(&resolution, version, refresh).await {
                    Ok(report) => {
                        if report.is_clean() {
                            if report.errors.is_empty() {
                                println!(" ✓ clean");
                            } else {
                                println!(" ⚠ no data ({})", report.errors.join("; "));
                            }
                        } else {
                            println!();
                            for vuln in &report.vulnerabilities {
                                println!();
                                println!("{}", vuln.format());
                            }
                            total_advisories += report.total_advisories();
                            vuln_packages += 1;
                            has_critical = has_critical || report.has_critical();
                        }
                    }
                    Err(e) => {
                        println!(" ✗ audit failed: {}", e);
                    }
                }
            }
            Err(_) => {
                println!(" ⊘ skipped (not in registry)");
            }
        }
    }

    println!();

    if total_advisories == 0 {
        print_success("All packages passed security audit");
    } else {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        let kind = if has_critical { "high/critical" } else { "known" };
        println!(
            "⚠️  {} advisor{} ({} severity) across {} package(s)",
            total_advisories,
            if total_advisories == 1 { "y" } else { "ies" },
            kind,
            vuln_packages,
        );
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!();
        println!("Update packages with: schalentier update");
    }

    Ok(())
}