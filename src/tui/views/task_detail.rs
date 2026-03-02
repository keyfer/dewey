use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::model::{Priority, TaskStatus};
use crate::tui::app::App;
use crate::tui::theme::Theme;

pub fn draw_detail(f: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let task = match app.get_selected_visible_task() {
        Some(t) => t,
        None => return,
    };

    let popup_area = super::centered_rect(70, 80, area);

    let block = Block::default()
        .title(" Task Detail ")
        .borders(Borders::ALL)
        .border_style(theme.style_accent())
        .style(theme.style_base());

    f.render_widget(Clear, popup_area);
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

    // Split inner area into content and bottom hint
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let content_area = chunks[0];
    let hint_area = chunks[1];

    // Build content lines
    let mut lines: Vec<Line> = Vec::new();

    // Title (bold)
    lines.push(Line::from(Span::styled(
        task.title.clone(),
        theme.style_accent().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Status
    let status_str = match task.status {
        TaskStatus::Pending => "Pending",
        TaskStatus::Done => "Done",
    };
    let status_style = match task.status {
        TaskStatus::Pending => theme.style_warning(),
        TaskStatus::Done => theme.style_success(),
    };
    lines.push(Line::from(vec![
        Span::styled("Status: ", theme.style_muted()),
        Span::styled(status_str, status_style),
    ]));

    // Priority
    let (priority_str, priority_style) = match task.priority {
        Priority::None => ("None", theme.style_muted()),
        Priority::Low => ("Low", theme.style_muted()),
        Priority::Medium => ("Medium", theme.style_warning()),
        Priority::High => ("High", theme.style_error()),
    };
    lines.push(Line::from(vec![
        Span::styled("Priority: ", theme.style_muted()),
        Span::styled(priority_str, priority_style),
    ]));

    // Due
    let due_str = match task.due {
        Some(d) => d.to_string(),
        None => "None".to_string(),
    };
    lines.push(Line::from(vec![
        Span::styled("Due: ", theme.style_muted()),
        Span::styled(due_str, theme.style_default()),
    ]));

    // Tags
    let tags_str = if task.tags.is_empty() {
        "None".to_string()
    } else {
        task.tags.join(", ")
    };
    lines.push(Line::from(vec![
        Span::styled("Tags: ", theme.style_muted()),
        Span::styled(tags_str, theme.style_highlight()),
    ]));

    // Backend
    lines.push(Line::from(vec![
        Span::styled("Backend: ", theme.style_muted()),
        Span::styled(task.backend_key.clone(), theme.style_default()),
    ]));

    // Project
    let project_str = task
        .project
        .as_deref()
        .unwrap_or("None");
    lines.push(Line::from(vec![
        Span::styled("Project: ", theme.style_muted()),
        Span::styled(project_str.to_string(), theme.style_accent()),
    ]));

    lines.push(Line::from(""));

    // Description
    if let Some(ref desc) = task.description {
        lines.push(Line::from(Span::styled(
            "Description:",
            theme.style_muted().add_modifier(Modifier::BOLD),
        )));
        for line in desc.lines() {
            lines.push(Line::from(Span::styled(line.to_string(), theme.style_default())));
        }
        lines.push(Line::from(""));
    }

    // ID
    lines.push(Line::from(vec![
        Span::styled("ID: ", theme.style_muted()),
        Span::styled(task.id.clone(), theme.style_default()),
    ]));

    // Source path (as URL substitute)
    if let Some(ref path) = task.source_path {
        lines.push(Line::from(vec![
            Span::styled("Path: ", theme.style_muted()),
            Span::styled(path.clone(), theme.style_default()),
        ]));
    }

    // Created
    let created_str = match task.created_at {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => "\u{2014}".to_string(),
    };
    lines.push(Line::from(vec![
        Span::styled("Created: ", theme.style_muted()),
        Span::styled(created_str, theme.style_default()),
    ]));

    // Completed
    let completed_str = match task.completed_at {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => "\u{2014}".to_string(),
    };
    lines.push(Line::from(vec![
        Span::styled("Completed: ", theme.style_muted()),
        Span::styled(completed_str, theme.style_default()),
    ]));

    let content = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(content, content_area);

    // Bottom hint line
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("j/k", theme.style_accent()),
        Span::styled(" scroll  ", theme.style_muted()),
        Span::styled("Esc", theme.style_accent()),
        Span::styled(" close  ", theme.style_muted()),
        Span::styled("x", theme.style_accent()),
        Span::styled(" toggle  ", theme.style_muted()),
        Span::styled("e", theme.style_accent()),
        Span::styled(" edit  ", theme.style_muted()),
        Span::styled("o", theme.style_accent()),
        Span::styled(" open", theme.style_muted()),
    ]));
    f.render_widget(hint, hint_area);
}
