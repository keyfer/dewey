use crossterm::event::{KeyCode, KeyEvent};

use crate::backends::linear::setup::SetupStep;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    MoveUp,
    MoveDown,
    MoveToNextGroup,
    MoveToPreviousGroup,
    ToggleGroup,
    ToggleAllGroups,
    ToggleTask,
    ViewDetail,
    EditTask,
    OpenInSource,
    OpenConfig,
    DeleteTask,
    QuickAdd,
    Search,
    Refresh,
    Help,
    Cancel,
    Submit,
    Backspace,
    Char(char),
    LaunchAgent,
    AgentInteractive,
    AgentBackground,
    AgentStatus,
    Setup,
    // Edit form actions
    EditFormUp,
    EditFormDown,
    EditFormCyclePriority,
    EditFormSave,
    EditFormCancel,
    EditFormBackspace,
    EditFormChar(char),
    EditFormCursorLeft,
    EditFormCursorRight,
}

/// Actions specific to the setup wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupAction {
    Cancel,
    Submit,
    Backspace,
    Char(char),
    MoveUp,
    MoveDown,
    ToggleItem,
    AnyKey,
}

pub struct KeyBindings;

impl KeyBindings {
    pub fn handle_normal(key: KeyEvent) -> Option<Action> {
        match key.code {
            // Quit
            KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),

            // Navigation
            KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
            KeyCode::Tab => Some(Action::MoveToNextGroup),
            KeyCode::BackTab => Some(Action::MoveToPreviousGroup),

            // Group actions
            KeyCode::Char(' ') => Some(Action::ToggleGroup),
            KeyCode::Char('C') => Some(Action::ToggleAllGroups),

            // Actions
            KeyCode::Char('x') => Some(Action::ToggleTask),
            KeyCode::Enter => Some(Action::ViewDetail),
            KeyCode::Char('e') => Some(Action::EditTask),
            KeyCode::Char('o') => Some(Action::OpenInSource),
            KeyCode::Char('c') => Some(Action::OpenConfig),
            KeyCode::Char('d') => {
                // Check for 'dd' (vim-style delete)
                // For now, just single 'd' opens delete confirmation
                Some(Action::DeleteTask)
            }
            KeyCode::Char('a') => Some(Action::QuickAdd),
            KeyCode::Char('/') => Some(Action::Search),
            KeyCode::Char('r') => Some(Action::Refresh),
            KeyCode::Char('?') => Some(Action::Help),

            // Agent actions
            KeyCode::Char('A') => Some(Action::LaunchAgent),
            KeyCode::Char('S') => Some(Action::AgentStatus),

            // Setup wizard
            KeyCode::Char('L') => Some(Action::Setup),

            _ => None,
        }
    }

    pub fn handle_input(key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc => Some(Action::Cancel),
            KeyCode::Enter => Some(Action::Submit),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Char(c) => Some(Action::Char(c)),
            _ => None,
        }
    }

    pub fn handle_help(key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') => Some(Action::Cancel),
            _ => Some(Action::Cancel), // Any key closes help
        }
    }

    pub fn handle_detail(key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::Cancel),
            KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
            KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
            KeyCode::Char('x') => Some(Action::ToggleTask),
            KeyCode::Char('o') => Some(Action::OpenInSource),
            KeyCode::Char('e') => Some(Action::EditTask),
            _ => None,
        }
    }

    pub fn handle_edit_form(key: KeyEvent, is_cycle_field: bool) -> Option<Action> {
        match key.code {
            KeyCode::Esc => Some(Action::EditFormCancel),
            KeyCode::Enter => Some(Action::EditFormSave),
            KeyCode::Char('j') | KeyCode::Down if is_cycle_field => Some(Action::EditFormDown),
            KeyCode::Char('k') | KeyCode::Up if is_cycle_field => Some(Action::EditFormUp),
            KeyCode::Tab | KeyCode::Down => Some(Action::EditFormDown),
            KeyCode::BackTab | KeyCode::Up => Some(Action::EditFormUp),
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if is_cycle_field => {
                Some(Action::EditFormCyclePriority)
            }
            KeyCode::Left => Some(Action::EditFormCursorLeft),
            KeyCode::Right => Some(Action::EditFormCursorRight),
            KeyCode::Backspace => Some(Action::EditFormBackspace),
            KeyCode::Char(c) if !is_cycle_field => Some(Action::EditFormChar(c)),
            _ => None,
        }
    }

    pub fn handle_agent_menu(key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('i') => Some(Action::AgentInteractive),
            KeyCode::Char('b') => Some(Action::AgentBackground),
            KeyCode::Esc => Some(Action::Cancel),
            _ => None,
        }
    }

    /// Handle key events during the setup wizard.
    /// The behaviour depends on which step is active.
    pub fn handle_setup(key: KeyEvent, step: &SetupStep) -> Option<SetupAction> {
        match step {
            SetupStep::Welcome => match key.code {
                KeyCode::Enter => Some(SetupAction::Submit),
                KeyCode::Esc => Some(SetupAction::Cancel),
                _ => None,
            },
            SetupStep::SelectBackends { .. } => match key.code {
                KeyCode::Char('j') | KeyCode::Down => Some(SetupAction::MoveDown),
                KeyCode::Char('k') | KeyCode::Up => Some(SetupAction::MoveUp),
                KeyCode::Char(' ') => Some(SetupAction::ToggleItem),
                KeyCode::Enter => Some(SetupAction::Submit),
                KeyCode::Esc => Some(SetupAction::Cancel),
                _ => None,
            },
            SetupStep::BackendName => match key.code {
                KeyCode::Esc => Some(SetupAction::Cancel),
                KeyCode::Enter => Some(SetupAction::Submit),
                KeyCode::Backspace => Some(SetupAction::Backspace),
                KeyCode::Char(c) => Some(SetupAction::Char(c)),
                _ => None,
            },
            SetupStep::ApiKey => match key.code {
                KeyCode::Esc => Some(SetupAction::Cancel),
                KeyCode::Enter => Some(SetupAction::Submit),
                KeyCode::Backspace => Some(SetupAction::Backspace),
                KeyCode::Char(c) => Some(SetupAction::Char(c)),
                _ => None,
            },
            SetupStep::ValidatingKey => {
                // No input accepted while validating.
                None
            }
            SetupStep::SelectTeam { .. } | SetupStep::SelectAssignee { .. } => match key.code {
                KeyCode::Esc => Some(SetupAction::Cancel),
                KeyCode::Enter => Some(SetupAction::Submit),
                KeyCode::Char('j') | KeyCode::Down => Some(SetupAction::MoveDown),
                KeyCode::Char('k') | KeyCode::Up => Some(SetupAction::MoveUp),
                _ => None,
            },
            SetupStep::SelectStatuses { .. } => match key.code {
                KeyCode::Esc => Some(SetupAction::Cancel),
                KeyCode::Enter => Some(SetupAction::Submit),
                KeyCode::Char('j') | KeyCode::Down => Some(SetupAction::MoveDown),
                KeyCode::Char('k') | KeyCode::Up => Some(SetupAction::MoveUp),
                KeyCode::Char(' ') => Some(SetupAction::ToggleItem),
                _ => None,
            },
            SetupStep::Complete => match key.code {
                _ => Some(SetupAction::AnyKey),
            },
            SetupStep::Error(_) => match key.code {
                _ => Some(SetupAction::AnyKey),
            },
        }
    }
}
