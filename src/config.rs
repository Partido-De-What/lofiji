use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Notifications {
    #[default]
    Off,
    Bell,
    Desktop,
}
impl Notifications {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Bell => "Terminal bell",
            Self::Desktop => "Desktop",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Bell,
            Self::Bell => Self::Desktop,
            Self::Desktop => Self::Off,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Weather {
    #[default]
    Clear,
    Rain,
    Snow,
}
impl Weather {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clear => "Clear",
            Self::Rain => "Rain",
            Self::Snow => "Snow",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Clear => Self::Rain,
            Self::Rain => Self::Snow,
            Self::Snow => Self::Clear,
        }
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "clear" => Ok(Self::Clear),
            "rain" => Ok(Self::Rain),
            "snow" => Ok(Self::Snow),
            _ => Err("Weather must be clear, rain, or snow".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClockStyle {
    #[default]
    Pixel,
    Round,
    Digital,
    Dots,
    Outline,
    Block,
    Slant,
    Scanline,
}
impl ClockStyle {
    pub const ALL: [Self; 8] = [
        Self::Pixel,
        Self::Round,
        Self::Digital,
        Self::Dots,
        Self::Outline,
        Self::Block,
        Self::Slant,
        Self::Scanline,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Pixel => "Pixel",
            Self::Round => "Rounded",
            Self::Digital => "Seven segment",
            Self::Dots => "Dot matrix",
            Self::Outline => "Outline",
            Self::Block => "Block",
            Self::Slant => "Slanted",
            Self::Scanline => "Scanlines",
        }
    }
    pub fn next(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|s| *s == self).unwrap() + 1) % Self::ALL.len()]
    }
    pub fn previous(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|s| *s == self).unwrap() + Self::ALL.len() - 1)
            % Self::ALL.len()]
    }
}

pub const SOUND_COUNT: usize = 11;
pub const DEFAULT_MUSIC_URL: &str = "https://www.youtube.com/watch?v=DEWzT1geuPU";
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Background {
    #[default]
    #[serde(alias = "moonlight", alias = "cafe")]
    Window,
    #[serde(alias = "tokyo", alias = "train")]
    City,
    #[serde(alias = "pines")]
    Forest,
    Snow,
    Bonfire,
    Image,
}
impl Background {
    pub const ALL: [Self; 6] = [
        Self::Window,
        Self::City,
        Self::Forest,
        Self::Snow,
        Self::Bonfire,
        Self::Image,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Window => "Rainy window · GIF",
            Self::City => "Moonlit city · PNG",
            Self::Forest => "Quiet forest · PNG",
            Self::Snow => "Snow mountains · PNG",
            Self::Bonfire => "Bonfire · PNG",
            Self::Image => "Custom PNG / GIF",
        }
    }
    pub fn next(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|s| *s == self).unwrap() + 1) % Self::ALL.len()]
    }
    pub fn previous(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|s| *s == self).unwrap() + Self::ALL.len() - 1)
            % Self::ALL.len()]
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "window" | "moonlight" | "cafe" => Ok(Self::Window),
            "city" | "tokyo" | "train" => Ok(Self::City),
            "forest" | "pines" => Ok(Self::Forest),
            "snow" => Ok(Self::Snow),
            "bonfire" => Ok(Self::Bonfire),
            "image" => Ok(Self::Image),
            _ => Err("Scene must be window, city, forest, snow, bonfire, or image".into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClockPalette {
    pub name: String,
    pub hours: String,
    pub seconds: String,
    pub separator: String,
    pub date: String,
}
impl ClockPalette {
    pub fn defaults() -> Vec<Self> {
        [
            ("Tokyo", ["#7aa2f7", "#bb9af7", "#8095bf", "#c0caf5"]),
            ("Moonlight", ["#b9d8ea", "#97b5d4", "#829db2", "#cbd4e2"]),
            ("Ember", ["#edc291", "#df9b79", "#b59987", "#e1d1bd"]),
            ("Sage", ["#b1c9aa", "#87bdb5", "#849d90", "#d3ddcc"]),
            ("Lavender", ["#c8b6e8", "#e2b7ce", "#9f8dab", "#dfd5e8"]),
        ]
        .into_iter()
        .map(|(name, colors)| Self {
            name: name.into(),
            hours: colors[0].into(),
            seconds: colors[1].into(),
            separator: colors[2].into(),
            date: colors[3].into(),
        })
        .collect()
    }
}
pub fn hex_color(value: &str) -> Result<ratatui::style::Color, String> {
    if value.len() != 7
        || !value.starts_with('#')
        || !value[1..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Use a color in #RRGGBB format".into());
    }
    let n = u32::from_str_radix(&value[1..], 16).map_err(|_| "Invalid hex color")?;
    Ok(ratatui::style::Color::Rgb(
        (n >> 16) as u8,
        (n >> 8) as u8,
        n as u8,
    ))
}
pub fn validate_music_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Ok(());
    }
    let host = url
        .strip_prefix("https://")
        .and_then(|s| s.split('/').next())
        .unwrap_or("");
    if url.len() > 2048
        || url.chars().any(|c| c.is_control() || c.is_whitespace())
        || !matches!(
            host,
            "youtube.com" | "www.youtube.com" | "music.youtube.com" | "youtu.be"
        )
    {
        return Err("Use an HTTPS YouTube or youtu.be link".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub format_24h: bool,
    pub show_plan: bool,
    pub ai: crate::ai::Settings,
    pub show_seconds: bool,
    pub blink_separator: bool,
    pub animations: bool,
    pub weather: Weather,
    pub background: Background,
    pub image_path: String,
    pub image_brightness: u8,
    pub music_url: String,
    pub music_volume: u8,
    pub clock_palette: usize,
    pub clock_style: ClockStyle,
    pub clock_palettes: Vec<ClockPalette>,
    pub sound_volumes: [u8; SOUND_COUNT],
    pub focus_minutes: u64,
    pub short_break_minutes: u64,
    pub long_break_minutes: u64,
    pub auto_advance: bool,
    pub notifications: Notifications,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            format_24h: true,
            show_plan: true,
            ai: crate::ai::Settings::default(),
            show_seconds: true,
            blink_separator: true,
            animations: true,
            weather: Weather::Clear,
            background: Background::Window,
            image_path: String::new(),
            image_brightness: 30,
            music_url: DEFAULT_MUSIC_URL.into(),
            music_volume: 25,
            clock_palette: 0,
            clock_style: ClockStyle::Pixel,
            clock_palettes: ClockPalette::defaults(),
            sound_volumes: [0; SOUND_COUNT],
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            auto_advance: false,
            notifications: Notifications::Off,
        }
    }
}
impl Config {
    pub fn palette_index(&self) -> usize {
        if self.clock_palette > 0 {
            return (self.clock_palette - 1).min(self.clock_palettes.len() - 1);
        }
        match self.background {
            Background::City => 0,
            Background::Window | Background::Snow | Background::Image => 1,
            Background::Bonfire => 2,
            Background::Forest => 3,
        }
    }
    pub fn palette(&self) -> &ClockPalette {
        &self.clock_palettes[self.palette_index()]
    }
    pub fn validate(&self) -> Result<(), String> {
        validate_music_url(&self.music_url)?;
        self.ai.validate()?;
        if self.clock_palettes.len() < 5
            || self.clock_palettes.len() > 16
            || self.clock_palette > self.clock_palettes.len()
        {
            return Err(
                "Keep 5–16 clock palettes and select an existing palette (0 means Scene)".into(),
            );
        }
        for p in &self.clock_palettes {
            if p.name.is_empty() || p.name.len() > 32 || p.name.chars().any(char::is_control) {
                return Err("Palette names must contain 1–32 characters".into());
            }
            for color in [&p.hours, &p.seconds, &p.separator, &p.date] {
                hex_color(color)?;
            }
        }
        if self.sound_volumes.iter().any(|v| *v > 100) {
            return Err("Sound volumes must be between 0 and 100".into());
        }

        if self.image_brightness > 100 || self.music_volume > 100 {
            return Err("Image brightness and music volume must be between 0 and 100".into());
        }
        if self.image_path.len() > 4096 || self.image_path.chars().any(char::is_control) {
            return Err("Invalid PNG / GIF path".into());
        }
        for (name, value) in [
            ("focus", self.focus_minutes),
            ("short break", self.short_break_minutes),
            ("long break", self.long_break_minutes),
        ] {
            if !(1..=180).contains(&value) {
                return Err(format!("{name} duration must be between 1 and 180 minutes"));
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("Cannot read settings: {e}")),
        };
        let mut values: toml::Table =
            toml::from_str(&text).map_err(|e| format!("Invalid settings: {e}"))?;
        // Accept old settings files while dropping the retired feature on save.
        values.remove("pointer_tracking");
        values.remove("skyline");
        if values.get("weather").and_then(toml::Value::as_str) == Some("fireplace") {
            values.insert("weather".into(), toml::Value::String("clear".into()));
        }
        if let Some(toml::Value::Array(volumes)) = values.get_mut("sound_volumes") {
            if volumes.len() == SOUND_COUNT - 1 {
                volumes.push(toml::Value::Integer(0));
            }
        }
        let config: Self = values
            .try_into()
            .map_err(|e| format!("Invalid settings: {e}"))?;
        config.validate()?;
        Ok(config)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create settings directory: {e}"))?;
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let temporary = path.with_extension(format!("toml.{}.tmp", std::process::id()));
        let write = || -> io::Result<()> {
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            Ok(())
        };
        write().map_err(|e| {
            let _ = fs::remove_file(&temporary);
            format!("Cannot save settings: {e}")
        })
    }
}
pub fn config_path() -> Result<PathBuf, String> {
    if let Some(root) = env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(root.join("lofiji/config.toml"));
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/lofiji/config.toml"))
        .ok_or_else(|| "Set HOME or XDG_CONFIG_HOME, or use --config PATH".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn style_navigation_wraps_in_both_directions() {
        for style in ClockStyle::ALL {
            assert_eq!(style.next().previous(), style);
            assert_eq!(style.previous().next(), style);
            let config = Config {
                clock_style: style,
                ..Config::default()
            };
            let serialized = toml::to_string(&config).unwrap();
            assert_eq!(
                toml::from_str::<Config>(&serialized).unwrap().clock_style,
                style
            );
        }
    }
    #[test]
    fn fireplace_is_appended_without_reordering_old_sound_levels() {
        let path = env::temp_dir().join(format!(
            "lofiji-sound-migration-{}.toml",
            std::process::id()
        ));
        fs::write(&path, "sound_volumes = [5,10,15,20,25,30,35,40,45,50]\n").unwrap();
        let mut config = Config::load(&path).unwrap();
        assert_eq!(
            config.sound_volumes,
            [5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 0]
        );
        config.sound_volumes[10] = 35;
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().sound_volumes[10], 35);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn scene_palettes_and_invalid_colors_are_checked() {
        let mut c = Config::default();
        assert_eq!(c.palette().name, "Moonlight");
        c.background = Background::Bonfire;
        assert_eq!(c.palette().name, "Ember");
        c.clock_palette = 1;
        assert_eq!(c.palette().name, "Tokyo");
        c.clock_palettes[0].hours = "#nope!!".into();
        assert!(c.validate().is_err());
        assert!(hex_color("#ffffff").is_ok());
        assert!(hex_color("#éaaaa").is_err());
        let old: Config = toml::from_str(r#"background = "tokyo" "#).unwrap();
        assert_eq!(old.background, Background::City);
    }
    #[test]
    fn partial_settings_use_defaults_and_invalid_values_are_rejected() {
        let c: Config = toml::from_str("show_seconds = false").unwrap();
        assert!(!c.show_seconds);
        assert_eq!(c.focus_minutes, 25);
        assert!(toml::from_str::<Config>("foucs_minutes = 20").is_err());
        assert!(Config {
            focus_minutes: 0,
            ..Config::default()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn settings_roundtrip_atomically() {
        let path = env::temp_dir().join(format!(
            "lofiji-test-{}-settings/config.toml",
            std::process::id()
        ));
        let c = Config {
            notifications: Notifications::Desktop,
            focus_minutes: 41,
            ..Config::default()
        };
        c.save(&path).unwrap();
        assert_eq!(c, Config::load(&path).unwrap());
        c.save(&path).unwrap();
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn retired_fireplace_migrates_to_clear_and_is_not_a_cli_option() {
        let path = env::temp_dir().join(format!(
            "lofiji-retired-weather-{}.toml",
            std::process::id()
        ));
        fs::write(&path, "weather = \"fireplace\"\n").unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.weather, Weather::Clear);
        config.save(&path).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("fireplace"));
        assert!(Weather::parse("fireplace").is_err());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn retired_decoration_settings_load_and_are_removed_on_save() {
        let path = env::temp_dir().join(format!("lofiji-old-settings-{}.toml", std::process::id()));
        fs::write(
            &path,
            "pointer_tracking = true\nskyline = false\nweather = \"snow\"\n",
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.weather, Weather::Snow);
        config.save(&path).unwrap();
        assert!(!fs::read_to_string(&path)
            .unwrap()
            .contains("pointer_tracking"));
        assert!(!fs::read_to_string(&path).unwrap().contains("skyline"));
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn music_source_is_a_single_youtube_https_url() {
        for url in [
            DEFAULT_MUSIC_URL,
            "https://youtu.be/123",
            "https://youtube.com/live/123",
            "",
        ] {
            assert!(validate_music_url(url).is_ok());
        }
        for url in [
            "--script=/tmp/x",
            "http://youtube.com/watch?v=1",
            "https://youtube.com.evil.test/x",
            "https://youtube.com/a\narg",
        ] {
            assert!(validate_music_url(url).is_err());
        }
        assert!(Config {
            image_brightness: 101,
            ..Config::default()
        }
        .validate()
        .is_err());
    }
}
