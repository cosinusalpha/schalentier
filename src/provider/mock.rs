use super::{InstallResult, Installer, SearchResult};
use crate::config::Provider;
use crate::error::Result;
use async_trait::async_trait;

/// Mock provider for testing
pub struct MockProvider {
    available: bool,
    provider: Provider,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            available: true,
            provider: Provider::Binary,
        }
    }

    pub fn unavailable() -> Self {
        Self {
            available: false,
            provider: Provider::Binary,
        }
    }

    /// Override the provider type this mock reports as (default: `Binary`). Useful
    /// for testing multi-provider fallback with more than one mock registered.
    pub fn with_provider(mut self, provider: Provider) -> Self {
        self.provider = provider;
        self
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Installer for MockProvider {
    fn provider(&self) -> Provider {
        self.provider.clone()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Return some mock results
        let results = vec![
            SearchResult {
                name: format!("{}-tool", query),
                description: Some(format!("A tool related to {}", query)),
                version: Some("1.0.0".to_string()),
                provider: self.provider.clone(),
                metadata: std::collections::HashMap::new(),
            },
            SearchResult {
                name: format!("{}-cli", query),
                description: Some(format!("CLI for {}", query)),
                version: Some("2.0.0".to_string()),
                provider: self.provider.clone(),
                metadata: std::collections::HashMap::new(),
            },
        ];

        Ok(results.into_iter().take(limit).collect())
    }

    async fn install(&self, name: &str, version: Option<&str>) -> Result<InstallResult> {
        Ok(InstallResult {
            path: Some(std::path::PathBuf::from(format!("/usr/local/bin/{}", name))),
            version: version
                .map(String::from)
                .or_else(|| Some("1.0.0".to_string())),
            success: true,
            message: Some(format!("Mock installed {}", name)),
        })
    }

    async fn uninstall(&self, name: &str) -> Result<()> {
        tracing::info!("Mock uninstalling {}", name);
        Ok(())
    }

    async fn is_installed(&self, _name: &str) -> Result<bool> {
        Ok(false)
    }

    async fn installed_version(&self, _name: &str) -> Result<Option<String>> {
        Ok(None)
    }

    fn is_available(&self) -> bool {
        self.available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_search() {
        let provider = MockProvider::new();
        let results = provider.search("test", 5).await.unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].name.contains("test"));
    }

    #[tokio::test]
    async fn test_mock_install() {
        let provider = MockProvider::new();
        let result = provider.install("test-tool", Some("1.2.3")).await.unwrap();

        assert!(result.success);
        assert_eq!(result.version, Some("1.2.3".to_string()));
    }

    #[tokio::test]
    async fn test_mock_availability() {
        let available = MockProvider::new();
        assert!(available.is_available());

        let unavailable = MockProvider::unavailable();
        assert!(!unavailable.is_available());
    }
}
