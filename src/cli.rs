use clap::{Parser, Subcommand};
use clap_complete::Shell;

/// Schalentier - A cross-platform package and tool manager with configuration sync
#[derive(Parser, Debug)]
#[command(name = "schalentier")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize schalentier in the current environment
    Init {
        /// Force re-initialization even if already initialized
        #[arg(short, long)]
        force: bool,

        /// Skip interactive prompts and use defaults
        #[arg(short, long)]
        yes: bool,

        /// Skip bootstrap (don't install uv or conda)
        #[arg(long)]
        skip_bootstrap: bool,

        /// Add schalentier's environment setup to your shell config (~/.bashrc etc.)
        /// without prompting
        #[arg(long)]
        setup_shell: bool,
    },

    /// Add a tool or package to your configuration
    Add {
        /// Name of the tool/package to add
        #[arg(required = true)]
        name: String,

        /// Specify the provider to use (binary, cargo, brew, conda, uv, system)
        #[arg(short, long)]
        provider: Option<String>,

        /// Don't actually install, just add to config
        #[arg(long)]
        no_install: bool,

        /// Show what would be installed without installing
        #[arg(long)]
        dry_run: bool,
    },

    /// Synchronize configuration with remote
    Sync {
        /// Remote URL or path to sync with
        #[arg(short, long)]
        remote: Option<String>,

        /// Push local changes to remote
        #[arg(long)]
        push: bool,

        /// Pull remote changes to local
        #[arg(long)]
        pull: bool,

        /// Remove tools that are no longer in the config after sync
        #[arg(long)]
        prune: bool,

        /// Show what would be synced without making changes
        #[arg(long)]
        dry_run: bool,

        /// Create/update as public gist (default: secret)
        #[arg(long, conflicts_with = "secret")]
        public: bool,

        /// Create/update as secret gist (explicit override)
        #[arg(long, conflicts_with = "public")]
        secret: bool,
    },

    /// Update installed tools to their latest versions
    Update {
        /// Specific tool to update (updates all if not specified)
        name: Option<String>,

        /// Check for updates without installing
        #[arg(long)]
        dry_run: bool,

        /// Update even tools pinned to a specific version in schalentier.toml
        #[arg(long)]
        force: bool,
    },

    /// Check system health and diagnose issues
    Doctor {
        /// Attempt to fix issues automatically
        #[arg(long)]
        fix: bool,
    },

    /// Remove a tool from your configuration
    Remove {
        /// Name of the tool/package to remove
        #[arg(required = true)]
        name: String,

        /// Keep the tool installed, just remove from config
        #[arg(long)]
        keep_installed: bool,
    },

    /// List all managed tools
    List {
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,

        /// Filter by provider
        #[arg(short, long)]
        provider: Option<String>,

        /// Show cached security advisory status per tool (no network call; run
        /// `schalentier audit` first to populate the cache)
        #[arg(long)]
        security: bool,
    },

    /// Search for available packages across all providers
    Search {
        /// Search query
        #[arg(required = true)]
        query: String,

        /// Limit results per provider
        #[arg(short, long, default_value = "5")]
        limit: usize,

        /// Search only in a specific provider (binary, cargo, brew, conda, uv, system)
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Create shell aliases as executable scripts (non-intrusive)
    ///
    /// Examples:
    ///   schalentier alias ll="ls -la"
    ///   schalentier alias --list
    ///   schalentier alias --remove ll
    Alias {
        /// Alias definition in format NAME="COMMAND" (e.g., ll="ls -la")
        definition: Option<String>,

        /// List all defined aliases
        #[arg(short, long)]
        list: bool,

        /// Remove an alias by name
        #[arg(short, long)]
        remove: Option<String>,
    },

    /// Manage shell snippets for tools requiring sourcing (yazi, zoxide, fzf, etc.)
    ///
    /// Examples:
    ///   schalentier snippet list
    ///   schalentier snippet add yazi
    ///   schalentier snippet remove yazi
    Snippet {
        #[command(subcommand)]
        action: SnippetAction,
    },

    /// Manage dotfile and config file patching
    ///
    /// Applies settings from `dotfiles` section in schalentier.toml to target config files.
    /// Supports JSON, TOML, YAML, INI, and KeyValue formats with intelligent merging.
    ///
    /// Examples:
    ///   schalentier config apply       # Apply all dotfile patches
    ///   schalentier config diff        # Show what would change (dry-run)
    ///   schalentier config list        # List managed dotfiles
    ///   schalentier config reset FILE  # Restore file from backup
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Generate shell completions
    ///
    /// Examples:
    ///   schalentier completions bash > ~/.local/share/bash-completion/completions/schalentier
    ///   schalentier completions zsh > ~/.zfunc/_schalentier
    ///   schalentier completions fish > ~/.config/fish/completions/schalentier.fish
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Manage encrypted secrets (age + OS keyring)
    ///
    /// Examples:
    ///   schalentier secret set GITHUB_TOKEN --tags work,ci
    ///   schalentier secret export --tags work
    ///   schalentier secret shell --tags ci
    ///   schalentier secret run --tags ci -- ./deploy.sh
    Secret {
        #[command(subcommand)]
        action: SecretAction,
    },

    /// Manage package registry
    ///
    /// Examples:
    ///   schalentier registry validate  # Validate registry format
    ///   schalentier registry info      # Show registry statistics
    ///   schalentier registry update    # Download latest from GitHub
    Registry {
        #[command(subcommand)]
        action: RegistryAction,
    },

    /// Check installed packages for security vulnerabilities
    ///
    /// Audits packages for known vulnerabilities via OSV.dev
    ///
    /// Examples:
    ///   schalentier audit              # Check all installed packages
    ///   schalentier audit ripgrep      # Check specific package
    Audit {
        /// Specific package to audit (optional, audits all if not specified)
        package: Option<String>,

        /// Bypass the cache and re-query OSV.dev for every package
        #[arg(long)]
        refresh: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum RegistryAction {
    /// Validate registry format and show errors
    Validate,

    /// Show registry statistics (package count, provider distribution)
    Info,

    /// Download latest registry from GitHub
    Update,
}

#[derive(Subcommand, Debug)]
pub enum SnippetAction {
    /// List all installed snippets
    List,

    /// Add a snippet (from built-in registry or custom file)
    Add {
        /// Name of the tool (uses built-in snippet) or --file for custom
        name: Option<String>,

        /// Path to custom snippet file
        #[arg(short, long)]
        file: Option<String>,
    },

    /// Remove a snippet
    Remove {
        /// Name of the snippet to remove
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum SecretAction {
    /// Store a secret (prompts for value if --value not given)
    ///
    /// When run inside a project with a `.schalentier/` directory, secrets are
    /// stored in the project-local store by default. Use --global to force the
    /// global (~/.config/schalentier/) store instead.
    Set {
        /// Name of the secret
        name: String,

        /// Value to store (prompts interactively if omitted)
        #[arg(long)]
        value: Option<String>,

        /// Comma-separated tags to attach (e.g. work,ci)
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,

        /// Force the global secret store, even inside a project directory
        #[arg(long)]
        global: bool,
    },

    /// Print a secret's value to stdout (no trailing newline)
    ///
    /// Project secrets take precedence over global secrets with the same name.
    Get {
        /// Name of the secret
        name: String,
    },

    /// List secret names (and their tags)
    ///
    /// Shows a "Project secrets" / "Global secrets" split when run inside a
    /// project with a `.schalentier/` directory.
    List {
        /// Only show secrets matching any of these tags
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
    },

    /// Remove a secret
    Delete {
        /// Name of the secret
        name: String,

        /// Force the global secret store, even inside a project directory
        #[arg(long)]
        global: bool,
    },

    /// Output shell export statements for eval/source
    Export {
        /// Shell syntax to emit
        #[arg(long, default_value = "bash")]
        shell: String,

        /// Only export secrets matching any of these tags
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
    },

    /// Decrypt secrets, open in $EDITOR, then re-encrypt
    Edit,

    /// Re-encrypt all secrets with a new master password
    ChangePassword,

    /// Spawn a shell with secrets exported as environment variables
    Shell {
        /// Only expose secrets matching any of these tags
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
    },

    /// Run a command with secrets exported as environment variables
    Run {
        /// Only expose secrets matching any of these tags
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,

        /// Command and arguments to run (prefix with `--`)
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Apply all dotfile patches from configuration
    Apply,

    /// Show what would change without applying (dry-run)
    Diff,

    /// List all managed dotfiles
    List,

    /// Restore a file from its backup
    Reset {
        /// Path to the file to restore
        file: String,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Cli::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        // Verify the CLI configuration is valid
        Cli::command().debug_assert();
    }

    #[test]
    fn test_add_requires_name() {
        let result = Cli::try_parse_from(["schalentier", "add"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_with_name() {
        let cli = Cli::try_parse_from(["schalentier", "add", "ripgrep"]).unwrap();
        match cli.command {
            Commands::Add { name, .. } => assert_eq!(name, "ripgrep"),
            _ => panic!("Expected Add command"),
        }
    }

    #[test]
    fn test_add_with_provider() {
        let cli =
            Cli::try_parse_from(["schalentier", "add", "ripgrep", "--provider", "cargo"]).unwrap();
        match cli.command {
            Commands::Add { name, provider, .. } => {
                assert_eq!(name, "ripgrep");
                assert_eq!(provider, Some("cargo".to_string()));
            }
            _ => panic!("Expected Add command"),
        }
    }

    #[test]
    fn test_search_requires_query() {
        let result = Cli::try_parse_from(["schalentier", "search"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_verbose_flag() {
        let cli = Cli::try_parse_from(["schalentier", "-v", "doctor"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_search_with_provider() {
        let cli = Cli::try_parse_from(["schalentier", "search", "ripgrep", "--provider", "cargo"])
            .unwrap();
        match cli.command {
            Commands::Search {
                query, provider, ..
            } => {
                assert_eq!(query, "ripgrep");
                assert_eq!(provider, Some("cargo".to_string()));
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_search_without_provider() {
        let cli = Cli::try_parse_from(["schalentier", "search", "ripgrep"]).unwrap();
        match cli.command {
            Commands::Search {
                query, provider, ..
            } => {
                assert_eq!(query, "ripgrep");
                assert!(provider.is_none());
            }
            _ => panic!("Expected Search command"),
        }
    }
}
