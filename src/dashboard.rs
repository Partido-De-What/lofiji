//! The quiet text workspace below the clock: reminders, objectives and conversation.
use crate::{
    ai::Provider,
    app::{App, Overlay},
    planner::Kind,
    planning::Field,
    planning_ui::{
        chat_controls, chat_status, clipped, colors, input_line, wrap_lines, wrapped_input,
    },
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

pub fn render(
    f: &mut Frame,
    app: &App,
    config: &crate::config::Config,
    area: Rect,
    date: NaiveDate,
    elapsed: std::time::Duration,
) {
    let [accent, secondary, body, muted] = colors(config.palette());
    let opacity = u16::from(config.dashboard_opacity.min(100));
    // Keep the scene visible behind the workspace text according to dashboard opacity.
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = f.buffer_mut().get_mut(x, y);
            let backing = match (cell.bg, crate::theme::BG) {
                (Color::Rgb(r, g, b), Color::Rgb(br, bg, bb)) => {
                    let blend = |scene: u8, theme: u8| {
                        ((u16::from(scene) * (100 - opacity) + u16::from(theme) * opacity) / 100)
                            as u8
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
    let content = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height - if area.height >= 12 { 2 } else { 1 },
    );
    let (active, ai_action) = match &app.overlay {
        Overlay::Planner(view) => {
            if content.height > 0 {
                entries(f, app, view, content, date);
            }
            (Some(usize::from(view.kind != Kind::Character)), None)
        }
        _ => (
            None,
            overview(f, app, content, date, [accent, secondary, body, muted]),
        ),
    };
    let (ai_full, ai_short) = ai_action.unwrap_or(("", ""));
    let count = if ai_action.is_some() { 5 } else { 4 };
    let labels = [
        [
            "h Character",
            "g Tasks & objectives",
            "c Talk",
            "+ Add",
            ai_full,
        ],
        [
            "h Character",
            "g Tasks & objectives",
            "c Talk",
            "+ Add",
            ai_short,
        ],
        ["h Character", "g Tasks", "c Talk", "+ Add", ai_short],
        ["h Self", "g Tasks", "c Talk", "+ Add", ai_short],
        ["h", "g", "c", "+", "d"],
    ]
    .into_iter()
    .find(|labels| {
        let width = labels[..count]
            .iter()
            .map(|label| label.len())
            .sum::<usize>();
        width + (count - 1) * 3 + usize::from(active.is_some()) * 2 <= usize::from(area.width)
    })
    .unwrap_or(["h", "g", "c", "+", "d"]);
    let mut tabs = Vec::new();
    for (i, label) in labels.into_iter().take(count).enumerate() {
        if i > 0 {
            tabs.push(Span::raw("   "));
        }
        let selected = active == Some(i);
        tabs.push(Span::styled(
            if selected {
                format!("› {label}")
            } else {
                label.into()
            },
            Style::default().fg(if selected { secondary } else { accent }),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(tabs)),
        Rect::new(area.x, area.bottom() - 1, area.width, 1),
    );
}

fn overview(
    f: &mut Frame,
    app: &App,
    area: Rect,
    date: NaiveDate,
    palette: [Color; 4],
) -> Option<(&'static str, &'static str)> {
    let [accent, _, body, muted] = palette;
    if let Some(error) = &app.planner.error {
        line(
            f,
            area,
            0,
            format!("Goals couldn't be loaded · g for details: {error}"),
            accent,
        );
        return None;
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
    let next_step = brief.map(|brief| brief.next_step.as_str());
    let tip = brief.and_then(|brief| brief.tip);
    let ai_action = if app.config.ai.provider != Provider::Off && !tasks.is_empty() {
        Some(if brief.is_some() {
            ("d Refresh AI suggestion", "d Refresh AI")
        } else {
            ("d Ask AI for a next step", "d Ask AI")
        })
    } else {
        None
    };
    let wide = area.width >= 68 && area.height >= 4;
    let mut columns = if wide {
        let character_width = area.width / 4;
        let task_width = area.width * 2 / 5;
        let goals_x = character_width + task_width + 4;
        vec![
            (
                Rect::new(area.x, area.y, character_width, area.height),
                Vec::new(),
            ),
            (
                Rect::new(
                    area.x + character_width + 2,
                    area.y,
                    task_width,
                    area.height,
                ),
                Vec::new(),
            ),
            (
                Rect::new(area.x + goals_x, area.y, area.width - goals_x, area.height),
                Vec::new(),
            ),
        ]
    } else {
        vec![(area, Vec::new())]
    };
    let task_column = usize::from(wide);
    let goal_column = task_column * 2;

    if wide {
        for ((column, _), heading) in columns.iter_mut().zip(["CHARACTER", "TASKS", "OBJECTIVES"]) {
            line(f, *column, 0, heading, accent);
            column.y += 1;
            column.height -= 1;
        }
        if reminders.is_empty() {
            push_wrapped(&mut columns[0], "h then + adds a reminder", muted);
        }
        for reminder in &reminders {
            push_wrapped(&mut columns[0], &format!("· {}", reminder.title), body);
        }
    }
    if tasks.is_empty() {
        push_wrapped(&mut columns[task_column], "+ adds a task", muted);
    }
    for (i, entry) in tasks
        .iter()
        .take(if wide { tasks.len() } else { 1 })
        .enumerate()
    {
        let due = match entry
            .due
            .as_deref()
            .and_then(|due| NaiveDate::parse_from_str(due, "%Y-%m-%d").ok())
        {
            Some(due) if due < date => "overdue · ",
            Some(due) if due == date => "today · ",
            _ => "",
        };
        let prefix = if !wide {
            "Start with:"
        } else if brief.is_some_and(|brief| brief.task_id == entry.id) {
            "AI ›"
        } else if i == 0 {
            "›"
        } else {
            "·"
        };
        push_wrapped(
            &mut columns[task_column],
            &format!("{prefix} {due}{}", entry.title),
            body,
        );
    }
    if !wide {
        let reminder = reminders
            .first()
            .map_or("h then + to add a reminder", |entry| entry.title.as_str());
        push_wrapped(&mut columns[0], &format!("Character · {reminder}"), body);
    }

    for (kind, label) in [(Kind::Short, "Short term"), (Kind::Long, "Long term")] {
        for goal in app
            .planner
            .data
            .entries
            .iter()
            .filter(|entry| entry.kind == kind && !entry.done && !entry.archived)
        {
            push_wrapped(
                &mut columns[goal_column],
                &format!("{label} · {}", goal.title),
                body,
            );
        }
    }
    if wide && columns[goal_column].1.is_empty() {
        push_wrapped(&mut columns[goal_column], "g opens objectives", muted);
    }
    let mut advice = (area, Vec::new());
    if let Some(step) = next_step {
        push_wrapped(&mut advice, &format!("AI next step · {step}"), body);
    }
    if let Some(tip) = tip {
        push_wrapped(&mut advice, &format!("Tip · {tip}"), body);
    }
    let body_height = columns[0].0.height;
    if advice.1.len() > usize::from(body_height / 2) {
        columns[task_column].1.append(&mut advice.1);
    }
    let advice_rows = advice.1.len() as u16;
    let rows = body_height - advice_rows;
    let longest = columns
        .iter()
        .map(|(_, lines)| lines.len())
        .max()
        .unwrap_or(0);
    let show_scroll_hint = rows >= 4 && longest > usize::from(rows);
    let visible = rows.saturating_sub(u16::from(show_scroll_hint));
    let offset = app
        .workspace_scroll
        .get()
        .min(longest.saturating_sub(usize::from(visible)));
    if visible > 0 {
        app.workspace_scroll.set(offset);
        for (column, lines) in columns {
            let lines = lines
                .into_iter()
                .skip(offset)
                .take(usize::from(visible))
                .collect::<Vec<_>>();
            f.render_widget(
                Paragraph::new(lines),
                Rect::new(column.x, column.y, column.width, visible),
            );
        }
    }
    if show_scroll_hint {
        line(
            f,
            area,
            area.height - advice_rows - 1,
            "↑↓ scroll · Home top",
            muted,
        );
    }
    if advice_rows > 0 {
        f.render_widget(
            Paragraph::new(advice.1),
            Rect::new(area.x, area.bottom() - advice_rows, area.width, advice_rows),
        );
    }
    ai_action
}

fn push_wrapped(column: &mut (Rect, Vec<Line<'static>>), value: &str, color: Color) {
    column.1.extend(
        wrap_lines(value, usize::from(column.0.width))
            .into_iter()
            .map(|row| Line::from(row).style(Style::default().fg(color))),
    );
}

pub(crate) fn entries(
    f: &mut Frame,
    app: &App,
    view: &crate::planning::Panel,
    area: Rect,
    date: NaiveDate,
) {
    let [accent, secondary, body, muted] = colors(app.config.palette());
    let character = view.kind == Kind::Character;
    if let Some(editor) = &view.editor {
        let active = editor.active_field();
        let all_fields = editor.fields();
        let full = area.height
            >= if editor.entry.kind == Kind::Task {
                17
            } else {
                15
            };
        let fields = if full {
            all_fields
        } else {
            std::slice::from_ref(&active)
        };
        let mut y = 0;
        for &field in fields {
            let selected = field == active;
            let (label, input) = match field {
                Field::Title if !full && editor.entry.kind == Kind::Task => {
                    ("Title · Tab for description", Some(&editor.title))
                }
                Field::Title => ("Title", Some(&editor.title)),
                Field::Description => ("Description · optional", Some(&editor.description)),
                Field::Link => ("Linked objective · ←→ choose", None),
                Field::Due => ("Due date · YYYY-MM-DD or blank", Some(&editor.due)),
                Field::Priority => ("Priority · ←→ choose", None),
            };
            let heading = if full {
                format!("{} {label}", if selected { "›" } else { " " })
            } else {
                let index = all_fields
                    .iter()
                    .position(|item| *item == field)
                    .unwrap_or(0);
                format!("{}/{} · {label}", index + 1, all_fields.len())
            };
            line(f, area, y, heading, secondary);
            let wrapped = field == Field::Description
                || (field == Field::Title && editor.entry.kind == Kind::Task);
            let height = if wrapped {
                if full {
                    3
                } else {
                    area.height.saturating_sub(3)
                }
            } else {
                1
            };
            let rect = Rect::new(
                area.x + 2,
                area.y + y + 1,
                area.width.saturating_sub(2),
                height,
            );
            if let Some(input) = input {
                if wrapped {
                    wrapped_input(f, rect, input, selected, body);
                } else if selected {
                    input_line(f, rect, input, body);
                } else {
                    text(
                        f,
                        rect.x,
                        rect.y,
                        rect.width,
                        &input.value,
                        body,
                        Alignment::Left,
                    );
                }
            } else {
                let value = if field == Field::Priority {
                    editor.entry.priority.label()
                } else {
                    editor
                        .entry
                        .parent
                        .and_then(|id| app.planner.data.entry(id))
                        .map_or("No linked objective", |e| e.title.as_str())
                };
                text(f, rect.x, rect.y, rect.width, value, body, Alignment::Left);
            }
            y += height + 1;
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
    let entries: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let status = match (character, entry.done) {
                (true, true) => "paused",
                (true, false) => "·",
                (false, true) => "x",
                (false, false) => " ",
            };
            let title = format!(
                "{} [{status}] #{} {}",
                if i == view.selected { "›" } else { " " },
                entry.id,
                entry.title,
            );
            let rows = if task_view {
                wrap_lines(&title, usize::from(area.width))
            } else {
                vec![title]
            };
            let color = if i == view.selected {
                secondary
            } else if entry.done {
                muted
            } else {
                body
            };
            (rows, color)
        })
        .collect();
    let selected = view.selected.min(entries.len().saturating_sub(1));
    let mut start = selected;
    let selected_rows = entries.get(selected).map_or(0, |(rows, _)| rows.len());
    let mut remaining = slots.saturating_sub(selected_rows);
    for (rows, _) in entries[..selected].iter().rev() {
        if rows.len() > remaining {
            break;
        }
        remaining -= rows.len();
        start -= 1;
    }
    for (i, (value, color)) in entries[start..]
        .iter()
        .flat_map(|(rows, color)| rows.iter().map(move |row| (row, *color)))
        .take(slots)
        .enumerate()
    {
        line(f, area, 2 + i as u16, value, color);
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
                "+ add · Enter edit · F focus · Space complete · x archive · u undo"
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
                if view.completed {
                    "Esc back · ↑↓ select · ←→ goals"
                } else {
                    "Esc back · ↑↓ select · ←→ goals · F focus"
                }
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
