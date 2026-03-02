use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;

pub type TaskId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TaskStatus {
    Pending,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Priority {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
}

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub status: TaskStatus,
    pub priority: Priority,
    pub due: Option<NaiveDate>,
    pub tags: Vec<String>,
    pub source: BackendSource,
    pub backend_key: String,
    pub source_line: Option<usize>,
    pub source_path: Option<String>,
    pub created_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub description: Option<String>,
    pub project: Option<String>,
    pub state_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum BackendSource {
    LocalFile,
    Linear,
}

impl BackendSource {
    pub fn name(&self) -> &str {
        match self {
            Self::LocalFile => "local",
            Self::Linear => "linear",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::LocalFile => "■",
            Self::Linear => "◇",
        }
    }
}

pub struct NewTask {
    pub title: String,
    pub priority: Priority,
    pub due: Option<NaiveDate>,
    pub tags: Vec<String>,
    pub backend: String,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskUpdate {
    pub title: Option<String>,
    pub status: Option<TaskStatus>,
    pub priority: Option<Priority>,
    pub due: Option<Option<NaiveDate>>,
    pub tags: Option<Vec<String>>,
    pub description: Option<Option<String>>,
    pub state_name: Option<String>,
    pub project: Option<Option<String>>,
}

#[derive(Default)]
pub struct TaskFilter {
    pub status: Option<TaskStatus>,
    pub due_before: Option<NaiveDate>,
    pub due_after: Option<NaiveDate>,
    pub search: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_source_linear_name_and_icon() {
        let source = BackendSource::Linear;
        assert_eq!(source.name(), "linear");
        assert_eq!(source.icon(), "◇");
    }
}
