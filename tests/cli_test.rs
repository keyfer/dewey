use assert_cmd::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_waybar_outputs_valid_json() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    fs::write(&config_path, "").unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("waybar").arg("--config").arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"text\":"))
        .stdout(predicate::str::contains("\"tooltip\":"));
}

#[test]
fn test_config_command() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    fs::write(&config_path, "[general]\ndefault_view = \"upcoming\"\n").unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("config").arg("--config").arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("default_view = \"upcoming\""));
}

#[test]
fn test_list_command_empty() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let todo_path = temp_dir.path().join("todo.txt");

    fs::write(
        &config_path,
        format!(
            "[backends.local]\nenabled = true\npath = \"{}\"\n",
            todo_path.to_string_lossy()
        ),
    )
    .unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("list").arg("all").arg("--config").arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No tasks found"));
}

#[test]
fn test_list_command_with_tasks() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let todo_path = temp_dir.path().join("todo.txt");

    fs::write(&todo_path, "Test task 1\nTest task 2\n").unwrap();
    fs::write(
        &config_path,
        format!(
            "[backends.local]\nenabled = true\npath = \"{}\"\n",
            todo_path.to_string_lossy()
        ),
    )
    .unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("list").arg("all").arg("--config").arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Test task 1"))
        .stdout(predicate::str::contains("Test task 2"));
}

#[test]
fn test_list_command_json_format() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let todo_path = temp_dir.path().join("todo.txt");

    fs::write(&todo_path, "Test task\n").unwrap();
    fs::write(
        &config_path,
        format!(
            "[backends.local]\nenabled = true\npath = \"{}\"\n",
            todo_path.to_string_lossy()
        ),
    )
    .unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("list")
        .arg("all")
        .arg("--format")
        .arg("json")
        .arg("--config")
        .arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"title\""))
        .stdout(predicate::str::contains("Test task"));
}

#[test]
fn test_waybar_with_todo_txt() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let todo_path = temp_dir.path().join("todo.txt");

    // Use future date to avoid overdue
    let future_date = chrono::Local::now().date_naive() + chrono::Duration::days(30);
    fs::write(
        &todo_path,
        format!("(p1) Test task 1\n(p2) Test task 2 due:{}\n", future_date),
    )
    .unwrap();

    let config = format!(
        "[backends.local]\nenabled = true\npath = \"{}\"\n",
        todo_path.to_string_lossy()
    );
    fs::write(&config_path, config).unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("waybar").arg("--config").arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"text\":\"2\""))
        .stdout(predicate::str::contains("has-tasks"));
}

#[test]
fn test_help_command() {
    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("waybar"))
        .stdout(predicate::str::contains("tui"))
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("list"));
}

#[test]
fn test_no_backends_error() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    fs::write(&config_path, "").unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("list").arg("--config").arg(&config_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No backends enabled"));
}

// ── Config integration tests ────────────────────────────────────────

#[test]
fn test_full_config_with_linear_and_agent() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let todo_path = temp_dir.path().join("todo.txt");

    // Write a valid todo file so the local backend works
    fs::write(&todo_path, "Local task\n").unwrap();

    // Create a complete config file with all sections (local, linear, agent)
    let config_content = format!(
        r#"
[general]
default_view = "upcoming"
theme = "dark"

[waybar]
tooltip_scope = "all"

[backends.local]
enabled = true
path = "{}"

[backends.linear]
enabled = true
api_key = "lin_api_test_integration"
team_id = "TEAM-INT"

[agent]
enabled = true
mode = "interactive"
poll_interval_secs = 600
command = "my-custom-agent"
"#,
        todo_path.to_string_lossy()
    );
    fs::write(&config_path, &config_content).unwrap();

    // Verify the config command can load and print back all sections
    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("config").arg("--config").arg(&config_path);

    cmd.assert()
        .success()
        // general section
        .stdout(predicate::str::contains("default_view = \"upcoming\""))
        .stdout(predicate::str::contains("theme = \"dark\""))
        // waybar section
        .stdout(predicate::str::contains("tooltip_scope = \"all\""))
        // local backend
        .stdout(predicate::str::contains("enabled = true"))
        // linear backend
        .stdout(predicate::str::contains("lin_api_test_integration"))
        .stdout(predicate::str::contains("TEAM-INT"))
        // agent section
        .stdout(predicate::str::contains("mode = \"interactive\""))
        .stdout(predicate::str::contains("poll_interval_secs = 600"))
        .stdout(predicate::str::contains("my-custom-agent"));
}

#[test]
fn test_config_with_only_linear_backend() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[backends.linear]
enabled = true
api_key = "lin_api_only_linear"
"#;
    fs::write(&config_path, config_content).unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("config").arg("--config").arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("lin_api_only_linear"))
        .stdout(predicate::str::contains("[backends.linear]"));
}

#[test]
fn test_config_defaults_applied_when_sections_missing() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Write an empty config — all defaults should be applied
    fs::write(&config_path, "").unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("config").arg("--config").arg(&config_path);

    cmd.assert()
        .success()
        // Default general values
        .stdout(predicate::str::contains("default_view = \"today\""))
        .stdout(predicate::str::contains("theme = \"omarchy\""))
        // Default waybar values
        .stdout(predicate::str::contains("tooltip_scope = \"overdue_today\""));
}

// ── CLI subcommand tests ────────────────────────────────────────────

#[test]
fn test_setup_linear_command() {
    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("setup").arg("linear");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("[backends.linear]"))
        .stdout(predicate::str::contains("enabled = true"))
        .stdout(predicate::str::contains("setup wizard"));
}

#[test]
fn test_setup_unknown_backend_fails() {
    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("setup").arg("unknown");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown backend: unknown"))
        .stderr(predicate::str::contains("Available backends: linear"));
}

#[test]
fn test_agent_status_command() {
    // With no agents running, the status command should print the empty message
    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("agent").arg("status");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No background agents running."));
}

#[test]
fn test_agent_logs_missing_issue() {
    // Requesting logs for a non-existent issue should fail gracefully
    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("agent").arg("logs").arg("NONEXIST-999");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("No log found for"));
}

#[test]
fn test_help_shows_new_subcommands() {
    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("setup"))
        .stdout(predicate::str::contains("agent"));
}

#[test]
fn test_setup_help() {
    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("setup").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("backend"));
}

#[test]
fn test_agent_help() {
    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("agent").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("logs"));
}

// ── Quick-add NLP with @linear integration test ─────────────────────

#[test]
fn test_quick_add_linear_parsing_via_cli() {
    // The "add" command with @linear should attempt to route to the Linear
    // backend. Since no Linear backend is configured, it should fail —
    // but we can verify the NLP parsing works by checking the error mentions
    // "No backends" rather than a parse failure.
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let todo_path = temp_dir.path().join("todo.txt");

    // Provide only a local backend; @linear target has no matching backend
    fs::write(&todo_path, "").unwrap();
    fs::write(
        &config_path,
        format!(
            "[backends.local]\nenabled = true\npath = \"{}\"\n",
            todo_path.to_string_lossy()
        ),
    )
    .unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("add")
        .arg("Fix")
        .arg("bug")
        .arg("(p1)")
        .arg("tomorrow")
        .arg("@linear")
        .arg("--config")
        .arg(&config_path);

    // The NLP parser should succeed and route to Linear, but since there's
    // no Linear backend configured, it falls back to the first available
    // backend (local). The task should be created successfully on the local backend.
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created task"))
        .stdout(predicate::str::contains("Fix bug"));
}

#[test]
fn test_quick_add_local_via_cli() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let todo_path = temp_dir.path().join("todo.txt");

    fs::write(&todo_path, "").unwrap();
    fs::write(
        &config_path,
        format!(
            "[backends.local]\nenabled = true\npath = \"{}\"\n",
            todo_path.to_string_lossy()
        ),
    )
    .unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("add")
        .arg("Buy")
        .arg("milk")
        .arg("(p2)")
        .arg("tomorrow")
        .arg("--config")
        .arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created task"))
        .stdout(predicate::str::contains("Buy milk"));

    // Verify the task was actually written to the todo file
    let content = fs::read_to_string(&todo_path).unwrap();
    assert!(content.contains("Buy milk"));
}

#[test]
fn test_quick_add_with_tags_via_cli() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let todo_path = temp_dir.path().join("todo.txt");

    fs::write(&todo_path, "").unwrap();
    fs::write(
        &config_path,
        format!(
            "[backends.local]\nenabled = true\npath = \"{}\"\n",
            todo_path.to_string_lossy()
        ),
    )
    .unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("add")
        .arg("Review")
        .arg("PR")
        .arg("#work")
        .arg("(p1)")
        .arg("--config")
        .arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created task"))
        .stdout(predicate::str::contains("Review PR"));
}

// ── Waybar with Linear config (no real API) ─────────────────────────

#[test]
fn test_waybar_with_linear_config_no_api_key() {
    // Linear backend is enabled but has no api_key — the waybar output
    // should still succeed (linear backend skipped because setup incomplete).
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config_content = r#"
[backends.linear]
enabled = true
"#;
    fs::write(&config_path, config_content).unwrap();

    let mut cmd = cargo_bin_cmd!("dewey");
    cmd.arg("waybar").arg("--config").arg(&config_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"text\":"))
        .stdout(predicate::str::contains("\"tooltip\":"));
}
