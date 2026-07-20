//! Templating engine for dotfiles: renders `{{ ... }}` expressions using minijinja
//! (Jinja2 syntax) before a dotfile patch is merged into its target file.
//!
//! Context variables available to templates:
//! - `os`, `arch`, `hostname`, `username`, `home`
//! - `env.VARNAME` — process environment variables
//! - `secret.NAME` — decrypted secrets (see [`crate::secrets`])
//! - `var.NAME` — user-defined variables from the `[variables]` config section

use crate::error::{Result, SchalentierError};
use minijinja::value::Value as JinjaValue;
use minijinja::{Environment, UndefinedBehavior};
use std::collections::HashMap;

/// Context passed into every template render.
#[derive(Debug, Clone)]
pub struct TemplateContext {
    pub os: String,
    pub arch: String,
    pub hostname: String,
    pub username: String,
    pub home: String,
    pub env: HashMap<String, String>,
    pub secrets: HashMap<String, String>,
    pub variables: toml::Value,
}

impl TemplateContext {
    /// Build a context from the current machine's environment.
    pub fn from_system(os: &str, arch: &str) -> Self {
        Self {
            os: os.to_string(),
            arch: arch.to_string(),
            hostname: detect_hostname(),
            username: detect_username(),
            home: dirs::home_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            env: std::env::vars().collect(),
            secrets: HashMap::new(),
            variables: toml::Value::Table(toml::map::Map::new()),
        }
    }

    /// Attach decrypted secrets (name -> value) to the context.
    pub fn with_secrets(mut self, secrets: HashMap<String, String>) -> Self {
        self.secrets = secrets;
        self
    }

    /// Attach the `[variables]` table from `schalentier.toml`.
    pub fn with_variables(mut self, variables: toml::Value) -> Self {
        self.variables = variables;
        self
    }

    /// Top-level keys valid in a template, used for "did you mean" hints.
    fn top_level_keys(&self) -> Vec<&'static str> {
        vec![
            "os", "arch", "hostname", "username", "home", "env", "secret", "var",
        ]
    }

    fn to_jinja_value(&self) -> JinjaValue {
        let mut map = HashMap::new();
        map.insert("os".to_string(), JinjaValue::from(self.os.clone()));
        map.insert("arch".to_string(), JinjaValue::from(self.arch.clone()));
        map.insert(
            "hostname".to_string(),
            JinjaValue::from(self.hostname.clone()),
        );
        map.insert(
            "username".to_string(),
            JinjaValue::from(self.username.clone()),
        );
        map.insert("home".to_string(), JinjaValue::from(self.home.clone()));
        map.insert("env".to_string(), JinjaValue::from_serialize(&self.env));
        map.insert(
            "secret".to_string(),
            JinjaValue::from_serialize(&self.secrets),
        );
        map.insert("var".to_string(), toml_to_jinja(&self.variables));
        JinjaValue::from(map)
    }
}

fn detect_hostname() -> String {
    if let Ok(name) = std::env::var("HOSTNAME") {
        return name;
    }
    if let Ok(name) = std::env::var("COMPUTERNAME") {
        return name;
    }
    std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn detect_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

fn toml_to_jinja(value: &toml::Value) -> JinjaValue {
    match value {
        toml::Value::String(s) => JinjaValue::from(s.clone()),
        toml::Value::Integer(i) => JinjaValue::from(*i),
        toml::Value::Float(f) => JinjaValue::from(*f),
        toml::Value::Boolean(b) => JinjaValue::from(*b),
        toml::Value::Array(arr) => JinjaValue::from(arr.iter().map(toml_to_jinja).collect::<Vec<_>>()),
        toml::Value::Table(table) => {
            let map: HashMap<String, JinjaValue> = table
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_jinja(v)))
                .collect();
            JinjaValue::from(map)
        }
        toml::Value::Datetime(dt) => JinjaValue::from(dt.to_string()),
    }
}

/// Render a single template string against the given context.
///
/// Errors are annotated with a "did you mean" suggestion when the failure looks
/// like a typo'd top-level variable (e.g. `{{ hotsname }}`), and with a pointer to
/// `schalentier secret set` when it looks like a missing secret.
pub fn render(template: &str, ctx: &TemplateContext) -> Result<String> {
    if let Some(hint) = check_known_references(template, ctx) {
        return Err(anyhow::anyhow!(SchalentierError::Template(hint)));
    }

    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    // We're templating file content (dotfiles), not HTML fragments, so a
    // trailing newline in the source is meaningful and must be preserved.
    env.set_keep_trailing_newline(true);

    env.render_str(template, ctx.to_jinja_value())
        .map_err(|e| anyhow::anyhow!(SchalentierError::Template(e.to_string())))
}

/// Pre-scan a template for `{{ x.y }}` / `{{ x }}` references to catch typos and
/// missing secrets with a specific, actionable message before minijinja's generic
/// "undefined value" error would otherwise fire.
fn check_known_references(template: &str, ctx: &TemplateContext) -> Option<String> {
    for reference in extract_references(template) {
        let mut parts = reference.splitn(2, '.');
        let head = parts.next().unwrap_or_default();
        let rest = parts.next();

        if head == "secret" {
            if let Some(name) = rest {
                if !ctx.secrets.contains_key(name) {
                    return Some(format!(
                        "Secret '{name}' not found\n  Run: schalentier secret set {name}"
                    ));
                }
            }
            continue;
        }

        if head == "var" || head == "env" {
            // Values under var.*/env.* are user-defined; we can't know all valid
            // keys in advance without penalizing legitimately-optional lookups.
            continue;
        }

        let known = ctx.top_level_keys();
        if !known.contains(&head) {
            if let Some(suggestion) = closest_match(head, &known) {
                return Some(format!(
                    "Undefined variable '{head}' (did you mean '{suggestion}'?)"
                ));
            }
        }
    }
    None
}

/// Extract the dotted-path identifier immediately following `{{` (ignoring
/// whitespace and simple filters like `{{ x | default(...) }}`).
fn extract_references(template: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let expr_end = after.find("}}").unwrap_or(after.len());
        let expr = after[..expr_end].trim();
        let ident: String = expr
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        if !ident.is_empty() {
            refs.push(ident);
        }
        rest = &after[expr_end.min(after.len())..];
        if rest.len() < 2 {
            break;
        }
    }
    refs
}

/// Suggest a known key within edit-distance 1 of `word` (simple typo detection).
fn closest_match(word: &str, candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|c| levenshtein_distance_1(word, c))
        .map(|c| c.to_string())
}

/// True if `a` and `b` are within Damerau-Levenshtein distance 1: at most one
/// character insertion, deletion, substitution, or adjacent transposition
/// (e.g. `hotsname` -> `hostname`), without pulling in a full crate for it.
fn levenshtein_distance_1(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    if long.len() - short.len() > 1 {
        return false;
    }

    if long.len() == short.len() {
        let diff_positions: Vec<usize> = short
            .iter()
            .zip(long.iter())
            .enumerate()
            .filter(|(_, (x, y))| x != y)
            .map(|(i, _)| i)
            .collect();

        match diff_positions.as_slice() {
            [] => true,
            [_] => true, // single substitution
            [i, j] if *j == *i + 1 => short[*i] == long[*j] && short[*j] == long[*i], // adjacent transposition
            _ => false,
        }
    } else {
        // insertion/deletion: walk both, allow exactly one skip in `long`
        let mut i = 0;
        let mut j = 0;
        let mut skipped = false;
        while i < short.len() && j < long.len() {
            if short[i] == long[j] {
                i += 1;
                j += 1;
            } else if !skipped {
                skipped = true;
                j += 1;
            } else {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> TemplateContext {
        let mut secrets = HashMap::new();
        secrets.insert("GITHUB_TOKEN".to_string(), "ghp_xxx".to_string());

        let mut variables = toml::map::Map::new();
        variables.insert(
            "work_email".to_string(),
            toml::Value::String("ada@company.com".to_string()),
        );
        let mut work = toml::map::Map::new();
        work.insert(
            "email".to_string(),
            toml::Value::String("ada@company.com".to_string()),
        );
        variables.insert("work".to_string(), toml::Value::Table(work));

        TemplateContext {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            hostname: "my-laptop".to_string(),
            username: "ada".to_string(),
            home: "/home/ada".to_string(),
            env: HashMap::new(),
            secrets,
            variables: toml::Value::Table(variables),
        }
    }

    #[test]
    fn test_render_simple_variables() {
        let ctx = test_context();
        assert_eq!(render("{{ hostname }}", &ctx).unwrap(), "my-laptop");
        assert_eq!(render("{{ os }}-{{ arch }}", &ctx).unwrap(), "linux-x86_64");
    }

    #[test]
    fn test_render_secret() {
        let ctx = test_context();
        assert_eq!(
            render("token={{ secret.GITHUB_TOKEN }}", &ctx).unwrap(),
            "token=ghp_xxx"
        );
    }

    #[test]
    fn test_render_variables_nested() {
        let ctx = test_context();
        assert_eq!(
            render("{{ var.work_email }}", &ctx).unwrap(),
            "ada@company.com"
        );
        assert_eq!(
            render("{{ var.work.email }}", &ctx).unwrap(),
            "ada@company.com"
        );
    }

    #[test]
    fn test_render_conditional() {
        let ctx = test_context();
        let tpl = "{% if hostname == 'my-laptop' %}yes{% else %}no{% endif %}";
        assert_eq!(render(tpl, &ctx).unwrap(), "yes");
    }

    #[test]
    fn test_undefined_variable_gives_did_you_mean() {
        let ctx = test_context();
        let err = render("{{ hotsname }}", &ctx).unwrap_err();
        assert!(err.to_string().contains("did you mean 'hostname'"));
    }

    #[test]
    fn test_missing_secret_gives_helpful_error() {
        let ctx = test_context();
        let err = render("{{ secret.MISSING_TOKEN }}", &ctx).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Secret 'MISSING_TOKEN' not found"));
        assert!(msg.contains("schalentier secret set MISSING_TOKEN"));
    }

    #[test]
    fn test_non_templated_string_passthrough() {
        let ctx = test_context();
        assert_eq!(render("plain text, no braces", &ctx).unwrap(), "plain text, no braces");
    }

    #[test]
    fn test_extract_references() {
        let refs = extract_references("{{ hostname }} and {{ secret.FOO }} and {{ var.bar.baz }}");
        assert_eq!(refs, vec!["hostname", "secret.FOO", "var.bar.baz"]);
    }

    #[test]
    fn test_levenshtein_distance_1() {
        assert!(levenshtein_distance_1("hostname", "hostname"));
        assert!(levenshtein_distance_1("hotsname", "hostname")); // adjacent transposition
        assert!(levenshtein_distance_1("hostnam", "hostname")); // deletion
        assert!(levenshtein_distance_1("hostnamee", "hostname")); // insertion
        assert!(levenshtein_distance_1("hostnaee", "hostname")); // substitution (m -> e)
        assert!(!levenshtein_distance_1("completely", "different"));
    }
}
