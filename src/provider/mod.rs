pub mod binary;
pub mod mock;

use crate::bootstrap::{Arch, Os};
use crate::config::Provider;
use crate::error::Result;
use async_trait::async_trait;
use std::path::PathBuf;

/// Search result from a provider
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Package name
    pub name: String,
    /// Package description
    pub description: Option<String>,
    /// Available version
    pub version: Option<String>,
    /// The provider this result came from
    pub provider: Provider,
    /// Additional metadata (e.g., download count, stars)
    pub metadata: std::collections::HashMap<String, String>,
}

/// Installation result
#[derive(Debug)]
pub struct InstallResult {
    /// Path to the installed binary/package
    pub path: Option<PathBuf>,
    /// Installed version
    pub version: Option<String>,
    /// Whether the installation was successful
    pub success: bool,
    /// Any additional message
    pub message: Option<String>,
}

/// The Installer trait - all providers must implement this
#[async_trait]
pub trait Installer: Send + Sync {
    /// Get the provider type
    fn provider(&self) -> Provider;

    /// Search for packages matching the query
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Install a package
    async fn install(&self, name: &str, version: Option<&str>) -> Result<InstallResult>;

    /// Uninstall a package
    async fn uninstall(&self, name: &str) -> Result<()>;

    /// Check if a package is installed
    async fn is_installed(&self, name: &str) -> Result<bool>;

    /// Get the installed version of a package
    async fn installed_version(&self, name: &str) -> Result<Option<String>>;

    /// Check if this provider is available on the current system
    fn is_available(&self) -> bool {
        true
    }
}

/// Provider registry - manages all available providers
pub struct ProviderRegistry {
    providers: Vec<Box<dyn Installer>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Add a provider to the registry
    pub fn register(&mut self, provider: Box<dyn Installer>) {
        self.providers.push(provider);
    }

    /// Get all available providers
    pub fn providers(&self) -> &[Box<dyn Installer>] {
        &self.providers
    }

    /// Get a provider by type
    pub fn get(&self, provider_type: Provider) -> Option<&dyn Installer> {
        self.providers
            .iter()
            .find(|p| p.provider() == provider_type)
            .map(|p| p.as_ref())
    }

    /// Search across all providers in parallel
    pub async fn search_all(&self, query: &str, limit_per_provider: usize) -> Vec<SearchResult> {
        use futures_util::future::join_all;

        let futures: Vec<_> = self
            .providers
            .iter()
            .filter(|p| p.is_available())
            .map(|p| async move {
                match p.search(query, limit_per_provider).await {
                    Ok(results) => results,
                    Err(e) => {
                        tracing::warn!("Search failed for {:?}: {}", p.provider(), e);
                        Vec::new()
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;
        results.into_iter().flatten().collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a default provider registry with all available providers
pub fn create_default_registry(arch: Arch, os: Os) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();

    // Add Binary provider (GitHub releases)
    registry.register(Box::new(binary::BinaryProvider::new(arch, os)));

    // TODO: Add other providers
    // registry.register(Box::new(cargo::CargoProvider::new()));
    // registry.register(Box::new(conda::CondaProvider::new()));
    // registry.register(Box::new(system::SystemProvider::new()));

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use mock::MockProvider;

    #[tokio::test]
    async fn test_provider_registry() {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider::new()));

        assert_eq!(registry.providers().len(), 1);

        let provider = registry.get(Provider::Binary);
        assert!(provider.is_some());
    }

    #[tokio::test]
    async fn test_search_all() {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider::new()));

        let results = registry.search_all("test", 5).await;
        assert!(!results.is_empty());
    }
}
