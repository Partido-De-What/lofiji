use crate::{
    background::Image,
    config::{hex_color, validate_music_url, Background, Config, SOUND_COUNT},
    timer::{Pomodoro, Status},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    cell::Cell,
    path::PathBuf,
    time::{Duration, Instant},
};

pub const AI_PROVIDER_ROW: usize = 22;
pub const IMAGE_ROW: usize = 11;
pub const MUSIC_ROW: usize = 13;
pub const AI_MODEL_ROW: usize = 23;
pub const AI_URL_ROW: usize = 24;
pub const AI_KEY_ROW: usize = 25;
pub const CHATGPT_CONNECT_ROW: usize = 28;
pub const AI_EFFORT_ROW: usize = 32;
const CHATGPT_ACTIONS: [&str; 4] = ["connect", "add", "switch", "disconnect"];

pub fn ai_rows(settings: &crate::ai::Settings) -> &'static [usize] {
    match settings.provider {
        crate::ai::Provider::Off => &[22, 28],
        crate::ai::Provider::Chatgpt => &[22, 28, 23, AI_EFFORT_ROW, 26, 27, 29, 30, 31],
        _ if settings.supports_effort() => &[22, 23, AI_EFFORT_ROW, 24, 25, 26, 27],
        _ => &[22, 23, 24, 25, 26, 27],
    }
}

const SETTINGS_MENU: usize = 33;
pub const SETTINGS_SECTIONS: [(&str, &[usize]); 4] = [
    (
        "Appearance",
        &[0, 1, 2, 3, 10, 11, 12, 15, 16, 17, 18, 19, 20, 21, 9],
    ),
    ("Pomodoro", &[4, 5, 6, 7, 8]),
    ("Audio", &[13, 14]),
    ("AI", &[AI_PROVIDER_ROW]),
];

pub fn settings_section(row: usize) -> usize {
    if row >= SETTINGS_MENU {
        row - SETTINGS_MENU
    } else if row >= AI_PROVIDER_ROW {
        3
    } else {
        SETTINGS_SECTIONS
            .iter()
            .position(|(_, rows)| rows.contains(&row))
            .unwrap_or(0)
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
    pub selected: usize,
    pub error: Option<String>,
    pub input: Option<TextInput>,
}
impl Settings {
    pub fn in_menu(&self) -> bool {
        self.selected >= SETTINGS_MENU
    }
    pub fn rows(&self) -> &'static [usize] {
        let section = settings_section(self.selected);
        if section == 3 {
            ai_rows(&self.draft.ai)
        } else {
            SETTINGS_SECTIONS[section].1
        }
    }

    pub fn menu(draft: Config) -> Self {
        Self {
            selected: SETTINGS_MENU,
            ..Self::new(draft)
        }
    }
    pub fn new(draft: Config) -> Self {
        Self {
            draft,
            selected: 0,
            error: None,
            input: None,
        }
    }
}
pub enum Overlay {
    None,
    Help,
    Settings(Settings),
    Sounds(Settings),
    Planner(crate::planning::Panel),
    Conversation(crate::planning::Conversation),
}
pub struct App {
    pub config: Config,
    pub config_path: PathBuf,
    pub timer: Pomodoro,
    pub pomodoro_visible: bool,
    pub overlay: Overlay,
    pub started: Instant,
    pub notice: Option<(String, Instant)>,
    pub quit: bool,
    pub image: Option<Image>,
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
            overlay: Overlay::None,
            started: now,
            notice,
            quit: false,
            image,
            music_on: false,
            music_ready: false,
            ambient_on: false,
        }
    }
    pub fn message(&mut self, text: impl Into<String>, now: Instant) {
        self.notice = Some((text.into(), now + Duration::from_secs(6)));
    }
    pub fn tick(&mut self, now: Instant) -> u64 {
        if self.notice.as_ref().is_some_and(|(_, until)| now >= *until) {
            self.notice = None;
        }
        let completions = self.timer.tick(now, &self.config);
        if completions > 0 {
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
                                    settings.selected = AI_MODEL_ROW;
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
                        s.draft.sound_volumes[layer] =
                            s.draft.sound_volumes[layer].saturating_sub(5)
                    }
                    KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                        s.draft.sound_volumes[layer] = (s.draft.sound_volumes[layer] + 5).min(100)
                    }
                    KeyCode::Char(' ') => {
                        let v = &mut s.draft.sound_volumes[layer];
                        *v = if *v == 0 { 25 } else { 0 };
                    }
                    KeyCode::Char('x') => s.draft.sound_volumes.fill(0),
                    KeyCode::Enter => {
                        let draft = s.draft.clone();
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
                                            AI_MODEL_ROW
                                        } else {
                                            CHATGPT_CONNECT_ROW
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
                            if s.selected == MUSIC_ROW {
                                if let Err(e) = validate_music_url(&value) {
                                    s.error = Some(e);
                                    return;
                                }
                                s.draft.music_url = value;
                            } else if s.selected == IMAGE_ROW {
                                s.draft.image_path = value;
                            } else if s.selected == AI_MODEL_ROW {
                                s.draft.ai.model = value;
                            } else if s.selected == AI_URL_ROW {
                                s.draft.ai.base_url = value;
                            } else if s.selected == AI_KEY_ROW {
                                s.draft.ai.key_env = value;
                            } else {
                                if let Err(e) = hex_color(&value) {
                                    s.error = Some(e);
                                    return;
                                }
                                let index = s.draft.palette_index();
                                let p = &mut s.draft.clock_palettes[index];
                                match s.selected {
                                    16 => p.hours = value,
                                    17 => p.seconds = value,
                                    18 => p.separator = value,
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
                match key.code {
                    KeyCode::Esc if s.in_menu() => self.overlay = Overlay::None,
                    KeyCode::Esc | KeyCode::Home => {
                        s.selected = SETTINGS_MENU + settings_section(s.selected);
                        s.error = None;
                    }
                    KeyCode::Enter if s.in_menu() => {
                        s.selected = s.rows()[0];
                        s.error = None;
                    }
                    KeyCode::Enter if s.selected == AI_PROVIDER_ROW => {
                        let index = crate::ai::Provider::ALL
                            .iter()
                            .position(|provider| *provider == s.draft.ai.provider)
                            .unwrap();
                        self.ai_picker = Some(AiPicker::Provider(index));
                    }
                    KeyCode::Enter
                        if s.selected == AI_MODEL_ROW
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
                    KeyCode::Enter if (CHATGPT_CONNECT_ROW..=31).contains(&s.selected) => {
                        if s.draft.ai.provider != crate::ai::Provider::Chatgpt {
                            s.draft.ai.select(crate::ai::Provider::Chatgpt);
                        }
                        self.chatgpt_models.clear();
                        self.chatgpt_client.cancel();
                        self.chatgpt_started = false;
                        self.chatgpt_action =
                            Some(CHATGPT_ACTIONS[s.selected - CHATGPT_CONNECT_ROW]);
                        self.ai_daily = None;
                        self.ai_daily_data = None;
                        self.ai_daily_settings = None;
                        self.ai_daily_date = None;
                        s.error = None;
                    }
                    KeyCode::End => s.selected = AI_PROVIDER_ROW,
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
                    KeyCode::Enter if matches!(s.selected, 0..=3 | 7..=10 | 15 | 20 | 21 | 26 | AI_EFFORT_ROW) =>
                    {
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
                self.timer.reset(&self.config);
                self.message("Session reset · ready to start", now);
            }
            KeyCode::Char('n') if self.pomodoro_visible => {
                self.timer.skip(&self.config, now);
                self.message("Session skipped", now);
            }
            KeyCode::Char('s') => {
                self.overlay = Overlay::Settings(Settings::menu(self.config.clone()));
                self.chatgpt_models.clear();
                if self.config_path.with_extension("chatgpt.json").exists()
                    && self.chatgpt_action.is_none()
                {
                    self.chatgpt_action = Some("status");
                }
            }
            KeyCode::Char('a') => {
                self.overlay = Overlay::Sounds(Settings::new(self.config.clone()))
            }
            KeyCode::Char('z') => {
                if !self.ambient_on && self.config.sound_volumes.iter().all(|v| *v == 0) {
                    self.overlay = Overlay::Sounds(Settings::new(self.config.clone()));
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
        let index = settings_section(settings.selected);
        settings.selected = SETTINGS_MENU
            + if down {
                (index + 1).min(SETTINGS_SECTIONS.len() - 1)
            } else {
                index.saturating_sub(1)
            };
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
            && matches!(settings.selected, AI_MODEL_ROW | AI_URL_ROW | AI_KEY_ROW))
}
fn is_text_row(row: usize) -> bool {
    matches!(
        row,
        IMAGE_ROW | MUSIC_ROW | 16..=19 | AI_MODEL_ROW | AI_URL_ROW | AI_KEY_ROW
    )
}
fn setting_text(c: &Config, row: usize) -> String {
    match row {
        AI_MODEL_ROW => c.ai.model.clone(),
        AI_URL_ROW => c.ai.base_url.clone(),
        AI_KEY_ROW => c.ai.key_env.clone(),
        IMAGE_ROW => c.image_path.clone(),
        MUSIC_ROW => c.music_url.clone(),
        16 => c.palette().hours.clone(),
        17 => c.palette().seconds.clone(),
        18 => c.palette().separator.clone(),
        _ => c.palette().date.clone(),
    }
}
fn adjust(c: &mut Config, row: usize, increase: bool) {
    let duration = match row {
        4 => Some(&mut c.focus_minutes),
        5 => Some(&mut c.short_break_minutes),
        6 => Some(&mut c.long_break_minutes),
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
        0 => c.format_24h = !c.format_24h,
        1 => c.show_seconds = !c.show_seconds,
        2 => c.blink_separator = !c.blink_separator,
        3 => c.animations = !c.animations,
        7 => c.auto_advance = !c.auto_advance,
        8 => {
            c.notifications = if increase {
                c.notifications.next()
            } else {
                c.notifications.next().next()
            }
        }
        9 => {
            c.weather = if increase {
                c.weather.next()
            } else {
                c.weather.next().next()
            }
        }
        10 => {
            c.background = if increase {
                c.background.next()
            } else {
                c.background.previous()
            }
        }
        12 => {
            c.image_brightness = if increase {
                (c.image_brightness + 5).min(100)
            } else {
                c.image_brightness.saturating_sub(5)
            }
        }
        21 => c.show_plan = !c.show_plan,
        22 => {
            let provider = if increase {
                c.ai.provider.next()
            } else {
                c.ai.provider.previous()
            };
            c.ai.select(provider);
        }
        AI_EFFORT_ROW => c.ai.thinking_effort = c.ai.thinking_effort.cycle(increase),
        26 => c.ai.auto_brief = !c.ai.auto_brief,
        27 => {
            c.ai.timeout_seconds = if increase {
                (c.ai.timeout_seconds + 5).min(180)
            } else {
                c.ai.timeout_seconds.saturating_sub(5).max(5)
            }
        }
        20 => {
            c.clock_style = if increase {
                c.clock_style.next()
            } else {
                c.clock_style.previous()
            };
        }
        15 => {
            c.clock_palette = if increase {
                (c.clock_palette + 1) % (c.clock_palettes.len() + 1)
            } else {
                (c.clock_palette + c.clock_palettes.len()) % (c.clock_palettes.len() + 1)
            }
        }
        14 => {
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
        settings.selected = 16;
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
        settings.selected = MUSIC_ROW;
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
        settings.selected = MUSIC_ROW;
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
}
