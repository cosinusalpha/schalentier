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
    },

    /// Update installed tools to their latest versions
    Update {
        /// Specific tool to update (updates all if not specified)
        name: Option<String>,

        /// Check for updates without installing
        #[arg(long)]
        dry_run: bool,
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
