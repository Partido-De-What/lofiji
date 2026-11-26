//! Optional, offline ambient layers. Each voice owns its mpv process group.
use crate::config::SOUND_COUNT;
use std::{
    fs,
    io::Write,
    os::unix::{fs::DirBuilderExt, net::UnixStream, process::CommandExt},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const NAMES: [&str; SOUND_COUNT] = [
    "Campfire",
    "Light rain",
    "Street café",
    "Thunderstorm",
    "Wind",
    "Summer night",
    "Forest birds",
    "White noise",
    "Pink noise",
    "Brown noise",
    "Fireplace",
];
pub const ORDER: [usize; SOUND_COUNT] = [0, 10, 1, 2, 3, 4, 5, 6, 7, 8, 9];
const AUDIO: [&[u8]; SOUND_COUNT] = [
    include_bytes!("../assets/sounds/fire.ogg"),
    include_bytes!("../assets/sounds/rain.ogg"),
    include_bytes!("../assets/sounds/coffee.ogg"),
    include_bytes!("../assets/sounds/storm.ogg"),
    include_bytes!("../assets/sounds/wind.ogg"),
    include_bytes!("../assets/sounds/night.ogg"),
    include_bytes!("../assets/sounds/birds-tree.ogg"),
    include_bytes!("../assets/sounds/white.ogg"),
    include_bytes!("../assets/sounds/pink.ogg"),
    include_bytes!("../assets/sounds/brown.ogg"),
    include_bytes!("../assets/sounds/fireplace.ogg"),
];
struct Voice {
    child: Child,
    volume: u8,
    reaped: bool,
    pending_since: Option<Instant>,
    socket: PathBuf,
}
impl Voice {
    fn set_volume(&mut self, volume: u8) -> bool {
        let Ok(mut stream) = UnixStream::connect(&self.socket) else {
            return false;
        };
        if stream
            .set_write_timeout(Some(Duration::from_millis(10)))
            .is_err()
        {
            return false;
        }
        let command = format!("{{\"command\":[\"set_property\",\"volume\",{volume}]}}\n");
        if stream.write_all(command.as_bytes()).is_ok() {
            self.volume = volume;
            true
        } else {
            false
        }
    }
}
impl Drop for Voice {
    fn drop(&mut self) {
        if !self.reaped && matches!(self.child.try_wait(), Ok(None)) {
            unsafe {
                libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = self.child.wait();
        }
        let _ = fs::remove_file(&self.socket);
    }
}
pub struct Mixer {
    voices: Vec<Option<Voice>>,
    directory: Option<PathBuf>,
}
impl Mixer {
    pub fn new() -> Self {
        Self {
            voices: (0..SOUND_COUNT).map(|_| None).collect(),
            directory: None,
        }
    }
    pub fn sync(&mut self, enabled: bool, volumes: &[u8; SOUND_COUNT]) -> Result<(), String> {
        if !enabled {
            for voice in &mut self.voices {
                *voice = None;
            }
            return Ok(());
        }
        if self.directory.is_none() && volumes.iter().any(|v| *v > 0) {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("lofiji-sounds-{}-{stamp}", std::process::id()));
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&dir)
                .map_err(|e| format!("Cannot prepare ambient sounds: {e}"))?;
            self.directory = Some(dir.clone());
            for (i, bytes) in AUDIO.iter().enumerate() {
                fs::write(dir.join(format!("{i}.ogg")), bytes)
                    .map_err(|e| format!("Cannot prepare sound: {e}"))?;
            }
        }
        for (i, volume) in volumes.iter().copied().enumerate() {
            if volume == 0 {
                self.voices[i] = None;
                continue;
            }
            if let Some(voice) = &mut self.voices[i] {
                match voice.child.try_wait() {
                    Ok(None) => {
                        if voice.volume == volume || voice.set_volume(volume) {
                            voice.pending_since = None;
                        } else if voice
                            .pending_since
                            .get_or_insert_with(Instant::now)
                            .elapsed()
                            > Duration::from_secs(2)
                        {
                            return Err(format!("Cannot adjust {} volume through mpv", NAMES[i]));
                        }
                        continue;
                    }
                    Ok(Some(_)) => {
                        voice.reaped = true;
                        self.voices[i] = None;
                        return Err(format!(
                            "{} stopped; check mpv and the sound device",
                            NAMES[i]
                        ));
                    }
                    Err(e) => return Err(format!("Sound player error: {e}")),
                }
            }
            let dir = self.directory.as_ref().ok_or("Sound assets unavailable")?;
            let file = dir.join(format!("{i}.ogg"));
            let socket = dir.join(format!("{i}.sock"));
            let _ = fs::remove_file(&socket);
            let child = Command::new("mpv")
                .args([
                    "--no-config",
                    "--no-video",
                    "--no-terminal",
                    "--input-terminal=no",
                    "--loop-file=inf",
                ])
                .arg(format!("--input-ipc-server={}", socket.display()))
                .arg(format!("--volume={volume}"))
                .arg("--")
                .arg(file)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
                .map_err(|e| format!("Ambient sounds need mpv: {e}"))?;
            self.voices[i] = Some(Voice {
                child,
                volume,
                reaped: false,
                pending_since: None,
                socket,
            });
        }
        Ok(())
    }
}
impl Drop for Mixer {
    fn drop(&mut self) {
        self.voices.clear();
        if let Some(path) = self.directory.take() {
            let _ = fs::remove_dir_all(path);
        }
    }
}
