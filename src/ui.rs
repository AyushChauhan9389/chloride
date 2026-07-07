use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, AuthForm, InputKind, Mode, StatusKind, View, human_size};

const ACCENT: Color = Color::Cyan;

pub fn render(frame: &mut Frame, app: &App) {
    let [header, body, status, help] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_header(frame, header);
    match app.view {
        View::FileManager => render_list(frame, body, app),
        View::Files => render_files_list(frame, body, app),
    }
    render_status(frame, status, app);
    render_help(frame, help, app);

    match &app.mode {
        Mode::Input { kind, buffer } => render_input(frame, *kind, buffer),
        Mode::Confirm { name, is_dir } => render_confirm(frame, name, *is_dir),
        Mode::ConfirmRemoteDelete { name, .. } => render_confirm_remote_delete(frame, name),
        Mode::Message { title, text } => render_message(frame, title, text),
        Mode::Auth(form) => render_auth(frame, form),
        Mode::Quota(state) => render_quota(frame, state),
        Mode::Browse => {}
    }
}

fn render_header(frame: &mut Frame, area: Rect) {
    let title = Line::from(vec![
        Span::styled("🧪 ", Style::new()),
        Span::styled("Chloride", Style::new().fg(ACCENT).bold()),
        Span::styled(" TUI", Style::new().bold()),
    ]);
    frame.render_widget(
        Paragraph::new(title)
            .centered()
            .block(Block::bordered().border_style(Style::new().fg(ACCENT))),
        area,
    );
}

fn render_list(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .entries
        .iter()
        .map(|e| {
            let icon = if e.parent {
                "⬆  "
            } else if e.is_dir {
                "📁 "
            } else {
                "📄 "
            };

            let name_style = if e.is_dir {
                Style::new().fg(ACCENT).bold()
            } else {
                Style::new()
            };

            let mut spans = vec![Span::raw(icon), Span::styled(e.name.clone(), name_style)];
            if !e.is_dir && !e.parent {
                spans.push(Span::styled(
                    format!("  ·  {}", human_size(e.size)),
                    Style::new().dark_gray(),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = format!(" {} ", app.cwd.display());
    let count = Line::from(format!(" {} items ", app.entries.len()))
        .right_aligned()
        .dark_gray();

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(title)
                .title(count)
                .border_style(Style::new().fg(ACCENT)),
        )
        .highlight_style(
            Style::new()
                .bg(Color::Indexed(238))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    if !app.entries.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_files_list(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .remote_files
        .iter()
        .map(|f| {
            let mut spans = vec![
                Span::raw("📄 "),
                Span::styled(f.name.clone(), Style::new().fg(Color::White)),
            ];
            spans.push(Span::styled(
                format!("  ·  {}", human_size(f.size as u64)),
                Style::new().dark_gray(),
            ));
            if let Some(date) = f.created_at.split('T').next() {
                spans.push(Span::styled(
                    format!("  ·  {date}"),
                    Style::new().dark_gray(),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let count = Line::from(format!(" {} files ", app.remote_files.len()))
        .right_aligned()
        .dark_gray();

    let title = if app.config.is_logged_in() {
        format!(
            " My Files — {} ",
            app.config
                .user
                .as_ref()
                .map(|u| u.email.as_str())
                .unwrap_or("?")
        )
    } else {
        " My Files (not logged in) ".to_string()
    };

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(title)
                .title(count)
                .border_style(Style::new().fg(ACCENT)),
        )
        .highlight_style(
            Style::new()
                .bg(Color::Indexed(238))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    if !app.remote_files.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_status(frame: &mut Frame, area: Rect, app: &App) {
    let (symbol, color) = match app.status.kind {
        StatusKind::Info => ("•", Color::Gray),
        StatusKind::Success => ("✔", Color::Green),
        StatusKind::Error => ("✖", Color::Red),
    };
    let line = Line::from(vec![
        Span::styled(format!(" {symbol} "), Style::new().fg(color)),
        Span::styled(app.status.message.clone(), Style::new().fg(color)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_help(frame: &mut Frame, area: Rect, app: &App) {
    let key = |k: &'static str| Span::styled(k, Style::new().fg(ACCENT).bold());
    let sep = || Span::styled("  ", Style::new());
    let desc = |d: &'static str| Span::styled(d, Style::new().dark_gray());

    let common_right = vec![
        sep(),
        key("L"),
        desc("login"),
        sep(),
        key("S"),
        desc("signup"),
        sep(),
        key("u"),
        desc("quota"),
        sep(),
        key("r"),
        desc("refresh"),
        sep(),
        key("q"),
        desc("quit"),
    ];

    let line = match app.view {
        View::Files => Line::from({
            let mut v = vec![
                sep(),
                key("↑↓/jk"),
                desc(" move"),
                sep(),
                key("Enter"),
                desc(" info"),
                sep(),
                key("R"),
                desc(" regen"),
                sep(),
                key("c"),
                desc(" copy"),
                sep(),
                key("D"),
                desc(" download"),
                sep(),
                key("d"),
                desc(" delete"),
                sep(),
                key("f"),
                desc(" filemgr"),
            ];
            v.extend(common_right);
            v
        }),
        View::FileManager => Line::from({
            let mut v = vec![
                sep(),
                key("↑↓/jk"),
                desc(" move"),
                sep(),
                key("Enter"),
                desc(" open"),
                sep(),
                key("Bksp"),
                desc(" up"),
                sep(),
                key("t"),
                desc(" touch"),
                sep(),
                key("m"),
                desc(" mkdir"),
                sep(),
                key("d"),
                desc(" delete"),
                sep(),
                key("f"),
                desc(" files"),
            ];
            v.extend(common_right);
            v
        }),
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn render_input(frame: &mut Frame, kind: InputKind, buffer: &str) {
    let area = centered_rect(frame.area(), 60, 3);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(kind.title())
        .border_style(Style::new().fg(Color::Yellow));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    frame.render_widget(Paragraph::new(buffer), inner);
    // Draw the cursor at the end of the buffer.
    let cursor_x = inner.x + buffer.chars().count() as u16;
    frame.set_cursor_position((cursor_x.min(inner.x + inner.width.saturating_sub(1)), inner.y));
}

fn render_confirm(frame: &mut Frame, name: &str, is_dir: bool) {
    let area = centered_rect(frame.area(), 60, 5);
    frame.render_widget(Clear, area);

    let kind = if is_dir { "directory" } else { "file" };
    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Delete "),
            Span::styled(kind, Style::new().dark_gray()),
            Span::raw(" "),
            Span::styled(format!("'{name}'"), Style::new().fg(Color::Red).bold()),
            Span::raw("?"),
        ])
        .centered(),
        Line::from(vec![
            Span::styled("y", Style::new().fg(Color::Green).bold()),
            Span::raw(" confirm    "),
            Span::styled("n/Esc", Style::new().fg(ACCENT).bold()),
            Span::raw(" cancel"),
        ])
        .centered(),
    ];

    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .title(" Confirm delete ")
                .border_style(Style::new().fg(Color::Red)),
        ),
        area,
    );
}

fn render_confirm_remote_delete(frame: &mut Frame, name: &str) {
    let area = centered_rect(frame.area(), 64, 5);
    frame.render_widget(Clear, area);

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Delete remote file "),
            Span::styled(format!("'{name}'"), Style::new().fg(Color::Red).bold()),
            Span::raw("?"),
        ])
        .centered(),
        Line::from(vec![
            Span::styled("y", Style::new().fg(Color::Green).bold()),
            Span::raw(" confirm    "),
            Span::styled("n/Esc", Style::new().fg(ACCENT).bold()),
            Span::raw(" cancel"),
        ])
        .centered(),
    ];

    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .title(" Confirm remote delete ")
                .border_style(Style::new().fg(Color::Red)),
        ),
        area,
    );
}

fn render_message(frame: &mut Frame, title: &str, text: &str) {
    let area = centered_rect(frame.area(), 86, 7);
    frame.render_widget(Clear, area);

    let block = Block::bordered().title(title).border_style(Style::new().fg(ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(""),
        Line::from(text.to_string()),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Select/copy from terminal ", Style::new().dark_gray()),
            Span::styled("Esc/any key", Style::new().fg(ACCENT).bold()),
            Span::styled(" close", Style::new().dark_gray()),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_auth(frame: &mut Frame, form: &AuthForm) {
    let fields = form.field_count();
    // Layout: blank + fields + blank + status + blank + help
    let height = (fields + 5) as u16;
    let area = centered_rect(frame.area(), 58, height);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(form.kind.title())
        .border_style(Style::new().fg(ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let label_width = 12u16;
    let field_defs = [
        ("Email:", &form.email, false),
        ("Password:", &form.password, true),
        ("Confirm:", &form.confirm, true),
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, (label, value, is_pw)) in field_defs.iter().take(fields).enumerate() {
        let display = if *is_pw {
            "•".repeat(value.chars().count())
        } else {
            (**value).to_string()
        };
        let active = i == form.active;
        let val_style = if active {
            Style::new().fg(Color::Black).bg(ACCENT)
        } else {
            Style::new().fg(Color::White)
        };
        let label_str = format!(" {:<w$}", label, w = (label_width - 2) as usize);
        lines.push(Line::from(vec![
            Span::styled(label_str, Style::new().dark_gray()),
            Span::styled(" ", Style::new()),
            Span::styled(display, val_style),
        ]));
    }

    lines.push(Line::from(""));

    if let Some(err) = &form.error {
        lines.push(Line::from(vec![
            Span::styled(" ✖ ", Style::new().fg(Color::Red)),
            Span::styled(err.clone(), Style::new().fg(Color::Red)),
        ]));
    } else {
        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![
        Span::styled(" Tab ", Style::new().fg(ACCENT).bold()),
        Span::styled("next   ", Style::new().dark_gray()),
        Span::styled("Enter ", Style::new().fg(ACCENT).bold()),
        Span::styled("submit   ", Style::new().dark_gray()),
        Span::styled("Esc ", Style::new().fg(ACCENT).bold()),
        Span::styled("cancel", Style::new().dark_gray()),
    ]));

    frame.render_widget(Paragraph::new(lines), inner);

    // Cursor at end of the active field's value.
    let active_value = match form.active {
        0 => &form.email,
        1 => &form.password,
        _ => &form.confirm,
    };
    let cursor_x = inner.x + label_width + active_value.chars().count() as u16;
    let cursor_y = inner.y + 1 + form.active as u16;
    frame.set_cursor_position((
        cursor_x.min(inner.x + inner.width.saturating_sub(2)),
        cursor_y,
    ));
}

fn render_quota(frame: &mut Frame, state: &Option<Result<crate::api::StorageInfo, String>>) {
    let area = centered_rect(frame.area(), 56, 11);
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(" Storage Quota ")
        .border_style(Style::new().fg(ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    match state {
        Some(Ok(info)) => {
            if info.is_unlimited() {
                lines.push(Line::from(vec![
                    Span::styled(" Used:      ", Style::new().dark_gray()),
                    Span::styled(
                        info.used_formatted.clone().unwrap_or_else(|| "?".into()),
                        Style::new().fg(Color::White),
                    ),
                ]));
                lines.push(Line::from(vec![
                    Span::styled(" Limit:     ", Style::new().dark_gray()),
                    Span::styled("∞ Unlimited", Style::new().fg(Color::Green).bold()),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from("No storage cap — upload as much as you want.").dark_gray());
            } else {
                let used = info.used_formatted.clone().unwrap_or_else(|| "?".into());
                let left = info.left_formatted.clone().unwrap_or_else(|| "?".into());
                let limit = info.limit_formatted.clone().unwrap_or_else(|| "?".into());
                let pct = info.percentageUsed;

                lines.push(Line::from(vec![
                    Span::styled(" Used:      ", Style::new().dark_gray()),
                    Span::styled(used, Style::new().fg(Color::White)),
                ]));
                lines.push(Line::from(vec![
                    Span::styled(" Free:      ", Style::new().dark_gray()),
                    Span::styled(
                        left.clone(),
                        if info.left <= 0 {
                            Style::new().fg(Color::Red).bold()
                        } else {
                            Style::new().fg(Color::Green)
                        },
                    ),
                ]));
                lines.push(Line::from(vec![
                    Span::styled(" Limit:     ", Style::new().dark_gray()),
                    Span::styled(limit, Style::new().fg(Color::White)),
                ]));
                lines.push(Line::from(""));

                let bar_width = 28usize;
                let filled = ((pct / 100.0) * bar_width as f64).round() as usize;
                let filled = filled.min(bar_width);
                let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);
                let bar_color = if pct >= 90.0 {
                    Color::Red
                } else if pct >= 75.0 {
                    Color::Yellow
                } else {
                    Color::Green
                };
                lines.push(Line::from(vec![
                    Span::styled(" ", Style::new()),
                    Span::styled(bar, Style::new().fg(bar_color)),
                    Span::styled(format!(" {pct:.1}%"), Style::new().fg(bar_color).bold()),
                ]));
            }
        }
        Some(Err(msg)) => {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(" ✖ ", Style::new().fg(Color::Red)),
                Span::styled(msg.clone(), Style::new().fg(Color::Red)),
            ]));
        }
        None => {
            lines.push(Line::from(""));
            lines.push(Line::from(" Loading… ").dark_gray());
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" Esc ", Style::new().fg(ACCENT).bold()),
        Span::styled("any key to close", Style::new().dark_gray()),
    ]));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// A centered rectangle of the given width/height (in cells) within `area`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let [vertical] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [rect] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(vertical);
    rect
}
