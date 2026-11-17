//! The quiet text workspace below the clock: reminders, objectives and conversation.
use crate::{
    ai::Provider,
    app::{App, Overlay},
    planner::Kind,
    planning_ui::{chat_controls, chat_status, clipped, colors, input_line, wrap_lines},
    ui::text,
};
use chrono::NaiveDate;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

fn line(f: &mut Frame, area: Rect, row: u16, value: impl Into<String>, color: Color) {
    if row < area.height {
        text(
            f,
            area.x,
            area.y + row,
            area.width,
            clipped(&value.into(), area.width),
            color,
            Alignment::Left,
        );
    }
}

pub fn render(f: &mut Frame, app: &App, area: Rect, date: NaiveDate, elapsed: std::time::Duration) {
    let [accent, secondary, body, muted] = colors(app.config.palette());
    // Keep the scene visible at low contrast behind the workspace text.
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = f.buffer_mut().get_mut(x, y);
            let backing = match (cell.bg, crate::theme::BG) {
                (Color::Rgb(r, g, b), Color::Rgb(br, bg, bb)) => {
                    let blend = |scene: u8, theme: u8| {
                        ((u16::from(scene) + 3 * u16::from(theme)) / 4) as u8
                    };
                    Color::Rgb(blend(r, br), blend(g, bg), blend(b, bb))
                }
                _ => crate::theme::BG,
            };
            cell.reset();
            cell.set_bg(backing);
        }
    }
    let area = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );
    if let Overlay::Conversation(view) = &app.overlay {
        chat(f, app, view, area, elapsed);
        return;
    }
    if let Overlay::Planner(view) = &app.overlay {
        if view.editor.is_some() {
            entries(f, app, view, area, date);
            return;
        }
    }
    let active = match &app.overlay {
        Overlay::Planner(p) if p.kind == Kind::Character => 0,
        Overlay::Planner(_) => 1,
        Overlay::Conversation(_) => 2,
        _ => usize::MAX,
    };
    let labels = if area.width >= 66 {
        ["h Character", "g Tasks & objectives", "c Talk", "+ Add"]
    } else if area.width >= 40 {
        ["h Character", "g Tasks", "c Talk", "+ Add"]
    } else {
        ["h Self", "g Tasks", "c Talk", "+ Add"]
    };
    let mut tabs = Vec::new();
    for (i, label) in labels.into_iter().enumerate() {
        if i > 0 {
            tabs.push(Span::styled("  ", Style::default().fg(muted)));
        }
        tabs.push(Span::styled(
            format!("{} {label}", if i == active { "›" } else { " " }),
            Style::default().fg(if i == active { secondary } else { accent }),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(tabs)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if area.height < 3 {
        return;
    }
    let gap = if area.height < 12 { 1 } else { 2 };
    let content = Rect::new(area.x, area.y + gap, area.width, area.height - gap);
    match &app.overlay {
        Overlay::Planner(view) => entries(f, app, view, content, date),
        _ => overview(f, app, content, date, [accent, secondary, body, muted]),
    }
}

fn overview(f: &mut Frame, app: &App, area: Rect, date: NaiveDate, palette: [Color; 4]) {
    let [accent, _, body, muted] = palette;
    if let Some(error) = &app.planner.error {
        line(
            f,
            area,
            0,
            format!("Goals couldn't be loaded · g for details: {error}"),
            accent,
        );
        return;
    }
    let reminders: Vec<_> = app
        .planner
        .data
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Character && !e.done && !e.archived)
        .collect();
    let tasks = app.planner.data.pending(date);
    let brief = if app.ai_daily_data.as_ref() == Some(&app.planner.data)
        && app.ai_daily_settings.as_ref() == Some(&app.config.ai)
        && app.ai_daily_date == Some(date)
        && app.config.ai.provider != Provider::Off
    {
        app.ai_daily.as_ref()
    } else {
        None
    };
    let next_step = brief.and_then(|brief| brief.next_step.as_deref());
    let tip = brief.and_then(|brief| brief.tip);
    let (short, long) = app.planner.data.goals_for(tasks.first().copied());
    if area.height < 5 || area.width < 58 {
        let task = tasks
            .first()
            .map_or("+ to add a task", |entry| entry.title.as_str());
        let reminder = reminders
            .first()
            .map_or("h then + to add a reminder", |entry| entry.title.as_str());
        line(
            f,
            area,
            0,
            format!(
                "{} {task}",
                if tasks.is_empty() {
                    "Next ·"
                } else {
                    "Start with:"
                }
            ),
            body,
        );
        line(f, area, 1, format!("Character · {reminder}"), body);
        if let Some(step) = next_step {
            line(f, area, 2, format!("d Refresh · AI · {step}"), body);
        } else if let Some(goal) = short.or(long) {
            line(f, area, 2, format!("Toward · {}", goal.title), body);
        }
        if let Some(tip) = tip {
            line(f, area, 3, format!("Tip · {tip}"), body);
        } else if app.config.ai.provider != Provider::Off && brief.is_none() {
            line(f, area, 3, "d Ask AI for a next step", accent);
        }
        return;
    }
    let advice_rows = if tip.is_some() && area.height >= 6 {
        2
    } else if app.config.ai.provider != Provider::Off {
        1
    } else {
        0
    };
    let left_width = (area.width * 2 / 5).min(37);
    let left = Rect::new(area.x, area.y, left_width, area.height);
    let right = Rect::new(
        area.x + left_width + 4,
        area.y,
        area.width - left_width - 4,
        area.height,
    );
    line(f, left, 0, "CHARACTER", accent);
    line(f, right, 0, "TASKS & OBJECTIVES", accent);
    let rows = area.height.saturating_sub(5).clamp(1, 4);
    if reminders.is_empty() {
        line(f, left, 2, "What would you like to practise?", body);
        line(f, left, 3, "h then + adds a reminder", muted);
    }
    for (i, entry) in reminders.iter().take(usize::from(rows)).enumerate() {
        line(f, left, 2 + i as u16, format!("· {}", entry.title), body);
    }
    if tasks.is_empty() {
        line(f, right, 2, "A small step is enough.", body);
        line(f, right, 3, "+ adds a task · g edits objectives", muted);
    }
    for (i, entry) in tasks.iter().take(usize::from(rows)).enumerate() {
        let due = match entry
            .due
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        {
            Some(d) if d < date => "overdue · ",
            Some(d) if d == date => "today · ",
            _ => "",
        };
        line(
            f,
            right,
            2 + i as u16,
            format!(
                "{} {}{title}",
                if i == 0 { "›" } else { "·" },
                due,
                title = entry.title
            ),
            body,
        );
    }
    let goal_row = (tasks.len().min(usize::from(rows)) as u16).max(2) + 3;
    if goal_row < area.height - advice_rows {
        if let Some(goal) = short.or(long) {
            line(
                f,
                right,
                goal_row,
                format!("Toward · {}", goal.title),
                muted,
            );
        }
    }
    if area.height >= 9 && goal_row + 1 < area.height - advice_rows {
        if let (Some(short), Some(long)) = (short, long) {
            if short.id != long.id {
                line(
                    f,
                    right,
                    goal_row + 1,
                    format!("Long term · {}", long.title),
                    muted,
                );
            }
        }
    }
    if app.config.ai.provider != Provider::Off {
        let footer = area.height - advice_rows;
        let caption = next_step.map_or_else(
            || "d Ask AI for a next step".into(),
            |step| format!("d Refresh · AI · {step}"),
        );
        line(f, area, footer, caption, body);
        if advice_rows == 2 {
            if let Some(tip) = tip {
                line(f, area, footer + 1, format!("Tip · {tip}"), body);
            }
        }
    }
}

fn entries(f: &mut Frame, app: &App, view: &crate::planning::Panel, area: Rect, date: NaiveDate) {
    let [accent, secondary, body, muted] = colors(app.config.palette());
    let character = view.kind == Kind::Character;
    if let Some(editor) = &view.editor {
        let fields = if !character && area.height >= 15 {
            (0..4).collect::<Vec<_>>()
        } else {
            vec![editor.field]
        };
        let labels = [
            "Title",
            "Linked objective · ←→ choose",
            "Due date · YYYY-MM-DD or blank",
            "Priority · ←→ choose",
        ];
        for (i, field) in fields.into_iter().enumerate() {
            let y = i as u16 * 3;
            line(
                f,
                area,
                y,
                format!(
                    "{} {}",
                    if field == editor.field { "›" } else { " " },
                    labels[field]
                ),
                secondary,
            );
            let value = match field {
                0 => editor.title.value.clone(),
                1 => editor
                    .entry
                    .parent
                    .and_then(|id| app.planner.data.entry(id))
                    .map(|e| e.title.clone())
                    .unwrap_or_else(|| "No linked objective".into()),
                2 => editor.due.value.clone(),
                _ => editor.entry.priority.label().into(),
            };
            let input = Rect::new(area.x + 2, area.y + y + 1, area.width.saturating_sub(2), 1);
            if field == editor.field && (field == 0 || field == 2) {
                input_line(
                    f,
                    input,
                    if field == 0 {
                        &editor.title
                    } else {
                        &editor.due
                    },
                    body,
                );
            } else {
                text(
                    f,
                    input.x,
                    input.y,
                    input.width,
                    value,
                    body,
                    Alignment::Left,
                );
            }
        }
        if area.height >= 5 {
            line(
                f,
                area,
                area.height - 2,
                if character {
                    "A quality, habit or principle to keep in mind"
                } else {
                    "Tab / Shift-Tab field · ←→ choose · Ctrl-U clear"
                },
                muted,
            );
        }
        line(
            f,
            area,
            area.height - 1,
            view.error.as_deref().unwrap_or("Enter save · Esc cancel"),
            accent,
        );
        return;
    }
    let task_view = view.kind == Kind::Task;
    if !character && area.width >= 58 && !task_view {
        let tabs = Kind::OBJECTIVES
            .into_iter()
            .map(|kind| {
                Span::styled(
                    format!(
                        "{} {}   ",
                        if kind == view.kind { "›" } else { " " },
                        kind.label()
                    ),
                    Style::default().fg(if kind == view.kind { secondary } else { muted }),
                )
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(Line::from(tabs)),
            Rect::new(area.x, area.y, area.width, 1),
        );
    } else {
        line(
            f,
            area,
            0,
            if character {
                "Reminders for the person you want to become".into()
            } else if task_view {
                if view.completed {
                    "← Completed tasks → · f pending".into()
                } else {
                    "← Pending tasks → · f completed".into()
                }
            } else {
                format!("← {} →", view.kind.label())
            },
            body,
        );
    }
    if area.height < 4 {
        return;
    }
    let items = view.entries(&app.planner.data, date);
    let slots = usize::from(area.height.saturating_sub(4)).max(1);
    let start = (view.selected + 1).saturating_sub(slots);
    if items.is_empty() {
        line(
            f,
            area,
            2,
            if character {
                "+  Add a quality or habit to practise"
            } else if view.kind == Kind::Task && view.completed {
                "No completed tasks"
            } else {
                "+  Add a task or objective"
            },
            body,
        );
    }
    for (i, entry) in items.iter().enumerate().skip(start).take(slots) {
        let status = if character {
            if entry.done {
                "paused"
            } else {
                "·"
            }
        } else if entry.done {
            "x"
        } else {
            " "
        };
        line(
            f,
            area,
            2 + (i - start) as u16,
            format!(
                "{} [{status}] #{} {}",
                if i == view.selected { "›" } else { " " },
                entry.id,
                entry.title
            ),
            if i == view.selected {
                secondary
            } else if entry.done {
                muted
            } else {
                body
            },
        );
    }
    if area.height >= 6 {
        line(
            f,
            area,
            area.height - 2,
            if character {
                "+ add · Enter edit · Space pause/resume · x archive · u undo"
            } else if view.kind == Kind::Task && view.completed {
                "Enter edit · Space reopen · x archive · u undo"
            } else {
                "+ add · Enter edit · Space complete · x archive · u undo"
            },
            muted,
        );
    }
    line(
        f,
        area,
        area.height - 1,
        view.error
            .as_deref()
            .or(app.planner.error.as_deref())
            .unwrap_or(if task_view {
                "Esc back · ↑↓ select · ←→ goals"
            } else {
                "Esc back · ↑↓ select"
            }),
        accent,
    );
}

fn chat(
    f: &mut Frame,
    app: &App,
    view: &crate::planning::Conversation,
    area: Rect,
    elapsed: std::time::Duration,
) {
    let [accent, secondary, body, _] = colors(app.config.palette());
    let ai = &app.config.ai;
    line(
        f,
        area,
        0,
        chat_status(view, ai, elapsed),
        if view.busy { secondary } else { body },
    );
    if area.height < 5 {
        return;
    }
    let privacy = ai.provider != Provider::Off && area.height >= 7;
    let history_height = area.height.saturating_sub(4 + u16::from(privacy)).max(1);
    let mut lines = Vec::new();
    for (who, message) in &view.messages {
        lines.push(Line::from(Span::styled(
            who.clone(),
            Style::default().fg(if who == "You" { secondary } else { accent }),
        )));
        for raw in message.lines() {
            for value in wrap_lines(raw, usize::from(area.width)) {
                lines.push(Line::from(Span::styled(value, Style::default().fg(body))));
            }
        }
        lines.push(Line::from(""));
    }
    if lines.last().is_some_and(|line| line.width() == 0) {
        lines.pop();
    }
    let scroll = lines
        .len()
        .saturating_sub(usize::from(history_height))
        .saturating_sub(view.scroll_back)
        .min(usize::from(u16::MAX)) as u16;
    f.render_widget(
        Paragraph::new(lines).scroll((scroll, 0)),
        Rect::new(area.x, area.y + 1, area.width, history_height),
    );
    let input_y = area.bottom() - 3;
    text(f, area.x, input_y, 2, ">", secondary, Alignment::Left);
    input_line(
        f,
        Rect::new(area.x + 2, input_y, area.width - 2, 1),
        &view.input,
        body,
    );
    if privacy {
        line(
            f,
            area,
            area.height - 4,
            "Shares active goals, reminders, tasks and recent chat",
            body,
        );
    }
    line(
        f,
        area,
        area.height - 2,
        chat_controls(view, ai, area.width),
        body,
    );
    if area.height >= 6 {
        line(
            f,
            area,
            area.height - 1,
            "PgUp/PgDn history · task: … · reminder: …",
            body,
        );
    }
}
