//! Optional audio-only mpv playback; the player owns and cleans up its process group.
use crate::config::{validate_music_url, Config};
use std::{
    collections::VecDeque,
    env,
    io::{BufRead, BufReader, Read},
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

pub struct Player {
    child: Option<Child>,
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
            monitor(stdout, id, self.send.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            monitor(stderr, id, self.send.clone());
        }
        self.child = Some(child);
        self.settings = Some(settings);
        Ok(())
    }
    pub fn is_ready(&self) -> bool {
        self.ready
    }
    pub fn poll(&mut self) -> Option<String> {
        let id = self.child.as_ref()?.id();
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
        let failure = self
            .errors
            .iter()
            .any(|line| line.contains("ERROR:") || line.contains("Failed to open"));
        let exit = self.child.as_mut()?.try_wait();
        match exit {
            Ok(Some(status)) => {
                let message = if status.success() {
                    "Music stopped".into()
                } else {
                    error_message(&self.errors)
                };
                self.stop();
                Some(message)
            }
            Err(e) => {
                self.stop();
                Some(format!("Music player error: {e}"))
            }
            Ok(None) if !self.ready && failure => {
                let message = error_message(&self.errors);
                self.stop();
                Some(message)
            }
            Ok(None) => None,
        }
    }
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // process_group(0) creates this dedicated group, including yt-dlp helpers.
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.wait();
        }
        self.settings = None;
        self.ready = false;
        self.errors.clear();
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

fn monitor(stream: impl Read + Send + 'static, id: u32, send: Sender<(u32, String)>) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let line = line
                .chars()
                .filter(|c| !c.is_control())
                .take(400)
                .collect::<String>();
            if send.send((id, line)).is_err() {
                break;
            }
        }
    });
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
}
