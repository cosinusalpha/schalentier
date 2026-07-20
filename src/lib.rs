pub mod archive;
pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod detection;
pub mod dotfiles;
pub mod error;
pub mod gist;
pub mod logging;
pub mod provider;
pub mod registry;
pub mod secrets;
pub mod security;
pub mod shell;
pub mod state;
pub mod template;


pub use bootstrap::{Arch, Bootstrap, Os};
pub use clap_complete::Shell;
pub use cli::{Cli, Commands, ConfigAction};
pub use config::{LocalState, Provider, SchalentierConfig};
pub use dotfiles::{ConfigFormat, DotfileManager};
pub use error::{Result, SchalentierError};
pub use provider::{Installer, ProviderRegistry, SearchResult};
pub use registry::{PackageRegistry, Registry};
