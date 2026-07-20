use std::fmt;
use thiserror::Error;

/// Custom error types for Schalentier
#[derive(Error, Debug)]
pub enum SchalentierError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Provider '{provider}' not found")]
    ProviderNotFound { provider: String },

    #[error("Package '{package}' not found in any provider")]
    PackageNotFound { package: String },

    #[error("Installation failed for '{package}': {reason}")]
    InstallFailed { package: String, reason: String },

    #[error("Bootstrap failed: {0}")]
    BootstrapFailed(String),

    #[error("Sync failed: {0}")]
    SyncFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Not initialized. Run 'schalentier init' first.")]
    NotInitialized,

    #[error("Already initialized. Use --force to re-initialize.")]
    AlreadyInitialized,

    #[error("Unsupported architecture: {0}")]
    UnsupportedArch(String),

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Merge conflict: {0}")]
    MergeConflict(String),

    #[error("Provider not available: {0}")]
    ProviderNotAvailable(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Secret '{name}' not found")]
    SecretNotFound { name: String },

    #[error("Failed to decrypt secrets: {0}")]
    DecryptionFailed(String),

    #[error("No master password available. Run 'schalentier secret set' to create one.")]
    NoMasterPassword,

    #[error("Template error: {0}")]
    Template(String),
}

/// Result type alias for Schalentier operations
pub type Result<T> = std::result::Result<T, anyhow::Error>;

/// Pretty-print an error for user display
pub struct PrettyError<'a>(pub &'a anyhow::Error);

impl fmt::Display for PrettyError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error: {}", self.0)?;

        // Print the chain of causes
        let mut cause = self.0.source();
        while let Some(e) = cause {
            write!(f, "\n  Caused by: {}", e)?;
            cause = e.source();
        }

        Ok(())
    }
}

/// Print an error message in red to stderr
pub fn print_error(err: &anyhow::Error) {
    // Use ANSI escape codes for red text
    eprintln!("\x1b[31m{}\x1b[0m", PrettyError(err));
}

/// Print a warning message in yellow to stderr
pub fn print_warning(msg: &str) {
    eprintln!("\x1b[33mWarning: {}\x1b[0m", msg);
}

/// Print a success message in green
pub fn print_success(msg: &str) {
    println!("\x1b[32m{}\x1b[0m", msg);
}

/// Print an info message in blue
pub fn print_info(msg: &str) {
    println!("\x1b[34m{}\x1b[0m", msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SchalentierError::PackageNotFound {
            package: "foo".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "Package 'foo' not found in any provider"
        );
    }

    #[test]
    fn test_error_chain() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: anyhow::Error = anyhow::Error::from(io_err)
            .context("Failed to read config")
            .context("Initialization failed");

        let pretty = format!("{}", PrettyError(&err));
        assert!(pretty.contains("Initialization failed"));
        assert!(pretty.contains("Failed to read config"));
        assert!(pretty.contains("file not found"));
    }

    #[test]
    fn test_custom_errors() {
        let err = SchalentierError::ProviderNotFound {
            provider: "conda".to_string(),
        };
        assert_eq!(format!("{}", err), "Provider 'conda' not found");

        let err = SchalentierError::NotInitialized;
        assert!(format!("{}", err).contains("schalentier init"));
    }
}
