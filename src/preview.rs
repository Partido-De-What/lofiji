//! Deterministic exports use the same Ratatui renderer as the live terminal.
use crate::{
    app::{App, Overlay, Settings},
    config::Config,
    ui,
};
use chrono::{DateTime, Local};
use ratatui::{backend::TestBackend, buffer::Buffer, style::Color, Terminal};
use std::{fmt::Write, path::PathBuf, time::Duration};

pub fn snapshot(
    width: u16,
    height: u16,
    mode: &str,
    wall: DateTime<Local>,
    config: Config,
    elapsed: f64,
) -> Result<Buffer, String> {
    if width == 0 || height == 0 || width > 500 || height > 200 {
        return Err("Snapshot size must be between 1x1 and 500x200".into());
    }
    if !elapsed.is_finite() || !(0.0..=86400.0).contains(&elapsed) {
        return Err("Snapshot elapsed time must be between 0 and 86400 seconds".into());
    }
    let mut app = App::new(config, PathBuf::new());
    if matches!(mode, "plan" | "goals" | "conversation" | "character") {
        use crate::planner::{Entry, Kind, Priority};
        for (id, kind, title, parent, due, priority) in [
            (
                1,
                Kind::Long,
                "Publish a thoughtful Rust project",
                None,
                None,
                Priority::Normal,
            ),
            (
                2,
                Kind::Short,
                "Finish the first release",
                Some(1),
                None,
                Priority::Normal,
            ),
            (
                3,
                Kind::Task,
                "Review the task editor",
                Some(2),
                Some(wall.date_naive().to_string()),
                Priority::High,
            ),
            (
                4,
                Kind::Task,
                "Write the usage guide",
                Some(2),
                None,
                Priority::Normal,
            ),
            (
                5,
                Kind::Task,
                "Run the release checks",
                Some(2),
                None,
                Priority::Normal,
            ),
            (
                6,
                Kind::Character,
                "Listen before responding",
                None,
                None,
                Priority::Normal,
            ),
            (
                7,
                Kind::Character,
                "Choose patience over haste",
                None,
                None,
                Priority::Normal,
            ),
            (
                8,
                Kind::Character,
                "Keep small promises to yourself",
                None,
                None,
                Priority::Normal,
            ),
        ] {
            app.planner.data.entries.push(Entry {
                id,
                kind,
                title: title.into(),
                description: String::new(),
                parent,
                due,
                priority,
                done: false,
                archived: false,
            });
        }
    }
    match mode {
        "clock" | "plan" => {},
        "goals" => app.overlay=Overlay::Planner(crate::planning::Panel::new()),
        "character" => app.overlay=Overlay::Planner(crate::planning::Panel::character()),
        "conversation" => app.overlay=Overlay::Conversation(crate::planning::Conversation::new(&app.planner,wall.date_naive())),
        "focus" => {
            app.pomodoro_visible = true;
            app.timer.toggle(app.started);
        }
        "paused" => {
            app.pomodoro_visible = true;
            app.timer.toggle(app.started);
            app.timer.toggle(app.started);
        }
        "break" => {
            app.pomodoro_visible = true;
            app.timer.skip(&app.config, app.started);
            app.timer.toggle(app.started);
        }
        "ai_settings" => {app.overlay=Overlay::Settings(Settings {draft:app.config.clone(),selected:crate::app::AI_PROVIDER_ROW,error:None,input:None});}
        "settings" => app.overlay = Overlay::Settings(Settings::menu(app.config.clone())),
        "help" => app.overlay = Overlay::Help,
        "sounds" => app.overlay = Overlay::Sounds(Settings::new(app.config.clone())),
        _ => {
            return Err(
                "Mode must be clock, focus, paused, break, settings, sounds, plan, goals, character, conversation, ai_settings, or help".into(),
            )
        }
    }
    let now = app.started + Duration::from_secs_f64(elapsed);
    app.tick(now);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).map_err(|e| e.to_string())?;
    terminal
        .draw(|f| ui::render(f, &app, wall, now))
        .map_err(|e| e.to_string())?;
    Ok(terminal.backend().buffer().clone())
}
pub fn plain(buffer: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer.get(x, y).symbol());
        }
        out.push('\n');
    }
    out
}
fn color(c: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => "#1a1b26".into(),
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn svg(buffer: &Buffer) -> String {
    let mut out = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"#1a1b26\"/>", buffer.area.width*10, buffer.area.height*20, buffer.area.width*10, buffer.area.height*20);
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let c = buffer.get(x, y);
            let px = x * 10;
            let py = y * 20;
            let _ = write!(
                out,
                "<rect x=\"{px}\" y=\"{py}\" width=\"10\" height=\"20\" fill=\"{}\"/>",
                color(c.bg)
            );
            if c.symbol() == "█" {
                let _ = write!(
                    out,
                    "<rect x=\"{px}\" y=\"{py}\" width=\"10\" height=\"20\" fill=\"{}\"/>",
                    color(c.fg)
                );
            } else if c.symbol() == "▀" {
                let _ = write!(
                    out,
                    "<rect x=\"{px}\" y=\"{py}\" width=\"10\" height=\"10\" fill=\"{}\"/>",
                    color(c.fg)
                );
            } else if c.symbol() == "▄" {
                let _ = write!(
                    out,
                    "<rect x=\"{px}\" y=\"{}\" width=\"10\" height=\"10\" fill=\"{}\"/>",
                    py + 10,
                    color(c.fg)
                );
            } else if c.symbol() != " " {
                let _ = write!(out, "<text x=\"{px}\" y=\"{}\" fill=\"{}\" font-family=\"DejaVu Sans Mono,monospace\" font-size=\"16\">{}</text>", py+16, color(c.fg), escape(c.symbol()));
            }
        }
    }
    out.push_str("</svg>\n");
    out
}
