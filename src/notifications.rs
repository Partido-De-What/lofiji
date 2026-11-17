use crate::config::Notifications;
use std::{
    io::{self, Write},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

pub struct Notifier {
    requests: Sender<String>,
    results: Receiver<String>,
}
impl Notifier {
    pub fn new() -> Self {
        let (requests, receive) = mpsc::channel::<String>();
        let (result, results) = mpsc::channel();
        thread::spawn(move || {
            for message in receive {
                if let Err(e) = desktop(&message) {
                    let _ = result.send(e);
                }
            }
        });
        Self { requests, results }
    }
    pub fn send(&self, kind: Notifications, message: String) {
        match kind {
            Notifications::Off => {}
            Notifications::Bell => {
                let _ = io::stdout().write_all(b"\x07");
                let _ = io::stdout().flush();
            }
            Notifications::Desktop => {
                let _ = self.requests.send(message);
            }
        }
    }
    pub fn error(&self) -> Option<String> {
        self.results.try_recv().ok()
    }
}
fn desktop(message: &str) -> Result<(), String> {
    let mut child = Command::new("notify-send")
        .args(["--app-name=lofiji", "--expire-time=7000", "Lofiji", message])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| {
            "Desktop notification unavailable · install notify-send or choose terminal bell"
                .to_string()
        })?;
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => {
                return Err("Desktop notification failed · check your notification service".into())
            }
            Ok(None) if Instant::now() < until => thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Desktop notification timed out".into());
            }
        }
    }
}
