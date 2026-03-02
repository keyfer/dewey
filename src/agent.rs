use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use tracing::{info, warn};

use crate::error::Result;
use crate::model::Task;

/// Represents a running background agent process.
#[derive(Debug, Clone)]
pub struct RunningAgent {
    pub task_id: String,
    pub pid: u32,
    pub log_path: PathBuf,
}

/// Returns the agent command from config, defaulting to "claude".
pub fn agent_command(config: &Option<toml::Table>) -> String {
    config
        .as_ref()
        .and_then(|t| t.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("claude")
        .to_string()
}

/// Builds a prompt string with task context for the AI agent.
pub fn build_agent_prompt(task: &Task) -> String {
    let identifier = task.id.strip_prefix("linear:").unwrap_or(&task.id);
    let git_branch = identifier.to_lowercase();

    let mut lines = Vec::new();

    lines.push(format!(
        "You are working on Linear issue {}: \"{}\"",
        identifier, task.title
    ));
    lines.push(String::new()); // blank line

    if let Some(ref source_path) = task.source_path {
        lines.push(format!("Linear URL: {}", source_path));
    }

    if !task.tags.is_empty() {
        lines.push(format!("Labels: {}", task.tags.join(", ")));
    }

    if let Some(due) = task.due {
        lines.push(format!("Due: {}", due));
    }

    lines.push(format!("Git branch: {}", git_branch));
    lines.push(String::new()); // blank line

    lines.push(
        "Work on this task. Check out the git branch if it exists, or create it if needed."
            .to_string(),
    );

    lines.join("\n")
}

/// Spawns the agent command as an interactive process.
pub fn launch_interactive(task: &Task, agent_cmd: &str) -> Result<()> {
    let prompt = build_agent_prompt(task);

    let status = std::process::Command::new(agent_cmd)
        .arg("-p")
        .arg(&prompt)
        .status();

    match status {
        Ok(s) if !s.success() => {
            warn!(
                "Agent process exited with code {}",
                s.code().unwrap_or(-1)
            );
        }
        Err(e) => {
            warn!("Failed to launch agent: {}", e);
        }
        _ => {}
    }

    Ok(())
}

/// Returns the directory for agent state files: `~/.dewey/agents/`.
pub fn agent_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".dewey")
        .join("agents")
}

/// Sanitizes a task ID for use as a filename by replacing `:` and `/` with `-`.
pub(crate) fn sanitize_id(task_id: &str) -> String {
    task_id.replace([':', '/'], "-")
}

/// Spawns the agent command as a detached background process.
///
/// Creates log and PID files under `~/.dewey/agents/` and returns
/// a `RunningAgent` with the process details.
pub fn launch_background(task: &Task, agent_cmd: &str) -> Result<RunningAgent> {
    let dir = agent_dir();
    fs::create_dir_all(&dir)?;

    let safe_id = sanitize_id(&task.id);
    let log_path = dir.join(format!("{}.log", safe_id));
    let pid_path = dir.join(format!("{}.pid", safe_id));

    let prompt = build_agent_prompt(task);

    let log_file = fs::File::create(&log_path)?;
    let log_file_clone = log_file.try_clone()?;

    let child = Command::new(agent_cmd)
        .arg("-p")
        .arg(&prompt)
        .arg("--output-format")
        .arg("json")
        .stdout(log_file)
        .stderr(log_file_clone)
        .stdin(Stdio::null())
        .spawn()?;

    let pid = child.id();
    fs::write(&pid_path, pid.to_string())?;

    info!(
        "Background agent started for {} (PID {}), log: {}",
        task.id,
        pid,
        log_path.display()
    );

    Ok(RunningAgent {
        task_id: task.id.clone(),
        pid,
        log_path,
    })
}

/// Scans the agent directory for `.pid` files and returns a list of
/// currently running agents. Stale PID files (where the process is no
/// longer alive) are cleaned up automatically.
pub fn list_running_agents() -> Vec<RunningAgent> {
    let dir = agent_dir();
    let mut agents = Vec::new();

    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return agents,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pid") {
            continue;
        }

        let pid_str = match fs::read_to_string(&path) {
            Ok(s) => s.trim().to_string(),
            Err(_) => continue,
        };

        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let proc_path = PathBuf::from(format!("/proc/{}", pid));
        if proc_path.exists() {
            // Process is alive — derive the task_id from the filename.
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let log_path = dir.join(format!("{}.log", stem));

            agents.push(RunningAgent {
                task_id: stem,
                pid,
                log_path,
            });
        } else {
            // Process is dead — clean up the stale PID file.
            let _ = fs::remove_file(&path);
        }
    }

    agents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BackendSource, Priority, TaskStatus};
    use chrono::NaiveDate;

    fn full_task() -> Task {
        Task {
            id: "linear:PROJ-123".to_string(),
            title: "Implement agent module".to_string(),
            status: TaskStatus::Pending,
            priority: Priority::High,
            due: Some(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()),
            tags: vec!["backend".to_string(), "agent".to_string()],
            source: BackendSource::Linear,
            backend_key: "linear".to_string(),
            source_line: None,
            source_path: Some("https://linear.app/team/issue/PROJ-123".to_string()),
            created_at: None,
            completed_at: None,
            description: None,
            project: None,
            state_name: None,
        }
    }

    fn minimal_task() -> Task {
        Task {
            id: "linear:PROJ-456".to_string(),
            title: "Fix a bug".to_string(),
            status: TaskStatus::Pending,
            priority: Priority::None,
            due: None,
            tags: vec![],
            source: BackendSource::Linear,
            backend_key: "linear".to_string(),
            source_line: None,
            source_path: None,
            created_at: None,
            completed_at: None,
            description: None,
            project: None,
            state_name: None,
        }
    }

    #[test]
    fn build_agent_prompt_full_task() {
        let task = full_task();
        let prompt = build_agent_prompt(&task);

        assert!(prompt.contains("PROJ-123"));
        assert!(prompt.contains("Implement agent module"));
        assert!(prompt.contains("Linear URL: https://linear.app/team/issue/PROJ-123"));
        assert!(prompt.contains("Labels: backend, agent"));
        assert!(prompt.contains("Due: 2026-03-01"));
        assert!(prompt.contains("Git branch: proj-123"));
        assert!(prompt.contains("Work on this task."));
    }

    #[test]
    fn build_agent_prompt_minimal_task() {
        let task = minimal_task();
        let prompt = build_agent_prompt(&task);

        assert!(prompt.contains("PROJ-456"));
        assert!(prompt.contains("Fix a bug"));
        assert!(!prompt.contains("Linear URL:"));
        assert!(!prompt.contains("Labels:"));
        assert!(!prompt.contains("Due:"));
        assert!(prompt.contains("Git branch: proj-456"));
        assert!(prompt.contains("Work on this task."));
    }

    #[test]
    fn agent_command_with_config() {
        let mut table = toml::Table::new();
        table.insert(
            "command".to_string(),
            toml::Value::String("my-agent".to_string()),
        );
        let config = Some(table);

        assert_eq!(agent_command(&config), "my-agent");
    }

    #[test]
    fn agent_command_with_none_config() {
        let config: Option<toml::Table> = None;
        assert_eq!(agent_command(&config), "claude");
    }

    #[test]
    fn agent_command_with_config_missing_command_key() {
        let mut table = toml::Table::new();
        table.insert(
            "enabled".to_string(),
            toml::Value::Boolean(true),
        );
        let config = Some(table);

        assert_eq!(agent_command(&config), "claude");
    }

    #[test]
    fn agent_dir_ends_with_dewey_agents() {
        let dir = agent_dir();
        assert!(dir.ends_with(".dewey/agents"));
    }

    #[test]
    fn sanitize_id_replaces_colons_and_slashes() {
        assert_eq!(sanitize_id("linear:PROJ-123"), "linear-PROJ-123");
        assert_eq!(sanitize_id("a/b:c/d"), "a-b-c-d");
        assert_eq!(sanitize_id("no-special"), "no-special");
    }

    #[test]
    fn list_running_agents_returns_empty_when_no_agents() {
        // The agent dir may not exist or may have no .pid files —
        // either way, list_running_agents should not panic and should
        // return a Vec. In a clean test environment it will be empty.
        let agents = list_running_agents();
        // Verify it returns a valid Vec (we just exercise the function).
        let _ = agents.len();
    }

    #[test]
    fn running_agent_struct_can_be_created_and_cloned() {
        let agent = RunningAgent {
            task_id: "linear:TEST-1".to_string(),
            pid: 12345,
            log_path: PathBuf::from("/tmp/test.log"),
        };

        let cloned = agent.clone();
        assert_eq!(cloned.task_id, "linear:TEST-1");
        assert_eq!(cloned.pid, 12345);
        assert_eq!(cloned.log_path, PathBuf::from("/tmp/test.log"));
    }
}
