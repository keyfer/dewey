pub mod graphql;
pub mod setup;

use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::{json, Map, Value};
use tracing::warn;

use crate::backends::TaskBackend;
use crate::error::{Result, DeweyError};
use crate::model::{
    BackendSource, NewTask, Priority, Task, TaskFilter, TaskId, TaskStatus, TaskUpdate,
};

// ---------------------------------------------------------------------------
// LinearConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LinearConfig {
    pub name: String,
    pub api_key: String,
    pub team_id: String,
    pub team_name: String,
    pub assignee: String,
    pub user_id: String,
    pub filter_status: Vec<String>,
}

impl LinearConfig {
    /// Parse a `LinearConfig` from a TOML table with an explicit backend name.
    ///
    /// Returns `Ok(None)` when:
    /// - The backend is not enabled.
    /// - The backend is enabled but the `api_key` is missing or empty (needs
    ///   the setup wizard to run first).
    pub fn from_named_table(name: &str, table: &toml::Table) -> Result<Option<Self>> {
        let enabled = table
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !enabled {
            return Ok(None);
        }

        let api_key = table
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if api_key.is_empty() {
            // Needs setup wizard — gracefully skip.
            return Ok(None);
        }

        let team_id = table
            .get("team_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let team_name = table
            .get("team_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let assignee = table
            .get("assignee")
            .and_then(|v| v.as_str())
            .unwrap_or("me")
            .to_string();

        let user_id = table
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let filter_status = table
            .get("filter_status")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    "In Progress".to_string(),
                    "Todo".to_string(),
                    "Backlog".to_string(),
                ]
            });

        Ok(Some(Self {
            name: name.to_string(),
            api_key,
            team_id,
            team_name,
            assignee,
            user_id,
            filter_status,
        }))
    }

    /// Parse a `LinearConfig` from a TOML table (backward compat, defaults name to "linear").
    pub fn from_table(table: &toml::Table) -> Result<Option<Self>> {
        Self::from_named_table("linear", table)
    }
}

// ---------------------------------------------------------------------------
// Priority mapping helpers
// ---------------------------------------------------------------------------

/// Map a Linear priority integer to a Dewey `Priority`.
fn linear_priority_to_dewey(linear: i64) -> Priority {
    match linear {
        1 | 2 => Priority::High,
        3 => Priority::Medium,
        4 => Priority::Low,
        _ => Priority::None, // 0 (None) or anything unexpected
    }
}

/// Map a Dewey `Priority` to a Linear priority integer (for create/update).
fn dewey_priority_to_linear(priority: Priority) -> i32 {
    match priority {
        Priority::High => 2,
        Priority::Medium => 3,
        Priority::Low => 4,
        Priority::None => 0,
    }
}

// ---------------------------------------------------------------------------
// LinearBackend
// ---------------------------------------------------------------------------

pub struct LinearBackend {
    config: LinearConfig,
    client: reqwest::Client,
}

impl LinearBackend {
    pub fn new(config: LinearConfig) -> Self {
        let client = reqwest::Client::new();
        Self { config, client }
    }

    // ---- low-level helpers ------------------------------------------------

    /// Execute a GraphQL request against the Linear API.
    async fn graphql(&self, body: Value) -> Result<Value> {
        let response = self
            .client
            .post("https://api.linear.app/graphql")
            .header("Authorization", &self.config.api_key)
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

        // Surface GraphQL-level errors.
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

    /// Parse a single GraphQL issue node into a Dewey `Task`.
    fn parse_issue(&self, node: &Value) -> Option<Task> {
        let identifier = node.get("identifier")?.as_str()?;
        let title = node.get("title")?.as_str()?;

        let priority_num = node
            .get("priority")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let due = node
            .get("dueDate")
            .and_then(|v| v.as_str())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let state_type = node
            .pointer("/state/type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let status = if state_type == "completed" || state_type == "canceled" {
            TaskStatus::Done
        } else {
            TaskStatus::Pending
        };

        // Labels become tags.
        let tags: Vec<String> = node
            .pointer("/labels/nodes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|n| n.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let project = node
            .pointer("/project/name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let created_at = node
            .get("createdAt")
            .and_then(|v| v.as_str())
            .and_then(|s| {
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ").ok()
            });

        let completed_at = node
            .get("completedAt")
            .and_then(|v| v.as_str())
            .and_then(|s| {
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ").ok()
            });

        let url = node
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let description = node
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let state_name = node
            .pointer("/state/name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Some(Task {
            id: format!("{}:{identifier}", self.config.name),
            title: format!("{identifier} {title}"),
            status,
            priority: linear_priority_to_dewey(priority_num),
            due,
            tags,
            source: BackendSource::Linear,
            backend_key: self.config.name.clone(),
            source_line: None,
            source_path: url,
            created_at,
            completed_at,
            description,
            project,
            state_name,
        })
    }

    /// Resolve a Dewey task ID like `"linear:MORE-74"` to the Linear
    /// human-readable identifier (e.g. `"MORE-74"`). Linear's API accepts
    /// these directly in mutations like `issueUpdate(id:)` and
    /// `issueArchive(id:)`, so no UUID lookup is needed.
    async fn resolve_issue_id(&self, task_id: &TaskId) -> Result<String> {
        let prefix = format!("{}:", self.config.name);
        task_id
            .strip_prefix(&prefix)
            .map(|s| s.to_string())
            .ok_or_else(|| DeweyError::Parse(format!("Invalid Linear task ID: {task_id}")))
    }

    /// Find the first workflow state of the given `state_type` (e.g.
    /// "completed", "unstarted") for the configured team.
    async fn state_id_by_type(&self, state_type: &str) -> Result<String> {
        let body = graphql::workflow_states_query(&self.config.team_id);
        let resp = self.graphql(body).await?;

        let nodes = resp
            .pointer("/data/workflowStates/nodes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DeweyError::Backend {
                backend: "Linear".into(),
                message: "Failed to fetch workflow states".into(),
            })?;

        for node in nodes {
            if node.get("type").and_then(|v| v.as_str()) == Some(state_type) {
                if let Some(id) = node.get("id").and_then(|v| v.as_str()) {
                    return Ok(id.to_string());
                }
            }
        }

        Err(DeweyError::Backend {
            backend: "Linear".into(),
            message: format!("No workflow state of type '{state_type}' found"),
        })
    }

    /// Find a workflow state by name (case-insensitive) for the configured team.
    async fn state_id_by_name(&self, name: &str) -> Result<String> {
        let body = graphql::workflow_states_query(&self.config.team_id);
        let resp = self.graphql(body).await?;

        let nodes = resp
            .pointer("/data/workflowStates/nodes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DeweyError::Backend {
                backend: "Linear".into(),
                message: "Failed to fetch workflow states".into(),
            })?;

        let name_lower = name.to_lowercase();
        for node in nodes {
            if let Some(node_name) = node.get("name").and_then(|v| v.as_str()) {
                if node_name.to_lowercase() == name_lower {
                    if let Some(id) = node.get("id").and_then(|v| v.as_str()) {
                        return Ok(id.to_string());
                    }
                }
            }
        }

        Err(DeweyError::Backend {
            backend: "Linear".into(),
            message: format!("No workflow state named '{name}' found"),
        })
    }

    /// Get the ID of the first "completed" workflow state.
    async fn done_state_id(&self) -> Result<String> {
        self.state_id_by_type("completed").await
    }

    /// Get the ID of the first "unstarted" workflow state.
    async fn todo_state_id(&self) -> Result<String> {
        self.state_id_by_type("unstarted").await
    }

    /// Resolve tag names to Linear label IDs by querying the team's labels.
    async fn resolve_label_ids(&self, names: &[String]) -> Result<Vec<String>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }

        let body = graphql::team_labels_query(&self.config.team_id);
        let resp = self.graphql(body).await?;

        let empty = vec![];
        let nodes = resp
            .pointer("/data/issueLabels/nodes")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        let mut ids = Vec::new();
        let names_lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();

        for node in nodes {
            if let (Some(id), Some(name)) = (
                node.get("id").and_then(|v| v.as_str()),
                node.get("name").and_then(|v| v.as_str()),
            ) {
                if names_lower.contains(&name.to_lowercase()) {
                    ids.push(id.to_string());
                }
            }
        }

        Ok(ids)
    }

    /// Resolve a project name to a Linear project ID by querying the team's projects.
    async fn resolve_project_id(&self, name: &str) -> Result<Option<String>> {
        let body = graphql::team_projects_query(&self.config.team_id);
        let resp = self.graphql(body).await?;

        let empty = vec![];
        let nodes = resp
            .pointer("/data/team/projects/nodes")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        let name_lower = name.to_lowercase();

        for node in nodes {
            if let (Some(id), Some(proj_name)) = (
                node.get("id").and_then(|v| v.as_str()),
                node.get("name").and_then(|v| v.as_str()),
            ) {
                if proj_name.to_lowercase() == name_lower {
                    return Ok(Some(id.to_string()));
                }
            }
        }

        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// TaskBackend trait
// ---------------------------------------------------------------------------

#[async_trait]
impl TaskBackend for LinearBackend {
    fn name(&self) -> &str {
        if self.config.name == "linear" {
            "Linear"
        } else {
            // Leak to get a &'static str — there are very few backends
            Box::leak(format!("Linear ({})", self.config.name).into_boxed_str())
        }
    }

    fn source(&self) -> BackendSource {
        BackendSource::Linear
    }

    fn key(&self) -> &str {
        &self.config.name
    }

    async fn fetch_tasks(&self, filter: &TaskFilter) -> Result<Vec<Task>> {
        let body = graphql::issues_query(
            &self.config.team_id,
            &self.config.user_id,
            &self.config.filter_status,
        );

        let resp = self.graphql(body).await?;

        let nodes = resp
            .pointer("/data/issues/nodes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DeweyError::Backend {
                backend: "Linear".into(),
                message: "Unexpected response shape from issues query".into(),
            })?;

        let mut tasks: Vec<Task> = nodes.iter().filter_map(|n| self.parse_issue(n)).collect();

        // Apply client-side filters.
        if let Some(ref status) = filter.status {
            tasks.retain(|t| &t.status == status);
        }
        if let Some(ref due_before) = filter.due_before {
            tasks.retain(|t| t.due.map_or(false, |d| d <= *due_before));
        }
        if let Some(ref due_after) = filter.due_after {
            tasks.retain(|t| t.due.map_or(false, |d| d >= *due_after));
        }
        if let Some(ref search) = filter.search {
            let search_lower = search.to_lowercase();
            tasks.retain(|t| t.title.to_lowercase().contains(&search_lower));
        }

        Ok(tasks)
    }

    async fn create_task(&self, task: &NewTask) -> Result<Task> {
        let priority = dewey_priority_to_linear(task.priority);
        let due_str = task.due.map(|d| d.format("%Y-%m-%d").to_string());

        let label_ids = self.resolve_label_ids(&task.tags).await?;

        let project_id = if let Some(ref name) = task.project {
            self.resolve_project_id(name).await?
        } else {
            None
        };

        let body = graphql::create_issue_mutation(
            &self.config.team_id,
            &task.title,
            priority,
            due_str.as_deref(),
            Some(&self.config.user_id),
            &label_ids,
            project_id.as_deref(),
        );

        let resp = self.graphql(body).await?;

        let issue = resp
            .pointer("/data/issueCreate/issue")
            .ok_or_else(|| DeweyError::Backend {
                backend: "Linear".into(),
                message: "Create mutation did not return an issue".into(),
            })?;

        self.parse_issue(issue).ok_or_else(|| DeweyError::Backend {
            backend: "Linear".into(),
            message: "Failed to parse created issue".into(),
        })
    }

    async fn update_task(&self, id: &TaskId, update: &TaskUpdate) -> Result<Task> {
        let issue_id = self.resolve_issue_id(id).await?;
        let mut input = Map::new();

        if let Some(ref title) = update.title {
            input.insert("title".into(), json!(title));
        }
        if let Some(ref priority) = update.priority {
            input.insert(
                "priority".into(),
                json!(dewey_priority_to_linear(*priority)),
            );
        }
        if let Some(ref due) = update.due {
            match due {
                Some(date) => {
                    input.insert("dueDate".into(), json!(date.format("%Y-%m-%d").to_string()));
                }
                None => {
                    input.insert("dueDate".into(), Value::Null);
                }
            }
        }
        if let Some(ref status) = update.status {
            match status {
                TaskStatus::Done => {
                    let state_id = self.done_state_id().await?;
                    input.insert("stateId".into(), json!(state_id));
                }
                TaskStatus::Pending => {
                    let state_id = self.todo_state_id().await?;
                    input.insert("stateId".into(), json!(state_id));
                }
            }
        }

        if let Some(ref desc) = update.description {
            match desc {
                Some(text) => input.insert("description".into(), json!(text)),
                None => input.insert("description".into(), Value::Null),
            };
        }
        if let Some(ref name) = update.state_name {
            let state_id = self.state_id_by_name(name).await?;
            input.insert("stateId".into(), json!(state_id));
        }

        if let Some(ref proj) = update.project {
            match proj {
                Some(name) => {
                    if let Some(id) = self.resolve_project_id(name).await? {
                        input.insert("projectId".into(), json!(id));
                    }
                }
                None => {
                    input.insert("projectId".into(), Value::Null);
                }
            }
        }

        if let Some(ref tag_names) = update.tags {
            let label_ids = self.resolve_label_ids(tag_names).await?;
            input.insert("labelIds".into(), json!(label_ids));
        }

        if input.is_empty() {
            warn!("update_task called with no fields to update for {id}");
        }

        let body = graphql::update_issue_mutation(&issue_id, &input);
        let resp = self.graphql(body).await?;

        let issue = resp
            .pointer("/data/issueUpdate/issue")
            .ok_or_else(|| DeweyError::Backend {
                backend: "Linear".into(),
                message: "Update mutation did not return an issue".into(),
            })?;

        self.parse_issue(issue).ok_or_else(|| DeweyError::Backend {
            backend: "Linear".into(),
            message: "Failed to parse updated issue".into(),
        })
    }

    async fn complete_task(&self, id: &TaskId) -> Result<()> {
        let issue_id = self.resolve_issue_id(id).await?;
        let state_id = self.done_state_id().await?;

        let mut input = Map::new();
        input.insert("stateId".into(), json!(state_id));

        let body = graphql::update_issue_mutation(&issue_id, &input);
        let resp = self.graphql(body).await?;

        let success = resp
            .pointer("/data/issueUpdate/success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !success {
            return Err(DeweyError::Backend {
                backend: "Linear".into(),
                message: format!("Failed to complete issue {id}"),
            });
        }

        Ok(())
    }

    async fn uncomplete_task(&self, id: &TaskId) -> Result<()> {
        let issue_id = self.resolve_issue_id(id).await?;
        let state_id = self.todo_state_id().await?;

        let mut input = Map::new();
        input.insert("stateId".into(), json!(state_id));

        let body = graphql::update_issue_mutation(&issue_id, &input);
        let resp = self.graphql(body).await?;

        let success = resp
            .pointer("/data/issueUpdate/success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !success {
            return Err(DeweyError::Backend {
                backend: "Linear".into(),
                message: format!("Failed to uncomplete issue {id}"),
            });
        }

        Ok(())
    }

    async fn fetch_project_names(&self) -> Result<Vec<String>> {
        let body = graphql::team_projects_query(&self.config.team_id);
        let resp = self.graphql(body).await?;

        let empty = vec![];
        let nodes = resp
            .pointer("/data/team/projects/nodes")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        let names: Vec<String> = nodes
            .iter()
            .filter_map(|n| n.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();

        Ok(names)
    }

    async fn delete_task(&self, id: &TaskId) -> Result<()> {
        let issue_id = self.resolve_issue_id(id).await?;

        let body = graphql::archive_issue_mutation(&issue_id);
        let resp = self.graphql(body).await?;

        let success = resp
            .pointer("/data/issueArchive/success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !success {
            return Err(DeweyError::Backend {
                backend: "Linear".into(),
                message: format!("Failed to archive issue {id}"),
            });
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> LinearConfig {
        LinearConfig {
            name: "linear".into(),
            api_key: "lin_api_test".into(),
            team_id: "team-uuid".into(),
            team_name: "Engineering".into(),
            assignee: "me".into(),
            user_id: "user-uuid".into(),
            filter_status: vec!["In Progress".into(), "Todo".into()],
        }
    }

    fn sample_issue_node() -> Value {
        json!({
            "id": "issue-uuid-1",
            "identifier": "ENG-42",
            "title": "Fix login bug",
            "description": "Users cannot log in with SSO",
            "priority": 2,
            "priorityLabel": "High",
            "dueDate": "2026-03-15",
            "createdAt": "2026-02-20T10:30:00.000Z",
            "completedAt": null,
            "url": "https://linear.app/team/issue/ENG-42",
            "branchName": "fix-login-bug",
            "state": { "name": "In Progress", "type": "started" },
            "labels": { "nodes": [
                { "name": "bug" },
                { "name": "auth" }
            ]},
            "project": { "name": "Q1 Sprint" },
            "assignee": { "id": "user-uuid", "name": "Test User" }
        })
    }

    // -- parse_issue tests --------------------------------------------------

    #[test]
    fn parse_issue_basic() {
        let backend = LinearBackend::new(sample_config());
        let node = sample_issue_node();
        let task = backend.parse_issue(&node).unwrap();

        assert_eq!(task.id, "linear:ENG-42");
        assert_eq!(task.title, "ENG-42 Fix login bug");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.priority, Priority::High);
        assert_eq!(
            task.due,
            Some(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap())
        );
        assert_eq!(task.tags, vec!["bug".to_string(), "auth".to_string()]);
        assert_eq!(task.source, BackendSource::Linear);
        assert!(task.source_line.is_none());
        assert_eq!(
            task.source_path,
            Some("https://linear.app/team/issue/ENG-42".into())
        );
        assert!(task.created_at.is_some());
        assert!(task.completed_at.is_none());
        assert_eq!(task.project, Some("Q1 Sprint".to_string()));
    }

    #[test]
    fn parse_issue_completed_status() {
        let backend = LinearBackend::new(sample_config());
        let mut node = sample_issue_node();
        node["state"] = json!({ "name": "Done", "type": "completed" });
        node["completedAt"] = json!("2026-02-25T14:00:00.000Z");

        let task = backend.parse_issue(&node).unwrap();
        assert_eq!(task.status, TaskStatus::Done);
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn parse_issue_canceled_status() {
        let backend = LinearBackend::new(sample_config());
        let mut node = sample_issue_node();
        node["state"] = json!({ "name": "Canceled", "type": "canceled" });

        let task = backend.parse_issue(&node).unwrap();
        assert_eq!(task.status, TaskStatus::Done);
    }

    #[test]
    fn parse_issue_priority_mapping() {
        let backend = LinearBackend::new(sample_config());

        let test_cases = vec![
            (1, Priority::High),   // Urgent → High
            (2, Priority::High),   // High → High
            (3, Priority::Medium), // Medium → Medium
            (4, Priority::Low),    // Low → Low
            (0, Priority::None),   // None → None
        ];

        for (linear_priority, expected) in test_cases {
            let mut node = sample_issue_node();
            node["priority"] = json!(linear_priority);
            let task = backend.parse_issue(&node).unwrap();
            assert_eq!(
                task.priority, expected,
                "Linear priority {linear_priority} should map to {expected:?}"
            );
        }
    }

    #[test]
    fn parse_issue_no_due_date() {
        let backend = LinearBackend::new(sample_config());
        let mut node = sample_issue_node();
        node["dueDate"] = Value::Null;

        let task = backend.parse_issue(&node).unwrap();
        assert!(task.due.is_none());
    }

    #[test]
    fn parse_issue_no_labels() {
        let backend = LinearBackend::new(sample_config());
        let mut node = sample_issue_node();
        node["labels"] = json!({ "nodes": [] });

        let task = backend.parse_issue(&node).unwrap();
        assert!(task.tags.is_empty());
    }

    #[test]
    fn parse_issue_missing_identifier_returns_none() {
        let backend = LinearBackend::new(sample_config());
        let node = json!({
            "id": "issue-uuid",
            "title": "No identifier"
        });

        assert!(backend.parse_issue(&node).is_none());
    }

    #[test]
    fn parse_issue_missing_title_returns_none() {
        let backend = LinearBackend::new(sample_config());
        let node = json!({
            "id": "issue-uuid",
            "identifier": "ENG-1"
        });

        assert!(backend.parse_issue(&node).is_none());
    }

    // -- priority mapping tests ---------------------------------------------

    #[test]
    fn priority_roundtrip() {
        // Dewey → Linear → Dewey should be stable.
        for priority in [Priority::High, Priority::Medium, Priority::Low, Priority::None] {
            let linear = dewey_priority_to_linear(priority);
            let back = linear_priority_to_dewey(linear as i64);
            assert_eq!(
                priority, back,
                "Priority roundtrip failed for {priority:?}"
            );
        }
    }

    #[test]
    fn dewey_to_linear_priority_values() {
        assert_eq!(dewey_priority_to_linear(Priority::High), 2);
        assert_eq!(dewey_priority_to_linear(Priority::Medium), 3);
        assert_eq!(dewey_priority_to_linear(Priority::Low), 4);
        assert_eq!(dewey_priority_to_linear(Priority::None), 0);
    }

    // -- LinearConfig::from_table tests -------------------------------------

    #[test]
    fn config_not_enabled_returns_none() {
        let table: toml::Table = toml::from_str(
            r#"
            enabled = false
            api_key = "lin_api_xyz"
        "#,
        )
        .unwrap();

        assert!(LinearConfig::from_table(&table).unwrap().is_none());
    }

    #[test]
    fn config_enabled_no_api_key_returns_none() {
        let table: toml::Table = toml::from_str(
            r#"
            enabled = true
        "#,
        )
        .unwrap();

        assert!(LinearConfig::from_table(&table).unwrap().is_none());
    }

    #[test]
    fn config_enabled_empty_api_key_returns_none() {
        let table: toml::Table = toml::from_str(
            r#"
            enabled = true
            api_key = ""
        "#,
        )
        .unwrap();

        assert!(LinearConfig::from_table(&table).unwrap().is_none());
    }

    #[test]
    fn config_valid_full() {
        let table: toml::Table = toml::from_str(
            r#"
            enabled = true
            api_key = "lin_api_test123"
            team_id = "team-uuid"
            team_name = "Engineering"
            assignee = "me"
            user_id = "user-uuid"
            filter_status = ["In Progress", "Todo", "Backlog"]
        "#,
        )
        .unwrap();

        let config = LinearConfig::from_table(&table).unwrap().unwrap();
        assert_eq!(config.name, "linear");
        assert_eq!(config.api_key, "lin_api_test123");
        assert_eq!(config.team_id, "team-uuid");
        assert_eq!(config.team_name, "Engineering");
        assert_eq!(config.assignee, "me");
        assert_eq!(config.user_id, "user-uuid");
        assert_eq!(config.filter_status, vec!["In Progress", "Todo", "Backlog"]);
    }

    #[test]
    fn config_named_table() {
        let table: toml::Table = toml::from_str(
            r#"
            enabled = true
            api_key = "lin_api_work"
            team_id = "team-work"
            team_name = "Work"
        "#,
        )
        .unwrap();

        let config = LinearConfig::from_named_table("work", &table).unwrap().unwrap();
        assert_eq!(config.name, "work");
        assert_eq!(config.api_key, "lin_api_work");
    }

    #[test]
    fn config_defaults_applied() {
        let table: toml::Table = toml::from_str(
            r#"
            enabled = true
            api_key = "lin_api_key"
        "#,
        )
        .unwrap();

        let config = LinearConfig::from_table(&table).unwrap().unwrap();
        assert_eq!(config.assignee, "me");
        assert_eq!(config.team_id, "");
        assert_eq!(config.team_name, "");
        assert_eq!(config.user_id, "");
        assert_eq!(
            config.filter_status,
            vec!["In Progress", "Todo", "Backlog"]
        );
    }

    #[test]
    fn config_missing_enabled_defaults_to_false() {
        let table: toml::Table = toml::from_str(
            r#"
            api_key = "lin_api_key"
        "#,
        )
        .unwrap();

        assert!(LinearConfig::from_table(&table).unwrap().is_none());
    }
}
