//! Quiet front-screen guidance and progressively disclosed goal/conversation overlays.
use crate::{
    config::{hex_color, ClockPalette},
    input::TextInput,
    layout::Composition,
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
pub fn goals(f: &mut Frame, app: &crate::app::App, view: &Panel, date: chrono::NaiveDate) {
    let area = overlay_area(f, 82, 27);
    panel(f, area, "GOALS & NEXT STEPS");
    if area.width < 28 || area.height < 10 {
        text(
            f,
            area.x + 1,
            area.y + 2,
            area.width.saturating_sub(2),
            "Resize for goals · Esc close",
            MUTED,
            Alignment::Left,
        );
        return;
    }
    crate::dashboard::entries(
        f,
        app,
        view,
        Rect::new(area.x + 2, area.y + 2, area.width - 4, area.height - 3),
        date,
    );
}

pub(crate) fn wrapped_input(
    f: &mut Frame,
    area: Rect,
    input: &TextInput,
    active: bool,
    color: Color,
) {
    let width = usize::from(area.width);
    if width == 0 || area.height == 0 {
        return;
    }
    let mut rows = vec![String::new()];
    let mut columns = 0;
    let mut cursor = (0, 0);
    for (index, ch) in input.value.char_indices() {
        let step = Line::from(ch.to_string()).width();
        if columns + step > width {
            rows.push(String::new());
            columns = 0;
        }
        if index == input.cursor {
            cursor = (rows.len() - 1, columns);
        }
        rows.last_mut().unwrap().push(ch);
        columns += step;
    }
    if input.cursor == input.value.len() {
        if columns == width {
            rows.push(String::new());
            columns = 0;
        }
        cursor = (rows.len() - 1, columns);
    }
    let start = if active {
        (cursor.0 + 1).saturating_sub(usize::from(area.height))
    } else {
        0
    };
    let visible: Vec<_> = rows
        .into_iter()
        .skip(start)
        .take(usize::from(area.height))
        .map(Line::from)
        .collect();
    f.render_widget(
        Paragraph::new(visible).style(Style::default().fg(color)),
        area,
    );
    if active {
        f.set_cursor(area.x + cursor.1 as u16, area.y + (cursor.0 - start) as u16);
    }
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
        if size <= width {
            current.push_str(word);
            columns += size;
            continue;
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
