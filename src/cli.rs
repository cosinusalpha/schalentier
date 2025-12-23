use clap::{Parser, Subcommand};

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
    },

    /// Add a tool or package to your configuration
    Add {
        /// Name of the tool/package to add
        #[arg(required = true)]
        name: String,

        /// Specify the provider to use (system, conda, cargo, binary)
        #[arg(short, long)]
        provider: Option<String>,

        /// Don't actually install, just add to config
        #[arg(long)]
        no_install: bool,
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
        let cli = Cli::try_parse_from(["schalentier", "add", "ripgrep", "--provider", "cargo"]).unwrap();
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
}
