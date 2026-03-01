use chrono::{Local, NaiveDate};
use serde_json::{json, Value};

use crate::agent::{self, RunningAgent};
use crate::backends::BackendManager;
use crate::config::Config;
use crate::error::Result;
use crate::model::{Task, TaskFilter, TaskStatus};

pub async fn output(backend_manager: &BackendManager, config: &Config) -> Result<()> {
    if backend_manager.is_empty() {
        let output = json!({
            "text": "!",
            "tooltip": "No backends configured.\n\nCreate ~/.config/dewey/config.toml:\n\n[backends.local]\nenabled = true\n\nTasks stored in ~/.dewey/todo.txt",
            "class": "backend-error",
            "alt": "error"
        });
        println!("{}", output);
        return Ok(());
    }

    let filter = TaskFilter {
        status: Some(TaskStatus::Pending),
        ..Default::default()
    };

    let tasks = match backend_manager.all_tasks(&filter).await {
        Ok(t) => t,
        Err(e) => {
            let output = json!({
                "text": "!",
                "tooltip": format!("Error: {}", e),
                "class": "backend-error",
                "alt": "error"
            });
            println!("{}", output);
            return Ok(());
        }
    };

    let running_agents = agent::list_running_agents();
    let output = build_output(&tasks, &config.waybar.tooltip_scope, &running_agents);
    println!("{}", output);
    Ok(())
}

/// Returns true if any running agent matches the given task ID.
///
/// The `RunningAgent.task_id` is derived from the PID filename stem which
/// uses the sanitized form (`:` and `/` replaced with `-`), so we sanitize
/// the task ID before comparing.
fn has_running_agent(task_id: &str, running_agents: &[RunningAgent]) -> bool {
    let sanitized = agent::sanitize_id(task_id);
    running_agents.iter().any(|a| a.task_id == sanitized)
}

fn build_output(tasks: &[Task], tooltip_scope: &str, running_agents: &[RunningAgent]) -> Value {
    let today = Local::now().date_naive();

    let overdue: Vec<&Task> = tasks.iter()
        .filter(|t| t.due.map_or(false, |d| d < today))
        .collect();

    let due_today: Vec<&Task> = tasks.iter()
        .filter(|t| t.due.map_or(false, |d| d == today))
        .collect();

    let due_tomorrow: Vec<&Task> = tasks.iter()
        .filter(|t| t.due.map_or(false, |d| d == today + chrono::Duration::days(1)))
        .collect();

    let mut upcoming_by_day: Vec<(NaiveDate, Vec<&Task>)> = Vec::new();
    for day_offset in 2..=7 {
        let date = today + chrono::Duration::days(day_offset);
        let day_tasks: Vec<&Task> = tasks.iter()
            .filter(|t| t.due == Some(date))
            .collect();
        if !day_tasks.is_empty() {
            upcoming_by_day.push((date, day_tasks));
        }
    }

    let future: Vec<&Task> = tasks.iter()
        .filter(|t| t.due.map_or(false, |d| d > today + chrono::Duration::days(7)))
        .collect();

    let no_due: Vec<&Task> = tasks.iter()
        .filter(|t| t.due.is_none())
        .collect();

    let overdue_count = overdue.len();
    let today_count = due_today.len();
    let tomorrow_count = due_tomorrow.len();
    let upcoming_total: usize = upcoming_by_day.iter().map(|(_, tasks)| tasks.len()).sum();
    let future_count = future.len();
    let no_due_count = no_due.len();
    let total = tasks.len();

    // Smart badge: show the most urgent count
    let (display_text, class) = if overdue_count > 0 {
        (overdue_count.to_string(), "has-overdue")
    } else if today_count > 0 {
        (today_count.to_string(), "has-tasks")
    } else if tomorrow_count > 0 {
        (tomorrow_count.to_string(), "has-tasks")
    } else if upcoming_total > 0 {
        (upcoming_total.to_string(), "has-tasks")
    } else if total > 0 {
        (total.to_string(), "has-tasks")
    } else {
        ("✓".to_string(), "all-done")
    };

    // Append agent running indicator to badge and CSS class
    let agent_count = running_agents.len();
    let display_text = if agent_count > 0 {
        format!("{display_text} ⚡{agent_count}")
    } else {
        display_text
    };
    let class = if agent_count > 0 {
        format!("{class} agent-running")
    } else {
        class.to_string()
    };

    let scope = tooltip_scope;
    let mut tooltip_lines = Vec::new();

    if scope != "today_only" && overdue_count > 0 {
        tooltip_lines.push(format!("Overdue ({}):", overdue_count));
        for task in overdue.iter().take(10) {
            let agent_indicator = if has_running_agent(&task.id, running_agents) { "  ⚡ agent running" } else { "" };
            tooltip_lines.push(format!("  ☐ {} {}{}", task.title, task.source.icon(), agent_indicator));
        }
        if overdue_count > 10 {
            tooltip_lines.push(format!("  ... and {} more", overdue_count - 10));
        }
        tooltip_lines.push(String::new());
    }

    if today_count > 0 {
        tooltip_lines.push(format!("Today ({}):", today_count));
        for task in due_today.iter().take(10) {
            let agent_indicator = if has_running_agent(&task.id, running_agents) { "  ⚡ agent running" } else { "" };
            tooltip_lines.push(format!("  ☐ {} {}{}", task.title, task.source.icon(), agent_indicator));
        }
        if today_count > 10 {
            tooltip_lines.push(format!("  ... and {} more", today_count - 10));
        }
        tooltip_lines.push(String::new());
    }

    if scope == "all" {
        if tomorrow_count > 0 {
            tooltip_lines.push(format!("Tomorrow ({}):", tomorrow_count));
            for task in due_tomorrow.iter().take(5) {
                let agent_indicator = if has_running_agent(&task.id, running_agents) { "  ⚡ agent running" } else { "" };
                tooltip_lines.push(format!("  ☐ {} {}{}", task.title, task.source.icon(), agent_indicator));
            }
            if tomorrow_count > 5 {
                tooltip_lines.push(format!("  ... and {} more", tomorrow_count - 5));
            }
            tooltip_lines.push(String::new());
        }

        for (date, day_tasks) in &upcoming_by_day {
            let day_name = date.format("%A").to_string();
            tooltip_lines.push(format!("{} {} ({}):", day_name, date, day_tasks.len()));
            for task in day_tasks.iter().take(3) {
                let agent_indicator = if has_running_agent(&task.id, running_agents) { "  ⚡ agent running" } else { "" };
                tooltip_lines.push(format!("  ☐ {} {}{}", task.title, task.source.icon(), agent_indicator));
            }
            if day_tasks.len() > 3 {
                tooltip_lines.push(format!("  ... and {} more", day_tasks.len() - 3));
            }
            tooltip_lines.push(String::new());
        }

        if future_count > 0 {
            tooltip_lines.push(format!("Later ({}):", future_count));
            for task in future.iter().take(3) {
                let due_str = task.due.map(|d| d.to_string()).unwrap_or_default();
                let agent_indicator = if has_running_agent(&task.id, running_agents) { "  ⚡ agent running" } else { "" };
                tooltip_lines.push(format!("  ☐ {} ({}) {}{}", task.title, task.source.icon(), due_str, agent_indicator));
            }
            if future_count > 3 {
                tooltip_lines.push(format!("  ... and {} more", future_count - 3));
            }
            tooltip_lines.push(String::new());
        }

        if no_due_count > 0 {
            tooltip_lines.push(format!("No due date ({}):", no_due_count));
            for task in no_due.iter().take(5) {
                let agent_indicator = if has_running_agent(&task.id, running_agents) { "  ⚡ agent running" } else { "" };
                tooltip_lines.push(format!("  ☐ {} {}{}", task.title, task.source.icon(), agent_indicator));
            }
            if no_due_count > 5 {
                tooltip_lines.push(format!("  ... and {} more", no_due_count - 5));
            }
        }
    }

    let summary = if overdue_count > 0 {
        format!("{} overdue · {} today", overdue_count, today_count)
    } else if today_count > 0 {
        format!("{} today", today_count)
    } else if tomorrow_count > 0 {
        format!("{} tomorrow", tomorrow_count)
    } else if upcoming_total > 0 {
        format!("{} upcoming", upcoming_total)
    } else if future_count > 0 {
        format!("{} later", future_count)
    } else if no_due_count > 0 {
        format!("{} tasks", no_due_count)
    } else {
        "All done! ✓".to_string()
    };

    if !tooltip_lines.is_empty() && tooltip_lines.last().unwrap().is_empty() {
        tooltip_lines.pop();
    }
    tooltip_lines.push(String::new());
    tooltip_lines.push(summary);

    let tooltip = tooltip_lines.join("\n");

    json!({
        "text": display_text,
        "tooltip": tooltip,
        "class": class,
        "alt": "tasks"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BackendSource, Priority};
    use chrono::Local;

    fn make_task(title: &str, due: Option<NaiveDate>) -> Task {
        Task {
            id: format!("local:{}", title),
            title: title.to_string(),
            status: TaskStatus::Pending,
            priority: Priority::None,
            due,
            tags: vec![],
            source: BackendSource::LocalFile,
            backend_key: "local".to_string(),
            source_line: None,
            source_path: None,
            created_at: None,
            completed_at: None,
            description: None,
            project: None,
            state_name: None,
        }
    }

    fn today() -> NaiveDate {
        Local::now().date_naive()
    }

    #[test]
    fn test_badge_no_tasks() {
        let output = build_output(&[], "overdue_today", &[]);
        assert_eq!(output["text"], "✓");
        assert_eq!(output["class"], "all-done");
    }

    #[test]
    fn test_badge_only_overdue() {
        let yesterday = today() - chrono::Duration::days(1);
        let tasks = vec![
            make_task("Overdue 1", Some(yesterday)),
            make_task("Overdue 2", Some(yesterday)),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        assert_eq!(output["text"], "2");
        assert_eq!(output["class"], "has-overdue");
    }

    #[test]
    fn test_badge_overdue_plus_no_due() {
        let yesterday = today() - chrono::Duration::days(1);
        let tasks = vec![
            make_task("Overdue 1", Some(yesterday)),
            make_task("Overdue 2", Some(yesterday)),
            make_task("No due 1", None),
            make_task("No due 2", None),
            make_task("No due 3", None),
            make_task("No due 4", None),
            make_task("No due 5", None),
            make_task("No due 6", None),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        // Badge shows overdue count, not total
        assert_eq!(output["text"], "2");
        assert_eq!(output["class"], "has-overdue");
    }

    #[test]
    fn test_badge_only_today() {
        let tasks = vec![
            make_task("Today 1", Some(today())),
            make_task("Today 2", Some(today())),
            make_task("Today 3", Some(today())),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        assert_eq!(output["text"], "3");
        assert_eq!(output["class"], "has-tasks");
    }

    #[test]
    fn test_badge_only_tomorrow() {
        let tomorrow = today() + chrono::Duration::days(1);
        let tasks = vec![make_task("Tomorrow 1", Some(tomorrow))];
        let output = build_output(&tasks, "overdue_today", &[]);
        assert_eq!(output["text"], "1");
        assert_eq!(output["class"], "has-tasks");
    }

    #[test]
    fn test_badge_only_upcoming() {
        let in_3_days = today() + chrono::Duration::days(3);
        let tasks = vec![
            make_task("Upcoming 1", Some(in_3_days)),
            make_task("Upcoming 2", Some(in_3_days)),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        assert_eq!(output["text"], "2");
        assert_eq!(output["class"], "has-tasks");
    }

    #[test]
    fn test_badge_only_future() {
        let in_30_days = today() + chrono::Duration::days(30);
        let tasks = vec![make_task("Future 1", Some(in_30_days))];
        let output = build_output(&tasks, "overdue_today", &[]);
        assert_eq!(output["text"], "1");
        assert_eq!(output["class"], "has-tasks");
    }

    #[test]
    fn test_badge_only_no_due() {
        let tasks = vec![
            make_task("No due 1", None),
            make_task("No due 2", None),
            make_task("No due 3", None),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        // Falls through to total
        assert_eq!(output["text"], "3");
        assert_eq!(output["class"], "has-tasks");
    }

    #[test]
    fn test_badge_cascading_today_beats_tomorrow() {
        let tomorrow = today() + chrono::Duration::days(1);
        let tasks = vec![
            make_task("Today 1", Some(today())),
            make_task("Tomorrow 1", Some(tomorrow)),
            make_task("Tomorrow 2", Some(tomorrow)),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        assert_eq!(output["text"], "1");
    }

    #[test]
    fn test_badge_cascading_overdue_beats_today() {
        let yesterday = today() - chrono::Duration::days(1);
        let tasks = vec![
            make_task("Overdue 1", Some(yesterday)),
            make_task("Today 1", Some(today())),
            make_task("Today 2", Some(today())),
            make_task("Today 3", Some(today())),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        assert_eq!(output["text"], "1");
        assert_eq!(output["class"], "has-overdue");
    }

    // -- Tooltip scope tests --

    #[test]
    fn test_tooltip_scope_all_shows_no_due() {
        let yesterday = today() - chrono::Duration::days(1);
        let tasks = vec![
            make_task("Overdue 1", Some(yesterday)),
            make_task("No due 1", None),
            make_task("No due 2", None),
        ];
        let output = build_output(&tasks, "all", &[]);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(tooltip.contains("Overdue (1):"));
        assert!(tooltip.contains("No due date (2):"));
        assert!(tooltip.contains("No due 1"));
        assert!(tooltip.contains("No due 2"));
    }

    #[test]
    fn test_tooltip_scope_all_shows_tomorrow() {
        let tomorrow = today() + chrono::Duration::days(1);
        let tasks = vec![
            make_task("Tomorrow 1", Some(tomorrow)),
        ];
        let output = build_output(&tasks, "all", &[]);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(tooltip.contains("Tomorrow (1):"));
        assert!(tooltip.contains("Tomorrow 1"));
    }

    #[test]
    fn test_tooltip_scope_all_shows_future() {
        let in_30_days = today() + chrono::Duration::days(30);
        let tasks = vec![
            make_task("Future 1", Some(in_30_days)),
        ];
        let output = build_output(&tasks, "all", &[]);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(tooltip.contains("Later (1):"));
        assert!(tooltip.contains("Future 1"));
    }

    #[test]
    fn test_tooltip_scope_overdue_today_hides_no_due() {
        let tasks = vec![
            make_task("Today 1", Some(today())),
            make_task("No due 1", None),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(tooltip.contains("Today (1):"));
        assert!(!tooltip.contains("No due date"));
    }

    #[test]
    fn test_tooltip_scope_overdue_today_hides_tomorrow() {
        let tomorrow = today() + chrono::Duration::days(1);
        let tasks = vec![
            make_task("Today 1", Some(today())),
            make_task("Tomorrow 1", Some(tomorrow)),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(tooltip.contains("Today (1):"));
        assert!(!tooltip.contains("Tomorrow"));
    }

    #[test]
    fn test_tooltip_scope_today_only_hides_overdue() {
        let yesterday = today() - chrono::Duration::days(1);
        let tasks = vec![
            make_task("Overdue 1", Some(yesterday)),
            make_task("Today 1", Some(today())),
        ];
        let output = build_output(&tasks, "today_only", &[]);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(!tooltip.contains("Overdue"));
        assert!(tooltip.contains("Today (1):"));
    }

    #[test]
    fn test_tooltip_summary_overdue() {
        let yesterday = today() - chrono::Duration::days(1);
        let tasks = vec![
            make_task("Overdue 1", Some(yesterday)),
            make_task("Today 1", Some(today())),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(tooltip.contains("1 overdue · 1 today"));
    }

    #[test]
    fn test_tooltip_summary_all_done() {
        let output = build_output(&[], "overdue_today", &[]);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(tooltip.contains("All done!"));
    }

    // -- Agent indicator tests --

    fn make_running_agent(task_id: &str) -> RunningAgent {
        RunningAgent {
            task_id: task_id.to_string(),
            pid: 12345,
            log_path: std::path::PathBuf::from("/tmp/test.log"),
        }
    }

    #[test]
    fn test_badge_with_running_agents() {
        let tasks = vec![
            make_task("Today 1", Some(today())),
            make_task("Today 2", Some(today())),
        ];
        // Agent task_id is stored in sanitized form (colons replaced with dashes)
        let agents = vec![make_running_agent("local-Today 1")];
        let output = build_output(&tasks, "overdue_today", &agents);
        assert_eq!(output["text"], "2 ⚡1");
        assert_eq!(output["class"], "has-tasks agent-running");
    }

    #[test]
    fn test_badge_with_multiple_running_agents() {
        let tasks = vec![
            make_task("Today 1", Some(today())),
        ];
        let agents = vec![
            make_running_agent("local-Today 1"),
            make_running_agent("linear-OTHER-99"),
        ];
        let output = build_output(&tasks, "overdue_today", &agents);
        assert_eq!(output["text"], "1 ⚡2");
        assert_eq!(output["class"], "has-tasks agent-running");
    }

    #[test]
    fn test_badge_no_agents_no_class_suffix() {
        let tasks = vec![make_task("Today 1", Some(today()))];
        let output = build_output(&tasks, "overdue_today", &[]);
        assert_eq!(output["text"], "1");
        assert_eq!(output["class"], "has-tasks");
        // Should not contain agent-running
        assert!(!output["class"].as_str().unwrap().contains("agent-running"));
    }

    #[test]
    fn test_badge_all_done_with_agents() {
        let agents = vec![make_running_agent("linear-TASK-1")];
        let output = build_output(&[], "overdue_today", &agents);
        assert_eq!(output["text"], "✓ ⚡1");
        assert_eq!(output["class"], "all-done agent-running");
    }

    #[test]
    fn test_tooltip_agent_indicator_on_matching_task() {
        let tasks = vec![
            make_task("Today 1", Some(today())),
            make_task("Today 2", Some(today())),
        ];
        // make_task creates IDs like "local:Today 1", sanitized to "local-Today 1"
        let agents = vec![make_running_agent("local-Today 1")];
        let output = build_output(&tasks, "overdue_today", &agents);
        let tooltip = output["tooltip"].as_str().unwrap();
        // The task with a running agent should show the indicator
        assert!(tooltip.contains("Today 1 ■  ⚡ agent running"));
        // The other task should not
        assert!(!tooltip.contains("Today 2 ■  ⚡ agent running"));
    }

    #[test]
    fn test_tooltip_agent_indicator_no_agents() {
        let tasks = vec![
            make_task("Today 1", Some(today())),
        ];
        let output = build_output(&tasks, "overdue_today", &[]);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(!tooltip.contains("⚡ agent running"));
    }

    #[test]
    fn test_tooltip_agent_indicator_overdue_section() {
        let yesterday = today() - chrono::Duration::days(1);
        let tasks = vec![
            make_task("Overdue 1", Some(yesterday)),
        ];
        let agents = vec![make_running_agent("local-Overdue 1")];
        let output = build_output(&tasks, "overdue_today", &agents);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(tooltip.contains("Overdue 1 ■  ⚡ agent running"));
    }

    #[test]
    fn test_tooltip_agent_indicator_no_due_section() {
        let tasks = vec![
            make_task("No due 1", None),
        ];
        let agents = vec![make_running_agent("local-No due 1")];
        let output = build_output(&tasks, "all", &agents);
        let tooltip = output["tooltip"].as_str().unwrap();
        assert!(tooltip.contains("No due 1 ■  ⚡ agent running"));
    }

    #[test]
    fn test_has_running_agent_with_colon_id() {
        let agents = vec![make_running_agent("linear-MORE-74")];
        assert!(has_running_agent("linear:MORE-74", &agents));
        assert!(!has_running_agent("linear:OTHER-99", &agents));
    }

    #[test]
    fn test_has_running_agent_empty_list() {
        assert!(!has_running_agent("linear:MORE-74", &[]));
    }
}
