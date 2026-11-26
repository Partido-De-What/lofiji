use crate::{
    background::Image,
    config::{hex_color, validate_music_url, Background, Config, SOUND_COUNT},
    timer::{Phase, Pomodoro, Status},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Appearance,
    Pomodoro,
    Audio,
    Ai,
}
impl Section {
    pub const ALL: [Self; 4] = [Self::Appearance, Self::Pomodoro, Self::Audio, Self::Ai];
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Pomodoro => "Pomodoro",
            Self::Audio => "Audio",
            Self::Ai => "AI",
        }
    }
}
#[derive(Clone, Copy)]
pub enum SettingKind {
    Choice,
    Number,
    Text(&'static str),
    Color,
    Picker,
    Account(&'static str),
}

macro_rules! settings {
    ($($id:ident => ($section:ident, $label:literal, $kind:expr)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum SettingId { $($id,)+ Menu(Section) }
        impl SettingId {
            pub fn section(self) -> Section {
                match self { $(Self::$id => Section::$section,)+ Self::Menu(section) => section }
            }
            pub fn descriptor(self) -> (&'static str, SettingKind) {
                match self { $(Self::$id => ($label, $kind),)+ Self::Menu(section) => (section.label(), SettingKind::Picker) }
            }
        }
    };
}

settings! {
    TimeFormat => (Appearance, "Time format", SettingKind::Choice),
    Seconds => (Appearance, "Seconds", SettingKind::Choice),
    Blink => (Appearance, "Blink separators", SettingKind::Choice),
    Motion => (Appearance, "Scene motion", SettingKind::Choice),
    Background => (Appearance, "Background", SettingKind::Choice),
    ImagePath => (Appearance, "Image path", SettingKind::Text("PNG / GIF file path")),
    Brightness => (Appearance, "Image brightness", SettingKind::Number),
    Palette => (Appearance, "Clock palette", SettingKind::Choice),
    HoursColor => (Appearance, "Hours color", SettingKind::Color),
    SecondsColor => (Appearance, "Seconds color", SettingKind::Color),
    SeparatorColor => (Appearance, "Separator color", SettingKind::Color),
    DateColor => (Appearance, "Date color", SettingKind::Color),
    ClockStyle => (Appearance, "Clock style", SettingKind::Choice),
    Workspace => (Appearance, "Text workspace", SettingKind::Choice),
    DashboardOpacity => (Appearance, "Dashboard opacity", SettingKind::Number),
    Weather => (Appearance, "Weather", SettingKind::Choice),
    Focus => (Pomodoro, "Focus", SettingKind::Number),
    ShortBreak => (Pomodoro, "Short break", SettingKind::Number),
    LongBreak => (Pomodoro, "Long break", SettingKind::Number),
    AutoAdvance => (Pomodoro, "Auto transitions", SettingKind::Choice),
    Notifications => (Pomodoro, "Notifications", SettingKind::Choice),
    MusicUrl => (Audio, "YouTube link", SettingKind::Text("YouTube link")),
    MusicVolume => (Audio, "Music volume", SettingKind::Number),
    Provider => (Ai, "Provider", SettingKind::Picker),
    Model => (Ai, "Model", SettingKind::Text("Model ID from your provider / Ollama")),
    ApiUrl => (Ai, "API base", SettingKind::Text("API base URL · blank uses the provider default")),
    ApiKey => (Ai, "API key variable", SettingKind::Text("API key variable name · never paste the key")),
    AutoBrief => (Ai, "AI plan on open", SettingKind::Choice),
    Timeout => (Ai, "AI timeout", SettingKind::Number),
    Connect => (Ai, "Continue with ChatGPT", SettingKind::Account("connect")),
    AddAccount => (Ai, "Add another ChatGPT account", SettingKind::Account("add")),
    SwitchAccount => (Ai, "Switch ChatGPT account", SettingKind::Account("switch")),
    Disconnect => (Ai, "Sign out of ChatGPT", SettingKind::Account("disconnect")),
    Effort => (Ai, "Thinking effort", SettingKind::Choice),
}

fn setting_rows(section: Section, settings: &crate::ai::Settings) -> &'static [SettingId] {
    use SettingId::*;
    match section {
        Section::Appearance => &[
            TimeFormat,
            Seconds,
            Blink,
            Motion,
            Background,
            ImagePath,
            Brightness,
            Palette,
            HoursColor,
            SecondsColor,
            SeparatorColor,
            DateColor,
            ClockStyle,
            Workspace,
            DashboardOpacity,
            Weather,
        ],
        Section::Pomodoro => &[Focus, ShortBreak, LongBreak, AutoAdvance, Notifications],
        Section::Audio => &[MusicUrl, MusicVolume],
        Section::Ai => match settings.provider {
            crate::ai::Provider::Off => &[Provider, Connect],
            crate::ai::Provider::Chatgpt => &[
                Provider,
                Connect,
                Model,
                Effort,
                AutoBrief,
                Timeout,
                AddAccount,
                SwitchAccount,
                Disconnect,
            ],
            _ if settings.supports_effort() => {
                &[Provider, Model, Effort, ApiUrl, ApiKey, AutoBrief, Timeout]
            }
            _ => &[Provider, Model, ApiUrl, ApiKey, AutoBrief, Timeout],
        },
    }
}

#[derive(serde::Deserialize)]
pub struct ChatgptModel {
    pub slug: String,
    pub name: String,
}

#[derive(Clone, Copy)]
pub enum AiPicker {
    Provider(usize),
    Model(usize),
}
pub use crate::input::TextInput;
pub struct Settings {
    pub draft: Config,
    pub selected: SettingId,
    pub error: Option<String>,
    pub input: Option<TextInput>,
    pub confirm_discard: bool,
}
impl Settings {
    pub fn in_menu(&self) -> bool {
        matches!(self.selected, SettingId::Menu(_))
    }
    pub fn rows(&self) -> &'static [SettingId] {
        setting_rows(self.selected.section(), &self.draft.ai)
    }
    pub fn new(draft: Config) -> Self {
        Self {
            draft,
            selected: SettingId::Menu(Section::Appearance),
            error: None,
            input: None,
            confirm_discard: false,
        }
    }
}
pub struct SoundSettings {
    pub selected: usize,
    pub volumes: [u8; SOUND_COUNT],
    pub audition: bool,
    pub error: Option<String>,
}
impl SoundSettings {
    pub fn new(config: &Config, audition: bool) -> Self {
        Self {
            selected: 0,
            volumes: config.sound_volumes,
            audition,
            error: None,
        }
    }
}
pub enum Overlay {
    None,
    Help,
    Settings(Settings),
    Sounds(SoundSettings),
    Planner(crate::planning::Panel),
    Conversation(crate::planning::Conversation),
}
struct AppearancePreview {
    scene: Background,
    path: String,
    image: Option<Image>,
}

pub struct App {
    pub config: Config,
    pub config_path: PathBuf,
    pub timer: Pomodoro,
    pub pomodoro_visible: bool,
    pub focused_task_id: Option<u64>,
    pub pending_task_completion: Option<u64>,
    pub overlay: Overlay,
    pub started: Instant,
    pub notice: Option<(String, Instant)>,
    pub quit: bool,
    pub image: Option<Image>,
    appearance_preview: RefCell<Option<AppearancePreview>>,
    pub music_on: bool,
    pub music_ready: bool,
    pub ambient_on: bool,
    pub planner: crate::planner::Store,
    pub ai_session: u64,
    parked_chat: Option<crate::planning::Conversation>,
    pub help_scroll: usize,
    pub workspace_scroll: Cell<usize>,
    pub viewport_height: u16,
    pub ai_daily_requested: bool,
    pub ai_daily: Option<crate::ai::DailyBrief>,
    pub ai_daily_data: Option<crate::planner::Data>,
    pub ai_daily_settings: Option<crate::ai::Settings>,
    pub ai_daily_date: Option<chrono::NaiveDate>,
    chatgpt_client: crate::ai::Client,
    chatgpt_action: Option<&'static str>,
    chatgpt_started: bool,
    pub chatgpt_status: String,
    pub chatgpt_connected: bool,
    pub chatgpt_revision: u64,
    pub chatgpt_models: Vec<ChatgptModel>,
    pub ai_picker: Option<AiPicker>,
    open_model_picker: bool,
}
impl App {
    pub fn new(config: Config, config_path: PathBuf) -> Self {
        let loaded = Image::scene(config.background, &config.image_path, &config_path).map(Some);
        let now = Instant::now();
        let (image, notice) = match loaded {
            Ok(image) => (image, None),
            Err(e) => (None, Some((e, now + Duration::from_secs(8)))),
        };
        Self {
            ai_session: 0,
            parked_chat: None,
            help_scroll: 0,
            workspace_scroll: Cell::new(0),
            viewport_height: 25,
            chatgpt_client: crate::ai::Client::new(),
            chatgpt_action: None,
            chatgpt_started: false,
            chatgpt_status: "Enter to open browser".into(),
            chatgpt_connected: false,
            chatgpt_revision: 0,
            chatgpt_models: Vec::new(),
            ai_picker: None,
            open_model_picker: false,
            ai_daily_requested: false,
            ai_daily: None,
            ai_daily_data: None,
            ai_daily_settings: None,
            ai_daily_date: None,
            planner: crate::planner::Store::open(&config_path),
            timer: Pomodoro::new(&config),
            config,
            config_path,
            pomodoro_visible: false,
            focused_task_id: None,
            pending_task_completion: None,
            overlay: Overlay::None,
            started: now,
            notice,
            quit: false,
            image,
            appearance_preview: RefCell::default(),
            music_on: false,
            music_ready: false,
            ambient_on: false,
        }
    }
    pub fn message(&mut self, text: impl Into<String>, now: Instant) {
        self.notice = Some((text.into(), now + Duration::from_secs(6)));
    }
    pub fn appearance_draft(&self) -> Option<&Config> {
        match &self.overlay {
            Overlay::Settings(s) if !s.in_menu() && s.selected.section() == Section::Appearance => {
                Some(&s.draft)
            }
            _ => None,
        }
    }
    pub fn display_image(&self, config: &Config) -> Option<Image> {
        if config.background == self.config.background
            && config.image_path == self.config.image_path
        {
            return self.image.clone();
        }
        let mut preview = self.appearance_preview.borrow_mut();
        let entry = preview.get_or_insert_with(|| AppearancePreview {
            scene: config.background,
            path: config.image_path.clone(),
            image: Image::scene(config.background, &config.image_path, &self.config_path).ok(),
        });
        if entry.scene != config.background || entry.path != config.image_path {
            *entry = AppearancePreview {
                scene: config.background,
                path: config.image_path.clone(),
                image: Image::scene(config.background, &config.image_path, &self.config_path).ok(),
            };
        }
        entry.image.clone()
    }
    pub fn sound_mix(&self) -> (bool, &[u8; SOUND_COUNT]) {
        match &self.overlay {
            Overlay::Sounds(s) => (s.audition, &s.volumes),
            _ => (self.ambient_on, &self.config.sound_volumes),
        }
    }
    pub fn sound_error(&mut self, error: String, now: Instant) {
        if let Overlay::Sounds(s) = &mut self.overlay {
            s.audition = false;
            s.error = Some(error);
        } else {
            self.ambient_on = false;
            self.message(error, now);
        }
    }
    pub fn tick(&mut self, now: Instant) -> u64 {
        if self.notice.as_ref().is_some_and(|(_, until)| now >= *until) {
            self.notice = None;
        }
        let prior_focus = self.timer.completed_focus;
        let completions = self.timer.tick(now, &self.config);
        if self.timer.completed_focus > prior_focus {
            if let Some(id) = self
                .focused_task_id
                .take()
                .filter(|&id| self.pending_task(id))
            {
                self.pending_task_completion = Some(id);
                self.notice = None;
            }
        }
        if completions > 0 && self.pending_task_completion.is_none() {
            self.message(
                format!(
                    "Session complete · {} is {}",
                    self.timer.phase.label().to_lowercase(),
                    if self.timer.status == Status::Running {
                        "running"
                    } else {
                        "ready"
                    }
                ),
                now,
            );
        }
        completions
    }
    fn pending_task(&self, id: u64) -> bool {
        self.planner.data.entry(id).is_some_and(|task| {
            task.kind == crate::planner::Kind::Task && !task.done && !task.archived
        })
    }
    fn focus_task(&mut self, id: u64, now: Instant) {
        if self.timer.status != Status::Ready || self.timer.phase != Phase::Focus {
            self.message("Task focus is available when the focus timer is ready", now);
        } else if self.pending_task_completion.is_some() {
            self.message("Confirm the previous task first", now);
        } else if self.pending_task(id) {
            self.focused_task_id = Some(id);
            self.pomodoro_visible = true;
            self.timer.toggle(now);
            self.notice = None;
        }
    }
    pub fn chatgpt_busy(&self) -> bool {
        self.chatgpt_action.is_some_and(|action| action != "status")
    }
    pub fn tick_chatgpt(&mut self) -> bool {
        if let Some(action) = self.chatgpt_action {
            if !self.chatgpt_started {
                self.chatgpt_client
                    .account(self.config_path.clone(), action);
                self.chatgpt_started = true;
                return true;
            }
            if let Some((_, reply)) = self.chatgpt_client.poll() {
                self.chatgpt_action = None;
                self.chatgpt_started = false;
                if reply.is_ok() && matches!(action, "connect" | "add" | "switch" | "disconnect") {
                    self.chatgpt_revision += 1;
                    self.ai_daily = None;
                    self.ai_daily_data = None;
                    self.ai_daily_settings = None;
                    self.ai_daily_date = None;
                }
                if let Overlay::Settings(settings) = &mut self.overlay {
                    if action == "models" {
                        match reply.and_then(|text| {
                            serde_json::from_str::<Vec<ChatgptModel>>(&text)
                                .map_err(|_| "Could not read the ChatGPT model list".into())
                        }) {
                            Ok(models) if !models.is_empty() => {
                                self.chatgpt_models = models;
                                let model = self
                                    .chatgpt_models
                                    .iter()
                                    .position(|entry| entry.slug == settings.draft.ai.model);
                                if model.is_none() {
                                    settings.draft.ai.model.clear();
                                }
                                if self.open_model_picker {
                                    self.ai_picker =
                                        Some(AiPicker::Model(model.map_or(0, |index| index + 1)));
                                }
                            }
                            Ok(_) => {
                                settings.error =
                                    Some("No models are available on this ChatGPT account".into())
                            }
                            Err(error) => settings.error = Some(error),
                        }
                        self.open_model_picker = false;
                    } else {
                        match reply {
                            Ok(status) => {
                                self.chatgpt_connected = status.starts_with("Connected · ");
                                self.chatgpt_status = status;
                                settings.error = None;
                                if matches!(action, "connect" | "add" | "switch") {
                                    settings.draft.ai.model.clear();
                                    settings.selected = SettingId::Model;
                                    self.chatgpt_action = Some("models");
                                }
                            }
                            Err(error) => settings.error = Some(error),
                        }
                    }
                }
                return true;
            }
        }
        false
    }
    pub fn paste(&mut self, text: &str) {
        match &mut self.overlay {
            Overlay::Settings(s) => {
                if let Some(input) = &mut s.input {
                    input.insert(text);
                }
            }
            Overlay::Planner(panel) => panel.paste(text),
            Overlay::Conversation(view) => view.input.insert(text),
            _ => {}
        }
    }
    fn save_config(&mut self, draft: Config) -> Result<(), String> {
        draft.validate()?;
        let reload = draft.background != self.config.background
            || (draft.background == Background::Image
                && draft.image_path != self.config.image_path)
            || self.image.is_none();
        let image = if reload {
            Some(Image::scene(
                draft.background,
                &draft.image_path,
                &self.config_path,
            )?)
        } else {
            None
        };
        draft.save(&self.config_path)?;
        if image.is_some() {
            self.image = image;
        }
        self.config = draft;
        self.timer.apply_config(&self.config);
        Ok(())
    }
    pub fn chat_waiting(&self) -> bool {
        matches!(&self.overlay, Overlay::Conversation(view) if view.busy)
    }

    pub fn conversation_mut(&mut self) -> Option<&mut crate::planning::Conversation> {
        match &mut self.overlay {
            Overlay::Conversation(view) => Some(view),
            _ => self.parked_chat.as_mut(),
        }
    }
    fn fresh_conversation(&mut self) -> crate::planning::Conversation {
        self.ai_session += 1;
        let mut view =
            crate::planning::Conversation::new(&self.planner, chrono::Local::now().date_naive());
        view.session = self.ai_session;
        view
    }
    pub fn key(&mut self, key: KeyEvent, now: Instant) {
        if key.code == KeyCode::Char('q')
            && !matches!(&self.overlay,Overlay::Settings(s) if s.input.is_some())
            && !matches!(&self.overlay,Overlay::Planner(p) if p.editor.is_some())
            && !matches!(&self.overlay, Overlay::Conversation(_))
        {
            self.quit = true;
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        if matches!(&self.overlay, Overlay::Planner(panel) if panel.editor.is_none() &&
            (key.code == KeyCode::Char('c') ||
             (key.code == KeyCode::Char('h') && panel.kind != crate::planner::Kind::Character) ||
             (key.code == KeyCode::Char('g') && panel.kind == crate::planner::Kind::Character)))
        {
            self.overlay = Overlay::None;
        }
        match &mut self.overlay {
            Overlay::Planner(panel) => {
                if key.code == KeyCode::Char('F')
                    && panel.editor.is_none()
                    && panel.kind == crate::planner::Kind::Task
                    && !panel.completed
                {
                    let date = chrono::Local::now().date_naive();
                    if let Some(id) = panel
                        .entries(&self.planner.data, date)
                        .get(panel.selected)
                        .map(|task| task.id)
                    {
                        self.overlay = Overlay::None;
                        self.focus_task(id, now);
                    }
                    return;
                }
                if panel.key(key, &mut self.planner) {
                    self.overlay = Overlay::None;
                }
                return;
            }
            Overlay::Conversation(view) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('n') {
                    self.overlay = Overlay::Conversation(self.fresh_conversation());
                } else if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('k')
                {
                    view.cancel_reply();
                } else if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('e')
                {
                    view.cycle_effort(&self.config.ai);
                } else if view.key(key, &mut self.planner, &self.config.ai) {
                    if let Overlay::Conversation(view) =
                        std::mem::replace(&mut self.overlay, Overlay::None)
                    {
                        self.parked_chat = Some(view);
                    }
                }
                return;
            }
            Overlay::Help => {
                let page = crate::ui::help_page_size(self.viewport_height);
                let last = crate::ui::HELP_LINES.len().saturating_sub(page);
                let current = self.help_scroll.min(last);
                match key.code {
                    KeyCode::Esc | KeyCode::Char('?') => self.overlay = Overlay::None,
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.help_scroll = (current + 1).min(last)
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.help_scroll = current.saturating_sub(1)
                    }
                    KeyCode::PageDown => self.help_scroll = (current + page).min(last),
                    KeyCode::PageUp => self.help_scroll = current.saturating_sub(page),
                    KeyCode::End => self.help_scroll = last,
                    KeyCode::Home => self.help_scroll = 0,
                    _ => {}
                }
                return;
            }
            Overlay::Sounds(s) => {
                let layer = crate::ambient::ORDER[s.selected];
                match key.code {
                    KeyCode::Esc | KeyCode::Char('a') => self.overlay = Overlay::None,
                    KeyCode::Up | KeyCode::Char('k') => {
                        s.selected = (s.selected + SOUND_COUNT - 1) % SOUND_COUNT
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        s.selected = (s.selected + 1) % SOUND_COUNT
                    }
                    KeyCode::Left | KeyCode::Char('-') => {
                        s.volumes[layer] = s.volumes[layer].saturating_sub(5)
                    }
                    KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                        s.volumes[layer] = (s.volumes[layer] + 5).min(100)
                    }
                    KeyCode::Char(' ') => {
                        let v = &mut s.volumes[layer];
                        *v = if *v == 0 { 25 } else { 0 };
                    }
                    KeyCode::Char('x') => s.volumes.fill(0),
                    KeyCode::Char('v') => s.audition = !s.audition,
                    KeyCode::Char('1' | '2' | '3') => {
                        s.volumes.fill(0);
                        let layers = match key.code {
                            KeyCode::Char('1') => [(10, 28), (1, 16)],
                            KeyCode::Char('2') => [(2, 28), (1, 18)],
                            _ => [(6, 24), (4, 15)],
                        };
                        for (layer, volume) in layers {
                            s.volumes[layer] = volume;
                        }
                    }
                    KeyCode::Enter => {
                        let mut draft = self.config.clone();
                        draft.sound_volumes = s.volumes;
                        match self.save_config(draft) {
                            Ok(()) => {
                                self.ambient_on = self.config.sound_volumes.iter().any(|v| *v > 0);
                                self.overlay = Overlay::None;
                                self.message(
                                    if self.ambient_on {
                                        "Sound mix playing"
                                    } else {
                                        "Sounds off"
                                    },
                                    now,
                                );
                            }
                            Err(e) => {
                                if let Overlay::Sounds(s) = &mut self.overlay {
                                    s.error = Some(e);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }
            Overlay::Settings(s) => {
                if let Some(picker) = self.ai_picker {
                    let (index, len) = match picker {
                        AiPicker::Provider(i) => (i, crate::ai::Provider::ALL.len()),
                        AiPicker::Model(i) => (i, self.chatgpt_models.len() + 1),
                    };
                    match key.code {
                        KeyCode::Esc => self.ai_picker = None,
                        KeyCode::Enter => {
                            match picker {
                                AiPicker::Provider(i) => {
                                    let provider = crate::ai::Provider::ALL[i];
                                    if s.draft.ai.provider != provider {
                                        s.draft.ai.select(provider);
                                    }
                                    if provider == crate::ai::Provider::Chatgpt {
                                        s.selected = if self.chatgpt_connected {
                                            SettingId::Model
                                        } else {
                                            SettingId::Connect
                                        };
                                    }
                                    s.error = None;
                                }
                                AiPicker::Model(i) => {
                                    s.draft.ai.model =
                                        i.checked_sub(1).map_or_else(String::new, |i| {
                                            self.chatgpt_models[i].slug.clone()
                                        });
                                }
                            }
                            self.ai_picker = None;
                        }
                        _ => {
                            let next = match key.code {
                                KeyCode::Up | KeyCode::Char('k') => (index + len - 1) % len,
                                KeyCode::Down | KeyCode::Char('j') => (index + 1) % len,
                                KeyCode::Home => 0,
                                KeyCode::End => len - 1,
                                _ => index,
                            };
                            self.ai_picker = Some(match picker {
                                AiPicker::Provider(_) => AiPicker::Provider(next),
                                AiPicker::Model(_) => AiPicker::Model(next),
                            });
                        }
                    }
                    return;
                }
                if s.input.is_some() {
                    match key.code {
                        KeyCode::Esc => {
                            s.input = None;
                            s.error = None;
                        }
                        KeyCode::Enter => {
                            let value = s.input.as_ref().unwrap().value.trim().to_string();
                            if s.selected == SettingId::MusicUrl {
                                if let Err(e) = validate_music_url(&value) {
                                    s.error = Some(e);
                                    return;
                                }
                                s.draft.music_url = value;
                            } else if s.selected == SettingId::ImagePath {
                                s.draft.image_path = value;
                            } else if s.selected == SettingId::Model {
                                s.draft.ai.model = value;
                            } else if s.selected == SettingId::ApiUrl {
                                s.draft.ai.base_url = value;
                            } else if s.selected == SettingId::ApiKey {
                                s.draft.ai.key_env = value;
                            } else {
                                if let Err(e) = hex_color(&value) {
                                    s.error = Some(e);
                                    return;
                                }
                                let index = s.draft.palette_index();
                                let p = &mut s.draft.clock_palettes[index];
                                match s.selected {
                                    SettingId::HoursColor => p.hours = value,
                                    SettingId::SecondsColor => p.seconds = value,
                                    SettingId::SeparatorColor => p.separator = value,
                                    _ => p.date = value,
                                }
                            }
                            s.input = None;
                            s.error = None;
                        }
                        _ => s.input.as_mut().unwrap().key(key),
                    }
                    return;
                }
                if self.chatgpt_action.is_some_and(|action| action != "status") {
                    if key.code == KeyCode::Esc && self.chatgpt_action != Some("disconnect") {
                        self.chatgpt_client.cancel();
                        self.chatgpt_action = None;
                        self.chatgpt_started = false;
                        self.open_model_picker = false;
                        s.error = Some("ChatGPT action cancelled".into());
                    }
                    return;
                }
                let kind = s.selected.descriptor().1;
                if key.code != KeyCode::Esc && s.confirm_discard {
                    s.confirm_discard = false;
                    s.error = None;
                }
                match key.code {
                    KeyCode::Esc if s.in_menu() => {
                        if s.draft == self.config || s.confirm_discard {
                            self.overlay = Overlay::None;
                        } else {
                            s.confirm_discard = true;
                            s.error = Some("Unsaved changes · Esc discard · Ctrl+S save".into());
                        }
                    }
                    KeyCode::Esc => {
                        s.selected = SettingId::Menu(s.selected.section());
                        s.error = None;
                    }
                    KeyCode::Enter if s.in_menu() => {
                        s.selected = s.rows()[0];
                        s.error = None;
                    }
                    KeyCode::Enter if s.selected == SettingId::Provider => {
                        let index = crate::ai::Provider::ALL
                            .iter()
                            .position(|provider| *provider == s.draft.ai.provider)
                            .unwrap();
                        self.ai_picker = Some(AiPicker::Provider(index));
                    }
                    KeyCode::Enter
                        if s.selected == SettingId::Model
                            && s.draft.ai.provider == crate::ai::Provider::Chatgpt =>
                    {
                        if self.chatgpt_models.is_empty() {
                            self.chatgpt_client.cancel();
                            self.chatgpt_started = false;
                            self.chatgpt_action = Some("models");
                            self.open_model_picker = true;
                            s.error = None;
                        } else {
                            let index = self
                                .chatgpt_models
                                .iter()
                                .position(|model| model.slug == s.draft.ai.model)
                                .map_or(0, |index| index + 1);
                            self.ai_picker = Some(AiPicker::Model(index));
                        }
                    }
                    KeyCode::Enter if matches!(kind, SettingKind::Account(_)) => {
                        if s.draft.ai.provider != crate::ai::Provider::Chatgpt {
                            s.draft.ai.select(crate::ai::Provider::Chatgpt);
                        }
                        self.chatgpt_models.clear();
                        self.chatgpt_client.cancel();
                        self.chatgpt_started = false;
                        if let SettingKind::Account(action) = kind {
                            self.chatgpt_action = Some(action);
                        }
                        self.ai_daily = None;
                        self.ai_daily_data = None;
                        self.ai_daily_settings = None;
                        self.ai_daily_date = None;
                        s.error = None;
                    }
                    KeyCode::Home | KeyCode::End => {
                        let end = key.code == KeyCode::End;
                        s.selected = if s.in_menu() {
                            SettingId::Menu(if end {
                                Section::Ai
                            } else {
                                Section::Appearance
                            })
                        } else {
                            let rows = s.rows();
                            rows[if end { rows.len() - 1 } else { 0 }]
                        };
                    }
                    KeyCode::Up | KeyCode::Char('k') => move_settings(s, false),
                    KeyCode::Down | KeyCode::Char('j') => move_settings(s, true),
                    KeyCode::Left | KeyCode::Char('-') if !s.in_menu() => {
                        adjust(&mut s.draft, s.selected, false)
                    }
                    KeyCode::Right
                    | KeyCode::Char('+')
                    | KeyCode::Char('=')
                    | KeyCode::Char(' ')
                        if !s.in_menu() =>
                    {
                        if editable_setting(s) {
                            s.input = Some(TextInput::new(setting_text(&s.draft, s.selected)));
                        } else {
                            adjust(&mut s.draft, s.selected, true);
                        }
                    }
                    KeyCode::Enter if is_text_row(s.selected) && !editable_setting(s) => {}
                    KeyCode::Enter if editable_setting(s) => {
                        s.input = Some(TextInput::new(setting_text(&s.draft, s.selected)));
                    }
                    KeyCode::Enter if matches!(kind, SettingKind::Choice) => {
                        adjust(&mut s.draft, s.selected, true);
                    }
                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let draft = s.draft.clone();
                        match self.save_config(draft) {
                            Ok(()) => {
                                self.overlay = Overlay::None;
                                self.message("Settings saved", now);
                            }
                            Err(e) => {
                                if let Overlay::Settings(s) = &mut self.overlay {
                                    s.error = Some(e);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }
            Overlay::None => {}
        }
        if let Some(id) = self.pending_task_completion {
            match key.code {
                KeyCode::Char('y') => {
                    self.planner.refresh();
                    if self.pending_task(id) {
                        if let Err(error) = self.planner.modify(id, |task| task.done = true) {
                            self.message(error, now);
                            return;
                        }
                    }
                    self.pending_task_completion = None;
                    self.message("Task completion confirmed", now);
                    return;
                }
                KeyCode::Esc => {
                    self.pending_task_completion = None;
                    return;
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Down if self.config.show_plan => {
                self.workspace_scroll.set(self.workspace_scroll.get() + 1);
            }
            KeyCode::Up if self.config.show_plan => {
                self.workspace_scroll
                    .set(self.workspace_scroll.get().saturating_sub(1));
            }
            KeyCode::Home if self.config.show_plan => self.workspace_scroll.set(0),
            KeyCode::Char('h') => {
                self.planner.refresh();
                self.overlay = Overlay::Planner(crate::planning::Panel::character());
            }
            KeyCode::Char('g') => {
                self.planner.refresh();
                self.overlay = Overlay::Planner(crate::planning::Panel::new());
            }
            KeyCode::Char('+') => {
                self.planner.refresh();
                self.overlay = Overlay::Planner(crate::planning::Panel::quick_add());
            }
            KeyCode::Char('c') => {
                self.planner.refresh();
                let view = self
                    .parked_chat
                    .take()
                    .unwrap_or_else(|| self.fresh_conversation());
                self.overlay = Overlay::Conversation(view);
            }
            KeyCode::Char('d') => self.ai_daily_requested = true,
            KeyCode::Char('v') => {
                let mut draft = self.config.clone();
                draft.show_plan = !draft.show_plan;
                match self.save_config(draft) {
                    Ok(()) => {}
                    Err(e) => self.message(e, now),
                }
            }
            KeyCode::Char('p') => self.pomodoro_visible = !self.pomodoro_visible,
            KeyCode::Char(' ') if self.pomodoro_visible => self.timer.toggle(now),
            KeyCode::Char('r') if self.pomodoro_visible => {
                self.focused_task_id = None;
                self.timer.reset(&self.config);
                self.message("Session reset · ready to start", now);
            }
            KeyCode::Char('n') if self.pomodoro_visible => {
                self.focused_task_id = None;
                self.timer.skip(&self.config, now);
                self.message("Session skipped", now);
            }
            KeyCode::Char('s') => {
                self.overlay = Overlay::Settings(Settings::new(self.config.clone()));
                self.chatgpt_models.clear();
                if self.config_path.with_extension("chatgpt.json").exists()
                    && self.chatgpt_action.is_none()
                {
                    self.chatgpt_action = Some("status");
                }
            }
            KeyCode::Char('a') => {
                self.overlay = Overlay::Sounds(SoundSettings::new(&self.config, self.ambient_on))
            }
            KeyCode::Char('z') => {
                if !self.ambient_on && self.config.sound_volumes.iter().all(|v| *v == 0) {
                    self.overlay =
                        Overlay::Sounds(SoundSettings::new(&self.config, self.ambient_on));
                    return;
                }
                self.ambient_on = !self.ambient_on;
                self.message(
                    if self.ambient_on {
                        "Sound mix resumed"
                    } else {
                        "Sounds muted"
                    },
                    now,
                );
            }
            KeyCode::Char('m') => {
                self.music_on = !self.music_on;
                self.music_ready = false;
                self.message(
                    if self.music_on {
                        "Starting music…"
                    } else {
                        "Music off"
                    },
                    now,
                );
            }
            KeyCode::Char('t') => {
                let mut draft = self.config.clone();
                draft.clock_style = draft.clock_style.next();
                match self.save_config(draft) {
                    Ok(()) => {
                        self.message(format!("Clock: {}", self.config.clock_style.label()), now)
                    }
                    Err(e) => self.message(e, now),
                }
            }
            KeyCode::Char('w') | KeyCode::Char('b') => {
                let mut draft = self.config.clone();
                let label = match key.code {
                    KeyCode::Char('w') => {
                        draft.weather = draft.weather.next();
                        format!("Weather: {}", draft.weather.label())
                    }
                    KeyCode::Char('b') => {
                        draft.background = draft.background.next();
                        format!("Background: {}", draft.background.label())
                    }
                    _ => unreachable!(),
                };
                let image_cycle =
                    key.code == KeyCode::Char('b') && draft.background == Background::Image;
                match self.save_config(draft) {
                    Ok(()) => self.message(label, now),
                    Err(_) if image_cycle => {
                        let mut fallback = self.config.clone();
                        fallback.background = Background::Window;
                        match self.save_config(fallback) {
                            Ok(()) => self.message("Image unavailable · Rainy window", now),
                            Err(e) => self.message(e, now),
                        }
                    }
                    Err(e) => self.message(e, now),
                }
            }
            KeyCode::Char('?') => {
                self.help_scroll = 0;
                self.overlay = Overlay::Help;
            }
            _ => {}
        }
    }
}
pub fn move_settings(settings: &mut Settings, down: bool) {
    if settings.in_menu() {
        let index = settings.selected.section() as usize;
        settings.selected = SettingId::Menu(
            Section::ALL[if down {
                (index + 1).min(Section::ALL.len() - 1)
            } else {
                index.saturating_sub(1)
            }],
        );
        return;
    }
    let rows = settings.rows();
    let index = rows
        .iter()
        .position(|row| *row == settings.selected)
        .unwrap_or(0);
    settings.selected = rows[if down {
        (index + 1).min(rows.len() - 1)
    } else {
        index.saturating_sub(1)
    }];
}

fn editable_setting(settings: &Settings) -> bool {
    is_text_row(settings.selected)
        && !(settings.draft.ai.provider == crate::ai::Provider::Chatgpt
            && matches!(
                settings.selected,
                SettingId::Model | SettingId::ApiUrl | SettingId::ApiKey
            ))
}
fn is_text_row(row: SettingId) -> bool {
    matches!(
        row.descriptor().1,
        SettingKind::Text(_) | SettingKind::Color
    )
}
fn setting_text(c: &Config, row: SettingId) -> String {
    match row {
        SettingId::Model => c.ai.model.clone(),
        SettingId::ApiUrl => c.ai.base_url.clone(),
        SettingId::ApiKey => c.ai.key_env.clone(),
        SettingId::ImagePath => c.image_path.clone(),
        SettingId::MusicUrl => c.music_url.clone(),
        SettingId::HoursColor => c.palette().hours.clone(),
        SettingId::SecondsColor => c.palette().seconds.clone(),
        SettingId::SeparatorColor => c.palette().separator.clone(),
        _ => c.palette().date.clone(),
    }
}
fn adjust(c: &mut Config, row: SettingId, increase: bool) {
    let duration = match row {
        SettingId::Focus => Some(&mut c.focus_minutes),
        SettingId::ShortBreak => Some(&mut c.short_break_minutes),
        SettingId::LongBreak => Some(&mut c.long_break_minutes),
        _ => None,
    };
    if let Some(value) = duration {
        *value = if increase {
            (*value + 1).min(180)
        } else {
            value.saturating_sub(1).max(1)
        };
        return;
    }
    match row {
        SettingId::TimeFormat => c.format_24h = !c.format_24h,
        SettingId::Seconds => c.show_seconds = !c.show_seconds,
        SettingId::Blink => c.blink_separator = !c.blink_separator,
        SettingId::Motion => c.animations = !c.animations,
        SettingId::AutoAdvance => c.auto_advance = !c.auto_advance,
        SettingId::Notifications => {
            c.notifications = if increase {
                c.notifications.next()
            } else {
                c.notifications.next().next()
            }
        }
        SettingId::Weather => {
            c.weather = if increase {
                c.weather.next()
            } else {
                c.weather.next().next()
            }
        }
        SettingId::Background => {
            c.background = if increase {
                c.background.next()
            } else {
                c.background.previous()
            }
        }
        SettingId::Brightness => {
            c.image_brightness = if increase {
                (c.image_brightness + 5).min(100)
            } else {
                c.image_brightness.saturating_sub(5)
            }
        }
        SettingId::Workspace => c.show_plan = !c.show_plan,
        SettingId::DashboardOpacity => {
            c.dashboard_opacity = if increase {
                (c.dashboard_opacity + 5).min(100)
            } else {
                c.dashboard_opacity.saturating_sub(5)
            }
        }
        SettingId::Provider => {
            let provider = if increase {
                c.ai.provider.next()
            } else {
                c.ai.provider.previous()
            };
            c.ai.select(provider);
        }
        SettingId::Effort => c.ai.thinking_effort = c.ai.thinking_effort.cycle(increase),
        SettingId::AutoBrief => c.ai.auto_brief = !c.ai.auto_brief,
        SettingId::Timeout => {
            c.ai.timeout_seconds = if increase {
                (c.ai.timeout_seconds + 5).min(180)
            } else {
                c.ai.timeout_seconds.saturating_sub(5).max(5)
            }
        }
        SettingId::ClockStyle => {
            c.clock_style = if increase {
                c.clock_style.next()
            } else {
                c.clock_style.previous()
            };
        }
        SettingId::Palette => {
            c.clock_palette = if increase {
                (c.clock_palette + 1) % (c.clock_palettes.len() + 1)
            } else {
                (c.clock_palette + c.clock_palettes.len()) % (c.clock_palettes.len() + 1)
            }
        }
        SettingId::MusicVolume => {
            c.music_volume = if increase {
                (c.music_volume + 5).min(100)
            } else {
                c.music_volume.saturating_sub(5)
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn press(app: &mut App, c: char, now: Instant) {
        app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), now);
    }
    #[test]
    fn fireplace_has_its_own_level_next_to_campfire_in_the_menu() {
        let path =
            std::env::temp_dir().join(format!("lofiji-fireplace-{}.toml", std::process::id()));
        let mut app = App::new(Config::default(), path.clone());
        let now = app.started;
        press(&mut app, 'a', now);
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), now);
        press(&mut app, ' ', now);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        assert_eq!(app.config.sound_volumes[10], 25);
        assert_eq!(app.config.sound_volumes[0], 0);
        assert!(app.ambient_on);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn edited_palette_and_sound_mix_persist_but_cancel_does_not_apply() {
        let path =
            std::env::temp_dir().join(format!("lofiji-palette-mix-{}.toml", std::process::id()));
        let mut app = App::new(Config::default(), path.clone());
        let now = app.started;
        let mut settings = Settings::new(app.config.clone());
        settings.selected = SettingId::HoursColor;
        app.overlay = Overlay::Settings(settings);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        app.key(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            now,
        );
        app.paste("invalid");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        assert!(
            matches!(&app.overlay, Overlay::Settings(s) if s.error.is_some() && s.input.is_some())
        );
        app.key(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            now,
        );
        app.paste("#e0c8aa");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        app.key(
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            now,
        );
        assert_eq!(Config::load(&path).unwrap().palette().hours, "#e0c8aa");
        press(&mut app, 'a', now);
        press(&mut app, ' ', now);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), now);
        assert!(!app.ambient_on);
        assert_eq!(app.config.sound_volumes, [0; SOUND_COUNT]);
        press(&mut app, 'a', now);
        press(&mut app, ' ', now);
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), now);
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), now);
        press(&mut app, ' ', now);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        assert!(app.ambient_on);
        assert_eq!(&Config::load(&path).unwrap().sound_volumes[..2], &[25, 25]);
        press(&mut app, 'z', now);
        assert!(!app.ambient_on);
        assert_eq!(app.config.sound_volumes[0], 25);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn hiding_pomodoro_does_not_pause_and_hidden_keys_do_nothing() {
        let now = Instant::now();
        let mut app = App::new(Config::default(), PathBuf::new());
        press(&mut app, ' ', now);
        assert_eq!(app.timer.status, Status::Ready);
        press(&mut app, 'p', now);
        press(&mut app, ' ', now);
        press(&mut app, 'p', now);
        assert!(!app.pomodoro_visible);
        assert_eq!(app.timer.status, Status::Running);
        press(&mut app, 'r', now);
        assert_eq!(app.timer.status, Status::Running);
    }
    #[test]
    fn task_focus_requires_confirmation_after_a_completed_session() {
        use crate::planner::{Entry, Kind};
        let mut app = App::new(Config::default(), PathBuf::new());
        let now = app.started;
        let mut task = Entry::blank(Kind::Task);
        task.title = "Write a proof".into();
        let id = app.planner.put(task).unwrap();
        press(&mut app, 'g', now);
        press(&mut app, 'F', now);
        assert_eq!(app.focused_task_id, Some(id));
        assert_eq!(app.timer.status, Status::Running);
        press(&mut app, 'r', now);
        assert_eq!(app.focused_task_id, None);
        assert_eq!(app.pending_task_completion, None);
        press(&mut app, 'g', now);
        press(&mut app, 'F', now);

        let later = now + Phase::Focus.duration(&app.config);
        assert_eq!(app.tick(later), 1);
        assert_eq!(app.pending_task_completion, Some(id));
        assert!(!app.planner.data.entry(id).unwrap().done);
        press(&mut app, 'y', later);
        assert!(app.planner.data.entry(id).unwrap().done);
        assert_eq!(app.pending_task_completion, None);
    }
    #[test]
    fn settings_end_stays_in_section_and_escape_confirms_discard() {
        let mut app = App::new(Config::default(), PathBuf::new());
        let now = app.started;
        let key = |app: &mut App, code| app.key(KeyEvent::new(code, KeyModifiers::NONE), now);
        press(&mut app, 's', now);
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::End);
        assert!(matches!(&app.overlay, Overlay::Settings(s) if s.selected == SettingId::Weather));
        key(&mut app, KeyCode::Right);
        assert_ne!(app.appearance_draft().unwrap().weather, app.config.weather);
        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Esc);
        assert!(matches!(&app.overlay, Overlay::Settings(s) if s.confirm_discard));
        key(&mut app, KeyCode::Esc);
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(app.config.weather, crate::config::Weather::Clear);
    }
    #[test]
    fn cancelled_settings_do_not_change_config() {
        let now = Instant::now();
        let mut app = App::new(Config::default(), PathBuf::new());
        press(&mut app, 's', now);
        press(&mut app, ' ', now);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), now);
        assert!(app.config.format_24h);
    }
    #[test]
    fn weather_cycles_and_persists() {
        use crate::config::Weather;
        let path = std::env::temp_dir().join(format!("lofiji-weather-{}.toml", std::process::id()));
        let mut app = App::new(Config::default(), path.clone());
        let now = app.started;
        for mode in [Weather::Rain, Weather::Snow, Weather::Clear] {
            press(&mut app, 'w', now);
            assert_eq!(app.config.weather, mode);
            assert_eq!(Config::load(&path).unwrap().weather, mode);
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn scene_changes_persist_and_url_editing_does_not_quit() {
        let path =
            std::env::temp_dir().join(format!("lofiji-scene-edit-{}.toml", std::process::id()));
        let mut app = App::new(Config::default(), path.clone());
        let now = app.started;
        press(&mut app, 'b', now);
        assert_eq!(app.config.background, Background::City);
        assert_eq!(Config::load(&path).unwrap().background, Background::City);
        let mut settings = Settings::new(app.config.clone());
        settings.selected = SettingId::MusicUrl;
        app.overlay = Overlay::Settings(settings);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        app.key(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            now,
        );
        app.paste("https://www.youtube.com/watch?v=q123");
        press(&mut app, 'q', now);
        assert!(!app.quit);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        app.key(
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            now,
        );
        assert!(matches!(app.overlay, Overlay::None));
        assert!(Config::load(&path).unwrap().music_url.ends_with("q123q"));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn text_editing_preserves_unicode_boundaries_and_cancel_discards_text() {
        let mut input = TextInput::new("café/東京.png".into());
        input.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        input.key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
        input.insert("é");
        input.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(input.value, "afé/東京.png");
        let mut app = App::new(Config::default(), PathBuf::new());
        let now = app.started;
        let mut settings = Settings::new(app.config.clone());
        settings.selected = SettingId::MusicUrl;
        let original = settings.draft.music_url.clone();
        app.overlay = Overlay::Settings(settings);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        app.paste("new text");
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), now);
        if let Overlay::Settings(s) = &app.overlay {
            assert_eq!(s.draft.music_url, original);
        } else {
            panic!("lost settings");
        }
    }
    #[test]
    fn dashboard_opacity_adjusts_in_appearance_and_persists() {
        let path = std::env::temp_dir().join(format!(
            "lofiji-dashboard-opacity-{}.toml",
            std::process::id()
        ));
        let mut app = App::new(Config::default(), path.clone());
        let now = app.started;
        press(&mut app, 's', now);
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), now);
        while !matches!(&app.overlay, Overlay::Settings(s) if s.selected == SettingId::DashboardOpacity)
        {
            let prev = match &app.overlay {
                Overlay::Settings(s) => s.selected,
                _ => panic!("expected settings"),
            };
            app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), now);
            if matches!(&app.overlay, Overlay::Settings(s) if s.selected == prev) {
                panic!("Dashboard opacity row not found in Appearance");
            }
        }
        assert_eq!(SettingId::DashboardOpacity.section(), Section::Appearance);
        app.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), now);
        app.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), now);
        if let Overlay::Settings(s) = &app.overlay {
            assert_eq!(s.draft.dashboard_opacity, 65);
        }
        for _ in 0..10 {
            app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), now);
        }
        if let Overlay::Settings(s) = &app.overlay {
            assert_eq!(s.draft.dashboard_opacity, 100);
        }
        for _ in 0..25 {
            app.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), now);
        }
        if let Overlay::Settings(s) = &app.overlay {
            assert_eq!(s.draft.dashboard_opacity, 0);
        }
        app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE), now);
        app.key(
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            now,
        );
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(app.config.dashboard_opacity, 5);
        assert_eq!(Config::load(&path).unwrap().dashboard_opacity, 5);
        std::fs::remove_file(path).unwrap();
    }
}
