mod ai;
mod ai_runtime;
mod ambient;
mod app;
mod background;
mod clock;
mod config;
mod dashboard;
mod input;
mod layout;
mod music;
mod notifications;
mod planner;
mod planning;
mod planning_ui;
mod preview;
mod scene_motion;
mod terminal;
mod theme;
mod timer;
mod ui;
mod weather;

use app::App;
use chrono::{Local, NaiveDateTime, TimeZone, Timelike};
use crossterm::event::{Event, KeyEventKind};
use std::{
    env,
    io::{self, IsTerminal, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const HELP: &str = "Lofiji — a small pocket of quiet\n\nUsage: lofiji [--config PATH] [--background SCENE]\n       lofiji --snapshot WIDTHxHEIGHT [--mode MODE] [--output PATH.svg]\n\nKeys: h character · g goals · + task · c talk · d AI brief · v workspace · b background · w weather · m music · a sounds · t clock style · p Pomodoro · s settings · ? help · q quit\n\nOptions:\n  --config PATH       Settings (default: $XDG_CONFIG_HOME/lofiji/config.toml)\n  --background SCENE  window, city, forest, snow, bonfire, image\n  --image PATH        Custom PNG / GIF background (selects image scene)\n  --weather MODE      clear, rain, snow\n  --music             Start optional YouTube audio on launch\n  --music-url URL     Override the saved YouTube link for this launch\n  --snapshot WxH      Render a preview without entering the terminal\n  --mode MODE         clock, focus, paused, break, settings, sounds, help, plan, goals, character, conversation, ai_settings\n  --at TIME           Local date/time (YYYY-MM-DDTHH:MM:SS) for previews\n  --elapsed SECONDS   Preview animation after elapsed time\n  --output PATH       Save preview as SVG, otherwise print text\n  --check-config      Validate settings and print their location\n  -h, --help          Show help\n  -V, --version       Show version\n";

fn main() {
    if let Err(error) = run() {
        let _ = writeln!(io::stderr(), "lofiji: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mut path = None;
    let mut snapshot = None;
    let mut mode = "clock".to_string();
    let mut at = None;
    let mut output = None;
    let mut check = false;
    let mut weather = None;
    let mut elapsed = 0.0;
    let mut background = None;
    let mut image = None;
    let mut music_on = false;
    let mut music_url = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(());
            }
            "--version" | "-V" => {
                println!("lofiji {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--config" => {
                path = Some(PathBuf::from(
                    args.next().ok_or("--config requires a path")?,
                ))
            }
            "--snapshot" => snapshot = Some(args.next().ok_or("--snapshot requires WIDTHxHEIGHT")?),
            "--mode" => mode = args.next().ok_or("--mode requires a value")?,
            "--at" => at = Some(args.next().ok_or("--at requires a local timestamp")?),
            "--output" => {
                output = Some(PathBuf::from(
                    args.next().ok_or("--output requires a path")?,
                ))
            }
            "--elapsed" => {
                elapsed = args
                    .next()
                    .ok_or("--elapsed requires seconds")?
                    .parse::<f64>()
                    .map_err(|_| "Invalid elapsed seconds")?
            }
            "--check-config" => check = true,
            "--background" => {
                background = Some(config::Background::parse(
                    &args.next().ok_or("--background requires a scene")?,
                )?)
            }
            "--image" => {
                image = Some(
                    std::fs::canonicalize(args.next().ok_or("--image requires a PNG path")?)
                        .map_err(|e| format!("Cannot open PNG: {e}"))?,
                )
            }
            "--music" => music_on = true,
            "--music-url" => music_url = Some(args.next().ok_or("--music-url requires a link")?),
            "--weather" => {
                weather = Some(config::Weather::parse(
                    &args
                        .next()
                        .ok_or("--weather requires clear, rain, or snow")?,
                )?)
            }
            _ => return Err(format!("Unknown option {arg:?}. Try --help.")),
        }
    }
    if snapshot.is_none() && (at.is_some() || output.is_some() || mode != "clock" || elapsed != 0.0)
    {
        return Err("--at, --mode, --elapsed and --output require --snapshot".into());
    }
    let default_path = path.is_none();
    let path = match path {
        Some(p) => p,
        None => config::config_path()?,
    };
    // Import earlier preferences without writing back to the old application directory.
    let legacy = path
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("nekoji/config.toml"));
    let source = if default_path && !path.exists() {
        legacy.as_ref().filter(|p| p.exists()).unwrap_or(&path)
    } else {
        &path
    };
    let mut config = config::Config::load(source)?;
    if let Some(background) = background {
        config.background = background;
    }
    if let Some(image) = image {
        config.image_path = image.to_string_lossy().into_owned();
        config.background = config::Background::Image;
    }
    if let Some(url) = music_url {
        config.music_url = url;
    }
    config.validate()?;
    if let Some(weather) = weather {
        config.weather = weather;
    }
    if check {
        if config.background == config::Background::Image {
            background::Image::load(&config.image_path, &path)?;
        }
        println!("Settings valid: {}", path.display());
        return Ok(());
    }
    if config.background == config::Background::Image
        && !config.image_path.is_empty()
        && !config.image_path.starts_with("~/")
    {
        let p = std::path::Path::new(&config.image_path);
        if p.is_relative() {
            config.image_path = path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join(p)
                .canonicalize()
                .map_err(|e| format!("Cannot open PNG: {e}"))?
                .to_string_lossy()
                .into_owned();
        }
    }
    if let Some(size) = snapshot {
        let (w, h) = size.split_once('x').ok_or("Use --snapshot WIDTHxHEIGHT")?;
        let w = w.parse::<u16>().map_err(|_| "Invalid snapshot width")?;
        let h = h.parse::<u16>().map_err(|_| "Invalid snapshot height")?;
        let wall = if let Some(at) = at {
            let date = NaiveDateTime::parse_from_str(&at, "%Y-%m-%dT%H:%M:%S")
                .map_err(|e| e.to_string())?;
            Local
                .from_local_datetime(&date)
                .single()
                .ok_or("Snapshot timestamp is ambiguous or nonexistent in your timezone")?
        } else {
            Local::now()
        };
        if config.background == config::Background::Image {
            background::Image::load(&config.image_path, &path)?;
        }
        let buffer = preview::snapshot(w, h, &mode, wall, config, elapsed)?;
        if let Some(output) = output {
            std::fs::write(output, preview::svg(&buffer)).map_err(|e| e.to_string())?;
        } else {
            print!("{}", preview::plain(&buffer));
        }
        return Ok(());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Run in an interactive terminal, or use --snapshot 120x36".into());
    }
    let shutdown = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register(signal, Arc::clone(&shutdown)).map_err(|e| e.to_string())?;
    }
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        terminal::restore();
        old_hook(info);
    }));
    let mut session = terminal::Session::enter().map_err(|e| e.to_string())?;
    let mut app = App::new(config, path);
    app.music_on = music_on;
    if let Ok(size) = session.terminal.size() {
        app.viewport_height = size.height;
    }
    let mut music = music::Player::new();
    let mut ambient = ambient::Mixer::new();
    let notifier = notifications::Notifier::new();
    let mut assistant = ai_runtime::Runtime::new();
    app.ai_daily_requested = app.config.ai.auto_brief
        && app.config.ai.provider != ai::Provider::Off
        && !app.planner.data.entries.is_empty();
    let events = terminal::input_events();
    let mut next_draw = Instant::now();
    let mut dirty = true;
    let mut pending_event = None;
    while !app.quit && !shutdown.load(Ordering::Relaxed) {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            break;
        }
        let now = Instant::now();
        let wall = Local::now();
        let completions = app.tick(now);
        if completions > 0 {
            let message = format!(
                "Session complete. {} is {}.",
                app.timer.phase.label(),
                if app.timer.status == timer::Status::Running {
                    "running"
                } else {
                    "ready"
                }
            );
            notifier.send(app.config.notifications, message);
            dirty = true;
        }
        // Reconcile the deadline before processing input, including keys that
        // arrive just after a session ends while poll() was waiting.
        if let Some(event) = pending_event.take() {
            match event {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    app.key(key, now);
                    dirty = true;
                }
                Event::Resize(_, height) => {
                    app.viewport_height = height;
                    dirty = true;
                }
                Event::Paste(text) => {
                    app.paste(&text);
                    dirty = true;
                }
                _ => {}
            }
        }
        if let Err(error) = music.sync(app.music_on, &app.config) {
            app.music_on = false;
            app.message(error, now);
            dirty = true;
        }
        if let Some(message) = music.poll() {
            app.music_on = false;
            app.message(message, now);
            dirty = true;
        }
        if app.music_ready != music.is_ready() {
            app.music_ready = music.is_ready();
            dirty = true;
            if app.music_ready {
                app.message("Music playing", now);
            }
        }
        if let Err(error) = ambient.sync(app.ambient_on, &app.config.sound_volumes) {
            app.ambient_on = false;
            let _ = ambient.sync(false, &app.config.sound_volumes);
            app.message(error, now);
            dirty = true;
        }
        dirty |= assistant.tick(&mut app, now);
        dirty |= app.tick_chatgpt();
        if let Some(error) = notifier.error() {
            app.message(error, now);
            dirty = true;
        }
        if dirty || now >= next_draw {
            session
                .terminal
                .draw(|f| ui::render(f, &app, wall, now))
                .map_err(|e| e.to_string())?;
            let step = if !app.config.animations {
                1000
            } else if app.image.as_ref().is_some_and(|image| image.animated()) {
                100
            } else if app.config.weather == config::Weather::Clear {
                500
            } else {
                250
            };
            let until = step - (u64::from(wall.nanosecond()) / 1_000_000) % step;
            next_draw = now + Duration::from_millis(until);
            if app.chat_waiting() {
                let frame_ms = u128::from(planning_ui::WAIT_FRAME_MS);
                let elapsed = now.saturating_duration_since(app.started).as_millis();
                let next_frame = (frame_ms - elapsed % frame_ms) as u64;
                next_draw = next_draw.min(now + Duration::from_millis(next_frame));
            }
            dirty = false;
        }
        let timeout = next_draw
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(250));
        match events.recv_timeout(timeout) {
            Ok(event) => pending_event = Some(event.map_err(|e| e.to_string())?),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}
