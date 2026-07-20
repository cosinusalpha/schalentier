pub mod cache;
pub mod osv;

use crate::registry::MultiProviderResolution;
use anyhow::Result;
use cache::AuditCache;
use osv::{OsvAuditor, OsvVuln};
use std::collections::HashSet;
use std::path::PathBuf;

pub struct SecurityAuditor {
    osv: OsvAuditor,
    cache: Option<AuditCache>,
}

impl SecurityAuditor {
    /// Whether the auditors are backed by real vulnerability data sources.
    ///
    /// Now `true`: audits query the OSV.dev API (see [`osv`]). Providers OSV can't audit
    /// by package name (binary/brew/system) are simply skipped.
    pub const IMPLEMENTED: bool = true;

    pub fn new() -> Self {
        Self {
            osv: OsvAuditor::new(),
            cache: None,
        }
    }

    /// Enable result caching at `cache_path`, valid for `ttl_hours`. Without this, every
    /// `audit()` call hits the network.
    pub fn with_cache(mut self, cache_path: PathBuf, ttl_hours: u64) -> Self {
        self.cache = Some(AuditCache::load(cache_path, ttl_hours));
        self
    }

    /// Look up cached results only — never hits the network. Returns `None` for any
    /// ecosystem with no fresh cache entry, so callers can distinguish "known clean"
    /// from "not checked yet". Used by `list --security` to stay fast by default.
    pub fn peek_cache(
        &self,
        resolution: &MultiProviderResolution,
        version: Option<&str>,
    ) -> Option<SecurityReport> {
        let cache = self.cache.as_ref()?;
        let mut report = SecurityReport::default();
        let mut seen: HashSet<String> = HashSet::new();
        let mut any_hit = false;

        for (provider, res) in &resolution.available_providers {
            let Some(ecosystem) = OsvAuditor::ecosystem_for_provider(provider) else {
                continue;
            };
            let key = cache::cache_key(ecosystem, &res.package_name, version);
            let Some(vulns) = cache.get(&key) else {
                continue;
            };
            any_hit = true;
            for v in vulns {
                if seen.insert(v.id.clone()) {
                    report.vulnerabilities.push(v);
                }
            }
        }

        any_hit.then_some(report)
    }

    /// Audit every OSV-supported provider a package resolves to and merge the results.
    ///
    /// `version` narrows the query to advisories affecting that installed version; pass
    /// `None` to report all known advisories for the package. Vulnerabilities reported by
    /// multiple ecosystems are de-duplicated by OSV id. If caching is enabled (see
    /// [`Self::with_cache`]) and `refresh` is false, a fresh cache entry is served without
    /// a network call.
    pub async fn audit(
        &mut self,
        resolution: &MultiProviderResolution,
        version: Option<&str>,
        refresh: bool,
    ) -> Result<SecurityReport> {
        let mut report = SecurityReport::default();
        let mut seen: HashSet<String> = HashSet::new();

        for (provider, res) in &resolution.available_providers {
            let Some(ecosystem) = OsvAuditor::ecosystem_for_provider(provider) else {
                continue;
            };

            let key = cache::cache_key(ecosystem, &res.package_name, version);
            if !refresh {
                if let Some(cached) = self.cache.as_ref().and_then(|c| c.get(&key)) {
                    for v in cached {
                        if seen.insert(v.id.clone()) {
                            report.vulnerabilities.push(v);
                        }
                    }
                    continue;
                }
            }

            // Query using the provider-specific package name (e.g. cargo's "fd-find").
            match self.osv.query(ecosystem, &res.package_name, version).await {
                Ok(vulns) => {
                    if let Some(cache) = self.cache.as_mut() {
                        cache.put(key, vulns.clone());
                    }
                    for v in vulns {
                        if seen.insert(v.id.clone()) {
                            report.vulnerabilities.push(v);
                        }
                    }
                }
                // A single ecosystem query failure shouldn't abort the whole audit;
                // record it so callers can surface partial results honestly.
                Err(e) => report.errors.push(format!("{}: {}", ecosystem, e)),
            }
        }

        if let Some(cache) = self.cache.as_ref() {
            cache.save();
        }

        Ok(report)
    }
}

impl Default for SecurityAuditor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct SecurityReport {
    pub vulnerabilities: Vec<OsvVuln>,
    /// Non-fatal per-ecosystem query errors (e.g. network failures).
    pub errors: Vec<String>,
}

impl SecurityReport {
    pub fn is_clean(&self) -> bool {
        self.vulnerabilities.is_empty()
    }

    pub fn has_critical(&self) -> bool {
        self.vulnerabilities.iter().any(|v| v.is_critical())
    }

    pub fn total_advisories(&self) -> usize {
        self.vulnerabilities.len()
    }
}
