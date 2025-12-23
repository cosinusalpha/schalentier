use anyhow::Result;
use schalentier::{
    bootstrap::{get_arch, get_os, Bootstrap},
    config::{InstalledTool, ToolEntry, ToolStatus},
    error::{self, print_info, print_success, print_warning},
    provider::create_default_registry,
    shell::{shell_init_snippet, write_env_scripts, ShellType},
    state::default_data_dir,
    Cli, Commands, LocalState, Provider, SchalentierConfig,
};
use tracing::{debug, info};

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
        Commands::Init { force } => {
            cmd_init(force).await?;
        }
        Commands::Add {
            name,
            provider,
            no_install,
        } => {
            cmd_add(&name, provider.as_deref(), no_install).await?;
        }
        Commands::Sync { remote, push, pull } => {
            cmd_sync(remote.as_deref(), push, pull).await?;
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
        Commands::Search { query, limit } => {
            cmd_search(&query, limit).await?;
        }
    }

    Ok(())
}

/// Initialize schalentier
async fn cmd_init(force: bool) -> Result<()> {
    let mut state = LocalState::load()?;

    if state.initialized && !force {
        print_warning("Already initialized. Use --force to re-initialize.");
        return Ok(());
    }

    info!("Initializing schalentier...");

    // Run bootstrap
    let bootstrap = Bootstrap::new()?;
    bootstrap.run(&mut state).await?;

    // Write environment scripts
    let data_dir = default_data_dir()?;
    write_env_scripts(&data_dir)?;

    // Save state
    state.save()?;

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

/// Add a package to the configuration
async fn cmd_add(name: &str, provider: Option<&str>, no_install: bool) -> Result<()> {
    let mut config = SchalentierConfig::load()?;
    let mut state = LocalState::load()?;

    // Check if already in config
    if config.tools.contains_key(name) {
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
        _ => Provider::Binary, // Default
    });

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

    // Try to install
    let arch = get_arch()?;
    let os = get_os()?;
    let registry = create_default_registry(arch, os);

    info!("Searching for '{}'...", name);

    // If provider specified, use that; otherwise search all
    let results = registry.search_all(name, 5).await;

    if results.is_empty() {
        print_warning(&format!("No packages found for '{}'. Added to config but not installed.", name));
        config.save()?;
        return Ok(());
    }

    // For now, use the first result
    let result = &results[0];
    info!("Found: {} v{:?} from {:?}", result.name, result.version, result.provider);

    // Get the provider and install
    if let Some(installer) = registry.get(result.provider.clone()) {
        let install_result = installer.install(name, None).await?;

        if install_result.success {
            // Update state
            state.tools.insert(
                name.to_string(),
                InstalledTool {
                    provider: result.provider.clone(),
                    version: install_result.version.clone(),
                    path: install_result.path.clone(),
                    status: ToolStatus::Installed,
                    managed: true,
                    installed_at: Some(chrono_lite_now()),
                    last_checked: None,
                },
            );
            state.save()?;

            print_success(&format!(
                "Installed '{}' v{} via {}",
                name,
                install_result.version.as_deref().unwrap_or("unknown"),
                result.provider
            ));
        } else {
            print_warning(&format!(
                "Installation failed: {}",
                install_result.message.as_deref().unwrap_or("unknown error")
            ));
        }
    }

    config.save()?;
    Ok(())
}

/// Sync configuration with remote
async fn cmd_sync(remote: Option<&str>, push: bool, pull: bool) -> Result<()> {
    let config = SchalentierConfig::load()?;

    let remote_url = remote
        .map(String::from)
        .or(config.sync.remote.clone())
        .ok_or_else(|| anyhow::anyhow!("No remote URL specified. Use --remote or configure in schalentier.toml"))?;

    debug!("Sync with remote: {}", remote_url);

    if push {
        print_info("Push not yet implemented");
    } else if pull {
        print_info("Pull not yet implemented");
    } else {
        print_info("Bidirectional sync not yet implemented");
    }

    print_warning(&format!("Sync with {} not yet fully implemented", remote_url));
    Ok(())
}

/// Update installed packages
async fn cmd_update(name: Option<&str>, dry_run: bool) -> Result<()> {
    let state = LocalState::load()?;

    if dry_run {
        print_info("Checking for updates (dry run)...");
    } else {
        print_info("Updating packages...");
    }

    if let Some(tool_name) = name {
        if !state.tools.contains_key(tool_name) {
            print_warning(&format!("'{}' is not installed", tool_name));
            return Ok(());
        }
        print_info(&format!("Update for '{}' not yet implemented", tool_name));
    } else {
        print_info(&format!("Found {} installed tools", state.tools.len()));
        for (name, tool) in &state.tools {
            println!(
                "  {} v{} ({})",
                name,
                tool.version.as_deref().unwrap_or("?"),
                tool.provider
            );
        }
        print_warning("Update functionality not yet fully implemented");
    }

    Ok(())
}

/// Run diagnostics
async fn cmd_doctor(fix: bool) -> Result<()> {
    print_info("Running diagnostics...\n");

    let data_dir = default_data_dir()?;
    let state = LocalState::load()?;
    let config = SchalentierConfig::load()?;

    // Check data directory
    print!("Data directory: ");
    if data_dir.exists() {
        println!("OK ({})", data_dir.display());
    } else {
        println!("MISSING");
        if fix {
            std::fs::create_dir_all(&data_dir)?;
            println!("  -> Created");
        }
    }

    // Check initialization
    print!("Initialized: ");
    if state.initialized {
        println!("OK");
    } else {
        println!("NO - run 'schalentier init'");
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
        if fix {
            write_env_scripts(&data_dir)?;
            println!("  -> Generated");
        }
    }

    // Summary
    println!("\nConfiguration: {} tools defined", config.tools.len());
    println!("State: {} tools installed", state.tools.len());

    // Check for orphaned tools (in state but not in config)
    let orphaned: Vec<_> = state
        .tools
        .keys()
        .filter(|k| !config.tools.contains_key(*k))
        .collect();

    if !orphaned.is_empty() {
        print_warning(&format!(
            "{} orphaned tools (installed but not in config): {:?}",
            orphaned.len(),
            orphaned
        ));
    }

    // Check for missing tools (in config but not in state)
    let missing: Vec<_> = config
        .tools
        .keys()
        .filter(|k| !state.tools.contains_key(*k))
        .collect();

    if !missing.is_empty() {
        print_warning(&format!(
            "{} missing tools (in config but not installed): {:?}",
            missing.len(),
            missing
        ));
    }

    if orphaned.is_empty() && missing.is_empty() && state.initialized {
        print_success("\nAll checks passed!");
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

    // Remove from config
    config.tools.remove(name);
    config.save()?;

    if keep_installed {
        print_success(&format!("Removed '{}' from configuration (kept installed)", name));
        return Ok(());
    }

    // Uninstall if we have it in state
    if let Some(tool) = state.tools.remove(name) {
        let arch = get_arch()?;
        let os = get_os()?;
        let registry = create_default_registry(arch, os);

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
        _ => Provider::Binary,
    });

    println!("Managed tools:\n");

    for (name, entry) in &config.tools {
        // Apply filter
        if let Some(ref f) = filter {
            if entry.provider.as_ref() != Some(f) {
                continue;
            }
        }

        let installed = state.tools.get(name);

        if detailed {
            println!("{}:", name);
            println!(
                "  Provider: {}",
                entry
                    .provider
                    .as_ref()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "auto".to_string())
            );
            println!(
                "  Config version: {}",
                entry.version.as_deref().unwrap_or("any")
            );

            if let Some(tool) = installed {
                println!("  Status: {:?}", tool.status);
                println!(
                    "  Installed version: {}",
                    tool.version.as_deref().unwrap_or("unknown")
                );
                if let Some(ref path) = tool.path {
                    println!("  Path: {}", path.display());
                }
                println!("  Managed: {}", tool.managed);
            } else {
                println!("  Status: Not installed");
            }
            println!();
        } else {
            let status = installed
                .map(|t| format!("{:?}", t.status))
                .unwrap_or_else(|| "pending".to_string());
            let version = installed
                .and_then(|t| t.version.as_ref())
                .map(|v| format!("v{}", v))
                .unwrap_or_default();

            let provider_str = entry
                .provider
                .as_ref()
                .map(|p| format!("[{}]", p))
                .unwrap_or_default();

            println!("  {} {} {} {}", name, version, provider_str, status);
        }
    }

    Ok(())
}

/// Search for packages
async fn cmd_search(query: &str, limit: usize) -> Result<()> {
    let arch = get_arch()?;
    let os = get_os()?;
    let registry = create_default_registry(arch, os);

    print_info(&format!("Searching for '{}'...\n", query));

    let results = registry.search_all(query, limit).await;

    if results.is_empty() {
        println!("No results found for '{}'", query);
        return Ok(());
    }

    println!("Found {} results:\n", results.len());

    for result in results {
        println!(
            "  {} v{} [{}]",
            result.name,
            result.version.as_deref().unwrap_or("?"),
            result.provider
        );
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
