pub mod binary;
pub mod brew;
pub mod cargo;
pub mod conda;
pub mod mock;
pub mod system;
pub mod uv;

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

/// Provider information for clustered search results
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    /// The provider type
    pub provider: Provider,
    /// Version available from this provider
    pub version: Option<String>,
}

/// Clustered search result - groups results by package name across providers
#[derive(Debug, Clone)]
pub struct ClusteredSearchResult {
    /// Package name
    pub name: String,
    /// Package description (from first provider that had one)
    pub description: Option<String>,
    /// List of providers that have this package
    pub providers: Vec<ProviderInfo>,
    /// Merged metadata from all providers
    pub metadata: std::collections::HashMap<String, String>,
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

    /// Get the latest available version of a package.
    /// Default implementation uses search with limit=1.
    async fn latest_version(&self, name: &str) -> Result<Option<String>> {
        let results = self.search(name, 5).await?;
        // Find exact match or best match
        let exact = results.iter().find(|r| r.name == name || r.name.ends_with(&format!("/{}", name)));
        if let Some(result) = exact {
            return Ok(result.version.clone());
        }
        // Fall back to first result if it contains the name
        Ok(results.first().and_then(|r| r.version.clone()))
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

    /// Search across all providers and cluster results by name
    ///
    /// Returns clustered results where each unique package name appears once,
    /// with information about which providers have it available.
    pub async fn search_all_clustered(
        &self,
        query: &str,
        limit_per_provider: usize,
    ) -> Vec<ClusteredSearchResult> {
        use std::collections::HashMap;

        let results = self.search_all(query, limit_per_provider).await;

        // Group by normalized name (lowercase)
        let mut clusters: HashMap<String, ClusteredSearchResult> = HashMap::new();

        for result in results {
            let key = result.name.to_lowercase();

            clusters
                .entry(key)
                .and_modify(|cluster| {
                    // Add this provider to the cluster
                    cluster.providers.push(ProviderInfo {
                        provider: result.provider.clone(),
                        version: result.version.clone(),
                    });
                    // Merge metadata (prefer higher values for stars, etc.)
                    for (k, v) in &result.metadata {
                        if !cluster.metadata.contains_key(k) {
                            cluster.metadata.insert(k.clone(), v.clone());
                        }
                    }
                    // Use description if we don't have one
                    if cluster.description.is_none() && result.description.is_some() {
                        cluster.description = result.description.clone();
                    }
                })
                .or_insert_with(|| ClusteredSearchResult {
                    name: result.name.clone(),
                    description: result.description.clone(),
                    providers: vec![ProviderInfo {
                        provider: result.provider.clone(),
                        version: result.version.clone(),
                    }],
                    metadata: result.metadata.clone(),
                });
        }

        // Convert to vec and sort by number of providers (most available first)
        let mut clustered: Vec<_> = clusters.into_values().collect();
        clustered.sort_by(|a, b| {
            // Sort by provider count descending, then by name ascending
            b.providers.len().cmp(&a.providers.len())
                .then_with(|| a.name.cmp(&b.name))
        });

        clustered
    }

    /// Get all available providers (those that are actually usable on this system)
    pub fn available_providers(&self) -> Vec<&dyn Installer> {
        self.providers
            .iter()
            .filter(|p| p.is_available())
            .map(|p| p.as_ref())
            .collect()
    }

    /// Install a package with fallback to alternative providers.
    ///
    /// If a preferred provider is specified, try it first. If it fails or is unavailable,
    /// try other providers in registration order (which reflects priority).
    ///
    /// Returns the install result and the provider that was actually used.
    pub async fn install_with_fallback(
        &self,
        name: &str,
        version: Option<&str>,
        preferred: Option<Provider>,
    ) -> Result<(InstallResult, Provider)> {
        let mut tried_providers = Vec::new();

        // If preferred provider is specified and available, try it first
        if let Some(ref preferred_type) = preferred {
            if let Some(provider) = self.get(preferred_type.clone()) {
                if provider.is_available() {
                    tracing::info!(
                        "Trying preferred provider {:?} for {}",
                        preferred_type,
                        name
                    );
                    tried_providers.push(preferred_type.clone());

                    match provider.install(name, version).await {
                        Ok(result) if result.success => {
                            tracing::info!(
                                "Successfully installed {} via preferred provider {:?}",
                                name,
                                preferred_type
                            );
                            return Ok((result, preferred_type.clone()));
                        }
                        Ok(result) => {
                            tracing::warn!(
                                "Preferred provider {:?} failed for {}: {}",
                                preferred_type,
                                name,
                                result.message.as_deref().unwrap_or("unknown error")
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Preferred provider {:?} error for {}: {}",
                                preferred_type,
                                name,
                                e
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        "Preferred provider {:?} is not available, trying fallbacks",
                        preferred_type
                    );
                }
            }
        }

        // Try other available providers in order
        for provider in self.providers.iter().filter(|p| p.is_available()) {
            let provider_type = provider.provider();

            // Skip if we already tried this provider
            if tried_providers.contains(&provider_type) {
                continue;
            }

            tracing::info!(
                "Trying fallback provider {:?} for {}",
                provider_type,
                name
            );
            tried_providers.push(provider_type.clone());

            match provider.install(name, version).await {
                Ok(result) if result.success => {
                    if preferred.is_some() {
                        tracing::info!(
                            "Installed {} via fallback provider {:?} (preferred was {:?})",
                            name,
                            provider_type,
                            preferred
                        );
                    } else {
                        tracing::info!(
                            "Installed {} via {:?}",
                            name,
                            provider_type
                        );
                    }
                    return Ok((result, provider_type));
                }
                Ok(result) => {
                    tracing::debug!(
                        "Provider {:?} failed for {}: {}",
                        provider_type,
                        name,
                        result.message.as_deref().unwrap_or("unknown error")
                    );
                }
                Err(e) => {
                    tracing::debug!(
                        "Provider {:?} error for {}: {}",
                        provider_type,
                        name,
                        e
                    );
                }
            }
        }

        // All providers failed
        use crate::error::SchalentierError;
        Err(SchalentierError::InstallFailed {
            package: name.to_string(),
            reason: format!(
                "All providers failed. Tried: {:?}",
                tried_providers
            ),
        }
        .into())
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a default provider registry with all available providers
pub fn create_default_registry(arch: Arch, os: Os, data_dir: std::path::PathBuf) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();

    // Add Binary provider (GitHub releases) - always available
    registry.register(Box::new(binary::BinaryProvider::new(arch, os)));

    // Add Cargo provider (crates.io)
    let cargo_provider = cargo::CargoProvider::new();
    if cargo_provider.is_available() {
        registry.register(Box::new(cargo_provider));
    }

    // Add Brew provider (Homebrew/Linuxbrew)
    let brew_provider = brew::BrewProvider::new();
    if brew_provider.is_available() {
        registry.register(Box::new(brew_provider));
    }

    // Add Conda provider (conda-forge via mamba/conda)
    let conda_provider = conda::CondaProvider::new(data_dir.clone());
    if conda_provider.is_available() {
        registry.register(Box::new(conda_provider));
    }

    // Add System provider (apt, pacman, dnf, etc.)
    let system_provider = system::SystemProvider::new();
    if system_provider.is_available() {
        registry.register(Box::new(system_provider));
    }

    // Add UV provider (Python CLI tools via uv tool)
    let uv_provider = uv::UvProvider::new(data_dir);
    if uv_provider.is_available() {
        registry.register(Box::new(uv_provider));
    }

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

    #[tokio::test]
    async fn test_install_with_fallback_no_preference() {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider::new()));

        // No preferred provider - should use first available (MockProvider returns Binary)
        let result = registry.install_with_fallback("test-package", None, None).await;
        assert!(result.is_ok());

        let (install_result, provider) = result.unwrap();
        assert!(install_result.success);
        assert_eq!(provider, Provider::Binary);
    }

    #[tokio::test]
    async fn test_install_with_fallback_with_preferred() {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider::new())); // Returns Binary provider type

        // Request Binary provider (matches MockProvider)
        let result = registry.install_with_fallback("test-package", None, Some(Provider::Binary)).await;
        assert!(result.is_ok());

        let (install_result, provider) = result.unwrap();
        assert!(install_result.success);
        assert_eq!(provider, Provider::Binary);
    }

    #[tokio::test]
    async fn test_install_with_fallback_unavailable_preferred() {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider::new())); // Returns Binary provider type

        // Request Cargo provider (not registered), should fallback to Binary
        let result = registry.install_with_fallback("test-package", None, Some(Provider::Cargo)).await;
        assert!(result.is_ok());

        let (install_result, provider) = result.unwrap();
        assert!(install_result.success);
        assert_eq!(provider, Provider::Binary); // Fell back to MockProvider (Binary)
    }
}
