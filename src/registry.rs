use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct Registry {
    pub version: String,
    pub packages: Vec<Package>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Package {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub providers: HashMap<String, ProviderInfo>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ProviderInfo {
    #[serde(default = "default_available")]
    pub available: bool,
    pub name: Option<String>,
    pub reason: Option<String>,
    pub repo: Option<String>,
}

fn default_available() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    pub canonical_name: String,
    pub provider_name: String,
    pub source: ResolutionSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolutionSource {
    Registry,
    ProviderSearch,
}

pub struct PackageRegistry {
    packages: HashMap<String, Package>,
    aliases: HashMap<String, String>,
}

impl PackageRegistry {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    pub fn load() -> Result<Self> {
        let mut registry = Self::new();

        // 1. Load bundled registry (embedded at compile time)
        let bundled = include_str!("../registry/packages.json");
        registry.merge_json(bundled)?;

        // 2. Load downloaded registry (from GitHub)
        if let Some(downloaded) = Self::load_downloaded()? {
            registry.merge_json(&downloaded)?;
        }

        // 3. Load user aliases (from schalentier.toml)
        if let Ok(config) = crate::config::SchalentierConfig::load() {
            registry.merge_user_aliases(&config.aliases)?;
        }

        Ok(registry)
    }

    fn load_downloaded() -> Result<Option<String>> {
        let path = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("No data directory"))?
            .join("schalentier/registry/packages.json");

        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| "Failed to read downloaded registry")?;
            Ok(Some(content))
        } else {
            Ok(None)
        }
    }

    fn merge_json(&mut self, json: &str) -> Result<()> {
        let registry: Registry = serde_json::from_str(json)
            .with_context(|| "Failed to parse registry JSON")?;

        for package in registry.packages {
            let name_lower = package.name.to_lowercase();

            // Add aliases
            for alias in &package.aliases {
                self.aliases.insert(alias.to_lowercase(), name_lower.clone());
            }
            self.aliases.insert(name_lower.clone(), name_lower.clone());

            // Add package
            self.packages.insert(name_lower, package);
        }

        Ok(())
    }

    fn merge_user_aliases(
        &mut self,
        aliases: &HashMap<String, crate::config::UserAlias>,
    ) -> Result<()> {
        for (name, alias_config) in aliases {
            let name_lower = name.to_lowercase();

            // Convert UserAlias to Package
            let package = Package {
                name: name.clone(),
                description: alias_config.description.clone().unwrap_or_default(),
                aliases: vec![],
                keywords: vec![],
                providers: alias_config.providers.clone(),
            };

            // Override or add
            self.packages.insert(name_lower, package);
        }

        Ok(())
    }

    pub fn resolve(&self, name: &str, provider: &str) -> Result<ResolvedPackage> {
        let name_lower = name.to_lowercase();

        // Find package by name or alias
        let canonical = self
            .aliases
            .get(&name_lower)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

        let package = self
            .packages
            .get(canonical)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

        // Get provider-specific name
        let provider_info = package.providers.get(provider).ok_or_else(|| {
            anyhow::anyhow!(
                "Provider '{}' not available for package '{}'",
                provider,
                name
            )
        })?;

        if !provider_info.available {
            if let Some(reason) = &provider_info.reason {
                return Err(anyhow::anyhow!(
                    "Package '{}' not available via {}: {}",
                    name,
                    provider,
                    reason
                ));
            } else {
                return Err(anyhow::anyhow!(
                    "Package '{}' not available via {}",
                    name,
                    provider
                ));
            }
        }

        // Return provider-specific name (or canonical if not specified)
        let provider_name = provider_info
            .name
            .clone()
            .unwrap_or_else(|| package.name.clone());

        Ok(ResolvedPackage {
            canonical_name: package.name.clone(),
            provider_name,
            source: ResolutionSource::Registry,
        })
    }

    pub fn search(&self, query: &str) -> Vec<&Package> {
        let query_lower = query.to_lowercase();

        let mut results: Vec<&Package> = self
            .packages
            .values()
            .filter(|pkg| {
                // Check name
                if pkg.name.to_lowercase().contains(&query_lower) {
                    return true;
                }

                // Check aliases
                if pkg.aliases.iter().any(|a| a.to_lowercase().contains(&query_lower)) {
                    return true;
                }

                // Check keywords
                if pkg
                    .keywords
                    .iter()
                    .any(|k| k.to_lowercase().contains(&query_lower))
                {
                    return true;
                }

                // Check description
                if pkg.description.to_lowercase().contains(&query_lower) {
                    return true;
                }

                false
            })
            .collect();

        // Sort by relevance (exact match first, then by name)
        results.sort_by(|a, b| {
            let a_exact = a.name.to_lowercase() == query_lower;
            let b_exact = b.name.to_lowercase() == query_lower;

            if a_exact && !b_exact {
                std::cmp::Ordering::Less
            } else if !a_exact && b_exact {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });

        results
    }

    pub fn list_all(&self) -> Vec<&Package> {
        let mut packages: Vec<&Package> = self.packages.values().collect();
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        packages
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        // Check for duplicate aliases
        let mut seen_aliases: HashMap<String, String> = HashMap::new();
        for package in self.packages.values() {
            for alias in &package.aliases {
                let alias_lower = alias.to_lowercase();
                if let Some(existing) = seen_aliases.get(&alias_lower) {
                    errors.push(format!(
                        "Duplicate alias '{}' in packages '{}' and '{}'",
                        alias, existing, package.name
                    ));
                } else {
                    seen_aliases.insert(alias_lower, package.name.clone());
                }
            }
        }

        // Check for packages without providers
        for package in self.packages.values() {
            if package.providers.is_empty() {
                errors.push(format!("Package '{}' has no providers", package.name));
            }
        }

        errors
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    pub fn stats(&self) -> RegistryStats {
        let mut stats = RegistryStats::default();

        for package in self.packages.values() {
            for provider in package.providers.keys() {
                *stats.provider_counts.entry(provider.clone()).or_insert(0) += 1;
            }
        }

        stats.total_packages = self.packages.len();
        stats.total_aliases = self.aliases.len() - self.packages.len();

        stats
    }

    /// Resolve a package across ALL providers
    pub fn resolve_all_providers(&self, name: &str) -> Result<MultiProviderResolution> {
        let name_lower = name.to_lowercase();

        // Find package by name or alias
        let canonical = self
            .aliases
            .get(&name_lower)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

        let package = self
            .packages
            .get(canonical)
            .ok_or_else(|| anyhow::anyhow!("Package '{}' not found", name))?;

        // Collect all provider mappings
        let mut available = HashMap::new();
        let mut unavailable = HashMap::new();

        for (provider_name, provider_info) in &package.providers {
            if provider_info.available {
                let pkg_name = provider_info
                    .name
                    .clone()
                    .unwrap_or_else(|| package.name.clone());

                available.insert(
                    provider_name.clone(),
                    ProviderResolution {
                        package_name: pkg_name,
                        repo: provider_info.repo.clone(),
                    },
                );
            } else {
                unavailable.insert(
                    provider_name.clone(),
                    provider_info
                        .reason
                        .clone()
                        .unwrap_or_else(|| "Not available".to_string()),
                );
            }
        }

        Ok(MultiProviderResolution {
            canonical_name: package.name.clone(),
            description: package.description.clone(),
            available_providers: available,
            unavailable_providers: unavailable,
        })
    }
}

#[derive(Debug)]
pub struct MultiProviderResolution {
    pub canonical_name: String,
    pub description: String,
    pub available_providers: HashMap<String, ProviderResolution>,
    pub unavailable_providers: HashMap<String, String>,
}

#[derive(Debug)]
pub struct ProviderResolution {
    pub package_name: String,
    pub repo: Option<String>,
}

#[derive(Debug, Default)]
pub struct RegistryStats {
    pub total_packages: usize,
    pub total_aliases: usize,
    pub provider_counts: HashMap<String, usize>,
}
