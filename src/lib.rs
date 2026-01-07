pub mod archive;
pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod dotfiles;
pub mod error;
pub mod logging;
pub mod provider;
pub mod shell;
pub mod state;

pub use bootstrap::{Arch, Bootstrap, Os};
pub use cli::{Cli, Commands, ConfigAction};
pub use clap_complete::Shell;
pub use config::{LocalState, Provider, SchalentierConfig};
pub use dotfiles::{ConfigFormat, DotfileManager};
pub use error::{Result, SchalentierError};
pub use provider::{Installer, ProviderRegistry, SearchResult};
