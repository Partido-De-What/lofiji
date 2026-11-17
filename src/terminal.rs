use crossterm::{
    cursor::{Hide, Show},
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    style::{force_color_output, ResetColor},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io::{self, IsTerminal, Stdout, Write},
    sync::mpsc::{self, Receiver},
    thread,
};

/// Keep terminal reads off the main loop: Crossterm 0.27 may loop at PTY EOF.
/// Main can still observe signals and terminal hangup, restore state, and exit.
pub fn input_events() -> Receiver<io::Result<crossterm::event::Event>> {
    let (send, receive) = mpsc::sync_channel(64);
    thread::spawn(move || loop {
        let event = crossterm::event::read();
        let failed = event.is_err();
        if send.send(event).is_err() || failed {
            break;
        }
    });
    receive
}

pub fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        ResetColor,
        Show,
        DisableBracketedPaste,
        LeaveAlternateScreen
    );
}
/// A window can disappear between the main loop's TTY check and cleanup.
/// Ratatui 0.26 prints an error from Drop if cursor restoration fails;
/// tolerate writes only after the terminal is gone to avoid a second panic.
pub struct TerminalOutput(Stdout);
impl Write for TerminalOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self.0.write(bytes) {
            Err(_) if !self.0.is_terminal() => Ok(bytes.len()),
            result => result,
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self.0.flush() {
            Err(_) if !self.0.is_terminal() => Ok(()),
            result => result,
        }
    }
}
pub struct Session {
    pub terminal: Terminal<CrosstermBackend<TerminalOutput>>,
}
impl Session {
    pub fn enter() -> io::Result<Self> {
        // True color is required for scene pixels and custom PNG backgrounds.
        force_color_output(true);
        enable_raw_mode()?;
        let result = (|| {
            execute!(
                io::stdout(),
                EnterAlternateScreen,
                Hide,
                EnableBracketedPaste
            )?;
            Terminal::new(CrosstermBackend::new(TerminalOutput(io::stdout())))
        })();
        match result {
            Ok(terminal) => Ok(Self { terminal }),
            Err(e) => {
                restore();
                Err(e)
            }
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        restore();
    }
}
