//! Optional audio-only mpv playback; the player owns and cleans up its process group.
use crate::config::{validate_music_url, Config};
use std::{
    collections::VecDeque,
    env,
    io::{self, Read},
    os::fd::AsRawFd,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Player {
    child: Option<Child>,
    monitors: Vec<JoinHandle<()>>,
    stop_readers: Arc<AtomicBool>,
    exit: Option<(ExitStatus, Instant)>,
    settings: Option<(String, u8)>,
    ready: bool,
    messages: Receiver<(u32, String)>,
    send: Sender<(u32, String)>,
    errors: VecDeque<String>,
}
impl Player {
    pub fn new() -> Self {
        let (send, messages) = mpsc::channel();
        Self {
            child: None,
            monitors: Vec::new(),
            stop_readers: Arc::new(AtomicBool::new(false)),
            exit: None,
            settings: None,
            ready: false,
            messages,
            send,
            errors: VecDeque::new(),
        }
    }
    pub fn sync(&mut self, enabled: bool, config: &Config) -> Result<(), String> {
        if !enabled {
            self.stop();
            return Ok(());
        }
        let settings = (config.music_url.clone(), config.music_volume);
        if self.child.is_some() && self.settings.as_ref() == Some(&settings) {
            return Ok(());
        }
        self.stop();
        validate_music_url(&config.music_url)?;
        if config.music_url.is_empty() {
            return Err("Set a YouTube link in settings first".into());
        }
        let helper = yt_dlp().ok_or("Music needs yt-dlp; install it and press m again")?;
        let mut command = Command::new("mpv");
        command
            .args([
                "--no-config",
                "--no-video",
                "--terminal=yes",
                "--term-osd=no",
                "--term-status-msg=",
                "--term-playing-msg=LOFIJI_AUDIO_READY",
                "--msg-level=all=warn,cplayer=info",
                "--input-terminal=no",
                "--ytdl=yes",
                "--ytdl-format=bestaudio[protocol^=m3u8]/bestaudio/best",
                "--loop-playlist=inf",
                "--network-timeout=15",
            ])
            .arg(format!("--volume={}", config.music_volume))
            .arg(format!(
                "--script-opts=ytdl_hook-ytdl_path={}",
                helper.display()
            ))
            .arg("--ytdl-raw-options=js-runtimes=node")
            .arg("--")
            .arg(&config.music_url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot start mpv: {e}"))?;
        let id = child.id();
        if let Some(stdout) = child.stdout.take() {
            self.monitors.push(monitor(
                stdout,
                id,
                self.send.clone(),
                self.stop_readers.clone(),
            ));
        }
        if let Some(stderr) = child.stderr.take() {
            self.monitors.push(monitor(
                stderr,
                id,
                self.send.clone(),
                self.stop_readers.clone(),
            ));
        }
        self.child = Some(child);
        self.settings = Some(settings);
        Ok(())
    }
    pub fn is_ready(&self) -> bool {
        self.ready
    }
    fn drain_messages(&mut self, id: u32) {
        for (pid, line) in self.messages.try_iter() {
            if pid != id {
                continue;
            }
            if line.contains("LOFIJI_AUDIO_READY") || line.contains("AO: [") {
                self.ready = true;
            }
            if self.errors.len() >= 12 {
                self.errors.pop_front();
            }
            self.errors.push_back(line);
        }
    }
    fn reset_state(&mut self) {
        self.settings = None;
        self.ready = false;
        self.errors.clear();
    }
    pub fn poll(&mut self) -> Option<String> {
        let id = self.child.as_ref()?.id();
        self.drain_messages(id);
        let failure = self
            .errors
            .iter()
            .any(|line| line.contains("ERROR:") || line.contains("Failed to open"));
        if self.exit.is_none() {
            let child = self.child.as_mut()?;
            let status = match child.try_wait() {
                Ok(Some(status)) => Some(status),
                Ok(None) if !self.ready && failure => {
                    signal_group(child);
                    child.wait().ok()
                }
                Ok(None) => None,
                Err(e) => {
                    self.stop();
                    return Some(format!("Music player error: {e}"));
                }
            };
            self.exit = status.map(|status| (status, Instant::now()));
        }
        let (status, exited_at) = self.exit?;
        // A descendant can hold a pipe open after mpv exits. Bound the drain
        // window while allowing the monitor to publish the last diagnostic.
        if exited_at.elapsed() >= Duration::from_millis(300) {
            self.stop_readers.store(true, Ordering::Relaxed);
        }
        if self.monitors.iter().any(|reader| !reader.is_finished()) {
            return None;
        }
        self.join_readers();
        self.drain_messages(id);
        let message = if status.success() {
            "Music stopped".into()
        } else {
            error_message(&self.errors)
        };
        self.stop();
        Some(message)
    }
    fn join_readers(&mut self) {
        for handle in self.monitors.drain(..) {
            let _ = handle.join();
        }
    }
    fn stop(&mut self) {
        if self.child.is_none() && self.monitors.is_empty() {
            return;
        }
        if let Some(mut child) = self.child.take() {
            if self.exit.is_none() && matches!(child.try_wait(), Ok(None)) {
                signal_group(&child);
                let _ = child.wait();
            }
        }
        self.exit = None;
        self.stop_readers.store(true, Ordering::Relaxed);
        self.join_readers();
        self.stop_readers = Arc::new(AtomicBool::new(false));
        self.messages.try_iter().for_each(drop);
        self.reset_state();
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
fn yt_dlp() -> Option<PathBuf> {
    let mut paths = env::var_os("PATH")
        .map(|s| {
            env::split_paths(&s)
                .map(|p| p.join("yt-dlp"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(home) = env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".local/bin/yt-dlp"));
    }
    paths.into_iter().find(|p| p.is_file())
}

fn signal_group(child: &Child) {
    // Spawn places mpv and its yt-dlp helpers in a dedicated process group.
    unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
}

fn monitor(
    mut stream: impl Read + AsRawFd + Send + 'static,
    id: u32,
    send: Sender<(u32, String)>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let fd = stream.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
        {
            return;
        }
        let mut chunk = [0; 4096];
        let mut line = Vec::new();
        let emit = |line: &[u8]| {
            let safe = String::from_utf8_lossy(line)
                .chars()
                .filter(|c| !c.is_control())
                .take(400)
                .collect();
            let _ = send.send((id, safe));
        };
        while !stop.load(Ordering::Relaxed) {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    for &byte in &chunk[..n] {
                        if byte == b'\n' {
                            emit(&line);
                            line.clear();
                        } else if line.len() < 1600 {
                            line.push(byte);
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    let mut event = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    unsafe { libc::poll(&mut event, 1, 20) };
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        if !line.is_empty() {
            emit(&line);
        }
    })
}
fn error_message(lines: &VecDeque<String>) -> String {
    let text = lines
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if text.contains("403") || text.contains("forbidden") {
        "YouTube refused the stream (403); press m to retry".into()
    } else if text.contains("sign in") || text.contains("not a bot") {
        "YouTube requires sign-in for this link; choose another source".into()
    } else if text.contains("audio") && (text.contains("failed") || text.contains("unavailable")) {
        "Audio output unavailable; check the system sound device".into()
    } else if text.contains("javascript") || text.contains("runtime") {
        "YouTube extraction failed; check Node.js and update yt-dlp".into()
    } else {
        "Music could not play; check the YouTube link and update yt-dlp".into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn useful_failures_do_not_expose_media_urls() {
        let lines = VecDeque::from([
            "Failed to open https://media.test/private?token=secret: 403 Forbidden".to_string(),
        ]);
        let message = error_message(&lines);
        assert!(message.contains("403"));
        assert!(!message.contains("token"));
    }
    #[test]
    fn polling_after_exit_keeps_the_trailing_stderr_diagnostic() {
        let mut player = Player::new();
        let mut child = Command::new("sh")
            .args(["-c", "printf 'HTTP 403 Forbidden' >&2; exit 1"])
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        let stream = child.stderr.take().unwrap();
        player.monitors.push(monitor(
            stream,
            child.id(),
            player.send.clone(),
            player.stop_readers.clone(),
        ));
        player.child = Some(child);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(message) = player.poll() {
                assert!(message.contains("403"), "{message}");
                break;
            }
            assert!(Instant::now() < deadline, "music exit was never collected");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(player.child.is_none());
    }
}
