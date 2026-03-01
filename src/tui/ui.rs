use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::backends::linear::setup::SetupStep;
use crate::tui::app::{App, AppMode, EditFieldKind};
use crate::tui::theme::Theme;
use crate::tui::views::{quick_add, task_detail, task_list};

pub fn render(f: &mut Frame, app: &App, theme: &Theme) {
    let area = f.area();

    f.render_widget(
        ratatui::widgets::Block::default().style(theme.style_base()),
        area,
    );

    match app.mode {
        AppMode::Normal | AppMode::Input => {
            task_list::draw_task_list(f, app, theme, area);

            if app.mode == AppMode::Input {
                quick_add::draw_input(f, app, theme, area);
            }
        }
        AppMode::Help => {
            task_list::draw_task_list(f, app, theme, area);
            task_list::draw_help(f, theme, area);
        }
        AppMode::AgentMenu => {
            task_list::draw_task_list(f, app, theme, area);
            // Agent menu overlay will be implemented in a later task
        }
        AppMode::DetailView => {
            task_list::draw_task_list(f, app, theme, area);
            task_detail::draw_detail(f, app, theme, area);
        }
        AppMode::EditForm => {
            task_list::draw_task_list(f, app, theme, area);
            draw_edit_form(f, app, theme, area);
        }
        AppMode::Setup(ref step) => {
            draw_setup(f, app, step, theme, area);
        }
    }
}

// ---------------------------------------------------------------------------
// Edit form rendering
// ---------------------------------------------------------------------------

fn draw_edit_form(f: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let form = match &app.edit_form {
        Some(form) => form,
        None => return,
    };

    let field_count = form.fields.len() as u16;
    // Each field takes 3 lines (label+input+gap), plus title (2) + hint (2) + padding (2)
    let popup_height = (field_count * 3 + 6).min(area.height.saturating_sub(4));
    let popup_width = 60u16.min(area.width.saturating_sub(4));

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(popup_width)) / 2,
        y: area.y + (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    let outer_block = Block::default()
        .title(" Edit Task ")
        .borders(Borders::ALL)
        .border_style(theme.style_accent())
        .style(theme.style_base());

    f.render_widget(Clear, popup_area);
    f.render_widget(outer_block.clone(), popup_area);

    let inner = outer_block.inner(popup_area);

    // Build constraints: one Length(3) per field + hint + filler
    let mut constraints: Vec<Constraint> = form.fields.iter().map(|_| Constraint::Length(3)).collect();
    constraints.push(Constraint::Length(2)); // hint
    constraints.push(Constraint::Min(0));   // filler

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (i, field) in form.fields.iter().enumerate() {
        let is_active = i == form.cursor;
        let border_style = if is_active {
            theme.style_accent()
        } else {
            theme.style_muted()
        };

        match field.kind {
            EditFieldKind::Cycle => {
                let display = format!("< {} >", field.value);
                let block = Block::default()
                    .title(format!(" {} ", field.label))
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .style(theme.style_base());

                let text_style = if is_active {
                    theme.style_accent().add_modifier(Modifier::BOLD)
                } else {
                    theme.style_default()
                };

                let para = Paragraph::new(display).block(block).style(text_style);
                f.render_widget(para, chunks[i]);
            }
            EditFieldKind::Text => {
                let block = Block::default()
                    .title(format!(" {} ", field.label))
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .style(theme.style_base());

                let para = Paragraph::new(field.value.as_str())
                    .block(block)
                    .style(theme.style_default());
                f.render_widget(para, chunks[i]);

                if is_active {
                    let max_visible = chunks[i].width.saturating_sub(2) as usize;
                    let cursor_x = chunks[i].x + field.cursor_pos.min(max_visible) as u16 + 1;
                    let cursor_y = chunks[i].y + 1;
                    f.set_cursor_position((cursor_x, cursor_y));
                }
            }
        }
    }

    let hint_idx = form.fields.len();
    if hint_idx < chunks.len() {
        let hint = Line::from(vec![
            Span::styled("↑/↓", theme.style_accent()),
            Span::styled(" navigate  ", theme.style_muted()),
            Span::styled("←/→", theme.style_accent()),
            Span::styled(" cycle  ", theme.style_muted()),
            Span::styled("Enter", theme.style_accent()),
            Span::styled(" save  ", theme.style_muted()),
            Span::styled("Esc", theme.style_accent()),
            Span::styled(" cancel", theme.style_muted()),
        ]);
        let hint_para = Paragraph::new(Text::from(vec![hint])).alignment(Alignment::Center);
        f.render_widget(hint_para, chunks[hint_idx]);
    }
}

// ---------------------------------------------------------------------------
// Setup wizard rendering
// ---------------------------------------------------------------------------

fn draw_setup(f: &mut Frame, app: &App, step: &SetupStep, theme: &Theme, area: Rect) {
    let popup_area = crate::tui::views::centered_rect(70, 80, area);

    let outer_block = Block::default()
        .title(" Setup Wizard ")
        .borders(Borders::ALL)
        .border_style(theme.style_accent())
        .style(theme.style_base());

    f.render_widget(Clear, popup_area);
    f.render_widget(outer_block.clone(), popup_area);

    let inner = outer_block.inner(popup_area);

    match step {
        SetupStep::Welcome => draw_setup_welcome(f, theme, inner),
        SetupStep::SelectBackends { options, selected, cursor } => {
            draw_setup_select_backends(f, theme, inner, options, selected, *cursor);
        }
        SetupStep::BackendName => draw_setup_backend_name(f, app, theme, inner),
        SetupStep::ApiKey => draw_setup_api_key(f, app, theme, inner),
        SetupStep::ValidatingKey => draw_setup_validating(f, theme, inner),
        SetupStep::SelectTeam { teams, selected } => {
            draw_setup_select_team(f, theme, inner, teams, *selected);
        }
        SetupStep::SelectAssignee { members, selected } => {
            draw_setup_select_assignee(f, theme, inner, members, *selected);
        }
        SetupStep::SelectStatuses {
            states,
            selected,
            cursor,
        } => {
            draw_setup_select_statuses(f, theme, inner, states, selected, *cursor);
        }
        SetupStep::Complete => draw_setup_complete(f, app, theme, inner),
        SetupStep::Error(msg) => draw_setup_error(f, theme, inner, msg),
    }
}

fn draw_setup_welcome(f: &mut Frame, theme: &Theme, area: Rect) {
    let lines = vec![
        Line::from(Span::styled(
            "Welcome to Dewey!",
            theme.style_accent().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "A unified task manager for all your backends.",
            theme.style_default(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Let's set up your task sources.",
            theme.style_muted(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to continue",
            theme.style_accent(),
        )),
    ];

    let text = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
    let centered = centered_vertical(area, 7);
    f.render_widget(text, centered);
}

fn draw_setup_select_backends(
    f: &mut Frame,
    theme: &Theme,
    area: Rect,
    options: &[crate::backends::linear::setup::BackendOption],
    selected: &[bool],
    cursor: usize,
) {
    // Title (2 lines) + spacer (1) + options (2 lines each) + spacer (1) + hint (1) + filler
    let option_lines = options.len() as u16 * 2;
    let constraints = vec![
        Constraint::Length(2),            // title
        Constraint::Length(1),            // spacer
        Constraint::Length(option_lines), // options
        Constraint::Length(1),            // spacer
        Constraint::Length(1),            // hint
        Constraint::Min(0),              // filler
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let title = Paragraph::new("Select your task sources")
        .style(theme.style_accent().add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    // Build option lines
    let mut lines: Vec<Line> = Vec::new();
    for (i, opt) in options.iter().enumerate() {
        let is_cursor = i == cursor;
        let is_selected = selected.get(i).copied().unwrap_or(false);
        let checkbox = if is_selected { "[x]" } else { "[ ]" };
        let prefix = if is_cursor { "> " } else { "  " };

        let name_style = if is_cursor {
            theme.style_selected()
        } else {
            theme.style_default()
        };

        let desc_style = if is_cursor {
            theme.style_selected()
        } else {
            theme.style_muted()
        };

        lines.push(Line::from(Span::styled(
            format!("{prefix}{checkbox} {}", opt.name),
            name_style,
        )));
        lines.push(Line::from(Span::styled(
            format!("        {}", opt.description),
            desc_style,
        )));
    }

    let options_text = Paragraph::new(Text::from(lines));
    f.render_widget(options_text, chunks[2]);

    let hint = Line::from(vec![
        Span::styled("j/k", theme.style_accent()),
        Span::styled(" navigate | ", theme.style_muted()),
        Span::styled("Space", theme.style_accent()),
        Span::styled(" toggle | ", theme.style_muted()),
        Span::styled("Enter", theme.style_accent()),
        Span::styled(" continue | ", theme.style_muted()),
        Span::styled("Esc", theme.style_accent()),
        Span::styled(" cancel", theme.style_muted()),
    ]);
    let hint_para = Paragraph::new(Text::from(vec![hint])).alignment(Alignment::Center);
    f.render_widget(hint_para, chunks[4]);
}

fn draw_setup_backend_name(f: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // title
            Constraint::Length(1), // spacer
            Constraint::Length(2), // description
            Constraint::Length(1), // spacer
            Constraint::Length(3), // input field
            Constraint::Length(2), // hint
            Constraint::Min(0),   // filler
        ])
        .split(area);

    let title = Paragraph::new("Name this Linear backend")
        .style(theme.style_accent().add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    let desc = Paragraph::new("Choose a short name like \"work\" or \"personal\" to identify this account.")
        .style(theme.style_muted())
        .alignment(Alignment::Center);
    f.render_widget(desc, chunks[2]);

    let input_block = Block::default()
        .title(" Backend Name ")
        .borders(Borders::ALL)
        .border_style(theme.style_accent())
        .style(theme.style_base());

    let input = Paragraph::new(app.input_buffer.as_str())
        .block(input_block)
        .style(theme.style_default());
    f.render_widget(input, chunks[4]);

    let cursor_x = chunks[4].x + app.input_buffer.len().min(chunks[4].width.saturating_sub(2) as usize) as u16 + 1;
    let cursor_y = chunks[4].y + 1;
    f.set_cursor_position((cursor_x, cursor_y));

    let hint = Paragraph::new("Enter to continue | Esc to cancel")
        .style(theme.style_muted())
        .alignment(Alignment::Center);
    f.render_widget(hint, chunks[5]);
}

fn draw_setup_api_key(f: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // instructions
            Constraint::Length(1), // spacer
            Constraint::Length(1), // URL
            Constraint::Length(2), // spacer
            Constraint::Length(3), // input field
            Constraint::Length(2), // hint
            Constraint::Min(0),   // filler
        ])
        .split(area);

    let title = Paragraph::new("Step 1: Enter your Linear API key")
        .style(theme.style_accent().add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    let instructions = Paragraph::new(
        "Create a personal API key at: https://linear.app/settings/account/security",
    )
    .style(theme.style_default())
    .alignment(Alignment::Center);
    f.render_widget(instructions, chunks[2]);

    let input_block = Block::default()
        .title(" API Key ")
        .borders(Borders::ALL)
        .border_style(theme.style_accent())
        .style(theme.style_base());

    // Mask the API key with asterisks for privacy, showing only the last 4 chars.
    let display_text = if app.input_buffer.len() > 4 {
        let hidden = "*".repeat(app.input_buffer.len() - 4);
        let visible = &app.input_buffer[app.input_buffer.len() - 4..];
        format!("{hidden}{visible}")
    } else {
        app.input_buffer.clone()
    };

    let input = Paragraph::new(display_text)
        .block(input_block)
        .style(theme.style_default());
    f.render_widget(input, chunks[4]);

    // Place cursor at the end of input.
    let cursor_x = chunks[4].x + app.input_buffer.len().min(chunks[4].width.saturating_sub(2) as usize) as u16 + 1;
    let cursor_y = chunks[4].y + 1;
    f.set_cursor_position((cursor_x, cursor_y));

    let hint = Paragraph::new("Enter to validate | Esc to cancel")
        .style(theme.style_muted())
        .alignment(Alignment::Center);
    f.render_widget(hint, chunks[5]);
}

fn draw_setup_validating(f: &mut Frame, theme: &Theme, area: Rect) {
    let text = Paragraph::new("Validating API key...")
        .style(theme.style_accent().add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);

    let centered = centered_vertical(area, 1);
    f.render_widget(text, centered);
}

fn draw_setup_select_team(
    f: &mut Frame,
    theme: &Theme,
    area: Rect,
    teams: &[crate::backends::linear::setup::SetupTeam],
    selected: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // title
            Constraint::Length(1), // spacer
            Constraint::Min(3),   // list
            Constraint::Length(1), // hint
        ])
        .split(area);

    let title = Paragraph::new("Step 2: Select your team")
        .style(theme.style_accent().add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = teams
        .iter()
        .enumerate()
        .map(|(i, team)| {
            let prefix = if i == selected { "> " } else { "  " };
            let style = if i == selected {
                theme.style_selected().add_modifier(Modifier::BOLD)
            } else {
                theme.style_default()
            };
            ListItem::new(format!("{prefix}{} ({})", team.name, team.key)).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.style_muted()),
    );
    f.render_widget(list, chunks[2]);

    let hint = Paragraph::new("j/k to navigate | Enter to select | Esc to go back")
        .style(theme.style_muted())
        .alignment(Alignment::Center);
    f.render_widget(hint, chunks[3]);
}

fn draw_setup_select_assignee(
    f: &mut Frame,
    theme: &Theme,
    area: Rect,
    members: &[crate::backends::linear::setup::SetupUser],
    selected: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // title
            Constraint::Length(1), // spacer
            Constraint::Min(3),   // list
            Constraint::Length(1), // hint
        ])
        .split(area);

    let title = Paragraph::new("Step 3: Select assignee filter")
        .style(theme.style_accent().add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    let mut items: Vec<ListItem> = Vec::new();

    // First item: "Only my issues"
    let prefix = if selected == 0 { "> " } else { "  " };
    let style = if selected == 0 {
        theme.style_selected().add_modifier(Modifier::BOLD)
    } else {
        theme.style_default()
    };
    items.push(ListItem::new(format!("{prefix}Only my issues")).style(style));

    // Team members
    for (i, member) in members.iter().enumerate() {
        let idx = i + 1;
        let prefix = if idx == selected { "> " } else { "  " };
        let style = if idx == selected {
            theme.style_selected().add_modifier(Modifier::BOLD)
        } else {
            theme.style_default()
        };
        let label = if member.email.is_empty() {
            member.name.clone()
        } else {
            format!("{} ({})", member.name, member.email)
        };
        items.push(ListItem::new(format!("{prefix}{label}")).style(style));
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.style_muted()),
    );
    f.render_widget(list, chunks[2]);

    let hint = Paragraph::new("j/k to navigate | Enter to select | Esc to go back")
        .style(theme.style_muted())
        .alignment(Alignment::Center);
    f.render_widget(hint, chunks[3]);
}

fn draw_setup_select_statuses(
    f: &mut Frame,
    theme: &Theme,
    area: Rect,
    states: &[crate::backends::linear::setup::SetupState],
    selected: &[bool],
    cursor: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // title
            Constraint::Length(1), // spacer
            Constraint::Min(3),   // list
            Constraint::Length(1), // hint
        ])
        .split(area);

    let title = Paragraph::new("Step 4: Select workflow statuses to show")
        .style(theme.style_accent().add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = states
        .iter()
        .enumerate()
        .map(|(i, state)| {
            let is_cursor = i == cursor;
            let is_selected = selected.get(i).copied().unwrap_or(false);
            let checkbox = if is_selected { "[x]" } else { "[ ]" };
            let prefix = if is_cursor { "> " } else { "  " };
            let style = if is_cursor {
                theme.style_selected().add_modifier(Modifier::BOLD)
            } else {
                theme.style_default()
            };
            ListItem::new(format!(
                "{prefix}{checkbox} {} ({})",
                state.name, state.state_type
            ))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme.style_muted()),
    );
    f.render_widget(list, chunks[2]);

    let hint =
        Paragraph::new("j/k to navigate | Space to toggle | Enter to confirm | Esc to go back")
            .style(theme.style_muted())
            .alignment(Alignment::Center);
    f.render_widget(hint, chunks[3]);
}

fn draw_setup_complete(f: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Setup Complete!",
            theme.style_success().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Configured backends:",
            theme.style_muted(),
        )),
    ];

    if let Some(ref state) = app.setup_state {
        for backend in &state.selected_backends {
            lines.push(Line::from(Span::styled(
                format!("  - {backend}"),
                theme.style_default(),
            )));
        }

        // If linear was configured, show user, team, and backend name
        if state.selected_backends.contains(&"linear".to_string()) {
            let user_name = state
                .user
                .as_ref()
                .map(|u| u.name.as_str())
                .unwrap_or("(unknown)");
            let team_name = state
                .team
                .as_ref()
                .map(|t| t.name.as_str())
                .unwrap_or("(unknown)");

            lines.push(Line::from(""));
            if let Some(ref name) = state.backend_name {
                lines.push(Line::from(vec![
                    Span::styled("Backend: ", theme.style_muted()),
                    Span::styled(
                        format!("@{name}"),
                        theme.style_accent(),
                    ),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("User: ", theme.style_muted()),
                Span::styled(user_name, theme.style_default()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Team: ", theme.style_muted()),
                Span::styled(team_name, theme.style_default()),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press any key to start using Dewey",
        theme.style_accent(),
    )));

    let line_count = lines.len() as u16;
    let text = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
    let centered = centered_vertical(area, line_count);
    f.render_widget(text, centered);
}

fn draw_setup_error(f: &mut Frame, theme: &Theme, area: Rect, msg: &str) {
    let lines = vec![
        Line::from(Span::styled(
            "Error",
            theme.style_error().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(msg.to_string(), theme.style_error())),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to go back.",
            theme.style_muted(),
        )),
    ];

    let text = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
    let centered = centered_vertical(area, 5);
    f.render_widget(text, centered);
}

/// Helper: create a vertically-centered rect of a given height.
fn centered_vertical(area: Rect, height: u16) -> Rect {
    let top_pad = area.height.saturating_sub(height) / 2;
    Rect {
        x: area.x,
        y: area.y + top_pad,
        width: area.width,
        height: height.min(area.height),
    }
}
