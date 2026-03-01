use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{Result, DeweyError};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub waybar: WaybarConfig,
    #[serde(default)]
    pub backends: BackendsConfig,
    #[serde(default)]
    pub agent: Option<toml::Table>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WaybarConfig {
    /// "overdue_today" (default), "all", "today_only"
    #[serde(default = "default_tooltip_scope")]
    pub tooltip_scope: String,
}

impl Default for WaybarConfig {
    fn default() -> Self {
        Self {
            tooltip_scope: default_tooltip_scope(),
        }
    }
}

fn default_tooltip_scope() -> String {
    "overdue_today".into()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GeneralConfig {
    #[serde(default = "default_view")]
    pub default_view: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub default_backend: Option<String>,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_view: default_view(),
            theme: default_theme(),
            default_backend: None,
        }
    }
}

fn default_view() -> String {
    "today".into()
}

fn default_theme() -> String {
    "omarchy".into()
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct BackendsConfig {
    #[serde(default)]
    pub obsidian: Option<toml::Table>,
    #[serde(default)]
    pub local: Option<toml::Table>,
    #[serde(default)]
    pub linear: Option<toml::Table>,
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        let config_path = match path {
            Some(p) => p,
            None => Self::default_config_path()?,
        };

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| DeweyError::Config(format!("Failed to parse config: {}", e)))?;

        Ok(config)
    }

    pub fn default_config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| DeweyError::Config("Could not find config directory".into()))?;
        Ok(config_dir.join("dewey").join("config.toml"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            waybar: WaybarConfig::default(),
            backends: BackendsConfig::default(),
            agent: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_backend_config_deserializes() {
        let toml_str = r#"
[backends.linear]
enabled = true
api_key = "lin_api_test123"
team_id = "TEAM-1"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let linear = config.backends.linear.expect("linear config should be Some");
        assert_eq!(linear.get("enabled").unwrap().as_bool(), Some(true));
        assert_eq!(
            linear.get("api_key").unwrap().as_str(),
            Some("lin_api_test123")
        );
        assert_eq!(linear.get("team_id").unwrap().as_str(), Some("TEAM-1"));
    }

    #[test]
    fn test_agent_config_deserializes() {
        let toml_str = r#"
[agent]
enabled = true
mode = "interactive"
poll_interval_secs = 300
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let agent = config.agent.expect("agent config should be Some");
        assert_eq!(agent.get("enabled").unwrap().as_bool(), Some(true));
        assert_eq!(agent.get("mode").unwrap().as_str(), Some("interactive"));
        assert_eq!(
            agent.get("poll_interval_secs").unwrap().as_integer(),
            Some(300)
        );
    }

    #[test]
    fn test_config_without_linear_or_agent_sections() {
        let toml_str = r#"
[general]
default_view = "upcoming"

[backends.local]
enabled = true
path = "/tmp/todo.txt"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.backends.linear.is_none());
        assert!(config.agent.is_none());
        // Existing fields still work
        assert_eq!(config.general.default_view, "upcoming");
        assert!(config.backends.local.is_some());
    }

    #[test]
    fn test_empty_config_deserializes() {
        let toml_str = "";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.backends.linear.is_none());
        assert!(config.backends.obsidian.is_none());
        assert!(config.backends.local.is_none());
        assert!(config.agent.is_none());
        assert_eq!(config.general.default_view, "today");
        assert_eq!(config.general.theme, "omarchy");
    }

    #[test]
    fn test_full_config_with_all_sections() {
        let toml_str = r#"
[general]
default_view = "upcoming"
theme = "dark"

[waybar]
tooltip_scope = "all"

[backends.obsidian]
enabled = true
vault_path = "/home/user/vault"

[backends.local]
enabled = true
path = "/tmp/todo.txt"

[backends.linear]
enabled = true
api_key = "lin_api_xyz"

[agent]
enabled = false
mode = "background"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.default_view, "upcoming");
        assert_eq!(config.general.theme, "dark");
        assert_eq!(config.waybar.tooltip_scope, "all");
        assert!(config.backends.obsidian.is_some());
        assert!(config.backends.local.is_some());
        assert!(config.backends.linear.is_some());
        let agent = config.agent.expect("agent should be Some");
        assert_eq!(agent.get("enabled").unwrap().as_bool(), Some(false));
        assert_eq!(agent.get("mode").unwrap().as_str(), Some("background"));
    }
}
