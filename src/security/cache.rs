//! On-disk cache for OSV.dev audit results, so `audit`/`add` don't hit the network on
//! every invocation. Keyed by `(ecosystem, package, version)`, TTL-based.

use super::osv::OsvVuln;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;

/// Build the cache key for a given ecosystem/package/version triple.
pub fn cache_key(ecosystem: &str, package: &str, version: Option<&str>) -> String {
    format!("{}:{}:{}", ecosystem, package, version.unwrap_or("*"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    vulns: Vec<OsvVuln>,
    fetched_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    #[serde(default)]
    entries: HashMap<String, CacheEntry>,
}

/// A TTL-based cache of audit results, persisted as JSON at `path`. Interior-mutable so
/// [`SecurityAuditor::audit`](super::SecurityAuditor::audit) can update it through a
/// shared reference while iterating providers.
pub struct AuditCache {
    path: PathBuf,
    ttl_hours: u64,
    file: RefCell<CacheFile>,
}

impl AuditCache {
    pub fn load(path: PathBuf, ttl_hours: u64) -> Self {
        let file = std::fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default();

        Self {
            path,
            ttl_hours,
            file: RefCell::new(file),
        }
    }

    /// Return cached vulnerabilities for `key` if present and not older than the TTL.
    pub fn get(&self, key: &str) -> Option<Vec<OsvVuln>> {
        let file = self.file.borrow();
        let entry = file.entries.get(key)?;
        if now_secs().saturating_sub(entry.fetched_at) >= self.ttl_hours * 3600 {
            return None;
        }
        Some(entry.vulns.clone())
    }

    /// Record a fresh result for `key`, timestamped now.
    pub fn put(&self, key: String, vulns: Vec<OsvVuln>) {
        self.file.borrow_mut().entries.insert(
            key,
            CacheEntry {
                vulns,
                fetched_at: now_secs(),
            },
        );
    }

    /// Persist the current cache contents to disk. Best-effort: a write failure is
    /// logged, not propagated, since the cache is a pure optimization.
    pub fn save(&self) {
        let file = self.file.borrow();
        match serde_json::to_string_pretty(&*file) {
            Ok(json) => {
                if let Some(parent) = self.path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::write(&self.path, json) {
                    debug!("Failed to write audit cache to {}: {}", self.path.display(), e);
                }
            }
            Err(e) => debug!("Failed to serialize audit cache: {}", e),
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_vuln() -> OsvVuln {
        OsvVuln {
            id: "GHSA-xxxx".to_string(),
            ecosystem: "PyPI".to_string(),
            package: "black".to_string(),
            summary: "Example".to_string(),
            details: String::new(),
            severity: "high".to_string(),
            fixed_versions: vec!["1.0.0".to_string()],
            url: None,
        }
    }

    #[test]
    fn miss_when_empty() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::remove_file(temp.path()).unwrap();
        let cache = AuditCache::load(temp.path().to_path_buf(), 24);
        assert!(cache.get("PyPI:black:21.12b0").is_none());
    }

    #[test]
    fn hit_after_put() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::remove_file(temp.path()).unwrap();
        let cache = AuditCache::load(temp.path().to_path_buf(), 24);

        cache.put("PyPI:black:21.12b0".to_string(), vec![sample_vuln()]);
        let hit = cache.get("PyPI:black:21.12b0").unwrap();
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].id, "GHSA-xxxx");
    }

    #[test]
    fn expired_entry_is_a_miss() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::remove_file(temp.path()).unwrap();
        let cache = AuditCache::load(temp.path().to_path_buf(), 0);

        cache.put("PyPI:black:21.12b0".to_string(), vec![sample_vuln()]);
        // TTL of 0 hours means anything already written is immediately stale.
        assert!(cache.get("PyPI:black:21.12b0").is_none());
    }

    #[test]
    fn persists_across_loads() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        std::fs::remove_file(&path).unwrap();

        let cache1 = AuditCache::load(path.clone(), 24);
        cache1.put("PyPI:black:21.12b0".to_string(), vec![sample_vuln()]);
        cache1.save();

        let cache2 = AuditCache::load(path, 24);
        let hit = cache2.get("PyPI:black:21.12b0").unwrap();
        assert_eq!(hit[0].id, "GHSA-xxxx");
    }

    #[test]
    fn cache_key_uses_star_for_no_version() {
        assert_eq!(cache_key("PyPI", "black", None), "PyPI:black:*");
        assert_eq!(cache_key("PyPI", "black", Some("21.12b0")), "PyPI:black:21.12b0");
    }
}
