use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Configuration for casual-review, loaded from .casual-review.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Per-rule configuration (thresholds, enabled/disabled)
    #[serde(default)]
    pub rules: HashMap<String, RuleConfig>,

    /// Global path suppressions (patterns to ignore)
    #[serde(default)]
    pub suppress: SuppressionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    /// Is this rule enabled? (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Rule-specific configuration
    #[serde(flatten)]
    pub settings: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuppressionConfig {
    /// File path patterns to suppress (glob-style)
    #[serde(default)]
    pub paths: Vec<String>,

    /// Suppress specific rules globally
    #[serde(default)]
    pub rules: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Config {
    /// Load config from .casual-review.toml in current dir or parent dirs.
    /// Returns None if no config file found.
    pub fn load(start_path: &Path) -> std::io::Result<Option<Self>> {
        let mut current = start_path.to_path_buf();

        loop {
            let config_path = current.join(".casual-review.toml");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let config: Config = toml::from_str(&content)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                return Ok(Some(config));
            }

            if !current.pop() {
                break;
            }
        }

        Ok(None)
    }

    /// Check if a rule is enabled (default: true if not configured)
    pub fn is_rule_enabled(&self, rule_id: &str) -> bool {
        self.rules.get(rule_id).map(|c| c.enabled).unwrap_or(true)
    }

    /// Check if a file path should be suppressed
    pub fn should_suppress_path(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.suppress.paths.iter().any(|pattern| {
            glob::Pattern::new(pattern)
                .ok()
                .and_then(|p| p.matches(&path_str).then_some(true))
                .unwrap_or(false)
        })
    }

    /// Get the complexity threshold override for a rule
    pub fn get_complexity_threshold(&self, rule_id: &str) -> Option<u32> {
        self.rules
            .get(rule_id)
            .and_then(|c| c.settings.get("threshold"))
            .and_then(|v| v.as_integer().map(|i| i as u32))
    }

    /// Get the function size threshold override for a rule
    pub fn get_function_size_threshold(&self, rule_id: &str) -> Option<usize> {
        self.rules
            .get(rule_id)
            .and_then(|c| c.settings.get("max_lines"))
            .and_then(|v| v.as_integer().map(|i| i as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_enabled_default() {
        let config = Config::default();
        assert!(config.is_rule_enabled("any-rule"));
    }

    #[test]
    fn test_rule_disabled() {
        let mut config = Config::default();
        config.rules.insert(
            "todo-marker".to_string(),
            RuleConfig {
                enabled: false,
                settings: HashMap::new(),
            },
        );
        assert!(!config.is_rule_enabled("todo-marker"));
        assert!(config.is_rule_enabled("other-rule"));
    }
}
