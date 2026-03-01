use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use tracing::info;

mod agent;
mod backends;
mod cli;
mod config;
mod error;
mod model;
mod nlp;
mod tui;
mod waybar;

use backends::BackendManager;
use cli::{AgentCommand, Cli, Command};
use config::Config;
use error::{Result, DeweyError};
use model::{NewTask, Priority, TaskFilter, TaskStatus};
use nlp::parse_quick_add;

const NO_BACKENDS_MSG: &str = "No backends enabled.\n\nCreate ~/.config/dewey/config.toml with:\n\n[backends.local]\nenabled = true\n\nTasks are stored in ~/.dewey/todo.txt by default.";

fn setup_logging(verbose: u8) {
    let filter = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    setup_logging(cli.verbose);

    info!("Starting dewey v0.0.1");

    let config = match Config::load(cli.config.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            return ExitCode::from(1);
        }
    };

    // TTY = TUI, non-TTY = Waybar
    let is_tty = atty::is(atty::Stream::Stdout);
    let command = cli.command.unwrap_or_else(|| {
        if is_tty {
            Command::Tui
        } else {
            Command::Waybar
        }
    });

    match run(command, config).await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}

async fn run(command: Command, config: Config) -> Result<()> {
    match command {
        Command::Waybar => {
            let backend_manager = BackendManager::from_config(&config)?;
            waybar::output(&backend_manager, &config).await?;
        }
        Command::Tui => {
            let backend_manager = BackendManager::from_config(&config)?;

            // Allow TUI to start even with zero backends — the user can
            // press 'L' to set up the Linear backend from inside the TUI.
            tui::run(backend_manager, config).await?;
        }
        Command::Add { text } => {
            let task_text = text.join(" ");
            let backend_manager = BackendManager::from_config(&config)?;

            if backend_manager.is_empty() {
                return Err(DeweyError::Config(NO_BACKENDS_MSG.into()));
            }

            let keys = backend_manager.backend_keys();
            let key_refs: Vec<&str> = keys.iter().copied().collect();
            let (title, priority, due, tags, backend, project) =
                parse_quick_add(&task_text, config.general.default_backend.as_deref(), &key_refs)?;

            let new_task = NewTask {
                title,
                priority,
                due,
                tags,
                backend,
                project,
            };

            let task = backend_manager.create_task(&new_task).await?;
            println!("✓ Created task: {} (ID: {})", task.title, task.id);
        }
        Command::List { filter, format } => {
            let backend_manager = BackendManager::from_config(&config)?;

            if backend_manager.is_empty() {
                return Err(DeweyError::Config(NO_BACKENDS_MSG.into()));
            }

            let task_filter = match filter.as_str() {
                "today" => TaskFilter {
                    status: Some(TaskStatus::Pending),
                    due_before: Some(chrono::Local::now().date_naive()),
                    ..Default::default()
                },
                "upcoming" => TaskFilter {
                    status: Some(TaskStatus::Pending),
                    due_after: Some(chrono::Local::now().date_naive() + chrono::Duration::days(1)),
                    ..Default::default()
                },
                "all" => TaskFilter::default(),
                "done" => TaskFilter {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                },
                _ => TaskFilter::default(),
            };

            let tasks = backend_manager.all_tasks(&task_filter).await?;

            match format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&tasks)?;
                    println!("{}", json);
                }
                _ => {
                    if tasks.is_empty() {
                        println!("No tasks found.");
                    } else {
                        for task in tasks {
                            let icon = match task.status {
                                TaskStatus::Pending => "☐",
                                TaskStatus::Done => "✓",
                            };
                            let due_str = task
                                .due
                                .map(|d| format!(" (due {})", d))
                                .unwrap_or_default();
                            let priority_str = match task.priority {
                                Priority::High => " [!]",
                                Priority::Medium => "",
                                Priority::Low => "",
                                Priority::None => "",
                            };
                            println!("{} {}{}{}", icon, task.title, due_str, priority_str);
                        }
                    }
                }
            }
        }
        Command::Config => {
            let config_toml = toml::to_string_pretty(&config).map_err(|e| {
                error::DeweyError::Config(format!("Failed to serialize config: {}", e))
            })?;
            println!("{}", config_toml);
        }
        Command::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
        }
        Command::Setup { backend } => {
            match backend.as_str() {
                "linear" => {
                    println!("To set up Linear, add this to your config (~/.config/dewey/config.toml):\n");
                    println!("[backends.linear]");
                    println!("enabled = true\n");
                    println!("Then run `dewey tui` — the setup wizard will guide you through connecting to Linear.");
                }
                _ => {
                    eprintln!("Unknown backend: {backend}. Available backends: linear");
                    std::process::exit(1);
                }
            }
        }
        Command::Agent { command: agent_cmd } => {
            match agent_cmd {
                AgentCommand::Status => {
                    let agents = crate::agent::list_running_agents();
                    if agents.is_empty() {
                        println!("No background agents running.");
                    } else {
                        println!("Running agents:\n");
                        for agent in &agents {
                            println!("  {} — PID {} — log: {}", agent.task_id, agent.pid, agent.log_path.display());
                        }
                    }
                }
                AgentCommand::Logs { issue_id } => {
                    let dir = crate::agent::agent_dir();
                    // Try both with and without linear prefix
                    let candidates = vec![
                        format!("linear-{}", issue_id),
                        issue_id.replace(':', "-"),
                        issue_id.clone(),
                    ];

                    let mut found = false;
                    for safe_id in &candidates {
                        let log_path = dir.join(format!("{safe_id}.log"));
                        if log_path.exists() {
                            let content = std::fs::read_to_string(&log_path)?;
                            print!("{content}");
                            found = true;
                            break;
                        }
                    }

                    if !found {
                        eprintln!("No log found for '{issue_id}'. Check `dewey agent status` for running agents.");
                        std::process::exit(1);
                    }
                }
            }
        }
    }

    Ok(())
}
