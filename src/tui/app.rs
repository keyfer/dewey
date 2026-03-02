use std::collections::HashMap;
use std::time::Instant;

use crate::backends::linear::setup::{SetupStep, SetupTeam, SetupUser};
use crate::backends::BackendManager;
use crate::config::Config;
use crate::model::{BackendSource, Task, TaskFilter, TaskStatus};

#[derive(Debug, Clone)]
pub enum EditFieldKind {
    Text,
    Cycle,
}

#[derive(Debug, Clone)]
pub struct EditField {
    pub label: String,
    pub key: String,
    pub value: String,
    pub kind: EditFieldKind,
    pub options: Vec<String>,
    pub cursor_pos: usize,
}

#[derive(Debug, Clone)]
pub struct EditFormState {
    pub task_id: String,
    pub fields: Vec<EditField>,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub enum AppMode {
    Normal,
    Input,
    Help,
    AgentMenu,
    DetailView,
    EditForm,
    Setup(SetupStep),
}

impl PartialEq for AppMode {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (AppMode::Normal, AppMode::Normal)
                | (AppMode::Input, AppMode::Input)
                | (AppMode::Help, AppMode::Help)
                | (AppMode::AgentMenu, AppMode::AgentMenu)
                | (AppMode::DetailView, AppMode::DetailView)
                | (AppMode::EditForm, AppMode::EditForm)
                | (AppMode::Setup(_), AppMode::Setup(_))
        )
    }
}

impl Eq for AppMode {}

#[derive(Debug, Clone)]
pub struct SetupWizardState {
    pub selected_backends: Vec<String>,
    pub backend_name: Option<String>,
    pub api_key: String,
    pub user: Option<SetupUser>,
    pub team: Option<SetupTeam>,
    pub assignee: String,
    pub statuses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    QuickAdd,
    Search,
    EditTask(String), // Stores the task ID being edited
}

#[derive(Debug, Clone)]
pub struct TaskGroup {
    pub label: String,
    pub date: Option<chrono::NaiveDate>,
    pub tasks: Vec<Task>,
    pub collapsed: bool,
}

pub struct App {
    pub mode: AppMode,
    pub tasks: Vec<Task>,
    pub task_groups: Vec<TaskGroup>,
    pub selected_task: usize,
    pub selected_group: usize,
    pub task_filter: TaskFilter,
    pub input_buffer: String,
    pub input_mode: Option<InputMode>,
    pub status_message: Option<(String, StatusLevel, Instant)>,
    pub backend_manager: BackendManager,
    pub config: Config,
    pub should_quit: bool,
    pub setup_state: Option<SetupWizardState>,
    pub edit_form: Option<EditFormState>,
    pub detail_scroll: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub enum VisibleItem {
    Group(usize),
    Task(usize, Task),
    None,
}

impl App {
    pub fn new(backend_manager: BackendManager, config: Config) -> Self {
        Self {
            mode: AppMode::Normal,
            tasks: Vec::new(),
            task_groups: Vec::new(),
            selected_task: 0,
            selected_group: 0,
            task_filter: TaskFilter::default(),
            input_buffer: String::new(),
            input_mode: None,
            status_message: None,
            backend_manager,
            config,
            should_quit: false,
            setup_state: None,
            edit_form: None,
            detail_scroll: 0,
        }
    }

    /// Returns true when no backends are configured at all,
    /// meaning the setup wizard should auto-launch.
    pub fn needs_setup(&self) -> bool {
        let has_local = self
            .config
            .backends
            .local
            .as_ref()
            .and_then(|t| t.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let has_linear = self.config.backends.linear.as_ref().map_or(false, |t| {
            // Old format: [backends.linear] with enabled = true
            let old_format = t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            // New format: [backends.linear.work] etc. — has sub-tables
            let new_format = t.values().any(|v| v.is_table());
            old_format || new_format
        });

        !has_local && !has_linear
    }

    /// Enter the setup wizard, initialising setup state.
    pub fn enter_setup(&mut self) {
        self.setup_state = Some(SetupWizardState {
            selected_backends: Vec::new(),
            backend_name: None,
            api_key: String::new(),
            user: None,
            team: None,
            assignee: String::new(),
            statuses: Vec::new(),
        });
        self.input_buffer.clear();
        self.mode = AppMode::Setup(SetupStep::Welcome);
    }

    /// Enter the Linear setup wizard directly (for adding another Linear backend).
    pub fn enter_linear_setup(&mut self) {
        self.setup_state = Some(SetupWizardState {
            selected_backends: vec!["linear".to_string()],
            backend_name: None,
            api_key: String::new(),
            user: None,
            team: None,
            assignee: String::new(),
            statuses: Vec::new(),
        });
        self.input_buffer.clear();
        self.mode = AppMode::Setup(SetupStep::BackendName);
    }

    pub fn group_tasks(&mut self) {
        use chrono::Local;

        let today = Local::now().date_naive();
        let mut groups: Vec<TaskGroup> = Vec::new();
        let mut group_map: HashMap<Option<chrono::NaiveDate>, Vec<Task>> = HashMap::new();

        for task in &self.tasks {
            group_map.entry(task.due).or_default().push(task.clone());
        }

        let mut dates: Vec<_> = group_map.keys().copied().collect();
        dates.sort_by(|a, b| match (a, b) {
            (Some(da), Some(db)) => da.cmp(db),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        for date in dates {
            let tasks = group_map.remove(&date).unwrap();
            let label = match date {
                Some(d) if d < today => format!("Overdue - {}", d),
                Some(d) if d == today => "Today".to_string(),
                Some(d) if d == today + chrono::Duration::days(1) => "Tomorrow".to_string(),
                Some(d) => format!("{}", d.format("%A %Y-%m-%d")),
                None => "No due date".to_string(),
            };

            let collapsed = self
                .task_groups
                .iter()
                .find(|g| g.date == date)
                .map(|g| g.collapsed)
                .unwrap_or(false);

            groups.push(TaskGroup {
                label,
                date,
                tasks,
                collapsed,
            });
        }

        self.task_groups = groups;

        if !self.task_groups.is_empty() && self.selected_group >= self.task_groups.len() {
            self.selected_group = self.task_groups.len() - 1;
        }
    }

    pub fn visible_count(&self) -> usize {
        let mut count = self.task_groups.len();
        for group in &self.task_groups {
            if !group.collapsed {
                count += group.tasks.len();
            }
        }
        count
    }

    pub fn get_visible_item(&self, index: usize) -> VisibleItem {
        let mut current = 0;

        for (group_idx, group) in self.task_groups.iter().enumerate() {
            if current == index {
                return VisibleItem::Group(group_idx);
            }
            current += 1;

            if !group.collapsed {
                for task in group.tasks.iter() {
                    if current == index {
                        return VisibleItem::Task(group_idx, task.clone());
                    }
                    current += 1;
                }
            }
        }

        VisibleItem::None
    }

    pub fn toggle_selected_group(&mut self) {
        if let Some(group) = self.task_groups.get_mut(self.selected_group) {
            group.collapsed = !group.collapsed;
        }
    }

    pub fn toggle_all_groups(&mut self) {
        let all_collapsed = self.task_groups.iter().all(|g| g.collapsed);
        for group in &mut self.task_groups {
            group.collapsed = !all_collapsed;
        }
    }

    pub fn get_selected_visible_task(&self) -> Option<Task> {
        match self.get_visible_item(self.selected_task) {
            VisibleItem::Task(_, task) => Some(task),
            _ => None,
        }
    }

    pub fn move_selection_down(&mut self) {
        let visible = self.visible_count();
        if visible > 0 && self.selected_task < visible - 1 {
            self.selected_task += 1;
            self.update_selected_group();
        }
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_task > 0 {
            self.selected_task -= 1;
            self.update_selected_group();
        }
    }

    fn update_selected_group(&mut self) {
        match self.get_visible_item(self.selected_task) {
            VisibleItem::Group(idx) => {
                self.selected_group = idx;
            }
            VisibleItem::Task(group_idx, _) => {
                self.selected_group = group_idx;
            }
            _ => {}
        }
    }

    pub fn move_to_next_group(&mut self) {
        if self.selected_group < self.task_groups.len().saturating_sub(1) {
            self.selected_group += 1;
            self.selected_task = self.find_group_start(self.selected_group);
        }
    }

    pub fn move_to_previous_group(&mut self) {
        if self.selected_group > 0 {
            self.selected_group -= 1;
            self.selected_task = self.find_group_start(self.selected_group);
        }
    }

    fn find_group_start(&self, group_idx: usize) -> usize {
        let mut current = 0;
        for (idx, group) in self.task_groups.iter().enumerate() {
            if idx == group_idx {
                return current;
            }
            current += 1;
            if !group.collapsed {
                current += group.tasks.len();
            }
        }
        current
    }

    pub fn set_status(&mut self, message: impl Into<String>, level: StatusLevel) {
        self.status_message = Some((message.into(), level, Instant::now()));
    }

    /// Clear the status message if it has been visible long enough.
    pub fn expire_status(&mut self, duration: std::time::Duration) {
        if let Some((_, _, created)) = &self.status_message {
            if created.elapsed() >= duration {
                self.status_message = None;
            }
        }
    }

    pub async fn reload_config(&mut self) {
        match Config::load(None) {
            Ok(new_config) => {
                match crate::backends::BackendManager::from_config(&new_config) {
                    Ok(new_manager) => {
                        self.config = new_config;
                        self.backend_manager = new_manager;
                        self.refresh_tasks().await;
                        self.set_status("Config reloaded", StatusLevel::Success);
                    }
                    Err(e) => {
                        self.set_status(format!("Backend error: {}", e), StatusLevel::Error);
                    }
                }
            }
            Err(e) => {
                self.set_status(format!("Config error: {}", e), StatusLevel::Error);
            }
        }
    }

    pub async fn refresh_tasks(&mut self) {
        match self.backend_manager.all_tasks(&self.task_filter).await {
            Ok(tasks) => {
                self.tasks = tasks;
                self.group_tasks();
                let visible = self.visible_count();
                if self.selected_task >= visible && visible > 0 {
                    self.selected_task = visible - 1;
                }
            }
            Err(e) => {
                self.set_status(format!("Error loading tasks: {}", e), StatusLevel::Error);
            }
        }
    }

    pub async fn toggle_selected_task(&mut self) {
        if let Some(task) = self.get_selected_visible_task() {
            let task_id = task.id.clone();
            match task.status {
                TaskStatus::Pending => {
                    if let Err(e) = self.backend_manager.complete_task(&task_id).await {
                        self.set_status(format!("Failed to complete task: {}", e), StatusLevel::Error);
                    } else {
                        self.set_status("Task completed", StatusLevel::Success);
                    }
                }
                TaskStatus::Done => {
                    if let Err(e) = self.backend_manager.uncomplete_task(&task_id).await {
                        self.set_status(format!("Failed to uncomplete task: {}", e), StatusLevel::Error);
                    } else {
                        self.set_status("Task marked as pending", StatusLevel::Success);
                    }
                }
            }
            self.refresh_tasks().await;
        }
    }

    pub async fn delete_selected_task(&mut self) {
        if let Some(task) = self.get_selected_visible_task() {
            let task_id = task.id.clone();
            if let Err(e) = self.backend_manager.delete_task(&task_id).await {
                self.set_status(format!("Failed to delete task: {}", e), StatusLevel::Error);
            } else {
                self.set_status("Task deleted", StatusLevel::Success);
            }
            self.refresh_tasks().await;
        }
    }

        pub fn edit_selected_task(&mut self) {
        use crate::model::Priority;
        
        if let Some(task) = self.get_selected_visible_task() {
            let mut parts = vec![task.title.clone()];
            
            match task.priority {
                Priority::High => parts.push("(p1)".to_string()),
                Priority::Medium => parts.push("(p2)".to_string()),
                Priority::Low => parts.push("(p3)".to_string()),
                Priority::None => {}
            }
            
            if let Some(due) = task.due {
                parts.push(due.to_string());
            }
            
            for tag in &task.tags {
                parts.push(format!("#{}", tag));
            }

            if let Some(ref project) = task.project {
                parts.push(format!("+{}", project));
            }

            let edit_text = parts.join(" ");
            
            self.mode = AppMode::Input;
            self.input_mode = Some(InputMode::EditTask(task.id.clone()));
            self.input_buffer = edit_text;
        }
    }

    pub fn build_edit_form(&self, project_names: Vec<String>) -> Option<EditFormState> {
        use crate::model::Priority;

        let task = self.get_selected_visible_task()?;

        let mut fields = Vec::new();

        // Title — strip identifier prefix for Linear tasks (e.g., "ENG-42 Fix login bug" → "Fix login bug")
        let title_value = if task.source == BackendSource::Linear {
            task.title
                .splitn(2, ' ')
                .nth(1)
                .unwrap_or(&task.title)
                .to_string()
        } else {
            task.title.clone()
        };

        let title_len = title_value.chars().count();
        fields.push(EditField {
            label: "Title".to_string(),
            key: "title".to_string(),
            value: title_value,
            kind: EditFieldKind::Text,
            options: Vec::new(),
            cursor_pos: title_len,
        });

        // Description — only for Linear tasks
        if task.source == BackendSource::Linear {
            let desc_val = task.description.clone().unwrap_or_default();
            let desc_len = desc_val.chars().count();
            fields.push(EditField {
                label: "Description".to_string(),
                key: "description".to_string(),
                value: desc_val,
                kind: EditFieldKind::Text,
                options: Vec::new(),
                cursor_pos: desc_len,
            });
        }

        // Priority
        let priority_str = match task.priority {
            Priority::High => "High",
            Priority::Medium => "Medium",
            Priority::Low => "Low",
            Priority::None => "None",
        };
        fields.push(EditField {
            label: "Priority".to_string(),
            key: "priority".to_string(),
            value: priority_str.to_string(),
            kind: EditFieldKind::Cycle,
            options: vec!["None".into(), "Low".into(), "Medium".into(), "High".into()],
            cursor_pos: 0,
        });

        // Due
        let due_val = task.due.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default();
        let due_len = due_val.chars().count();
        fields.push(EditField {
            label: "Due".to_string(),
            key: "due".to_string(),
            value: due_val,
            kind: EditFieldKind::Text,
            options: Vec::new(),
            cursor_pos: due_len,
        });

        // Status — only for Linear tasks
        if task.source == BackendSource::Linear {
            let status_val = task.state_name.clone().unwrap_or_default();
            let status_len = status_val.chars().count();
            fields.push(EditField {
                label: "Status".to_string(),
                key: "status".to_string(),
                value: status_val,
                kind: EditFieldKind::Text,
                options: Vec::new(),
                cursor_pos: status_len,
            });

            let project_val = task.project.clone().unwrap_or("None".to_string());
            let mut project_options = vec!["None".to_string()];
            for name in &project_names {
                if !project_options.contains(name) {
                    project_options.push(name.clone());
                }
            }
            fields.push(EditField {
                label: "Project".to_string(),
                key: "project".to_string(),
                value: project_val,
                kind: EditFieldKind::Cycle,
                options: project_options,
                cursor_pos: 0,
            });
        }

        Some(EditFormState {
            task_id: task.id.clone(),
            fields,
            cursor: 0,
        })
    }

    pub fn start_quick_add(&mut self) {
        self.mode = AppMode::Input;
        self.input_mode = Some(InputMode::QuickAdd);
        self.input_buffer.clear();
    }

    pub fn start_search(&mut self) {
        self.mode = AppMode::Input;
        self.input_mode = Some(InputMode::Search);
        self.input_buffer.clear();
    }

    pub fn cancel_input(&mut self) {
        self.mode = AppMode::Normal;
        self.input_mode = None;
        self.input_buffer.clear();
    }

    pub async fn submit_input(&mut self) {
        if let Some(ref input_mode) = self.input_mode {
            match input_mode {
                InputMode::QuickAdd => {
                    if !self.input_buffer.is_empty() {
                        use crate::nlp::parse_quick_add;
                        use crate::model::NewTask;
                        
                        let keys = self.backend_manager.backend_keys();
                        let key_refs: Vec<&str> = keys.iter().copied().collect();
                        match parse_quick_add(&self.input_buffer, self.config.general.default_backend.as_deref(), &key_refs) {
                            Ok((title, priority, due, tags, backend, project)) => {
                                let new_task = NewTask {
                                    title,
                                    priority,
                                    due,
                                    tags,
                                    backend,
                                    project,
                                };

                                match self.backend_manager.create_task(&new_task).await {
                                    Ok(task) => {
                                        self.set_status(format!("Created: {}", task.title), StatusLevel::Success);
                                    }
                                    Err(e) => {
                                        self.set_status(format!("Failed to create task: {}", e), StatusLevel::Error);
                                    }
                                }
                            }
                            Err(e) => {
                                self.set_status(format!("Parse error: {}", e), StatusLevel::Error);
                            }
                        }
                        self.refresh_tasks().await;
                    }
                }
                InputMode::Search => {
                    self.task_filter.search = if self.input_buffer.is_empty() {
                        None
                    } else {
                        Some(self.input_buffer.clone())
                    };
                    self.refresh_tasks().await;
                }
                InputMode::EditTask(task_id) => {
                    let task_id = task_id.clone();
                    if !self.input_buffer.is_empty() {
                        use crate::nlp::parse_quick_add;
                        use crate::model::TaskUpdate;
                        
                        let keys = self.backend_manager.backend_keys();
                        let key_refs: Vec<&str> = keys.iter().copied().collect();
                        match parse_quick_add(&self.input_buffer, self.config.general.default_backend.as_deref(), &key_refs) {
                            Ok((title, priority, due, tags, _, _project)) => {
                                let update = TaskUpdate {
                                    title: Some(title),
                                    status: None,
                                    priority: Some(priority),
                                    due: Some(due),
                                    tags: Some(tags),
                                    ..Default::default()
                                };
                                
                                match self.backend_manager.update_task(&task_id, &update).await {
                                    Ok(task) => {
                                        self.set_status(format!("Updated: {}", task.title), StatusLevel::Success);
                                    }
                                    Err(e) => {
                                        self.set_status(format!("Failed to update task: {}", e), StatusLevel::Error);
                                    }
                                }
                            }
                            Err(e) => {
                                self.set_status(format!("Parse error: {}", e), StatusLevel::Error);
                            }
                        }
                        self.refresh_tasks().await;
                    }
                }
            }
        }
        self.mode = AppMode::Normal;
        self.input_mode = None;
        self.input_buffer.clear();
    }

    pub fn toggle_help(&mut self) {
        if self.mode == AppMode::Help {
            self.mode = AppMode::Normal;
        } else {
            self.mode = AppMode::Help;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::BackendManager;
    use crate::config::Config;

    fn make_app_with_config(toml_str: &str) -> App {
        let config: Config = toml::from_str(toml_str).unwrap();
        let manager = BackendManager::from_config(&config).unwrap();
        App::new(manager, config)
    }

    #[test]
    fn needs_setup_when_no_backends() {
        let app = make_app_with_config("");
        assert!(app.needs_setup());
    }

    #[test]
    fn needs_setup_when_linear_not_enabled() {
        let app = make_app_with_config(
            r#"
[backends.linear]
enabled = false
"#,
        );
        assert!(app.needs_setup());
    }

    #[test]
    fn no_setup_needed_when_linear_enabled() {
        let app = make_app_with_config(
            r#"
[backends.linear]
enabled = true
api_key = "lin_api_some_key"
"#,
        );
        assert!(!app.needs_setup());
    }

    #[test]
    fn no_setup_needed_when_local_enabled() {
        let app = make_app_with_config(
            r#"
[backends.local]
enabled = true
"#,
        );
        assert!(!app.needs_setup());
    }

    #[test]
    fn no_setup_needed_when_multi_linear() {
        let app = make_app_with_config(
            r#"
[backends.linear.work]
enabled = true
api_key = "lin_api_work"
team_id = "TEAM-WORK"

[backends.linear.personal]
enabled = true
api_key = "lin_api_personal"
team_id = "TEAM-PERSONAL"
"#,
        );
        assert!(!app.needs_setup());
    }

    #[test]
    fn enter_setup_sets_mode_and_state() {
        let mut app = make_app_with_config("");
        app.enter_setup();

        assert!(matches!(app.mode, AppMode::Setup(SetupStep::Welcome)));
        assert!(app.setup_state.is_some());
        let state = app.setup_state.as_ref().unwrap();
        assert!(state.api_key.is_empty());
        assert!(state.selected_backends.is_empty());
        assert!(state.user.is_none());
        assert!(state.team.is_none());
    }

    #[test]
    fn app_mode_equality() {
        assert_eq!(AppMode::Normal, AppMode::Normal);
        assert_eq!(AppMode::Input, AppMode::Input);
        assert_eq!(AppMode::Help, AppMode::Help);
        assert_eq!(AppMode::AgentMenu, AppMode::AgentMenu);

        // All Setup variants compare equal.
        assert_eq!(
            AppMode::Setup(SetupStep::ApiKey),
            AppMode::Setup(SetupStep::Complete)
        );

        assert_ne!(AppMode::Normal, AppMode::Input);
        assert_ne!(AppMode::Normal, AppMode::Setup(SetupStep::ApiKey));
    }
}
