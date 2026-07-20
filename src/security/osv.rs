//! OSV.dev vulnerability auditor.
//!
//! Queries the [OSV.dev](https://osv.dev) API — a single database that aggregates
//! advisories across ecosystems (crates.io, PyPI, npm, Go, and more). One HTTP endpoint
//! covers every provider schalentier installs from, so we don't need per-ecosystem tools
//! like `cargo audit`, `npm audit`, or `pip-audit` (note: `uv` has no `audit` subcommand).
//!
//! API: `POST https://api.osv.dev/v1/query` with `{"package": {"name", "ecosystem"}}`
//! and an optional `"version"`. Without a version, OSV returns all known vulnerabilities
//! for the package.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const OSV_QUERY_URL: &str = "https://api.osv.dev/v1/query";

/// A single vulnerability, normalized from an OSV record for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvVuln {
    pub id: String,
    pub ecosystem: String,
    pub package: String,
    pub summary: String,
    pub details: String,
    /// Normalized severity label (e.g. "critical", "high", "moderate", "low", "unknown").
    pub severity: String,
    /// Versions in which the issue is fixed, if OSV records any.
    pub fixed_versions: Vec<String>,
    pub url: Option<String>,
}

impl OsvVuln {
    /// True for high/critical severities — used to gate the stricter install prompt.
    pub fn is_critical(&self) -> bool {
        matches!(self.severity.as_str(), "high" | "critical")
    }

    pub fn format(&self) -> String {
        let fixed = if self.fixed_versions.is_empty() {
            "none".to_string()
        } else {
            self.fixed_versions.join(", ")
        };
        let body = if !self.summary.is_empty() {
            self.summary.clone()
        } else if !self.details.is_empty() {
            // Details can be long markdown; keep the first line for the box.
            self.details.lines().next().unwrap_or("").to_string()
        } else {
            "(no description provided)".to_string()
        };
        let url = self.url.clone().unwrap_or_else(|| {
            format!("https://osv.dev/vulnerability/{}", self.id)
        });

        format!(
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
             ⚠  SECURITY ADVISORY: {}\n\
             Severity: {}\n\
             Package: {} ({})\n\
             \n\
             {}\n\
             \n\
             Fixed versions: {}\n\
             URL: {}\n\
             ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
            self.id,
            self.severity.to_uppercase(),
            self.package,
            self.ecosystem,
            body,
            fixed,
            url,
        )
    }
}

pub struct OsvAuditor {
    client: reqwest::Client,
}

impl OsvAuditor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Map a schalentier provider name to its OSV ecosystem, if OSV covers it.
    ///
    /// Returns `None` for providers OSV can't audit by package name (e.g. `binary`
    /// GitHub-release tools, `brew`, `system` OS packages).
    pub fn ecosystem_for_provider(provider: &str) -> Option<&'static str> {
        match provider {
            "cargo" => Some("crates.io"),
            "uv" => Some("PyPI"),
            "npm" | "pnpm" | "yarn" => Some("npm"),
            "go" => Some("Go"),
            _ => None,
        }
    }

    /// Query OSV for a package in a given ecosystem. `version` narrows results to
    /// advisories affecting that version; omit it to get all known advisories.
    pub async fn query(
        &self,
        ecosystem: &str,
        package: &str,
        version: Option<&str>,
    ) -> Result<Vec<OsvVuln>> {
        let mut body = serde_json::json!({
            "package": { "name": package, "ecosystem": ecosystem }
        });
        if let Some(v) = version {
            body["version"] = serde_json::Value::String(v.to_string());
        }

        let response = self
            .client
            .post(OSV_QUERY_URL)
            .header("User-Agent", "schalentier")
            .json(&body)
            .send()
            .await
            .context("Failed to query OSV.dev")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("OSV.dev API error ({}): {}", status, text));
        }

        let parsed: OsvQueryResponse = response
            .json()
            .await
            .context("Failed to parse OSV.dev response")?;

        Ok(parsed
            .vulns
            .into_iter()
            .map(|v| v.normalize(ecosystem, package))
            .collect())
    }
}

impl Default for OsvAuditor {
    fn default() -> Self {
        Self::new()
    }
}

// --- OSV API response types (subset of the schema we consume) ---

#[derive(Debug, Deserialize)]
struct OsvQueryResponse {
    #[serde(default)]
    vulns: Vec<OsvRecord>,
}

#[derive(Debug, Deserialize)]
struct OsvRecord {
    id: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    details: Option<String>,
    #[serde(default)]
    affected: Vec<OsvAffected>,
    #[serde(default)]
    references: Vec<OsvReference>,
    #[serde(default)]
    severity: Vec<OsvSeverity>,
    #[serde(default)]
    database_specific: Option<OsvDatabaseSpecific>,
}

#[derive(Debug, Deserialize)]
struct OsvAffected {
    #[serde(default)]
    ranges: Vec<OsvRange>,
}

#[derive(Debug, Deserialize)]
struct OsvRange {
    #[serde(default)]
    events: Vec<OsvEvent>,
}

#[derive(Debug, Deserialize)]
struct OsvEvent {
    #[serde(default)]
    fixed: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OsvReference {
    #[serde(default)]
    url: Option<String>,
    #[serde(rename = "type", default)]
    ref_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OsvSeverity {
    #[serde(default)]
    score: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OsvDatabaseSpecific {
    #[serde(default)]
    severity: Option<String>,
}

impl OsvRecord {
    fn normalize(self, ecosystem: &str, package: &str) -> OsvVuln {
        let fixed_versions: Vec<String> = self
            .affected
            .iter()
            .flat_map(|a| a.ranges.iter())
            .flat_map(|r| r.events.iter())
            .filter_map(|e| e.fixed.clone())
            .collect();

        // Prefer a GHSA-style textual severity; otherwise derive from a CVSS score.
        let severity = self
            .database_specific
            .as_ref()
            .and_then(|d| d.severity.as_deref())
            .map(normalize_severity_label)
            .or_else(|| {
                self.severity
                    .iter()
                    .filter_map(|s| s.score.as_deref())
                    .find_map(severity_from_cvss)
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Prefer an ADVISORY reference URL, else the first reference.
        let url = self
            .references
            .iter()
            .find(|r| r.ref_type.as_deref() == Some("ADVISORY"))
            .or_else(|| self.references.first())
            .and_then(|r| r.url.clone());

        OsvVuln {
            id: self.id,
            ecosystem: ecosystem.to_string(),
            package: package.to_string(),
            summary: self.summary.unwrap_or_default(),
            details: self.details.unwrap_or_default(),
            severity,
            fixed_versions,
            url,
        }
    }
}

fn normalize_severity_label(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "critical" => "critical",
        "high" => "high",
        "moderate" | "medium" => "moderate",
        "low" => "low",
        _ => "unknown",
    }
    .to_string()
}

/// Derive a coarse severity label from a CVSS v3 base score (or vector string).
fn severity_from_cvss(score: &str) -> Option<String> {
    // OSV severity scores are usually CVSS vector strings (e.g. "CVSS:3.1/AV:N/...").
    // We can't parse the full vector cheaply; only map bare numeric scores here.
    let numeric: f32 = score.trim().parse().ok()?;
    let label = if numeric >= 9.0 {
        "critical"
    } else if numeric >= 7.0 {
        "high"
    } else if numeric >= 4.0 {
        "moderate"
    } else {
        "low"
    };
    Some(label.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecosystem_mapping() {
        assert_eq!(OsvAuditor::ecosystem_for_provider("cargo"), Some("crates.io"));
        assert_eq!(OsvAuditor::ecosystem_for_provider("uv"), Some("PyPI"));
        assert_eq!(OsvAuditor::ecosystem_for_provider("pnpm"), Some("npm"));
        assert_eq!(OsvAuditor::ecosystem_for_provider("go"), Some("Go"));
        assert_eq!(OsvAuditor::ecosystem_for_provider("binary"), None);
        assert_eq!(OsvAuditor::ecosystem_for_provider("system"), None);
    }

    #[test]
    fn severity_from_numeric_cvss() {
        assert_eq!(severity_from_cvss("9.8").as_deref(), Some("critical"));
        assert_eq!(severity_from_cvss("7.5").as_deref(), Some("high"));
        assert_eq!(severity_from_cvss("5.0").as_deref(), Some("moderate"));
        assert_eq!(severity_from_cvss("2.0").as_deref(), Some("low"));
        assert_eq!(severity_from_cvss("CVSS:3.1/AV:N"), None);
    }

    #[test]
    fn normalize_labels() {
        assert_eq!(normalize_severity_label("HIGH"), "high");
        assert_eq!(normalize_severity_label("Medium"), "moderate");
        assert_eq!(normalize_severity_label("bogus"), "unknown");
    }

    #[test]
    fn parses_and_normalizes_record() {
        let json = r#"{
            "vulns": [{
                "id": "GHSA-xxxx",
                "summary": "Example flaw",
                "details": "Long details here",
                "affected": [{"ranges": [{"events": [{"introduced": "0"}, {"fixed": "1.2.0"}]}]}],
                "references": [{"type": "ADVISORY", "url": "https://example.com/adv"}],
                "database_specific": {"severity": "HIGH"}
            }]
        }"#;
        let parsed: OsvQueryResponse = serde_json::from_str(json).unwrap();
        let vuln = parsed.vulns.into_iter().next().unwrap().normalize("PyPI", "example");
        assert_eq!(vuln.id, "GHSA-xxxx");
        assert_eq!(vuln.severity, "high");
        assert!(vuln.is_critical());
        assert_eq!(vuln.fixed_versions, vec!["1.2.0".to_string()]);
        assert_eq!(vuln.url.as_deref(), Some("https://example.com/adv"));
    }
}
