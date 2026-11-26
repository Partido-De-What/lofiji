use crate::{
    app::{AiPicker, App, Overlay, Section, SettingId, SettingKind, Settings, SoundSettings},
    background, clock,
    config::{hex_color, ClockStyle},
    layout::Composition,
    theme::*,
    timer::{Phase, Status},
};
use chrono::{DateTime, Local, Timelike};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::time::Instant;

pub(crate) fn text(
    f: &mut Frame,
    x: u16,
    y: u16,
    width: u16,
    value: impl Into<String>,
    color: Color,
    alignment: Alignment,
) {
    let area = f.size();
    if x >= area.width || y >= area.height {
        return;
    }
    f.render_widget(
        Paragraph::new(value.into())
            .style(Style::default().fg(color))
            .alignment(alignment),
        Rect::new(x, y, width.min(area.width - x), 1),
    );
}
pub(crate) fn center(
    f: &mut Frame,
    x: u16,
    y: u16,
    width: u16,
    value: impl Into<String>,
    color: Color,
) {
    text(f, x, y, width, value, color, Alignment::Center);
}
fn hints(f: &mut Frame, y: u16, pairs: &[(&str, &str)], width: u16, x: u16, colors: [Color; 2]) {
    let required = pairs
        .iter()
        .map(|(key, description)| Line::from(format!("{key} {description}")).width())
        .sum::<usize>();
    let gaps = pairs.len().saturating_sub(1);
    let spacing = if required + gaps * 3 <= usize::from(width) {
        3
    } else if required + gaps * 2 <= usize::from(width) {
        2
    } else {
        1
    };
    let mut spans = Vec::new();
    for (i, (key, description)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                " ".repeat(spacing),
                Style::default().fg(colors[1]),
            ));
        }
        spans.push(Span::styled(*key, Style::default().fg(colors[0])));
        spans.push(Span::styled(
            format!(" {description}"),
            Style::default().fg(colors[1]),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        Rect::new(x, y, width, 1),
    );
}
fn pixel_clock(
    buffer: &mut Buffer,
    area: Rect,
    value: &str,
    scale: u16,
    dim_separator: bool,
    config: &crate::config::Config,
) {
    let (x, y) = (area.x, area.y);
    let p = config.palette();
    let style = config.clock_style;
    let compact = area.height == 5;
    let gap = if compact { 1 } else { 2 };
    let mut cursor = x;
    let mut separators = 0;
    for character in value.chars() {
        if character == ':' {
            let color = hex_color(&p.separator).unwrap_or(MUTED);
            let color = if dim_separator { dim(color) } else { color };
            for row in if compact { [1, 3] } else { [2, 4] } {
                for dx in 0..scale {
                    buffer
                        .get_mut(cursor + dx, y + row)
                        .set_symbol(if matches!(style, ClockStyle::Dots | ClockStyle::Outline) {
                            if dx == scale / 2 {
                                "•"
                            } else {
                                " "
                            }
                        } else if style == ClockStyle::Scanline {
                            "▀"
                        } else {
                            "█"
                        })
                        .set_fg(color);
                }
            }
            cursor += scale + gap;
            separators += 1;
        } else if let Some(digit) = character.to_digit(10) {
            let color = hex_color(if separators >= 2 {
                &p.seconds
            } else {
                &p.hours
            })
            .unwrap_or(BLUE);
            if style == ClockStyle::Outline {
                let rows: &[&str] = if compact {
                    &clock::COMPACT_OUTLINE[digit as usize]
                } else {
                    &clock::OUTLINE_DIGITS[digit as usize]
                };
                for (row, line) in rows.iter().enumerate() {
                    for (col, ch) in line.chars().enumerate() {
                        for dx in 0..scale {
                            let symbol =
                                clock::wire_symbol(ch, col, if compact { 3 } else { 5 }, dx, scale);
                            if symbol != " " {
                                buffer
                                    .get_mut(cursor + col as u16 * scale + dx, y + row as u16)
                                    .set_symbol(symbol)
                                    .set_fg(color);
                            }
                        }
                    }
                }
                cursor += if compact {
                    3 * scale + 1
                } else {
                    5 * scale + 2
                };
                continue;
            }
            for row in 0..if compact { 5 } else { 7 } {
                for col in 0..if compact { 3 } else { 5 } {
                    if clock::pixel(style, digit as usize, col, row, compact) {
                        for dx in 0..scale {
                            let symbol = match style {
                                ClockStyle::Dots => {
                                    if dx == 0 {
                                        "●"
                                    } else {
                                        " "
                                    }
                                }
                                ClockStyle::Scanline => "▀",
                                _ => "█",
                            };
                            buffer
                                .get_mut(cursor + col as u16 * scale + dx, y + row as u16)
                                .set_symbol(symbol)
                                .set_fg(color);
                        }
                    }
                }
            }
            cursor += if compact {
                3 * scale + 1
            } else {
                5 * scale + 2
            };
        }
    }
}
fn phase_color(phase: Phase) -> Color {
    if phase == Phase::Focus {
        PINK
    } else {
        CYAN
    }
}
fn countdown(app: &App, now: Instant) -> String {
    let s = app.timer.seconds(now);
    format!("{:02}:{:02}", s / 60, s % 60)
}
fn timer_status(app: &App) -> &'static str {
    match app.timer.status {
        Status::Ready => "ready",
        Status::Running => "running",
        Status::Paused => "paused",
    }
}

pub fn render(f: &mut Frame, app: &App, wall: DateTime<Local>, now: Instant) {
    let size = f.size();
    f.buffer_mut()
        .set_style(size, Style::default().bg(BG).fg(TEXT));
    if size.width == 0 || size.height == 0 {
        return;
    }
    let appearance = app.appearance_draft();
    let config = appearance.unwrap_or(&app.config);
    let time = clock::time(wall, config);
    let workspace =
        config.show_plan || matches!(app.overlay, Overlay::Planner(_) | Overlay::Conversation(_));
    let mut layout = if workspace || appearance.is_some() {
        Composition::new(size, &time)
    } else {
        Composition::with_workspace(size, &time, false)
    };
    let timer_active = app.pomodoro_visible || app.timer.status != Status::Ready;
    let mut workspace_area =
        crate::planning_ui::workspace_area(size, &layout, timer_active, app.pomodoro_visible)
            .filter(|_| workspace);
    let editing_workspace = matches!(app.overlay, Overlay::Planner(_) | Overlay::Conversation(_));
    if editing_workspace
        && size.width >= 28
        && size.height >= 12
        && workspace_area.map_or(true, |area| area.height < 7)
    {
        layout.header = None;
        layout.clock = Rect::new(0, 1, size.width, 1);
        layout.scale = None;
        layout.date = None;
        layout.timer = timer_active.then_some(Rect::new(0, 2, size.width, 1));
        layout.progress = None;
        workspace_area = crate::planning_ui::workspace_area(size, &layout, timer_active, false);
    }
    let modal = matches!(
        app.overlay,
        Overlay::Help | Overlay::Settings(_) | Overlay::Sounds(_)
    );
    let protected = [
        layout.clock,
        layout.date.unwrap_or_default(),
        layout.timer.unwrap_or_default(),
        layout.progress.unwrap_or_default(),
        layout.header.unwrap_or_default(),
        workspace_area.unwrap_or_default(),
    ];
    let frame = if config.animations {
        now.saturating_duration_since(app.started).as_millis() as u64 / 250
    } else {
        0
    };
    if let Some(image) = app.display_image(config) {
        image.draw_at(
            f.buffer_mut(),
            config.image_brightness,
            &protected,
            if config.animations {
                now.saturating_duration_since(app.started).as_millis() as u64
            } else {
                0
            },
        );
    }
    if modal && appearance.is_none() {
        match &app.overlay {
            Overlay::Help => help(f, app.help_scroll),
            Overlay::Settings(settings_view) => settings(f, settings_view, app, None),
            Overlay::Sounds(s) => sounds(f, s),
            _ => unreachable!(),
        }
        return;
    }
    if let Some(header) = layout.header {
        let title = if size.width >= 70 {
            "l o f i j i"
        } else {
            "lofiji"
        };
        text(f, header.x, header.y, 12, title, MUTED, Alignment::Left);
        text(
            f,
            header.x,
            header.y,
            header.width,
            "a small pocket of quiet",
            MUTED,
            Alignment::Right,
        );
    }
    if let Some(scale) = layout.scale {
        pixel_clock(
            f.buffer_mut(),
            layout.clock,
            &time,
            scale,
            config.blink_separator && wall.second() % 2 == 1,
            config,
        );
    } else {
        let suffix = if config.format_24h {
            ""
        } else {
            clock::meridiem(wall)
        };
        f.render_widget(
            Paragraph::new(format!("{time} {suffix}").trim_end().to_string())
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(hex_color(&config.palette().hours).unwrap_or(BLUE))
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
            layout.clock,
        );
    }
    if let Some(area) = layout
        .date
        .filter(|_| !(layout.shared_date_timer && timer_active))
    {
        let date = if area.width < 16 {
            wall.format("%a %-d %b").to_string().to_uppercase()
        } else {
            clock::date(wall, area.width < 44)
        };
        let date = if layout.scale.is_some() && !config.format_24h {
            format!("{} · {date}", clock::meridiem(wall))
        } else {
            date
        };
        center(
            f,
            area.x,
            area.y,
            area.width,
            date,
            hex_color(&config.palette().date).unwrap_or(TEXT),
        );
    }
    if let Some(area) = layout.timer.filter(|_| timer_active) {
        if area.height > 1 {
            let phase = match app.timer.phase {
                Phase::ShortBreak if area.width < 20 => "BREAK",
                Phase::LongBreak if area.width < 17 => "LONG",
                phase => phase.label(),
            };
            center(
                f,
                area.x,
                area.y,
                area.width,
                format!("{} {}", phase, countdown(app, now)),
                phase_color(app.timer.phase),
            );
            center(f, area.x, area.y + 1, area.width, timer_status(app), MUTED);
        } else {
            let short_phase = match app.timer.phase {
                Phase::Focus => "FOCUS",
                Phase::ShortBreak => "BREAK",
                Phase::LongBreak => "LONG BREAK",
            };
            let short_status = match app.timer.status {
                Status::Running => "run",
                Status::Paused => "pause",
                Status::Ready => "ready",
            };
            let candidates = [
                format!(
                    "{} {} · {}",
                    app.timer.phase.label(),
                    countdown(app, now),
                    timer_status(app)
                ),
                format!(
                    "{} {} {}",
                    short_phase,
                    countdown(app, now),
                    timer_status(app)
                ),
                format!("{} {} {}", short_phase, countdown(app, now), short_status),
                format!("{} {}", countdown(app, now), timer_status(app)),
            ];
            let label = candidates
                .into_iter()
                .find(|text| text.chars().count() <= usize::from(area.width))
                .unwrap_or_else(|| countdown(app, now));
            center(
                f,
                area.x,
                area.y,
                area.width,
                label,
                phase_color(app.timer.phase),
            );
        }
        if let Some(bar) = layout.progress.filter(|_| app.pomodoro_visible) {
            let filled = (app.timer.progress(now) * f64::from(bar.width)).round() as u16;
            for dx in 0..bar.width {
                f.buffer_mut()
                    .get_mut(bar.x + dx, bar.y)
                    .set_symbol("━")
                    .set_fg(if dx < filled {
                        phase_color(app.timer.phase)
                    } else {
                        SURFACE
                    });
            }
        }
    }
    if let Some(area) = workspace_area {
        crate::dashboard::render(
            f,
            app,
            config,
            area,
            wall.date_naive(),
            now.saturating_duration_since(app.started),
        );
    }
    background::weather(f.buffer_mut(), config, frame, &protected);
    // The bottom two rows are reserved, regardless of content size.
    let palette = config.palette();
    let footer_colors = [
        hex_color(&palette.hours).unwrap_or(BLUE),
        hex_color(&palette.separator).unwrap_or(MUTED),
    ];
    if size.height >= 3 {
        let task_status = if matches!(app.overlay, Overlay::None) {
            app.pending_task_completion
                .and_then(|id| app.planner.data.entry(id))
                .map(|task| format!("y complete · Esc keep open · {}", task.title))
                .or_else(|| {
                    app.focused_task_id
                        .and_then(|id| app.planner.data.entry(id))
                        .map(|task| format!("Focusing · {}", task.title))
                })
        } else {
            None
        };
        if size.height >= 6 {
            if let Some(message) = app
                .notice
                .as_ref()
                .map(|(message, _)| message.as_str())
                .or(task_status.as_deref())
            {
                center(f, 0, size.height - 3, size.width, message, YELLOW);
            }
        }
        let action = match app.timer.status {
            Status::Ready => "start",
            Status::Running => "pause",
            Status::Paused => "resume",
        };
        if !editing_workspace {
            let mut controls = if app.pomodoro_visible {
                vec![
                    ("space", action),
                    ("p", "hide"),
                    ("s", "settings"),
                    ("?", "help"),
                ]
            } else {
                vec![("p", "pomo"), ("s", "settings"), ("?", "help")]
            };
            if app.ambient_on {
                controls.insert(if app.pomodoro_visible { 2 } else { 1 }, ("a", "sounds on"));
            }
            if app.music_on || app.music_ready {
                controls.insert(
                    if app.pomodoro_visible { 2 } else { 1 },
                    (
                        "m",
                        if app.music_ready {
                            "music on"
                        } else {
                            "loading…"
                        },
                    ),
                );
            }
            if app.pomodoro_visible {
                controls.extend([("r", "reset"), ("n", "skip"), ("q", "quit")]);
            } else {
                controls.push(("v", if workspace { "clock only" } else { "workspace" }));
                if workspace_area.is_some_and(|area| area.height >= 3) {
                    if !app.ambient_on {
                        controls.push(("a", "sounds"));
                    }
                    controls.push(("b", "scene"));
                    if !app.music_on && !app.music_ready {
                        controls.push(("m", "music"));
                    }
                } else {
                    controls.extend([("c", "talk"), ("+", "task")]);
                }
            }
            let mut used = 0;
            let fitting: Vec<_> = controls
                .into_iter()
                .filter(|(key, name)| {
                    let width = Line::from(format!("{key} {name}")).width() + usize::from(used > 0);
                    if used + width > usize::from(size.width) {
                        return false;
                    }
                    used += width;
                    true
                })
                .collect();
            hints(f, size.height - 2, &fitting, size.width, 0, footer_colors);
        }
    }

    match &app.overlay {
        Overlay::Settings(s) if appearance.is_some() => {
            let top = layout.date.unwrap_or(layout.clock).bottom();
            let height = size.bottom().saturating_sub(top).min(14);
            let preview_panel = (height >= 8).then(|| {
                let width = size.width.min(62);
                let x = if size.width >= 75 {
                    size.right() - width
                } else {
                    (size.width - width) / 2
                };
                Rect::new(x, size.bottom() - height, width, height)
            });
            settings(f, s, app, preview_panel);
        }
        Overlay::None => {}
        Overlay::Planner(view) if workspace_area.map_or(true, |area| area.height < 6) => {
            crate::planning_ui::goals(f, app, view, wall.date_naive())
        }
        Overlay::Conversation(view) if workspace_area.map_or(true, |area| area.height < 6) => {
            crate::planning_ui::conversation(
                f,
                view,
                config.palette(),
                &config.ai,
                now.saturating_duration_since(app.started),
            )
        }
        _ => {}
    }
}

pub(crate) fn overlay_area(f: &Frame, width: u16, height: u16) -> Rect {
    let size = f.size();
    let w = width.min(size.width);
    let h = height.min(size.height);
    Rect::new((size.width - w) / 2, (size.height - h) / 2, w, h)
}
pub(crate) fn panel(f: &mut Frame, area: Rect, title: &str) {
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default()
            .title(Span::styled(
                format!("  {title}  "),
                Style::default().fg(TEXT),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(DIM))
            .style(Style::default().bg(BG)),
        area,
    );
}
pub(crate) const HELP_LINES: &[&str] = &[
    "g / +   Goals & tasks / quick-add task",
    "h       Character reminders",
    "c / v   Talk / toggle text workspace",
    "d       Refresh AI daily suggestion (when configured)",
    "b       Cycle backgrounds",
    "w       Cycle clear / rain / snow",
    "m       Toggle optional YouTube music",
    "a       Ambient sound mixer · z mute/resume",
    "t       Cycle clock styles",
    "p       Show / hide Pomodoro",
    "space   Start / pause / resume Pomodoro",
    "r       Stop and reset current session",
    "n       Skip to next session",
    "s       Settings: appearance, Pomodoro, audio, AI",
    "?       Keyboard shortcuts",
    "q       Quit",
    "",
    "Settings: Enter opens/edits · Ctrl-S saves",
    "Chat: Ctrl+K cancels reply · Ctrl+N starts new",
    "Ctrl+E  Change thinking effort in Talk, when supported",
    "Tasks: f switches between pending and completed",
    "Workspace: ↑↓ scroll · Home returns to the top",
    "Music requires mpv and yt-dlp.",
    "Hiding Pomodoro keeps the timer running.",
];

pub(crate) fn help_page_size(height: u16) -> usize {
    usize::from(height.min(25).saturating_sub(4)).max(1)
}

fn help(f: &mut Frame, scroll: usize) {
    let area = overlay_area(f, 64, 25);
    panel(f, area, "MAKE YOURSELF AT HOME");
    if area.height < 4 || area.width < 5 {
        return;
    }
    let visible = help_page_size(area.height);
    let lines = HELP_LINES;
    let first = scroll.min(lines.len().saturating_sub(visible));
    for (i, line) in lines.iter().skip(first).take(visible).enumerate() {
        text(
            f,
            area.x + 2,
            area.y + 2 + i as u16,
            area.width - 4,
            *line,
            TEXT,
            Alignment::Left,
        );
    }
    let footer = if lines.len() > visible {
        format!(
            "Esc back · ↑↓ scroll · {}–{} / {}",
            first + 1,
            (first + visible).min(lines.len()),
            lines.len()
        )
    } else {
        "Esc back".into()
    };
    text(
        f,
        area.x + 2,
        area.bottom() - 2,
        area.width - 4,
        footer,
        BLUE,
        Alignment::Left,
    );
}
fn settings(f: &mut Frame, s: &Settings, app: &App, preview: Option<Rect>) {
    let menu = s.in_menu();
    let section = s.selected.section();
    let height = if menu {
        11
    } else if app.ai_picker.is_none() && s.input.is_none() {
        (s.rows().len() as u16 + 7).clamp(11, 25)
    } else {
        25
    };
    let area = preview.unwrap_or_else(|| overlay_area(f, 72, height));
    panel(f, area, if menu { "SETTINGS" } else { section.label() });
    if area.width < 26 || area.height < 8 {
        text(
            f,
            area.x + 1,
            area.y + 2,
            area.width.saturating_sub(2),
            "Resize for settings",
            MUTED,
            Alignment::Left,
        );
        text(
            f,
            area.x + 1,
            area.y + 3,
            area.width.saturating_sub(2),
            "Esc cancel",
            BLUE,
            Alignment::Left,
        );
        return;
    }
    if menu {
        let visible = usize::from(area.height - 5);
        let first = (section as usize + 1).saturating_sub(visible);
        for (line, item) in Section::ALL.iter().skip(first).take(visible).enumerate() {
            let selected = *item == section;
            let label = item.label();
            text(
                f,
                area.x + 2,
                area.y + 2 + line as u16,
                area.width - 4,
                format!("{} {label}", if selected { "›" } else { " " }),
                if selected { CYAN } else { TEXT },
                Alignment::Left,
            );
        }
        if let Some(error) = &s.error {
            text(
                f,
                area.x + 2,
                area.bottom() - 3,
                area.width - 4,
                error,
                PINK,
                Alignment::Left,
            );
        }
        text(
            f,
            area.x + 2,
            area.bottom() - 2,
            area.width - 4,
            if s.confirm_discard {
                "Esc discard · Ctrl+S save"
            } else {
                "Esc back · ↑↓ choose · Enter open · Ctrl+S save"
            },
            BLUE,
            Alignment::Left,
        );
        return;
    }
    if let Some(picker) = app.ai_picker {
        let (title, items, selected) = match picker {
            AiPicker::Provider(index) => (
                "AI PROVIDER",
                crate::ai::Provider::ALL
                    .iter()
                    .map(|provider| provider.label().to_owned())
                    .collect::<Vec<_>>(),
                index,
            ),
            AiPicker::Model(index) => {
                let default = app
                    .chatgpt_models
                    .first()
                    .map_or("Automatic", |model| model.name.as_str());
                let items = std::iter::once(format!("Automatic · {default}"))
                    .chain(app.chatgpt_models.iter().map(|model| model.name.clone()))
                    .collect();
                ("MODELS ON YOUR CHATGPT ACCOUNT", items, index)
            }
        };
        panel(f, area, title);
        let visible = (area.height - 5) as usize;
        let first = (selected + 1).saturating_sub(visible);
        for (row, (index, name)) in items
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .enumerate()
        {
            let cursor = if index == selected { "›" } else { " " };
            text(
                f,
                area.x + 2,
                area.y + 2 + row as u16,
                area.width - 4,
                format!("{cursor} {name}"),
                if index == selected { CYAN } else { TEXT },
                Alignment::Left,
            );
        }
        text(
            f,
            area.x + 2,
            area.bottom() - 2,
            area.width - 4,
            "↑↓ choose · Enter select · Esc back",
            BLUE,
            Alignment::Left,
        );
        return;
    }
    if let Some(input) = &s.input {
        let label = match s.selected.descriptor().1 {
            SettingKind::Text(label) => label,
            _ => "Clock color · #RRGGBB · edits selected palette",
        };
        text(
            f,
            area.x + 2,
            area.y + 2,
            area.width - 4,
            label,
            TEXT,
            Alignment::Left,
        );
        crate::planning_ui::input_line(
            f,
            Rect::new(area.x + 2, area.y + 4, area.width - 4, 1),
            input,
            CYAN,
        );
        text(
            f,
            area.x + 2,
            area.bottom() - 3,
            area.width - 4,
            s.error
                .as_deref()
                .unwrap_or("Enter accept · Esc undo · Ctrl-U clear"),
            if s.error.is_some() { PINK } else { MUTED },
            Alignment::Left,
        );
        return;
    }
    let c = &s.draft;
    let on = |value| {
        if value {
            "On".to_string()
        } else {
            "Off".to_string()
        }
    };
    let row = |id: SettingId| {
        let (label, _) = id.descriptor();
        let label = if id == SettingId::Connect && app.chatgpt_connected {
            "Reconnect ChatGPT"
        } else {
            label
        };
        let value = match id {
            SettingId::TimeFormat => {
                if c.format_24h {
                    "24-hour".into()
                } else {
                    "12-hour".into()
                }
            }
            SettingId::Seconds => on(c.show_seconds),
            SettingId::Blink => on(c.blink_separator),
            SettingId::Motion => on(c.animations),
            SettingId::Focus => format!("{} min", c.focus_minutes),
            SettingId::ShortBreak => format!("{} min", c.short_break_minutes),
            SettingId::LongBreak => format!("{} min", c.long_break_minutes),
            SettingId::AutoAdvance => on(c.auto_advance),
            SettingId::Notifications => c.notifications.label().to_string(),
            SettingId::Weather => c.weather.label().into(),
            SettingId::Background => c.background.label().into(),
            SettingId::ImagePath => {
                if c.image_path.is_empty() {
                    "Bundled wallpaper".into()
                } else {
                    std::path::Path::new(&c.image_path)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| c.image_path.clone())
                }
            }
            SettingId::Brightness => format!("{}%", c.image_brightness),
            SettingId::MusicUrl => {
                if c.music_url.is_empty() {
                    "Not set".into()
                } else {
                    c.music_url
                        .split("v=")
                        .nth(1)
                        .and_then(|s| s.split('&').next())
                        .unwrap_or_else(|| c.music_url.rsplit('/').next().unwrap_or(&c.music_url))
                        .to_string()
                }
            }
            SettingId::MusicVolume => format!("{}%", c.music_volume),
            SettingId::Palette => {
                if c.clock_palette == 0 {
                    format!("Scene · {}", c.palette().name)
                } else {
                    c.palette().name.clone()
                }
            }
            SettingId::HoursColor => c.palette().hours.clone(),
            SettingId::SecondsColor => c.palette().seconds.clone(),
            SettingId::SeparatorColor => c.palette().separator.clone(),
            SettingId::DateColor => c.palette().date.clone(),
            SettingId::ClockStyle => c.clock_style.label().into(),
            SettingId::Workspace => on(c.show_plan),
            SettingId::Provider => c.ai.provider.label().into(),
            SettingId::Model => {
                if c.ai.provider == crate::ai::Provider::Chatgpt {
                    let name = app
                        .chatgpt_models
                        .iter()
                        .find(|model| model.slug == c.ai.model)
                        .map(|model| model.name.as_str());
                    if c.ai.model.is_empty() {
                        app.chatgpt_models.first().map_or_else(
                            || "Automatic".into(),
                            |model| format!("Auto · {}", model.name),
                        )
                    } else {
                        name.unwrap_or(&c.ai.model).into()
                    }
                } else if c.ai.model.is_empty() {
                    "Choose a model".into()
                } else {
                    c.ai.model.clone()
                }
            }
            SettingId::ApiUrl => {
                if c.ai.endpoint().is_empty() {
                    "Set API base URL".into()
                } else if c.ai.base_url.is_empty() {
                    "Provider default".into()
                } else {
                    c.ai.base_url.clone()
                }
            }
            SettingId::ApiKey => {
                if c.ai.key_name().is_empty() {
                    "Not required".into()
                } else {
                    c.ai.key_name().into()
                }
            }
            SettingId::AutoBrief => on(c.ai.auto_brief),
            SettingId::Timeout => format!("{} sec", c.ai.timeout_seconds),
            SettingId::Connect => {
                if app.chatgpt_busy() {
                    "Working…".into()
                } else {
                    "Enter".into()
                }
            }
            SettingId::AddAccount => "Enter".into(),
            SettingId::SwitchAccount => "Enter · next saved".into(),
            SettingId::Disconnect => "Enter".into(),
            SettingId::Effort => c.ai.thinking_effort.label().into(),
            SettingId::DashboardOpacity => format!("{}%", c.dashboard_opacity),
            SettingId::Menu(_) => String::new(),
        };
        (label, value)
    };
    if area.width < 40 {
        let (label, value) = row(s.selected);
        text(
            f,
            area.x + 2,
            area.y + 2,
            area.width - 4,
            label,
            TEXT,
            Alignment::Left,
        );
        text(
            f,
            area.x + 2,
            area.y + 4,
            area.width - 4,
            s.error.as_deref().unwrap_or(&value),
            if s.error.is_some() { PINK } else { CYAN },
            Alignment::Left,
        );
        text(
            f,
            area.x + 2,
            area.y + area.height - 3,
            area.width - 4,
            "↑↓ select ←→ edit",
            MUTED,
            Alignment::Left,
        );
        text(
            f,
            area.x + 2,
            area.y + area.height - 2,
            area.width - 4,
            "Enter edit/save · Esc",
            BLUE,
            Alignment::Left,
        );
        return;
    }
    let page = s.rows();
    let selected_index = page.iter().position(|row| *row == s.selected).unwrap_or(0);
    let visible = usize::from(area.height - 7).min(page.len());
    let first = (selected_index + 1).saturating_sub(visible);
    let value_width = (area.width / 2).clamp(12, 34);
    for (index, &i) in page.iter().enumerate().skip(first).take(visible) {
        let (label, value) = row(i);
        let selected = i == s.selected;
        let y = area.y + 2 + (index - first) as u16;
        if selected {
            f.buffer_mut().set_style(
                Rect::new(area.x + 1, y, area.width - 2, 1),
                Style::default().bg(SURFACE),
            );
        }
        text(
            f,
            area.x + 2,
            y,
            area.width - value_width - 5,
            format!("{} {label}", if selected { "›" } else { " " }),
            if selected { BLUE } else { TEXT },
            Alignment::Left,
        );
        text(
            f,
            area.right() - value_width - 2,
            y,
            value_width,
            value.as_str(),
            if matches!(i.descriptor().1, SettingKind::Color) {
                hex_color(&value).unwrap_or(CYAN)
            } else if selected {
                CYAN
            } else {
                MUTED
            },
            Alignment::Right,
        );
    }
    let foot = area.y + area.height - 5;
    let account_hint = if app.chatgpt_busy() {
        Some(if s.selected == SettingId::Disconnect {
            "Signing out of ChatGPT…"
        } else if s.selected == SettingId::Model {
            "Loading models for your ChatGPT account…"
        } else if s.selected == SettingId::SwitchAccount {
            "Switching ChatGPT account…"
        } else {
            "Complete sign-in in your browser · Esc cancels"
        })
    } else if matches!(s.selected.descriptor().1, SettingKind::Account(_)) {
        Some("ChatGPT plan limits · Ctrl-S saves provider selection")
    } else {
        None
    };
    text(
        f,
        area.x + 2,
        foot,
        area.width - 4,
        if let Some(error) = &s.error {
            error.as_str()
        } else if let Some(hint) = account_hint {
            hint
        } else if section == Section::Ai {
            if s.selected == SettingId::Effort {
                "←→ change effort · availability varies by model"
            } else if s.selected == SettingId::Model {
                "Enter to choose a model"
            } else if s.selected == SettingId::Provider {
                "Enter to choose a provider"
            } else {
                "AI uses active reminders, goals and tasks"
            }
        } else if matches!(s.selected.descriptor().1, SettingKind::Number) {
            "←→ adjust value"
        } else {
            "←→ change values · Enter opens or edits"
        },
        if s.error.is_some() { PINK } else { TEXT },
        Alignment::Left,
    );
    if section == Section::Ai
        && matches!(
            c.ai.provider,
            crate::ai::Provider::Chatgpt | crate::ai::Provider::Off
        )
        && !app.chatgpt_busy()
    {
        text(
            f,
            area.x + 2,
            foot + 1,
            area.width - 4,
            &app.chatgpt_status,
            TEXT,
            Alignment::Left,
        );
    }
    text(
        f,
        area.x + 2,
        foot + 2,
        area.width - 4,
        "Esc back · ↑↓ select · Ctrl+S save",
        BLUE,
        Alignment::Left,
    );
}

fn dim(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(r / 2, g / 2, b / 2),
        _ => DIM,
    }
}
fn sounds(f: &mut Frame, s: &SoundSettings) {
    let area = overlay_area(f, 56, 19);
    panel(f, area, "LAYERED SOUNDS");
    if area.height < 7 || area.width < 24 {
        return;
    }
    let visible = usize::from(area.height.saturating_sub(7)).min(crate::config::SOUND_COUNT);
    let first = (s.selected + 1).saturating_sub(visible);
    for (i, layer) in crate::ambient::ORDER
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
    {
        let y = area.y + 2 + (i - first) as u16;
        let name = crate::ambient::NAMES[*layer];
        let volume = s.volumes[*layer];
        text(
            f,
            area.x + 2,
            y,
            area.width - 12,
            format!("{} {}", if i == s.selected { "›" } else { " " }, name),
            if i == s.selected { CYAN } else { TEXT },
            Alignment::Left,
        );
        text(
            f,
            area.right() - 9,
            y,
            7,
            if volume > 0 {
                format!("{volume}%")
            } else {
                "Off".into()
            },
            if volume > 0 { YELLOW } else { MUTED },
            Alignment::Right,
        );
    }
    let foot = area.bottom() - 4;
    text(
        f,
        area.x + 2,
        foot,
        area.width - 4,
        s.error
            .as_deref()
            .unwrap_or("↑↓ layer · ←→ volume · Space on/off"),
        if s.error.is_some() { PINK } else { MUTED },
        Alignment::Left,
    );
    text(
        f,
        area.x + 2,
        foot + 1,
        area.width - 4,
        "Esc cancel · Enter play/save · x clear",
        BLUE,
        Alignment::Left,
    );
    text(
        f,
        area.x + 2,
        foot + 2,
        area.width - 4,
        if s.audition {
            "v audition on · 1 Hearth  2 Café  3 Forest"
        } else {
            "v audition off · 1 Hearth  2 Café  3 Forest"
        },
        MUTED,
        Alignment::Left,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::Settings, config::Config};
    use ratatui::{backend::TestBackend, Terminal};
    use std::path::PathBuf;
    #[test]
    fn workspace_views_keep_the_clock_readable_and_edit_below_it() {
        use chrono::TimeZone;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let wall = Local.with_ymd_and_hms(2026, 11, 17, 22, 48, 7).unwrap();
        for (w, h) in [(80, 24), (120, 36)] {
            let baseline =
                crate::preview::snapshot(w, h, "plan", wall, Config::default(), 0.0).unwrap();
            let area = Composition::new(baseline.area, "22:48:07").clock;
            assert!(area.bottom() < h / 2);
            for mode in ["character", "goals", "conversation"] {
                let buffer =
                    crate::preview::snapshot(w, h, mode, wall, Config::default(), 0.0).unwrap();
                for y in area.y..area.bottom() {
                    for x in area.x..area.right() {
                        assert_eq!(
                            baseline.get(x, y),
                            buffer.get(x, y),
                            "{mode} obscured the clock"
                        );
                    }
                }
                let content = crate::preview::plain(&buffer);
                if mode == "conversation" {
                    assert!(content.contains("Ctrl+N new chat"));
                    assert!(!content.contains("g Tasks & objectives"));
                } else {
                    assert!(
                        content.contains("h Character") && content.contains("g Tasks & objectives")
                    );
                }
            }
            let mut app = App::new(Config::default(), PathBuf::new());
            let now = app.started;
            for code in [KeyCode::Char('h'), KeyCode::Char('+')] {
                app.key(KeyEvent::new(code, KeyModifiers::NONE), now);
            }
            app.paste("Practise patience 東京");
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| render(f, &app, wall, now)).unwrap();
            let content = crate::preview::plain(terminal.backend().buffer());
            assert!(
                content.contains("Practise patience") && content.contains("Enter save"),
                "{w}x{h}: {content}"
            );
            assert!(terminal.get_cursor().unwrap().1 > area.bottom());
            app.config.show_plan = false;
            app.overlay = Overlay::None;
            let centered =
                Composition::with_workspace(Rect::new(0, 0, w, h), "22:48:07", false).clock;
            assert!((i32::from(centered.y * 2 + centered.height) - i32::from(h)).abs() <= 1);
        }
    }
    #[test]
    fn appearance_draft_previews_the_clock_and_workspace_together() {
        let mut app = App::new(Config::default(), PathBuf::new());
        let mut settings = Settings::new(app.config.clone());
        settings.selected = SettingId::DashboardOpacity;
        settings.draft.dashboard_opacity = 100;
        let index = settings.draft.palette_index();
        settings.draft.clock_palettes[index].hours = "#ff9842".into();
        app.overlay = Overlay::Settings(settings);
        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        let wall = Local::now();
        terminal
            .draw(|f| render(f, &app, wall, app.started))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let layout = Composition::new(buffer.area, &clock::time(wall, &app.config));
        assert!((layout.clock.x..layout.clock.right()).any(|x| {
            (layout.clock.y..layout.clock.bottom())
                .any(|y| buffer.get(x, y).fg == Color::Rgb(255, 152, 66))
        }));
        let workspace =
            crate::planning_ui::workspace_area(buffer.area, &layout, false, false).unwrap();
        assert_eq!(buffer.get(workspace.x, workspace.y).bg, BG);
        assert_eq!(app.config.dashboard_opacity, 75);
    }
    #[test]
    fn compact_workspace_keeps_time_and_controls() {
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 11, 17, 22, 48, 7).unwrap();
        for mode in ["conversation", "goals", "character"] {
            let frame =
                crate::preview::snapshot(60, 17, mode, wall, Config::default(), 0.0).unwrap();
            let screen = crate::preview::plain(&frame);
            assert!(
                screen.contains("22:48:07") && screen.contains("Esc back"),
                "{mode}: {screen}"
            );
        }
        let frame = crate::preview::snapshot(60, 17, "plan", wall, Config::default(), 0.0).unwrap();
        let screen = crate::preview::plain(&frame);
        assert!(
            screen.contains("Review the task editor")
                && screen.contains("Listen before responding")
        );
        let frame = crate::preview::snapshot(60, 17, "help", wall, Config::default(), 0.0).unwrap();
        assert!(crate::preview::plain(&frame).contains("Esc back · ↑↓ scroll"));
    }
    #[test]
    fn workspace_navigation_ends_with_ai_at_the_bottom_of_the_panel() {
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 11, 17, 22, 48, 7).unwrap();
        for (w, h, action) in [
            (120, 36, "d Ask AI for a next step"),
            (80, 24, "d Ask AI"),
            (60, 17, "d Ask AI"),
            (28, 12, "d"),
        ] {
            let mut config = Config::default();
            config.ai.provider = crate::ai::Provider::Openai;
            let frame = crate::preview::snapshot(w, h, "plan", wall, config, 0.0).unwrap();
            let layout = Composition::new(frame.area, "22:48:07");
            let panel =
                crate::planning_ui::workspace_area(frame.area, &layout, false, false).unwrap();
            let row: String = (panel.x..panel.right())
                .map(|x| frame.get(x, panel.bottom() - 1).symbol())
                .collect();
            assert!(row.trim_start().starts_with('h'), "{w}x{h}: {row}");
            assert!(row.trim_end().ends_with(action), "{w}x{h}: {row}");
            assert!(row.find('+').unwrap() < row.rfind(action).unwrap());
        }
    }
    #[test]
    fn dashboard_wraps_task_titles_and_scrolls_to_unlinked_goals() {
        use crate::planner::{Entry, Kind};
        use chrono::TimeZone;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let wall = Local.with_ymd_and_hms(2026, 11, 17, 22, 48, 7).unwrap();
        let mut app = App::new(Config::default(), PathBuf::new());
        let mut task = Entry::blank(Kind::Task);
        task.title = "Write the first section before reviewing the final paragraph".into();
        app.planner.put(task).unwrap();
        for (kind, prefix) in [(Kind::Short, "Short-goal"), (Kind::Long, "Long-goal")] {
            for index in 1..=12 {
                let mut goal = Entry::blank(kind);
                goal.title = format!("{prefix}-{index:02}");
                app.planner.put(goal).unwrap();
            }
        }
        let mut archived = Entry::blank(Kind::Long);
        archived.title = "Archived-goal".into();
        archived.archived = true;
        app.planner.put(archived).unwrap();

        for (w, h) in [(120, 36), (60, 17)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            let now = app.started;
            app.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), now);
            terminal.draw(|f| render(f, &app, wall, now)).unwrap();
            let screen = crate::preview::plain(terminal.backend().buffer());
            let first = screen
                .lines()
                .position(|row| row.contains("Write the first"))
                .unwrap();
            let last = screen
                .lines()
                .position(|row| row.contains("final paragraph"))
                .unwrap();
            assert!(last > first);

            let mut seen = screen;
            for _ in 0..30 {
                app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), now);
                terminal.draw(|f| render(f, &app, wall, now)).unwrap();
                seen.push_str(&crate::preview::plain(terminal.backend().buffer()));
            }
            for prefix in ["Short-goal", "Long-goal"] {
                for index in 1..=12 {
                    assert!(seen.contains(&format!("{prefix}-{index:02}")));
                }
            }
            assert!(!seen.contains("Archived-goal"));
        }
    }
    #[test]
    fn daily_guidance_and_planning_overlays_fit_without_moving_the_clock() {
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 11, 17, 9, 32, 5).unwrap();
        for (w, h) in [(1, 1), (28, 10), (40, 12), (60, 17), (80, 24), (120, 36)] {
            for mode in ["plan", "goals", "conversation"] {
                let buffer =
                    crate::preview::snapshot(w, h, mode, wall, Config::default(), 0.0).unwrap();
                assert_eq!(buffer.area, Rect::new(0, 0, w, h));
            }
            for timer in [false, true] {
                let layout = Composition::new(Rect::new(0, 0, w, h), "09:32:05");
                if let Some(plan) =
                    crate::planning_ui::workspace_area(Rect::new(0, 0, w, h), &layout, timer, timer)
                {
                    assert!(!plan.intersects(layout.clock));
                    assert!(plan.bottom() <= h - 3);
                    if let Some(date) = layout.date {
                        assert!(!plan.intersects(date));
                    }
                    if timer {
                        for rect in [layout.timer, layout.progress].into_iter().flatten() {
                            assert!(!plan.intersects(rect));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn every_face_keeps_all_ten_numerals_distinct_at_both_scales() {
        for style in ClockStyle::ALL {
            for compact in [false, true] {
                for scale in [1, 2] {
                    let (cols, rows, gap) = if compact { (3, 5, 1) } else { (5, 7, 2) };
                    let advance = cols * scale + gap;
                    let mut buffer =
                        Buffer::empty(Rect::new(0, 0, advance * 10 - gap + 4, rows + 2));
                    let config = Config {
                        clock_style: style,
                        ..Config::default()
                    };
                    pixel_clock(
                        &mut buffer,
                        Rect::new(2, 1, advance * 10 - gap, rows),
                        "0123456789",
                        scale,
                        false,
                        &config,
                    );
                    let mut glyphs = Vec::new();
                    for digit in 0..10 {
                        let mut glyph = String::new();
                        for y in 1..=rows {
                            for x in 2 + digit * advance..2 + digit * advance + cols * scale {
                                glyph.push_str(buffer.get(x, y).symbol());
                            }
                        }
                        assert!(glyph.chars().any(|c| c != ' '));
                        assert!(!glyphs.contains(&glyph),"duplicate numeral {digit}, {style:?}, compact={compact}, scale={scale}");
                        glyphs.push(glyph);
                    }
                    for y in 0..rows + 2 {
                        assert_eq!(buffer.get(0, y).symbol(), " ");
                        assert_eq!(buffer.get(buffer.area.width - 1, y).symbol(), " ");
                    }
                }
            }
        }
    }
    #[test]
    fn outline_one_keeps_its_hook_and_baseline_aligned_with_the_stem() {
        for compact in [false, true] {
            for scale in [1, 2] {
                let (width, height, stem) = if compact { (3, 5, 1) } else { (5, 7, 2) };
                let mut buffer = Buffer::empty(Rect::new(0, 0, width * scale, height));
                pixel_clock(
                    &mut buffer,
                    Rect::new(0, 0, width * scale, height),
                    "1",
                    scale,
                    false,
                    &Config {
                        clock_style: ClockStyle::Outline,
                        ..Config::default()
                    },
                );
                assert_eq!(buffer.get(stem * scale, 0).symbol(), "╷");
                assert_eq!(buffer.get(stem * scale, 1).symbol(), "┤");
                assert_eq!(buffer.get(stem * scale, height - 1).symbol(), "┴");
                for y in 2..height - 1 {
                    assert_eq!(buffer.get(stem * scale, y).symbol(), "│");
                }
            }
        }
    }
    #[test]
    fn clock_styles_are_distinct_and_follow_the_selected_palette() {
        use crate::config::ClockStyle;
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 11, 16, 22, 48, 7).unwrap();
        let mut faces = Vec::new();
        let mut compact_faces = Vec::new();
        for style in ClockStyle::ALL {
            for (w, h) in [(40, 12), (60, 17), (80, 24), (120, 36)] {
                let config = Config {
                    clock_style: style,
                    clock_palette: 3,
                    ..Config::default()
                };
                let buffer =
                    crate::preview::snapshot(w, h, "clock", wall, config.clone(), 0.0).unwrap();
                let area = Composition::new(buffer.area, "22:48:07").clock;
                let pixels = (area.y..area.bottom())
                    .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
                    .map(|(x, y)| buffer.get(x, y))
                    .collect::<Vec<_>>();
                assert!(pixels
                    .iter()
                    .any(|cell| cell.fg == hex_color(&config.palette().hours).unwrap()));
                if w == 120 {
                    faces.push(pixels.iter().map(|c| c.symbol()).collect::<String>());
                } else if w == 60 {
                    compact_faces.push(pixels.iter().map(|c| c.symbol()).collect::<String>());
                }
            }
        }
        for set in [faces, compact_faces] {
            for i in 0..set.len() {
                for j in i + 1..set.len() {
                    assert_ne!(set[i], set[j]);
                }
            }
        }
    }
    #[test]
    fn renders_all_sizes_and_overlays_without_overflow() {
        for (w, h) in [
            (1, 1),
            (8, 3),
            (20, 6),
            (32, 10),
            (60, 18),
            (80, 24),
            (100, 30),
            (130, 36),
            (200, 60),
        ] {
            for mode in 0..5 {
                for weather in [
                    crate::config::Weather::Clear,
                    crate::config::Weather::Rain,
                    crate::config::Weather::Snow,
                ] {
                    let mut app = App::new(Config::default(), PathBuf::new());
                    app.config.weather = weather;
                    app.pomodoro_visible = mode == 1;
                    if mode == 2 {
                        app.overlay = Overlay::Help;
                    }
                    if mode == 3 {
                        app.overlay = Overlay::Settings(Settings {
                            draft: Config::default(),
                            selected: SettingId::Notifications,
                            confirm_discard: false,
                            error: None,
                            input: None,
                        });
                    }
                    if mode == 4 {
                        app.overlay = Overlay::Sounds(SoundSettings::new(&app.config, false));
                    }
                    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                    terminal
                        .draw(|f| render(f, &app, Local::now(), Instant::now()))
                        .unwrap();
                }
            }
        }
    }
    #[test]
    fn pomodoro_keeps_the_clock_pixels_in_place() {
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 10, 24, 22, 48, 7).unwrap();
        for (w, h) in [(70, 22), (80, 24), (120, 36)] {
            for seconds in [true, false] {
                let mut app = App::new(
                    Config {
                        show_seconds: seconds,
                        ..Config::default()
                    },
                    PathBuf::new(),
                );
                let now = app.started;
                let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| render(f, &app, wall, now)).unwrap();
                let clock_pixels = |b: &Buffer| {
                    b.content
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| {
                            c.symbol() == "█"
                                && (c.fg == hex_color(&app.config.palette().hours).unwrap()
                                    || c.fg == hex_color(&app.config.palette().seconds).unwrap())
                        })
                        .map(|(index, c)| (index, c.clone()))
                        .collect::<Vec<_>>()
                };
                let before = clock_pixels(terminal.backend().buffer());
                assert!(!before.is_empty());
                app.pomodoro_visible = true;
                app.timer.toggle(now);
                terminal.draw(|f| render(f, &app, wall, now)).unwrap();
                assert_eq!(before, clock_pixels(terminal.backend().buffer()));
                let text = crate::preview::plain(terminal.backend().buffer());
                assert!(text.contains("FOCUS") && text.contains("running"));
                assert!(text.contains("pause") && text.contains("quit"));
            }
        }
    }
    #[test]
    fn rendered_time_is_centered_in_compact_and_pixel_layouts() {
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 11, 2, 8, 48, 8).unwrap();
        for w in [18, 28, 34, 40, 44, 60, 70, 80, 120, 160] {
            for h in [8, 12, 18, 24, 36] {
                for seconds in [true, false] {
                    for format_24h in [true, false] {
                        let mut app = App::new(
                            Config {
                                show_seconds: seconds,
                                format_24h,
                                ..Config::default()
                            },
                            PathBuf::new(),
                        );
                        for pomodoro in [false, true] {
                            app.pomodoro_visible = pomodoro;
                            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                            terminal
                                .draw(|f| render(f, &app, wall, app.started))
                                .unwrap();
                            let area = Composition::new(
                                Rect::new(0, 0, w, h),
                                &clock::time(wall, &app.config),
                            )
                            .clock;
                            let mut columns = Vec::new();
                            for y in area.y..area.bottom() {
                                for x in area.x..area.right() {
                                    let cell = terminal.backend().buffer().get(x, y);
                                    if cell.symbol() != " " {
                                        columns.push(x);
                                    }
                                }
                            }
                            let left = *columns.iter().min().unwrap();
                            let right = *columns.iter().max().unwrap();
                            assert!(
                                (i32::from(left + right + 1) - i32::from(w)).abs() <= 1,
                                "uncentered rendered time at {w}x{h}"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn branding_and_clock_hierarchy_survive_normal_and_medium_windows() {
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 11, 16, 22, 48, 7).unwrap();
        for (w, h) in [(120, 36), (80, 24), (60, 17), (40, 12)] {
            for mode in ["clock", "focus"] {
                let buffer =
                    crate::preview::snapshot(w, h, mode, wall, Config::default(), 8.0).unwrap();
                let text = crate::preview::plain(&buffer);
                assert!(text.contains("a small pocket of quiet"), "{w}x{h}");
                assert!(text.contains("lofiji") || text.contains("l o f i j i"));
                let layout = Composition::new(buffer.area, "22:48:07");
                if h >= 16 {
                    assert!(layout.clock.height >= 5);
                }
                if let Some(header) = layout.header {
                    assert!(!header.intersects(layout.clock));
                }
            }
        }
    }
    #[test]
    fn scenery_never_changes_the_clock() {
        use crate::config::Weather;
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 10, 24, 18, 48, 7).unwrap();
        let mut app = App::new(Config::default(), PathBuf::new());
        let now = app.started;
        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        terminal.draw(|f| render(f, &app, wall, now)).unwrap();
        let before = terminal.backend().buffer().clone();
        for mode in [Weather::Rain, Weather::Snow] {
            app.config.weather = mode;
            terminal.draw(|f| render(f, &app, wall, now)).unwrap();
            let after = terminal.backend().buffer();
            for (i, cell) in before
                .content
                .iter()
                .enumerate()
                .filter(|(_, c)| c.symbol() == "█")
            {
                assert_eq!(cell, &after.content[i]);
            }
        }
    }
    #[test]
    fn every_scene_resizes_and_freezes_with_motion_disabled() {
        use crate::config::Background;
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 11, 16, 22, 48, 7).unwrap();
        for scene in Background::ALL {
            for (w, h) in [(1, 1), (20, 8), (40, 12), (60, 17), (80, 24), (120, 36)] {
                let config = Config {
                    background: scene,
                    animations: false,
                    ..Config::default()
                };
                let first =
                    crate::preview::snapshot(w, h, "clock", wall, config.clone(), 0.0).unwrap();
                let last = crate::preview::snapshot(w, h, "clock", wall, config, 30.0).unwrap();
                assert_eq!(first, last, "{scene:?} at {w}x{h}");
            }
        }
    }
    #[test]
    fn disabling_motion_freezes_gif_and_weather() {
        use crate::config::Weather;
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 11, 16, 22, 48, 7).unwrap();
        for weather in [Weather::Clear, Weather::Rain, Weather::Snow] {
            let config = Config {
                animations: false,
                weather,
                ..Config::default()
            };
            let first =
                crate::preview::snapshot(120, 36, "clock", wall, config.clone(), 0.0).unwrap();
            let later = crate::preview::snapshot(120, 36, "clock", wall, config, 60.0).unwrap();
            assert_eq!(first, later);
        }
    }
    #[test]
    fn dashboard_opacity_controls_scene_blending_and_appears_in_appearance() {
        use crate::config::Background;
        use chrono::TimeZone;
        let wall = Local.with_ymd_and_hms(2026, 11, 17, 22, 48, 7).unwrap();
        let layout = Composition::new(Rect::new(0, 0, 120, 36), "22:48:07");
        let panel =
            crate::planning_ui::workspace_area(Rect::new(0, 0, 120, 36), &layout, false, false)
                .unwrap();
        let render_with_opacity = |opacity| {
            crate::preview::snapshot(
                120,
                36,
                "plan",
                wall,
                Config {
                    background: Background::Bonfire,
                    image_brightness: 100,
                    dashboard_opacity: opacity,
                    ..Config::default()
                },
                0.0,
            )
            .unwrap()
        };
        let transparent = render_with_opacity(0);
        let default_buf = render_with_opacity(75);
        let opaque = render_with_opacity(100);
        let mut any_scene_diff = false;
        for y in panel.y..panel.bottom() {
            for x in panel.x..panel.right() {
                let opaque_cell = opaque.get(x, y);
                assert_eq!(opaque_cell.bg, BG);
                if transparent.get(x, y).bg != BG {
                    any_scene_diff = true;
                    assert_ne!(transparent.get(x, y).bg, default_buf.get(x, y).bg);
                    assert_ne!(default_buf.get(x, y).bg, BG);
                }
            }
        }
        assert!(any_scene_diff);

        let mut app = App::new(Config::default(), PathBuf::new());
        app.overlay = Overlay::Settings(Settings {
            draft: Config::default(),
            selected: SettingId::DashboardOpacity,
            confirm_discard: false,
            error: None,
            input: None,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| render(f, &app, wall, app.started))
            .unwrap();
        let screen = crate::preview::plain(terminal.backend().buffer());
        assert!(screen.contains("Appearance"));
        assert!(screen.contains("Dashboard opacity"));
        assert!(screen.contains("75%"));
        assert!(screen.contains("←→ adjust value"));
    }
}
