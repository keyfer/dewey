use crossterm::{
    event::{self, Event, KeyEvent},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use notify::{EventKind, Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::io;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use std::path::Path;

use crate::backends::linear::setup::{SetupStep, SetupWizard};
use crate::backends::BackendManager;
use crate::tui::app::{App, AppMode, StatusLevel};
use crate::tui::keybindings::{Action, KeyBindings, SetupAction};
use crate::tui::theme::{DynamicTheme, Theme};

pub mod app;
pub mod keybindings;
pub mod theme;
pub mod ui;
pub mod views;

fn setup_theme_watcher(theme: &Theme) -> crate::error::Result<(RecommendedWatcher, Receiver<NotifyEvent>)> {
    let (tx, rx) = channel::<NotifyEvent>();
    
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                let is_theme_event = event.paths.iter().any(|p| {
                    p.to_string_lossy().contains("/theme/") || 
                    p.file_name().map(|n| n == "theme").unwrap_or(false)
                });
                
                if is_theme_event {
                    // Only Create/Modify — Omarchy removes folder first, then recreates
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            let _ = tx.send(event);
                        }
                        _ => {}
                    }
                }
            }
        },
        notify::Config::default(),
    )?;
    
    // Watch parent dir — Omarchy replaces the theme subfolder on switch
    if let Some(path) = theme.watch_path() {
        watcher.watch(&path, RecursiveMode::NonRecursive)?;
    }
    
    Ok((watcher, rx))
}

fn setup_vault_watcher(config: &crate::config::Config) -> Option<(RecommendedWatcher, Receiver<NotifyEvent>)> {
    let vault_path = config
        .backends
        .obsidian
        .as_ref()
        .filter(|t| t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false))
        .and_then(|t| t.get("vault_path").and_then(|v| v.as_str()))
        .map(|s| shellexpand::tilde(s).into_owned())?;

    let vault_path = Path::new(&vault_path).to_path_buf();
    if !vault_path.exists() {
        return None;
    }

    let (tx, rx) = channel::<NotifyEvent>();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                let is_md_event = event.paths.iter().any(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("md")
                });

                if is_md_event {
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                            let _ = tx.send(event);
                        }
                        _ => {}
                    }
                }
            }
        },
        notify::Config::default(),
    )
    .ok()?;

    watcher
        .watch(&vault_path, RecursiveMode::Recursive)
        .ok()?;

    Some((watcher, rx))
}

fn setup_config_watcher() -> Option<(RecommendedWatcher, Receiver<NotifyEvent>)> {
    let config_path = crate::config::Config::default_config_path().ok()?;
    let parent = config_path.parent()?.to_path_buf();
    if !parent.exists() {
        return None;
    }

    let (tx, rx) = channel::<NotifyEvent>();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                let is_config_event = event.paths.iter().any(|p| {
                    p.file_name().and_then(|n| n.to_str()) == Some("config.toml")
                });

                if is_config_event {
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            let _ = tx.send(event);
                        }
                        _ => {}
                    }
                }
            }
        },
        notify::Config::default(),
    )
    .ok()?;

    watcher.watch(&parent, RecursiveMode::NonRecursive).ok()?;

    Some((watcher, rx))
}

pub async fn run(backend_manager: BackendManager, config: crate::config::Config) -> crate::error::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let initial_theme = Theme::load(&config.general.theme);
    let theme = DynamicTheme::new(initial_theme.clone());
    
    // _watcher must stay alive for the duration of the event loop
    let (_watcher, theme_rx) = match setup_theme_watcher(&initial_theme) {
        Ok((watcher, rx)) => (Some(watcher), Some(rx)),
        Err(_) => (None, None),
    };

    let (_vault_watcher, vault_rx) = match setup_vault_watcher(&config) {
        Some((watcher, rx)) => (Some(watcher), Some(rx)),
        None => (None, None),
    };

    let (_config_watcher, config_rx) = match setup_config_watcher() {
        Some((watcher, rx)) => (Some(watcher), Some(rx)),
        None => (None, None),
    };

    let mut app = App::new(backend_manager, config);
    let setup_wizard = SetupWizard::new();

    // Always load tasks on startup. If the user wants to add a Linear
    // backend, they press 'L' to launch the setup wizard on demand.
    app.refresh_tasks().await;

    if app.needs_setup() {
        app.enter_setup();
    }

    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();
    let mut last_theme_change = Instant::now();
    let mut last_vault_change = Instant::now();
    let mut last_config_change = Instant::now();

    loop {
        let current_theme = theme.get();
        terminal.draw(|f| ui::render(f, &app, &current_theme))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        let mut should_quit = false;
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // --- Setup mode is handled separately ---
                if let AppMode::Setup(ref step) = app.mode {
                    let step_clone = step.clone();
                    if let Some(setup_action) = KeyBindings::handle_setup(key, &step_clone) {
                        process_setup_action(setup_action, &mut app, &setup_wizard).await;
                    }
                } else if let Some(action) = handle_key(key, &app) {
                    // Actions that need to suspend the TUI for an external process
                    let external_cmd = match action {
                        Action::OpenInSource => {
                            match get_open_command(&app) {
                                Some(cmd) => Some(cmd),
                                None => {
                                    app.set_status(
                                        "Set $EDITOR or install source app to open this task",
                                        crate::tui::app::StatusLevel::Error,
                                    );
                                    None
                                }
                            }
                        }
                        Action::OpenConfig => get_config_command(),
                        _ => None,
                    };

                    if action == Action::AgentInteractive {
                        // Launch the interactive agent, suspending the TUI
                        if let Some(task) = app.get_selected_visible_task() {
                            disable_raw_mode()?;
                            terminal.backend_mut().execute(LeaveAlternateScreen)?;
                            terminal.show_cursor()?;

                            let cmd = crate::agent::agent_command(&app.config.agent);
                            let _ = crate::agent::launch_interactive(&task, &cmd);

                            enable_raw_mode()?;
                            terminal.backend_mut().execute(EnterAlternateScreen)?;
                            terminal.hide_cursor()?;
                            terminal.clear()?;

                            app.refresh_tasks().await;
                        } else {
                            app.set_status(
                                "No task selected",
                                crate::tui::app::StatusLevel::Warning,
                            );
                        }
                        app.mode = AppMode::Normal;
                    } else if let Some(cmd) = external_cmd {
                        disable_raw_mode()?;
                        terminal.backend_mut().execute(LeaveAlternateScreen)?;
                        terminal.show_cursor()?;

                        let status = std::process::Command::new(&cmd[0])
                            .args(&cmd[1..])
                            .status();

                        enable_raw_mode()?;
                        terminal.backend_mut().execute(EnterAlternateScreen)?;
                        terminal.hide_cursor()?;
                        terminal.clear()?;

                        match status {
                            Ok(s) if s.success() => {
                                if action == Action::OpenConfig {
                                    app.reload_config().await;
                                } else {
                                    app.refresh_tasks().await;
                                }
                            }
                            Ok(s) => {
                                app.set_status(
                                    format!("Editor exited with code {}", s.code().unwrap_or(-1)),
                                    crate::tui::app::StatusLevel::Warning,
                                );
                            }
                            Err(e) => {
                                app.set_status(
                                    format!("Failed to open: {}", e),
                                    crate::tui::app::StatusLevel::Error,
                                );
                            }
                        }
                        if app.mode == AppMode::DetailView {
                            app.mode = AppMode::Normal;
                        }
                    } else if action != Action::OpenInSource && action != Action::OpenConfig {
                        if process_action(action, &mut app).await {
                            should_quit = true;
                        }
                    }
                }
            }
        }

        if let Some(ref rx) = theme_rx {
            while let Ok(_event) = rx.try_recv() {
                if last_theme_change.elapsed() >= Duration::from_secs(1) {
                    let new_theme = Theme::load(&app.config.general.theme);
                    theme.update(new_theme);
                    last_theme_change = Instant::now();
                }
            }
        }

        if let Some(ref rx) = vault_rx {
            while let Ok(_event) = rx.try_recv() {
                if last_vault_change.elapsed() >= Duration::from_secs(1) {
                    app.refresh_tasks().await;
                    last_vault_change = Instant::now();
                }
            }
        }

        if let Some(ref rx) = config_rx {
            while let Ok(_event) = rx.try_recv() {
                if last_config_change.elapsed() >= Duration::from_secs(1) {
                    app.reload_config().await;
                    let new_theme = Theme::load(&app.config.general.theme);
                    theme.update(new_theme);
                    last_config_change = Instant::now();
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
            app.expire_status(Duration::from_secs(3));
        }

        if app.should_quit || should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn get_open_command(app: &App) -> Option<Vec<String>> {
    let task = app.get_selected_visible_task()?;

    if task.source == crate::model::BackendSource::Obsidian {
        if let Some(ref table) = app.config.backends.obsidian {
            if let Ok(obs_config) = crate::backends::obsidian::ObsidianConfig::from_table(table) {
                let backend = crate::backends::obsidian::ObsidianBackend::new(obs_config);
                if let Some(cmd) = backend.open_command(&task) {
                    return Some(cmd);
                }
            }
        }
    }

    let source_path = task.source_path.as_ref()?;

    if source_path.starts_with("http") {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        return Some(vec![opener.to_string(), source_path.clone()]);
    }

    let line_num = task.source_line.unwrap_or(1);

    if let Ok(editor) = std::env::var("EDITOR") {
        return Some(vec![editor, format!("+{}", line_num), source_path.clone()]);
    }

    None
}

fn get_config_command() -> Option<Vec<String>> {
    let editor = std::env::var("EDITOR").ok()?;
    let config_path = crate::config::Config::default_config_path().ok()?;
    Some(vec![editor, config_path.to_string_lossy().into_owned()])
}

fn handle_key(key: KeyEvent, app: &App) -> Option<Action> {
    match app.mode {
        AppMode::Normal => KeyBindings::handle_normal(key),
        AppMode::Input => KeyBindings::handle_input(key),
        AppMode::Help => KeyBindings::handle_help(key),
        AppMode::AgentMenu => KeyBindings::handle_agent_menu(key),
        AppMode::DetailView => KeyBindings::handle_detail(key),
        AppMode::EditForm => {
            let is_cycle = app.edit_form.as_ref().map_or(false, |form| {
                form.fields.get(form.cursor).map_or(false, |f| {
                    matches!(f.kind, crate::tui::app::EditFieldKind::Cycle)
                })
            });
            KeyBindings::handle_edit_form(key, is_cycle)
        }
        AppMode::Setup(_) => None, // Handled separately in the event loop.
    }
}

async fn process_action(action: Action, app: &mut App) -> bool {
    match action {
        Action::Quit => {
            app.should_quit = true;
            return true;
        }
        Action::MoveUp => {
            app.move_selection_up();
        }
        Action::MoveDown => {
            app.move_selection_down();
        }
        Action::MoveToNextGroup => {
            app.move_to_next_group();
        }
        Action::MoveToPreviousGroup => {
            app.move_to_previous_group();
        }
        Action::ToggleGroup => {
            app.toggle_selected_group();
        }
        Action::ToggleAllGroups => {
            app.toggle_all_groups();
        }
        Action::ToggleTask => {
            let was_detail = app.mode == AppMode::DetailView;
            app.toggle_selected_task().await;
            if was_detail {
                app.mode = AppMode::Normal;
            }
        }
        Action::EditTask => {
            let project_names = if let Some(task) = app.get_selected_visible_task() {
                if task.source == crate::model::BackendSource::Linear {
                    app.backend_manager.fetch_project_names(&task.id).await.unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            if let Some(form) = app.build_edit_form(project_names) {
                app.edit_form = Some(form);
                app.mode = AppMode::EditForm;
            }
        }
        Action::ViewDetail => {
            if app.get_selected_visible_task().is_some() {
                app.mode = AppMode::DetailView;
            }
        }
        Action::OpenInSource | Action::OpenConfig => {}
        Action::DeleteTask => {
            app.delete_selected_task().await;
        }
        Action::QuickAdd => {
            app.start_quick_add();
        }
        Action::Search => {
            app.start_search();
        }
        Action::Refresh => {
            app.refresh_tasks().await;
            app.set_status("Tasks refreshed", crate::tui::app::StatusLevel::Info);
        }
        Action::Help => {
            app.toggle_help();
        }
        Action::LaunchAgent => {
            app.mode = AppMode::AgentMenu;
        }
        Action::AgentInteractive => {
            // Handled in the main event loop (requires TUI suspend/resume).
        }
        Action::AgentBackground => {
            if let Some(task) = app.get_selected_visible_task() {
                let cmd = crate::agent::agent_command(&app.config.agent);
                match crate::agent::launch_background(&task, &cmd) {
                    Ok(agent) => {
                        app.set_status(
                            format!(
                                "Background agent started for {} (PID {})",
                                agent.task_id, agent.pid
                            ),
                            crate::tui::app::StatusLevel::Success,
                        );
                    }
                    Err(e) => {
                        app.set_status(
                            format!("Failed to start background agent: {}", e),
                            crate::tui::app::StatusLevel::Error,
                        );
                    }
                }
            } else {
                app.set_status(
                    "No task selected",
                    crate::tui::app::StatusLevel::Warning,
                );
            }
            app.mode = AppMode::Normal;
        }
        Action::AgentStatus => {
            let agents = crate::agent::list_running_agents();
            if agents.is_empty() {
                app.set_status("No background agents running", crate::tui::app::StatusLevel::Info);
            } else {
                let msg = agents
                    .iter()
                    .map(|a| format!("{} (PID {})", a.task_id, a.pid))
                    .collect::<Vec<_>>()
                    .join(", ");
                app.set_status(
                    format!("Running agents: {msg}"),
                    crate::tui::app::StatusLevel::Info,
                );
            }
        }
        Action::LinearSetup => {
            app.enter_linear_setup();
        }
        Action::EditFormUp => {
            if let Some(ref mut form) = app.edit_form {
                if form.cursor > 0 {
                    form.cursor -= 1;
                }
            }
        }
        Action::EditFormDown => {
            if let Some(ref mut form) = app.edit_form {
                if form.cursor + 1 < form.fields.len() {
                    form.cursor += 1;
                }
            }
        }
        Action::EditFormCyclePriority => {
            if let Some(ref mut form) = app.edit_form {
                let cursor = form.cursor;
                if let Some(field) = form.fields.get_mut(cursor) {
                    if !field.options.is_empty() {
                        let current_idx = field.options.iter().position(|o| o == &field.value).unwrap_or(0);
                        let next_idx = (current_idx + 1) % field.options.len();
                        field.value = field.options[next_idx].clone();
                    }
                }
            }
        }
        Action::EditFormCursorLeft => {
            if let Some(ref mut form) = app.edit_form {
                let cursor = form.cursor;
                if let Some(field) = form.fields.get_mut(cursor) {
                    if matches!(field.kind, crate::tui::app::EditFieldKind::Text) && field.cursor_pos > 0 {
                        field.cursor_pos -= 1;
                    }
                }
            }
        }
        Action::EditFormCursorRight => {
            if let Some(ref mut form) = app.edit_form {
                let cursor = form.cursor;
                if let Some(field) = form.fields.get_mut(cursor) {
                    if matches!(field.kind, crate::tui::app::EditFieldKind::Text)
                        && field.cursor_pos < field.value.chars().count()
                    {
                        field.cursor_pos += 1;
                    }
                }
            }
        }
        Action::EditFormBackspace => {
            if let Some(ref mut form) = app.edit_form {
                let cursor = form.cursor;
                if let Some(field) = form.fields.get_mut(cursor) {
                    if matches!(field.kind, crate::tui::app::EditFieldKind::Text) && field.cursor_pos > 0 {
                        let byte_pos = field.value.char_indices()
                            .nth(field.cursor_pos - 1)
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        field.value.remove(byte_pos);
                        field.cursor_pos -= 1;
                    }
                }
            }
        }
        Action::EditFormChar(c) => {
            if let Some(ref mut form) = app.edit_form {
                let cursor = form.cursor;
                if let Some(field) = form.fields.get_mut(cursor) {
                    if matches!(field.kind, crate::tui::app::EditFieldKind::Text) {
                        let byte_pos = field.value.char_indices()
                            .nth(field.cursor_pos)
                            .map(|(i, _)| i)
                            .unwrap_or(field.value.len());
                        field.value.insert(byte_pos, c);
                        field.cursor_pos += 1;
                    }
                }
            }
        }
        Action::EditFormSave => {
            process_edit_form_save(app).await;
        }
        Action::EditFormCancel => {
            app.edit_form = None;
            app.mode = AppMode::Normal;
        }
        Action::Cancel => {
            if app.mode == AppMode::Help {
                app.mode = AppMode::Normal;
            } else if app.mode == AppMode::AgentMenu {
                app.mode = AppMode::Normal;
            } else if app.mode == AppMode::DetailView {
                app.mode = AppMode::Normal;
            } else {
                app.cancel_input();
            }
        }
        Action::Submit => {
            app.submit_input().await;
        }
        Action::Backspace => {
            app.input_buffer.pop();
        }
        Action::Char(c) => {
            app.input_buffer.push(c);
        }
    }
    false
}

/// Process a save from the edit form: build a TaskUpdate and apply it.
async fn process_edit_form_save(app: &mut App) {
    use crate::model::{Priority, TaskUpdate};
    use chrono::NaiveDate;

    let form = match app.edit_form.take() {
        Some(f) => f,
        None => return,
    };

    let mut update = TaskUpdate::default();

    for field in &form.fields {
        match field.key.as_str() {
            "title" => {
                update.title = Some(field.value.clone());
            }
            "description" => {
                if field.value.is_empty() {
                    update.description = Some(None);
                } else {
                    update.description = Some(Some(field.value.clone()));
                }
            }
            "priority" => {
                update.priority = Some(match field.value.as_str() {
                    "High" => Priority::High,
                    "Medium" => Priority::Medium,
                    "Low" => Priority::Low,
                    _ => Priority::None,
                });
            }
            "due" => {
                if field.value.is_empty() {
                    update.due = Some(None);
                } else if let Ok(date) = NaiveDate::parse_from_str(&field.value, "%Y-%m-%d") {
                    update.due = Some(Some(date));
                }
                // If parse fails, leave due as None (no change)
            }
            "status" => {
                if !field.value.is_empty() {
                    update.state_name = Some(field.value.clone());
                }
            }
            "project" => {
                if field.value.is_empty() || field.value == "None" {
                    update.project = Some(None);
                } else {
                    update.project = Some(Some(field.value.clone()));
                }
            }
            _ => {}
        }
    }

    match app.backend_manager.update_task(&form.task_id, &update).await {
        Ok(task) => {
            app.set_status(format!("Updated: {}", task.title), StatusLevel::Success);
        }
        Err(e) => {
            app.set_status(format!("Failed to update: {}", e), StatusLevel::Error);
        }
    }

    app.refresh_tasks().await;
    app.mode = AppMode::Normal;
}

/// Process an action from the setup wizard.
async fn process_setup_action(action: SetupAction, app: &mut App, wizard: &SetupWizard) {
    let step = match &app.mode {
        AppMode::Setup(s) => s.clone(),
        _ => return,
    };

    match (&step, action) {
        // -- Welcome step --
        (SetupStep::Welcome, SetupAction::Submit) => {
            let options = crate::backends::linear::setup::backend_options();
            let selected = vec![false; options.len()];
            app.mode = AppMode::Setup(SetupStep::SelectBackends {
                options,
                selected,
                cursor: 0,
            });
        }
        (SetupStep::Welcome, SetupAction::Cancel) => {
            app.mode = AppMode::Normal;
            app.setup_state = None;
        }

        // -- SelectBackends step --
        (SetupStep::SelectBackends { options, selected, .. }, SetupAction::Submit) => {
            let chosen: Vec<String> = options
                .iter()
                .zip(selected.iter())
                .filter(|(_, &sel)| sel)
                .map(|(opt, _)| opt.key.clone())
                .collect();

            if chosen.is_empty() {
                app.set_status("Please select at least one backend", StatusLevel::Warning);
                return;
            }

            if let Some(ref mut state) = app.setup_state {
                state.selected_backends = chosen.clone();
            }

            if chosen.contains(&"linear".to_string()) {
                app.input_buffer.clear();
                app.mode = AppMode::Setup(SetupStep::ApiKey);
            } else {
                // Only local selected -- write config and complete
                let config_path = crate::config::Config::default_config_path().unwrap();
                let w = SetupWizard::new();
                if let Err(e) = w.write_general_config(
                    &config_path,
                    &chosen,
                    None,
                    None,
                    None,
                    "",
                    &[],
                    None,
                ) {
                    app.mode =
                        AppMode::Setup(SetupStep::Error(format!("Failed to save config: {e}")));
                } else {
                    app.mode = AppMode::Setup(SetupStep::Complete);
                }
            }
        }
        (SetupStep::SelectBackends { .. }, SetupAction::MoveDown) => {
            if let AppMode::Setup(SetupStep::SelectBackends {
                options, cursor, ..
            }) = &mut app.mode
            {
                if *cursor + 1 < options.len() {
                    *cursor += 1;
                }
            }
        }
        (SetupStep::SelectBackends { .. }, SetupAction::MoveUp) => {
            if let AppMode::Setup(SetupStep::SelectBackends { cursor, .. }) = &mut app.mode {
                *cursor = cursor.saturating_sub(1);
            }
        }
        (SetupStep::SelectBackends { .. }, SetupAction::ToggleItem) => {
            if let AppMode::Setup(SetupStep::SelectBackends { selected, cursor, .. }) =
                &mut app.mode
            {
                if let Some(val) = selected.get_mut(*cursor) {
                    *val = !*val;
                }
            }
        }
        (SetupStep::SelectBackends { .. }, SetupAction::Cancel) => {
            app.mode = AppMode::Normal;
            app.setup_state = None;
        }

        // -- BackendName step --
        (SetupStep::BackendName, SetupAction::Submit) => {
            let name = app.input_buffer.trim().to_lowercase();
            if name.is_empty() {
                app.set_status("Please enter a name for this backend", StatusLevel::Warning);
                return;
            }
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                app.set_status("Name must be alphanumeric (hyphens and underscores allowed)", StatusLevel::Warning);
                return;
            }
            if let Some(ref mut state) = app.setup_state {
                state.backend_name = Some(name);
            }
            app.input_buffer.clear();
            app.mode = AppMode::Setup(SetupStep::ApiKey);
        }
        (SetupStep::BackendName, SetupAction::Backspace) => {
            app.input_buffer.pop();
        }
        (SetupStep::BackendName, SetupAction::Char(c)) => {
            app.input_buffer.push(c);
        }
        (SetupStep::BackendName, SetupAction::Cancel) => {
            app.mode = AppMode::Normal;
            app.setup_state = None;
            app.input_buffer.clear();
        }

        // -- ApiKey step --
        (SetupStep::ApiKey, SetupAction::Cancel) => {
            app.setup_state = None;
            app.input_buffer.clear();
            app.mode = AppMode::Normal;
        }
        (SetupStep::ApiKey, SetupAction::Submit) => {
            let key = app.input_buffer.trim().to_string();
            if key.is_empty() {
                return;
            }
            if let Some(ref mut state) = app.setup_state {
                state.api_key = key.clone();
            }
            app.mode = AppMode::Setup(SetupStep::ValidatingKey);

            // Validate the key.
            match wizard.validate_key(&key).await {
                Ok(user) => {
                    if let Some(ref mut state) = app.setup_state {
                        state.user = Some(user);
                    }
                    // Fetch teams next.
                    match wizard.fetch_teams(&key).await {
                        Ok(teams) => {
                            if teams.is_empty() {
                                app.mode = AppMode::Setup(SetupStep::Error(
                                    "No teams found for this API key.".into(),
                                ));
                            } else {
                                app.mode = AppMode::Setup(SetupStep::SelectTeam {
                                    teams,
                                    selected: 0,
                                });
                            }
                        }
                        Err(e) => {
                            app.mode =
                                AppMode::Setup(SetupStep::Error(format!("Failed to fetch teams: {e}")));
                        }
                    }
                }
                Err(e) => {
                    app.mode = AppMode::Setup(SetupStep::Error(format!(
                        "Invalid API key: {e}"
                    )));
                }
            }
        }
        (SetupStep::ApiKey, SetupAction::Backspace) => {
            app.input_buffer.pop();
        }
        (SetupStep::ApiKey, SetupAction::Char(c)) => {
            app.input_buffer.push(c);
        }

        // -- SelectTeam step --
        (SetupStep::SelectTeam { teams, selected }, SetupAction::MoveDown) => {
            let new_sel = (*selected + 1).min(teams.len().saturating_sub(1));
            app.mode = AppMode::Setup(SetupStep::SelectTeam {
                teams: teams.clone(),
                selected: new_sel,
            });
        }
        (SetupStep::SelectTeam { teams, selected }, SetupAction::MoveUp) => {
            let new_sel = selected.saturating_sub(1);
            app.mode = AppMode::Setup(SetupStep::SelectTeam {
                teams: teams.clone(),
                selected: new_sel,
            });
        }
        (SetupStep::SelectTeam { teams, selected }, SetupAction::Submit) => {
            if let Some(team) = teams.get(*selected) {
                let team = team.clone();
                if let Some(ref mut state) = app.setup_state {
                    state.team = Some(team.clone());
                }
                // Fetch team members for assignee selection.
                let api_key = app
                    .setup_state
                    .as_ref()
                    .map(|s| s.api_key.clone())
                    .unwrap_or_default();
                match wizard.fetch_members(&api_key, &team.id).await {
                    Ok(members) => {
                        // The first option is "Only my issues" (index 0).
                        app.mode = AppMode::Setup(SetupStep::SelectAssignee {
                            members,
                            selected: 0,
                        });
                    }
                    Err(e) => {
                        app.mode = AppMode::Setup(SetupStep::Error(format!(
                            "Failed to fetch members: {e}"
                        )));
                    }
                }
            }
        }
        (SetupStep::SelectTeam { .. }, SetupAction::Cancel) => {
            // Go back to ApiKey.
            app.input_buffer.clear();
            if let Some(ref state) = app.setup_state {
                app.input_buffer = state.api_key.clone();
            }
            app.mode = AppMode::Setup(SetupStep::ApiKey);
        }

        // -- SelectAssignee step --
        (SetupStep::SelectAssignee { members, selected }, SetupAction::MoveDown) => {
            // +1 for "Only my issues" at index 0.
            let max = members.len(); // 0 = "my issues", 1..=len = members
            let new_sel = (*selected + 1).min(max);
            app.mode = AppMode::Setup(SetupStep::SelectAssignee {
                members: members.clone(),
                selected: new_sel,
            });
        }
        (SetupStep::SelectAssignee { members, selected }, SetupAction::MoveUp) => {
            let new_sel = selected.saturating_sub(1);
            app.mode = AppMode::Setup(SetupStep::SelectAssignee {
                members: members.clone(),
                selected: new_sel,
            });
        }
        (SetupStep::SelectAssignee { members, selected }, SetupAction::Submit) => {
            let assignee_value = if *selected == 0 {
                "me".to_string()
            } else if let Some(member) = members.get(*selected - 1) {
                member.id.clone()
            } else {
                "me".to_string()
            };

            if let Some(ref mut state) = app.setup_state {
                state.assignee = assignee_value;
            }

            // Fetch workflow states for the selected team.
            let api_key = app
                .setup_state
                .as_ref()
                .map(|s| s.api_key.clone())
                .unwrap_or_default();
            let team_id = app
                .setup_state
                .as_ref()
                .and_then(|s| s.team.as_ref())
                .map(|t| t.id.clone())
                .unwrap_or_default();

            match wizard.fetch_states(&api_key, &team_id).await {
                Ok(states) => {
                    // Default: select all non-completed/non-canceled states.
                    let selected_flags: Vec<bool> = states
                        .iter()
                        .map(|s| s.state_type != "completed" && s.state_type != "canceled")
                        .collect();
                    app.mode = AppMode::Setup(SetupStep::SelectStatuses {
                        states,
                        selected: selected_flags,
                        cursor: 0,
                    });
                }
                Err(e) => {
                    app.mode = AppMode::Setup(SetupStep::Error(format!(
                        "Failed to fetch states: {e}"
                    )));
                }
            }
        }
        (SetupStep::SelectAssignee { .. }, SetupAction::Cancel) => {
            // Go back to team selection — re-fetch teams.
            let api_key = app
                .setup_state
                .as_ref()
                .map(|s| s.api_key.clone())
                .unwrap_or_default();
            match wizard.fetch_teams(&api_key).await {
                Ok(teams) => {
                    app.mode = AppMode::Setup(SetupStep::SelectTeam {
                        teams,
                        selected: 0,
                    });
                }
                Err(e) => {
                    app.mode = AppMode::Setup(SetupStep::Error(format!(
                        "Failed to fetch teams: {e}"
                    )));
                }
            }
        }

        // -- SelectStatuses step --
        (SetupStep::SelectStatuses { states, selected, cursor }, SetupAction::MoveDown) => {
            let new_cursor = (*cursor + 1).min(states.len().saturating_sub(1));
            app.mode = AppMode::Setup(SetupStep::SelectStatuses {
                states: states.clone(),
                selected: selected.clone(),
                cursor: new_cursor,
            });
        }
        (SetupStep::SelectStatuses { states, selected, cursor }, SetupAction::MoveUp) => {
            let new_cursor = cursor.saturating_sub(1);
            app.mode = AppMode::Setup(SetupStep::SelectStatuses {
                states: states.clone(),
                selected: selected.clone(),
                cursor: new_cursor,
            });
        }
        (SetupStep::SelectStatuses { states, selected, cursor }, SetupAction::ToggleItem) => {
            let mut new_selected = selected.clone();
            if let Some(flag) = new_selected.get_mut(*cursor) {
                *flag = !*flag;
            }
            app.mode = AppMode::Setup(SetupStep::SelectStatuses {
                states: states.clone(),
                selected: new_selected,
                cursor: *cursor,
            });
        }
        (SetupStep::SelectStatuses { states, selected, .. }, SetupAction::Submit) => {
            // Collect the names of the selected statuses.
            let chosen: Vec<String> = states
                .iter()
                .zip(selected.iter())
                .filter_map(|(s, &flag)| if flag { Some(s.name.clone()) } else { None })
                .collect();

            if let Some(ref mut state) = app.setup_state {
                state.statuses = chosen;
            }

            // Write config using write_general_config.
            let config_path = crate::config::Config::default_config_path()
                .unwrap_or_else(|_| std::path::PathBuf::from("config.toml"));

            let write_result = {
                let setup = app.setup_state.as_ref().unwrap();
                wizard.write_general_config(
                    &config_path,
                    &setup.selected_backends,
                    Some(setup.api_key.as_str()),
                    setup.user.as_ref(),
                    setup.team.as_ref(),
                    &setup.assignee,
                    &setup.statuses,
                    setup.backend_name.as_deref(),
                )
            };

            match write_result {
                Ok(()) => {
                    app.mode = AppMode::Setup(SetupStep::Complete);
                }
                Err(e) => {
                    app.mode = AppMode::Setup(SetupStep::Error(format!(
                        "Failed to write config: {e}"
                    )));
                }
            }
        }
        (SetupStep::SelectStatuses { .. }, SetupAction::Cancel) => {
            // Go back to assignee selection.
            let api_key = app
                .setup_state
                .as_ref()
                .map(|s| s.api_key.clone())
                .unwrap_or_default();
            let team_id = app
                .setup_state
                .as_ref()
                .and_then(|s| s.team.as_ref())
                .map(|t| t.id.clone())
                .unwrap_or_default();
            match wizard.fetch_members(&api_key, &team_id).await {
                Ok(members) => {
                    app.mode = AppMode::Setup(SetupStep::SelectAssignee {
                        members,
                        selected: 0,
                    });
                }
                Err(e) => {
                    app.mode = AppMode::Setup(SetupStep::Error(format!(
                        "Failed to fetch members: {e}"
                    )));
                }
            }
        }

        // -- Complete step --
        (SetupStep::Complete, SetupAction::AnyKey) => {
            // Reload config and enter normal mode.
            app.setup_state = None;
            app.input_buffer.clear();
            app.mode = AppMode::Normal;
            app.reload_config().await;
        }

        // -- Error step --
        (SetupStep::Error(_), SetupAction::AnyKey) => {
            // Go back to ApiKey step.
            app.input_buffer.clear();
            if let Some(ref state) = app.setup_state {
                app.input_buffer = state.api_key.clone();
            }
            app.mode = AppMode::Setup(SetupStep::ApiKey);
        }

        // Catch-all for unhandled combinations.
        _ => {}
    }
}
