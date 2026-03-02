use std::path::Path;

use serde_json::Value;

use crate::error::{Result, DeweyError};

// ---------------------------------------------------------------------------
// Data structs for setup wizard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SetupTeam {
    pub id: String,
    pub name: String,
    pub key: String,
}

#[derive(Debug, Clone)]
pub struct SetupUser {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone)]
pub struct SetupState {
    pub id: String,
    pub name: String,
    pub state_type: String,
}

#[derive(Debug, Clone)]
pub struct BackendOption {
    pub name: String,
    pub key: String,
    pub description: String,
}

pub fn backend_options() -> Vec<BackendOption> {
    vec![
        BackendOption {
            name: "Local tasks".into(),
            key: "local".into(),
            description: "Simple file-based task list stored on your machine".into(),
        },
        BackendOption {
            name: "Linear".into(),
            key: "linear".into(),
            description: "Sync tasks with Linear project management".into(),
        },
    ]
}

// ---------------------------------------------------------------------------
// SetupStep enum — drives the wizard UI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum SetupStep {
    Welcome,
    SelectBackends {
        options: Vec<BackendOption>,
        selected: Vec<bool>,
        cursor: usize,
    },
    BackendName,
    ApiKey,
    ValidatingKey,
    SelectTeam {
        teams: Vec<SetupTeam>,
        selected: usize,
    },
    SelectAssignee {
        members: Vec<SetupUser>,
        selected: usize,
    },
    SelectStatuses {
        states: Vec<SetupState>,
        selected: Vec<bool>,
        cursor: usize,
    },
    Complete,
    Error(String),
}

// ---------------------------------------------------------------------------
// SetupWizard — handles API calls and config writing
// ---------------------------------------------------------------------------

pub struct SetupWizard {
    client: reqwest::Client,
}

impl SetupWizard {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    // ---- GraphQL helper ---------------------------------------------------

    async fn graphql(&self, api_key: &str, body: Value) -> Result<Value> {
        let response = self
            .client
            .post("https://api.linear.app/graphql")
            .header("Authorization", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| DeweyError::Backend {
                backend: "Linear".into(),
                message: format!("HTTP request failed: {e}"),
            })?;

        let status = response.status();
        let text = response.text().await.map_err(|e| DeweyError::Backend {
            backend: "Linear".into(),
            message: format!("Failed to read response body: {e}"),
        })?;

        if !status.is_success() {
            return Err(DeweyError::Backend {
                backend: "Linear".into(),
                message: format!("HTTP {status}: {text}"),
            });
        }

        let json: Value = serde_json::from_str(&text)?;

        if let Some(errors) = json.get("errors") {
            let msg = errors
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown GraphQL error");
            return Err(DeweyError::Backend {
                backend: "Linear".into(),
                message: msg.to_string(),
            });
        }

        Ok(json)
    }

    // ---- API methods ------------------------------------------------------

    /// Validate an API key by calling the viewer query.
    /// Returns the authenticated user on success.
    pub async fn validate_key(&self, api_key: &str) -> Result<SetupUser> {
        let body = super::graphql::viewer_query();
        let resp = self.graphql(api_key, body).await?;

        let viewer = resp
            .pointer("/data/viewer")
            .ok_or_else(|| DeweyError::Backend {
                backend: "Linear".into(),
                message: "Invalid API key or unexpected response".into(),
            })?;

        let id = viewer
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name = viewer
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let display_name = viewer
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(SetupUser { id, name, email: display_name })
    }

    /// Fetch all teams the user has access to.
    pub async fn fetch_teams(&self, api_key: &str) -> Result<Vec<SetupTeam>> {
        let body = super::graphql::teams_query();
        let resp = self.graphql(api_key, body).await?;

        let nodes = resp
            .pointer("/data/teams/nodes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DeweyError::Backend {
                backend: "Linear".into(),
                message: "Failed to fetch teams".into(),
            })?;

        let teams = nodes
            .iter()
            .filter_map(|n| {
                let id = n.get("id")?.as_str()?.to_string();
                let name = n.get("name")?.as_str()?.to_string();
                let key = n.get("key")?.as_str()?.to_string();
                Some(SetupTeam { id, name, key })
            })
            .collect();

        Ok(teams)
    }

    /// Fetch members of a specific team.
    pub async fn fetch_members(&self, api_key: &str, team_id: &str) -> Result<Vec<SetupUser>> {
        let body = super::graphql::team_members_query(team_id);
        let resp = self.graphql(api_key, body).await?;

        let nodes = resp
            .pointer("/data/team/members/nodes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DeweyError::Backend {
                backend: "Linear".into(),
                message: "Failed to fetch team members".into(),
            })?;

        let members = nodes
            .iter()
            .filter_map(|n| {
                let id = n.get("id")?.as_str()?.to_string();
                let name = n.get("name")?.as_str()?.to_string();
                let display_name = n
                    .get("displayName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(SetupUser { id, name, email: display_name })
            })
            .collect();

        Ok(members)
    }

    /// Fetch workflow states for a specific team.
    pub async fn fetch_states(&self, api_key: &str, team_id: &str) -> Result<Vec<SetupState>> {
        let body = super::graphql::workflow_states_query(team_id);
        let resp = self.graphql(api_key, body).await?;

        let nodes = resp
            .pointer("/data/workflowStates/nodes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DeweyError::Backend {
                backend: "Linear".into(),
                message: "Failed to fetch workflow states".into(),
            })?;

        let states = nodes
            .iter()
            .filter_map(|n| {
                let id = n.get("id")?.as_str()?.to_string();
                let name = n.get("name")?.as_str()?.to_string();
                let state_type = n.get("type")?.as_str()?.to_string();
                Some(SetupState {
                    id,
                    name,
                    state_type,
                })
            })
            .collect();

        Ok(states)
    }

    // ---- Config writing ---------------------------------------------------

    /// Write the completed setup to the config file.
    ///
    /// Reads the existing config (or starts fresh), updates the
    /// `[backends.linear]` section, and writes it back.
    pub fn write_config(
        &self,
        config_path: &Path,
        api_key: &str,
        user: &SetupUser,
        team: &SetupTeam,
        assignee: &str,
        statuses: &[String],
    ) -> Result<()> {
        // Read existing config or start with empty table.
        let mut doc: toml::Table = if config_path.exists() {
            let content = std::fs::read_to_string(config_path)?;
            content
                .parse::<toml::Table>()
                .map_err(|e| DeweyError::Config(format!("Failed to parse config: {e}")))?
        } else {
            toml::Table::new()
        };

        // Ensure [backends] exists.
        if !doc.contains_key("backends") {
            doc.insert(
                "backends".to_string(),
                toml::Value::Table(toml::Table::new()),
            );
        }

        let backends = doc
            .get_mut("backends")
            .and_then(|v| v.as_table_mut())
            .ok_or_else(|| DeweyError::Config("backends is not a table".into()))?;

        // Build the linear section.
        let mut linear = toml::Table::new();
        linear.insert("enabled".to_string(), toml::Value::Boolean(true));
        linear.insert(
            "api_key".to_string(),
            toml::Value::String(api_key.to_string()),
        );
        linear.insert(
            "team_id".to_string(),
            toml::Value::String(team.id.clone()),
        );
        linear.insert(
            "team_name".to_string(),
            toml::Value::String(team.name.clone()),
        );
        linear.insert(
            "assignee".to_string(),
            toml::Value::String(assignee.to_string()),
        );
        linear.insert(
            "user_id".to_string(),
            toml::Value::String(user.id.clone()),
        );

        let status_array: Vec<toml::Value> = statuses
            .iter()
            .map(|s| toml::Value::String(s.clone()))
            .collect();
        linear.insert(
            "filter_status".to_string(),
            toml::Value::Array(status_array),
        );

        backends.insert("linear".to_string(), toml::Value::Table(linear));

        // Ensure parent directory exists.
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = toml::to_string_pretty(&doc)
            .map_err(|e| DeweyError::Config(format!("Failed to serialize config: {e}")))?;

        std::fs::write(config_path, output)?;

        Ok(())
    }

    /// Write the completed general setup to the config file.
    ///
    /// Handles both local and linear backend selections.
    /// Reads the existing config (or starts fresh), updates the relevant
    /// backend sections, and writes it back.
    pub fn write_general_config(
        &self,
        config_path: &Path,
        selected_backends: &[String],
        api_key: Option<&str>,
        user: Option<&SetupUser>,
        team: Option<&SetupTeam>,
        assignee: &str,
        statuses: &[String],
        backend_name: Option<&str>,
    ) -> Result<()> {
        // Read existing config or start with empty table.
        let mut doc: toml::Table = if config_path.exists() {
            let content = std::fs::read_to_string(config_path)?;
            content
                .parse::<toml::Table>()
                .map_err(|e| DeweyError::Config(format!("Failed to parse config: {e}")))?
        } else {
            toml::Table::new()
        };

        // Ensure [general] exists.
        if !doc.contains_key("general") {
            doc.insert(
                "general".to_string(),
                toml::Value::Table(toml::Table::new()),
            );
        }

        let general = doc
            .get_mut("general")
            .and_then(|v| v.as_table_mut())
            .ok_or_else(|| DeweyError::Config("general is not a table".into()))?;

        // Set default_backend based on the backend name or selected backends.
        let default_backend = if let Some(name) = backend_name {
            name
        } else if selected_backends.contains(&"linear".to_string()) {
            "linear"
        } else {
            "local"
        };
        general.insert(
            "default_backend".to_string(),
            toml::Value::String(default_backend.to_string()),
        );

        // Set theme = "omarchy" if not already set.
        if !general.contains_key("theme") {
            general.insert(
                "theme".to_string(),
                toml::Value::String("omarchy".to_string()),
            );
        }

        // Ensure [backends] exists.
        if !doc.contains_key("backends") {
            doc.insert(
                "backends".to_string(),
                toml::Value::Table(toml::Table::new()),
            );
        }

        let backends = doc
            .get_mut("backends")
            .and_then(|v| v.as_table_mut())
            .ok_or_else(|| DeweyError::Config("backends is not a table".into()))?;

        // If local was selected, set [backends.local] enabled = true.
        if selected_backends.contains(&"local".to_string()) {
            let mut local = toml::Table::new();
            local.insert("enabled".to_string(), toml::Value::Boolean(true));
            backends.insert("local".to_string(), toml::Value::Table(local));
        }

        // If linear was selected, write the full linear section.
        if selected_backends.contains(&"linear".to_string()) {
            // Build the linear backend entry.
            let mut entry = toml::Table::new();
            entry.insert("enabled".to_string(), toml::Value::Boolean(true));

            if let Some(key) = api_key {
                entry.insert(
                    "api_key".to_string(),
                    toml::Value::String(key.to_string()),
                );
            }

            if let Some(team) = team {
                entry.insert(
                    "team_id".to_string(),
                    toml::Value::String(team.id.clone()),
                );
                entry.insert(
                    "team_name".to_string(),
                    toml::Value::String(team.name.clone()),
                );
            }

            if !assignee.is_empty() {
                entry.insert(
                    "assignee".to_string(),
                    toml::Value::String(assignee.to_string()),
                );
            }

            if let Some(user) = user {
                entry.insert(
                    "user_id".to_string(),
                    toml::Value::String(user.id.clone()),
                );
            }

            if !statuses.is_empty() {
                let status_array: Vec<toml::Value> = statuses
                    .iter()
                    .map(|s| toml::Value::String(s.clone()))
                    .collect();
                entry.insert(
                    "filter_status".to_string(),
                    toml::Value::Array(status_array),
                );
            }

            if let Some(name) = backend_name {
                // Named backend: write to [backends.linear.<name>].
                // First, migrate old format if it exists.
                if let Some(existing) = backends.get("linear") {
                    if let Some(existing_table) = existing.as_table() {
                        let is_old_format = existing_table.get("api_key").map_or(false, |v| v.is_str())
                            || existing_table.get("enabled").map_or(false, |v| v.is_bool());
                        if is_old_format {
                            // Migrate old format to a named subtable.
                            let old_name = existing_table
                                .get("team_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("default")
                                .to_lowercase()
                                .replace(' ', "-");
                            let old_entry = existing_table.clone();
                            let mut new_linear = toml::Table::new();
                            new_linear.insert(old_name, toml::Value::Table(old_entry));
                            new_linear.insert(name.to_string(), toml::Value::Table(entry));
                            backends.insert("linear".to_string(), toml::Value::Table(new_linear));
                        } else {
                            // Already multi-format, just add the new subtable.
                            let linear = backends
                                .get_mut("linear")
                                .and_then(|v| v.as_table_mut())
                                .unwrap();
                            linear.insert(name.to_string(), toml::Value::Table(entry));
                        }
                    } else {
                        // Unexpected format, overwrite.
                        let mut new_linear = toml::Table::new();
                        new_linear.insert(name.to_string(), toml::Value::Table(entry));
                        backends.insert("linear".to_string(), toml::Value::Table(new_linear));
                    }
                } else {
                    // No existing linear section, create with named subtable.
                    let mut new_linear = toml::Table::new();
                    new_linear.insert(name.to_string(), toml::Value::Table(entry));
                    backends.insert("linear".to_string(), toml::Value::Table(new_linear));
                }
            } else {
                // No name: write old format [backends.linear].
                backends.insert("linear".to_string(), toml::Value::Table(entry));
            }
        }

        // Ensure parent directory exists.
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = toml::to_string_pretty(&doc)
            .map_err(|e| DeweyError::Config(format!("Failed to serialize config: {e}")))?;

        std::fs::write(config_path, output)?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn write_config_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let wizard = SetupWizard::new();
        let user = SetupUser {
            id: "user-123".into(),
            name: "Test User".into(),
            email: "test@example.com".into(),
        };
        let team = SetupTeam {
            id: "team-456".into(),
            name: "Engineering".into(),
            key: "ENG".into(),
        };

        wizard
            .write_config(
                &config_path,
                "lin_api_testkey",
                &user,
                &team,
                "me",
                &["In Progress".into(), "Todo".into()],
            )
            .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let table: toml::Table = content.parse().unwrap();

        let linear = table["backends"]["linear"].as_table().unwrap();
        assert_eq!(linear["enabled"].as_bool(), Some(true));
        assert_eq!(linear["api_key"].as_str(), Some("lin_api_testkey"));
        assert_eq!(linear["team_id"].as_str(), Some("team-456"));
        assert_eq!(linear["team_name"].as_str(), Some("Engineering"));
        assert_eq!(linear["assignee"].as_str(), Some("me"));
        assert_eq!(linear["user_id"].as_str(), Some("user-123"));

        let statuses = linear["filter_status"].as_array().unwrap();
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses[0].as_str(), Some("In Progress"));
        assert_eq!(statuses[1].as_str(), Some("Todo"));
    }

    #[test]
    fn write_config_preserves_existing_sections() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        // Write initial config with other sections.
        let initial = r#"
[general]
default_view = "upcoming"
theme = "dark"

[backends.local]
enabled = true
"#;
        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(initial.as_bytes()).unwrap();

        let wizard = SetupWizard::new();
        let user = SetupUser {
            id: "user-789".into(),
            name: "Jane".into(),
            email: "jane@co.com".into(),
        };
        let team = SetupTeam {
            id: "team-abc".into(),
            name: "Product".into(),
            key: "PRD".into(),
        };

        wizard
            .write_config(
                &config_path,
                "lin_api_key2",
                &user,
                &team,
                "user-789",
                &["Backlog".into(), "In Progress".into(), "Todo".into()],
            )
            .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let table: toml::Table = content.parse().unwrap();

        // Original sections preserved.
        assert_eq!(
            table["general"]["default_view"].as_str(),
            Some("upcoming")
        );
        assert_eq!(table["general"]["theme"].as_str(), Some("dark"));
        assert_eq!(
            table["backends"]["local"]["enabled"].as_bool(),
            Some(true)
        );

        // New linear section present.
        let linear = table["backends"]["linear"].as_table().unwrap();
        assert_eq!(linear["api_key"].as_str(), Some("lin_api_key2"));
        assert_eq!(linear["team_id"].as_str(), Some("team-abc"));
        assert_eq!(linear["team_name"].as_str(), Some("Product"));
        assert_eq!(linear["assignee"].as_str(), Some("user-789"));
    }

    #[test]
    fn write_config_overwrites_existing_linear_section() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let initial = r#"
[backends.linear]
enabled = true
api_key = "old_key"
team_id = "old_team"
"#;
        std::fs::write(&config_path, initial).unwrap();

        let wizard = SetupWizard::new();
        let user = SetupUser {
            id: "new-user".into(),
            name: "New User".into(),
            email: "new@co.com".into(),
        };
        let team = SetupTeam {
            id: "new-team".into(),
            name: "New Team".into(),
            key: "NEW".into(),
        };

        wizard
            .write_config(
                &config_path,
                "new_api_key",
                &user,
                &team,
                "me",
                &["Todo".into()],
            )
            .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let table: toml::Table = content.parse().unwrap();

        let linear = table["backends"]["linear"].as_table().unwrap();
        assert_eq!(linear["api_key"].as_str(), Some("new_api_key"));
        assert_eq!(linear["team_id"].as_str(), Some("new-team"));
        assert_eq!(linear["team_name"].as_str(), Some("New Team"));
    }

    #[test]
    fn write_config_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("nested").join("dir").join("config.toml");

        let wizard = SetupWizard::new();
        let user = SetupUser {
            id: "u".into(),
            name: "U".into(),
            email: "u@x.com".into(),
        };
        let team = SetupTeam {
            id: "t".into(),
            name: "T".into(),
            key: "T".into(),
        };

        wizard
            .write_config(&config_path, "key", &user, &team, "me", &["Todo".into()])
            .unwrap();

        assert!(config_path.exists());
    }

    #[test]
    fn write_general_config_named_backend() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let wizard = SetupWizard::new();
        let user = SetupUser {
            id: "user-1".into(),
            name: "Alice".into(),
            email: "alice@co.com".into(),
        };
        let team = SetupTeam {
            id: "team-1".into(),
            name: "Work".into(),
            key: "WRK".into(),
        };

        wizard
            .write_general_config(
                &config_path,
                &["linear".to_string()],
                Some("lin_api_work"),
                Some(&user),
                Some(&team),
                "me",
                &["Todo".into()],
                Some("work"),
            )
            .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let table: toml::Table = content.parse().unwrap();

        // Should be under [backends.linear.work]
        let work = table["backends"]["linear"]["work"].as_table().unwrap();
        assert_eq!(work["api_key"].as_str(), Some("lin_api_work"));
        assert_eq!(work["team_id"].as_str(), Some("team-1"));
    }

    #[test]
    fn write_general_config_migrates_old_format() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        // Write initial old-format config.
        let initial = r#"
[general]
default_backend = "linear"

[backends.linear]
enabled = true
api_key = "lin_api_old"
team_id = "team-old"
team_name = "OldTeam"
user_id = "user-old"
filter_status = ["Todo"]
"#;
        std::fs::write(&config_path, initial).unwrap();

        let wizard = SetupWizard::new();
        let user = SetupUser {
            id: "user-new".into(),
            name: "Bob".into(),
            email: "bob@co.com".into(),
        };
        let team = SetupTeam {
            id: "team-new".into(),
            name: "NewTeam".into(),
            key: "NEW".into(),
        };

        wizard
            .write_general_config(
                &config_path,
                &["linear".to_string()],
                Some("lin_api_new"),
                Some(&user),
                Some(&team),
                "me",
                &["In Progress".into()],
                Some("personal"),
            )
            .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let table: toml::Table = content.parse().unwrap();

        // Old backend should be migrated to [backends.linear.oldteam]
        let oldteam = table["backends"]["linear"]["oldteam"].as_table().unwrap();
        assert_eq!(oldteam["api_key"].as_str(), Some("lin_api_old"));
        assert_eq!(oldteam["team_id"].as_str(), Some("team-old"));

        // New backend should be at [backends.linear.personal]
        let personal = table["backends"]["linear"]["personal"].as_table().unwrap();
        assert_eq!(personal["api_key"].as_str(), Some("lin_api_new"));
        assert_eq!(personal["team_id"].as_str(), Some("team-new"));
    }

    #[test]
    fn write_general_config_adds_to_existing_multi_format() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        // Write initial multi-format config.
        let initial = r#"
[backends.linear.work]
enabled = true
api_key = "lin_api_work"
team_id = "team-work"
team_name = "Work"
user_id = "user-1"
"#;
        std::fs::write(&config_path, initial).unwrap();

        let wizard = SetupWizard::new();
        let user = SetupUser {
            id: "user-2".into(),
            name: "Carol".into(),
            email: "carol@co.com".into(),
        };
        let team = SetupTeam {
            id: "team-personal".into(),
            name: "Personal".into(),
            key: "PER".into(),
        };

        wizard
            .write_general_config(
                &config_path,
                &["linear".to_string()],
                Some("lin_api_personal"),
                Some(&user),
                Some(&team),
                "me",
                &["Todo".into()],
                Some("personal"),
            )
            .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let table: toml::Table = content.parse().unwrap();

        // Both backends should exist
        let work = table["backends"]["linear"]["work"].as_table().unwrap();
        assert_eq!(work["api_key"].as_str(), Some("lin_api_work"));

        let personal = table["backends"]["linear"]["personal"].as_table().unwrap();
        assert_eq!(personal["api_key"].as_str(), Some("lin_api_personal"));
    }

    #[test]
    fn setup_step_variants_exist() {
        // Smoke test: ensure all variants are constructable.
        let _ = SetupStep::Welcome;
        let _ = SetupStep::SelectBackends {
            options: vec![],
            selected: vec![],
            cursor: 0,
        };
        let _ = SetupStep::BackendName;
        let _ = SetupStep::ApiKey;
        let _ = SetupStep::ValidatingKey;
        let _ = SetupStep::SelectTeam {
            teams: vec![],
            selected: 0,
        };
        let _ = SetupStep::SelectAssignee {
            members: vec![],
            selected: 0,
        };
        let _ = SetupStep::SelectStatuses {
            states: vec![],
            selected: vec![],
            cursor: 0,
        };
        let _ = SetupStep::Complete;
        let _ = SetupStep::Error("test".into());
    }
}
