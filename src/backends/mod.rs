use async_trait::async_trait;

use crate::error::Result;
use crate::model::{BackendSource, NewTask, Task, TaskFilter, TaskId, TaskUpdate};

pub mod obsidian;
pub mod localfile;
pub mod linear;

#[async_trait]
pub trait TaskBackend: Send + Sync {
    fn name(&self) -> &str;
    fn source(&self) -> BackendSource;

    /// Unique key used for task ID prefixes and routing.
    /// Defaults to the source name but can be overridden
    /// (e.g. named Linear backends use their config name).
    fn key(&self) -> &str {
        match self.source() {
            BackendSource::Obsidian => "obsidian",
            BackendSource::LocalFile => "local",
            BackendSource::Linear => "linear",
        }
    }

    async fn fetch_tasks(&self, filter: &TaskFilter) -> Result<Vec<Task>>;
    async fn create_task(&self, task: &NewTask) -> Result<Task>;
    async fn update_task(&self, id: &TaskId, update: &TaskUpdate) -> Result<Task>;
    async fn complete_task(&self, id: &TaskId) -> Result<()>;
    async fn uncomplete_task(&self, id: &TaskId) -> Result<()>;
    async fn delete_task(&self, id: &TaskId) -> Result<()>;

    /// Fetch project names available for this backend (e.g. for cycle selectors).
    /// Returns an empty list by default (backends without projects).
    async fn fetch_project_names(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}

pub struct BackendManager {
    backends: Vec<Box<dyn TaskBackend>>,
}

impl BackendManager {
    pub fn new(backends: Vec<Box<dyn TaskBackend>>) -> Self {
        Self { backends }
    }

    pub fn from_config(config: &crate::config::Config) -> Result<Self> {
        let mut backends: Vec<Box<dyn TaskBackend>> = Vec::new();

        if let Some(ref table) = config.backends.local {
            if table.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                let local_config = localfile::LocalFileConfig::from_table(table)?;
                backends.push(Box::new(localfile::LocalFileBackend::new(local_config)));
            }
        }

        if let Some(ref table) = config.backends.obsidian {
            if table.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                let obs_config = obsidian::ObsidianConfig::from_table(table)?;
                backends.push(Box::new(obsidian::ObsidianBackend::new(obs_config)));
            }
        }

        // Linear backend(s)
        //
        // Supports two formats:
        //   Old (single): [backends.linear] with scalar fields like api_key
        //   New (multi):  [backends.linear.work], [backends.linear.personal], etc.
        if let Some(ref table) = config.backends.linear {
            let is_old_format = table.get("api_key").map_or(false, |v| v.is_str())
                || table.get("enabled").map_or(false, |v| v.is_bool());

            if is_old_format {
                // Single Linear backend (backward compat)
                match linear::LinearConfig::from_named_table("linear", table)? {
                    Some(linear_config) => {
                        tracing::info!("Linear backend enabled for team: {}", linear_config.team_name);
                        backends.push(Box::new(linear::LinearBackend::new(linear_config)));
                    }
                    None => {
                        tracing::debug!("Linear backend enabled but not yet configured (needs setup)");
                    }
                }
            } else {
                // Multiple named Linear backends
                for (name, value) in table {
                    if let Some(subtable) = value.as_table() {
                        match linear::LinearConfig::from_named_table(name, subtable)? {
                            Some(linear_config) => {
                                tracing::info!("Linear backend '{}' enabled for team: {}", name, linear_config.team_name);
                                backends.push(Box::new(linear::LinearBackend::new(linear_config)));
                            }
                            None => {
                                tracing::debug!("Linear backend '{}' not yet configured (needs setup)", name);
                            }
                        }
                    }
                }
            }
        }

        Ok(Self::new(backends))
    }

    pub async fn all_tasks(&self, filter: &TaskFilter) -> Result<Vec<Task>> {
        use futures::future::join_all;
        use tracing::error;

        let futures: Vec<_> = self.backends.iter()
            .map(|backend| async move {
                let result = backend.fetch_tasks(filter).await;
                (backend.name(), result)
            })
            .collect();

        let results = join_all(futures).await;
        let mut all_tasks = Vec::new();
        let mut errors = Vec::new();

        for (name, result) in results {
            match result {
                Ok(tasks) => {
                    all_tasks.extend(tasks);
                }
                Err(e) => {
                    error!("Backend '{}' error: {}", name, e);
                    errors.push((name, e));
                }
            }
        }

        if !errors.is_empty() && all_tasks.is_empty() {
            return Err(crate::error::DeweyError::Backend {
                backend: errors[0].0.to_string(),
                message: format!("{}", errors[0].1),
            });
        }

        // Sort: overdue first, then due date, then priority, then title
        all_tasks.sort_by(|a, b| {
            use chrono::Local;
            let today = Local::now().date_naive();

            let score_a = match a.due {
                Some(d) if d < today => 0,
                Some(d) if d == today => 1,
                Some(_) => 2,
                None => 3,
            };
            let score_b = match b.due {
                Some(d) if d < today => 0,
                Some(d) if d == today => 1,
                Some(_) => 2,
                None => 3,
            };

            let due_cmp = score_a.cmp(&score_b);
            if due_cmp != std::cmp::Ordering::Equal {
                return due_cmp;
            }

            let date_cmp = match (a.due, b.due) {
                (Some(da), Some(db)) => da.cmp(&db),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            if date_cmp != std::cmp::Ordering::Equal {
                return date_cmp;
            }

            let priority_cmp = b.priority.cmp(&a.priority);
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }

            a.title.cmp(&b.title)
        });

        Ok(all_tasks)
    }

    pub async fn create_task(&self, task: &NewTask) -> Result<Task> {
        for backend in &self.backends {
            if backend.key() == task.backend {
                return backend.create_task(task).await;
            }
        }

        if let Some(backend) = self.backends.first() {
            return backend.create_task(task).await;
        }

        Err(crate::error::DeweyError::Backend {
            backend: "none".to_string(),
            message: "No backends configured".to_string(),
        })
    }

    pub async fn complete_task(&self, id: &TaskId) -> Result<()> {
        let prefix = id.split(':').next().unwrap_or("");

        for backend in &self.backends {
            if backend.key() == prefix {
                return backend.complete_task(id).await;
            }
        }

        Err(crate::error::DeweyError::Parse(format!(
            "No backend found for task ID: {}",
            id
        )))
    }

    pub async fn uncomplete_task(&self, id: &TaskId) -> Result<()> {
        let prefix = id.split(':').next().unwrap_or("");

        for backend in &self.backends {
            if backend.key() == prefix {
                return backend.uncomplete_task(id).await;
            }
        }

        Err(crate::error::DeweyError::Parse(format!(
            "No backend found for task ID: {}",
            id
        )))
    }

    pub async fn update_task(&self, id: &TaskId, update: &crate::model::TaskUpdate) -> Result<crate::model::Task> {
        let prefix = id.split(':').next().unwrap_or("");

        for backend in &self.backends {
            if backend.key() == prefix {
                return backend.update_task(id, update).await;
            }
        }

        Err(crate::error::DeweyError::Parse(format!(
            "No backend found for task ID: {}",
            id
        )))
    }

    pub async fn delete_task(&self, id: &TaskId) -> Result<()> {
        let prefix = id.split(':').next().unwrap_or("");

        for backend in &self.backends {
            if backend.key() == prefix {
                return backend.delete_task(id).await;
            }
        }

        Err(crate::error::DeweyError::Parse(format!(
            "No backend found for task ID: {}",
            id
        )))
    }

    pub async fn fetch_project_names(&self, task_id: &TaskId) -> Result<Vec<String>> {
        let prefix = task_id.split(':').next().unwrap_or("");
        for backend in &self.backends {
            if backend.key() == prefix {
                return backend.fetch_project_names().await;
            }
        }
        Ok(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    pub fn backend_keys(&self) -> Vec<&str> {
        self.backends.iter().map(|b| b.key()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn from_config_old_single_linear_format() {
        let config: Config = toml::from_str(
            r#"
[backends.linear]
enabled = true
api_key = "lin_api_test"
team_id = "TEAM-1"
team_name = "Engineering"
user_id = "user-1"
"#,
        )
        .unwrap();

        let manager = BackendManager::from_config(&config).unwrap();
        assert_eq!(manager.backend_keys(), vec!["linear"]);
    }

    #[test]
    fn from_config_multi_linear_format() {
        let config: Config = toml::from_str(
            r#"
[backends.linear.work]
enabled = true
api_key = "lin_api_work"
team_id = "TEAM-WORK"
team_name = "Work"
user_id = "user-1"

[backends.linear.personal]
enabled = true
api_key = "lin_api_personal"
team_id = "TEAM-PERSONAL"
team_name = "Personal"
user_id = "user-2"
"#,
        )
        .unwrap();

        let manager = BackendManager::from_config(&config).unwrap();
        let mut keys = manager.backend_keys();
        keys.sort();
        assert_eq!(keys, vec!["personal", "work"]);
    }

    #[test]
    fn from_config_mixed_backends() {
        let config: Config = toml::from_str(
            r#"
[backends.local]
enabled = true

[backends.linear]
enabled = true
api_key = "lin_api_test"
team_id = "TEAM-1"
team_name = "Engineering"
user_id = "user-1"
"#,
        )
        .unwrap();

        let manager = BackendManager::from_config(&config).unwrap();
        let keys = manager.backend_keys();
        assert!(keys.contains(&"local"));
        assert!(keys.contains(&"linear"));
    }
}
