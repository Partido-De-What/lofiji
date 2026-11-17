//! Quiet front-screen guidance and progressively disclosed goal/conversation overlays.
use crate::{
    config::{hex_color, ClockPalette},
    input::TextInput,
    layout::Composition,
    planner::{Kind, Store},
    planning::{Conversation, Panel},
    theme::*,
    ui::{overlay_area, panel, text},
};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn workspace_area(
    size: Rect,
    layout: &Composition,
    timer: bool,
    progress: bool,
) -> Option<Rect> {
    if size.width < 28 || size.height < 12 {
        return None;
    }
    let end = if timer {
        layout
            .progress
            .filter(|_| progress)
            .or(layout.timer)
            .or(layout.date)
            .unwrap_or(layout.clock)
            .bottom()
    } else {
        layout.date.unwrap_or(layout.clock).bottom()
    };
    let y = end + 1;
    let bottom = size.height.saturating_sub(3);
    if y >= bottom {
        return None;
    }
    let width = size.width.saturating_sub(8).min(92);
    Some(Rect::new(
        (size.width - width) / 2,
        y,
        width,
        (bottom - y).min(22),
    ))
}
pub(crate) fn clipped(value: &str, width: u16) -> String {
    if Line::from(value).width() <= usize::from(width) {
        return value.into();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let n = Line::from(ch.to_string()).width();
        if used + n + 1 > usize::from(width) {
            break;
        }
        out.push(ch);
        used += n;
    }
    out.push('…');
    out
}
pub fn input_line(f: &mut Frame, area: Rect, input: &TextInput, color: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut start = input.cursor;
    let mut columns = 0;
    for (index, ch) in input.value[..input.cursor].char_indices().rev() {
        let step = Line::from(ch.to_string()).width();
        if columns + step >= usize::from(area.width) {
            break;
        }
        columns += step;
        start = index;
    }
    text(
        f,
        area.x,
        area.y,
        area.width,
        &input.value[start..],
        color,
        Alignment::Left,
    );
    f.set_cursor(area.x + columns as u16, area.y);
}
pub fn goals(f: &mut Frame, view: &Panel, store: &Store, palette: &ClockPalette) {
    let [accent, secondary, body, muted] = colors(palette);
    let area = overlay_area(f, 82, 27);
    panel(f, area, "GOALS & NEXT STEPS");
    if area.width < 28 || area.height < 10 {
        text(
            f,
            area.x + 1,
            area.y + 2,
            area.width.saturating_sub(2),
            "Resize for goals · Esc close",
            muted,
            Alignment::Left,
        );
        return;
    }
    if let Some(editor) = &view.editor {
        let kind = match editor.entry.kind {
            Kind::Task => "task",
            Kind::Short => "short-term goal",
            Kind::Long => "long-term goal",
            Kind::Character => "character reminder",
        };
        text(
            f,
            area.x + 2,
            area.y + 2,
            area.width - 4,
            format!(
                "{} {kind}",
                if editor.entry.id == 0 { "New" } else { "Edit" }
            ),
            body,
            Alignment::Left,
        );
        let labels = [
            "Title",
            "Linked goal · ←→ choose",
            "Due date · YYYY-MM-DD or blank",
            "Priority · ←→ choose",
        ];
        let values = [
            editor.title.value.clone(),
            editor
                .entry
                .parent
                .and_then(|id| store.data.entry(id))
                .map(|e| e.title.clone())
                .unwrap_or_else(|| "No linked goal".into()),
            editor.due.value.clone(),
            editor.entry.priority.label().into(),
        ];
        let fields = if editor.entry.kind == Kind::Character {
            vec![0]
        } else if area.height >= 22 {
            (0..4).collect::<Vec<_>>()
        } else {
            vec![editor.field]
        };
        for (row, field) in fields.into_iter().enumerate() {
            let y = area.y + 4 + row as u16 * 3;
            let active = field == editor.field;
            text(
                f,
                area.x + 2,
                y,
                area.width - 4,
                format!("{} {}", if active { "›" } else { " " }, labels[field]),
                if active { secondary } else { muted },
                Alignment::Left,
            );
            if active && (field == 0 || field == 2) {
                input_line(
                    f,
                    Rect::new(area.x + 4, y + 1, area.width - 8, 1),
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
                    area.x + 4,
                    y + 1,
                    area.width - 8,
                    clipped(&values[field], area.width - 8),
                    body,
                    Alignment::Left,
                );
            }
        }
        text(
            f,
            area.x + 2,
            area.bottom() - 4,
            area.width - 4,
            view.error
                .as_deref()
                .unwrap_or("Enter saves · Tab / Shift-Tab selects a field"),
            if view.error.is_some() { PINK } else { muted },
            Alignment::Left,
        );
        text(
            f,
            area.x + 2,
            area.bottom() - 3,
            area.width - 4,
            "←→ choose links/priority · Ctrl-U clears text · Esc cancels",
            accent,
            Alignment::Left,
        );
        return;
    }
    if area.width < 62 {
        text(
            f,
            area.x + 2,
            area.y + 2,
            area.width - 4,
            format!("← {} →", view.kind.label()),
            secondary,
            Alignment::Left,
        );
    } else {
        let tabs = Kind::OBJECTIVES
            .into_iter()
            .map(|kind| {
                Span::styled(
                    format!("  {}  ", kind.label()),
                    Style::default().fg(if kind == view.kind { secondary } else { muted }),
                )
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(Line::from(tabs)),
            Rect::new(area.x + 2, area.y + 2, area.width - 4, 1),
        );
    }
    let entries = view.entries(&store.data, chrono::Local::now().date_naive());
    let visible = usize::from(area.height.saturating_sub(9) / 2).max(1);
    let first = (view.selected + 1).saturating_sub(visible);
    if entries.is_empty() {
        text(
            f,
            area.x + 3,
            area.y + 5,
            area.width - 6,
            "Nothing here yet. Press + to add one.",
            body,
            Alignment::Left,
        );
    }
    for (i, entry) in entries.iter().enumerate().skip(first).take(visible) {
        let y = area.y + 4 + (i - first) as u16 * 2;
        if i == view.selected {
            f.buffer_mut().set_style(
                Rect::new(area.x + 1, y, area.width - 2, 2),
                Style::default().bg(SURFACE),
            );
        }
        let title = format!(
            "{} [{}] #{} {}",
            if i == view.selected { "›" } else { " " },
            if entry.done { "x" } else { " " },
            entry.id,
            entry.title
        );
        text(
            f,
            area.x + 2,
            y,
            area.width - 4,
            clipped(&title, area.width - 4),
            if entry.done { muted } else { body },
            Alignment::Left,
        );
        let mut context = vec![entry.priority.label().to_owned()];
        if let Some(due) = &entry.due {
            context.push(format!("due {due}"));
        }
        if let Some(parent) = entry.parent.and_then(|id| store.data.entry(id)) {
            context.push(format!("→ {}", parent.title));
        }
        text(
            f,
            area.x + 6,
            y + 1,
            area.width - 8,
            clipped(&context.join(" · "), area.width - 8),
            muted,
            Alignment::Left,
        );
    }
    let error = view.error.as_deref().or(store.error.as_deref());
    text(
        f,
        area.x + 2,
        area.bottom() - 4,
        area.width - 4,
        error.unwrap_or("+ add · Enter edit · Space complete/reopen · p priority"),
        if error.is_some() { PINK } else { body },
        Alignment::Left,
    );
    text(
        f,
        area.x + 2,
        area.bottom() - 3,
        area.width - 4,
        "←→ category · ↑↓ select · x archive · u undo · Esc close",
        accent,
        Alignment::Left,
    );
    text(
        f,
        area.x + 2,
        area.bottom() - 2,
        area.width - 4,
        "Saved locally · c on the clock opens Plan with me",
        muted,
        Alignment::Left,
    );
}
pub(crate) const WAIT_FRAME_MS: u64 = 125;

pub(crate) fn chat_status(
    view: &Conversation,
    ai: &crate::ai::Settings,
    elapsed: std::time::Duration,
) -> String {
    if ai.provider == crate::ai::Provider::Off {
        return "Talk · Local suggestions · AI off".into();
    }
    let model = match (ai.provider, ai.model.as_str()) {
        (crate::ai::Provider::Chatgpt, "") => "automatic model",
        (_, "") => "choose a model in Settings",
        (_, model) => model,
    };
    let mut status = String::new();
    if view.busy {
        let frames = ["◐", "◓", "◑", "◒"];
        let frame = (elapsed.as_millis() / u128::from(WAIT_FRAME_MS)) % frames.len() as u128;
        status = format!("{} Waiting for AI · ", frames[frame as usize]);
    }
    status.push_str(ai.provider.label());
    if ai.supports_effort() {
        status.push_str(&format!(
            " · Effort {}",
            view.effort(ai.thinking_effort).label()
        ));
    }
    status.push_str(" · ");
    status.push_str(model);
    status
}

pub(crate) fn chat_controls(
    view: &Conversation,
    ai: &crate::ai::Settings,
    width: u16,
) -> &'static str {
    if view.busy {
        "Ctrl+K cancel reply · Esc back"
    } else if ai.supports_effort() {
        if width >= 56 {
            "Enter send · Ctrl+E effort · Ctrl+N new chat · Esc back"
        } else {
            "Enter send · Ctrl+E effort · Esc back"
        }
    } else {
        "Enter send · Esc back · Ctrl+N new chat"
    }
}

pub fn conversation(
    f: &mut Frame,
    view: &Conversation,
    palette: &ClockPalette,
    ai: &crate::ai::Settings,
    elapsed: std::time::Duration,
) {
    let [accent, secondary, body, muted] = colors(palette);
    let area = overlay_area(f, 86, 29);
    panel(f, area, "PLAN WITH ME");
    if area.width < 28 || area.height < 10 {
        return;
    }
    text(
        f,
        area.x + 2,
        area.y + 2,
        area.width - 4,
        clipped(&chat_status(view, ai, elapsed), area.width - 4),
        if view.busy { secondary } else { muted },
        Alignment::Left,
    );
    if ai.provider != crate::ai::Provider::Off {
        text(
            f,
            area.x + 2,
            area.bottom() - 3,
            area.width - 4,
            "Sends active goals/tasks and recent chat to the chosen endpoint",
            muted,
            Alignment::Left,
        );
    }
    let width = area.width - 6;
    let mut lines = Vec::new();
    for (who, message) in &view.messages {
        lines.push(Line::from(Span::styled(
            who.clone(),
            Style::default().fg(if who == "You" { secondary } else { accent }),
        )));
        for raw in message.lines() {
            for line in wrap_lines(raw, usize::from(width)) {
                lines.push(Line::from(Span::styled(line, Style::default().fg(body))));
            }
        }
        lines.push(Line::from(""));
    }
    let available = area.height - 9;
    let scroll = lines
        .len()
        .saturating_sub(usize::from(available))
        .saturating_sub(view.scroll_back)
        .min(usize::from(u16::MAX)) as u16;
    f.render_widget(
        Paragraph::new(lines).scroll((scroll, 0)),
        Rect::new(area.x + 3, area.y + 4, width, available),
    );
    text(
        f,
        area.x + 2,
        area.bottom() - 4,
        2,
        ">",
        secondary,
        Alignment::Left,
    );
    input_line(
        f,
        Rect::new(area.x + 4, area.bottom() - 4, area.width - 7, 1),
        &view.input,
        body,
    );
    text(
        f,
        area.x + 2,
        area.bottom() - 2,
        area.width - 4,
        chat_controls(view, ai, area.width - 4),
        body,
        Alignment::Left,
    );
}

pub(crate) fn wrap_lines(value: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut columns = 0;
    for word in value.split_whitespace() {
        let size = Line::from(word).width();
        if !current.is_empty() && columns + 1 + size > width {
            lines.push(std::mem::take(&mut current));
            columns = 0;
        }
        if !current.is_empty() {
            current.push(' ');
            columns += 1;
        }
        for ch in word.chars() {
            let step = Line::from(ch.to_string()).width();
            if columns + step > width && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                columns = 0;
            }
            current.push(ch);
            columns += step;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

pub(crate) fn colors(p: &ClockPalette) -> [Color; 4] {
    [
        hex_color(&p.hours).unwrap_or(BLUE),
        hex_color(&p.seconds).unwrap_or(CYAN),
        hex_color(&p.date).unwrap_or(TEXT),
        hex_color(&p.separator).unwrap_or(MUTED),
    ]
}
