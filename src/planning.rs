//! Local goal/reminder editing and provider-independent conversation input.
use crate::{
    input::TextInput,
    planner::{Data, Entry, Kind, Store},
};
use chrono::{Local, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub struct Editor {
    pub entry: Entry,
    pub title: TextInput,
    pub due: TextInput,
    pub field: usize,
}
impl Editor {
    fn new(entry: Entry) -> Self {
        Self {
            title: TextInput::new(entry.title.clone()),
            due: TextInput::new(entry.due.clone().unwrap_or_default()),
            entry,
            field: 0,
        }
    }
    pub fn active_input(&mut self) -> Option<&mut TextInput> {
        match self.field {
            0 => Some(&mut self.title),
            2 => Some(&mut self.due),
            _ => None,
        }
    }
    fn cycle_parent(&mut self, data: &Data, increase: bool) {
        let choices = std::iter::once(None)
            .chain(
                data.valid_parents(self.entry.kind)
                    .into_iter()
                    .map(|e| Some(e.id)),
            )
            .collect::<Vec<_>>();
        let index = choices
            .iter()
            .position(|id| *id == self.entry.parent)
            .unwrap_or(0);
        self.entry.parent = choices[if increase {
            (index + 1) % choices.len()
        } else {
            (index + choices.len() - 1) % choices.len()
        }];
    }
}
pub struct Panel {
    pub kind: Kind,
    pub selected: usize,
    pub completed: bool,
    pub editor: Option<Editor>,
    pub error: Option<String>,
    pub archived: Option<u64>,
    quick: bool,
}
impl Panel {
    pub fn new() -> Self {
        Self {
            kind: Kind::Task,
            selected: 0,
            completed: false,
            editor: None,
            error: None,
            archived: None,
            quick: false,
        }
    }
    pub fn character() -> Self {
        Self {
            kind: Kind::Character,
            ..Self::new()
        }
    }
    pub fn quick_add() -> Self {
        let mut panel = Self::new();
        panel.editor = Some(Editor::new(Entry::blank(Kind::Task)));
        panel.quick = true;
        panel
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(editor) = &mut self.editor {
            if let Some(input) = editor.active_input() {
                input.insert(text);
            }
        }
    }
    pub fn entries<'a>(&self, data: &'a Data, date: NaiveDate) -> Vec<&'a Entry> {
        if self.kind != Kind::Task {
            data.visible(self.kind)
        } else if self.completed {
            data.visible(Kind::Task)
                .into_iter()
                .filter(|entry| entry.done)
                .collect()
        } else {
            data.pending(date)
        }
    }
    pub fn key(&mut self, key: KeyEvent, store: &mut Store) -> bool {
        let date = Local::now().date_naive();
        if let Some(editor) = &mut self.editor {
            match key.code {
                KeyCode::Esc => {
                    self.editor = None;
                    self.error = None;
                    return self.quick;
                }
                KeyCode::Tab => {
                    editor.field = (editor.field + 1)
                        % if editor.entry.kind == Kind::Character {
                            1
                        } else {
                            4
                        }
                }
                KeyCode::BackTab => {
                    editor.field = (editor.field + 3)
                        % if editor.entry.kind == Kind::Character {
                            1
                        } else {
                            4
                        }
                }
                KeyCode::Enter => {
                    let mut entry = editor.entry.clone();
                    entry.title = editor.title.value.trim().into();
                    entry.due = (!editor.due.value.trim().is_empty())
                        .then(|| editor.due.value.trim().to_owned());
                    match store.put(entry) {
                        Ok(id) => {
                            self.selected = self
                                .entries(&store.data, date)
                                .iter()
                                .position(|e| e.id == id)
                                .unwrap_or(0);
                            self.editor = None;
                            self.error = None;
                            if self.quick {
                                return true;
                            }
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                KeyCode::Left | KeyCode::Right if editor.field == 1 => {
                    editor.cycle_parent(&store.data, key.code == KeyCode::Right)
                }
                KeyCode::Left | KeyCode::Right if editor.field == 3 => {
                    editor.entry.priority = if key.code == KeyCode::Right {
                        editor.entry.priority.next()
                    } else {
                        editor.entry.priority.previous()
                    }
                }
                _ => {
                    if let Some(input) = editor.active_input() {
                        input.key(key);
                    }
                }
            }
            return false;
        }
        let visible = self.entries(&store.data, date);
        let len = visible.len();
        self.selected = self.selected.min(len.saturating_sub(1));
        let entry = visible.get(self.selected).map(|e| (*e).clone());
        self.error = None;
        let result = match key.code {
            KeyCode::Esc => return true,
            KeyCode::Char('g') if self.kind != Kind::Character => return true,
            KeyCode::Char('h') if self.kind == Kind::Character => return true,
            KeyCode::Tab | KeyCode::Right => {
                self.kind = self.kind.next();
                self.selected = 0;
                Ok(())
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.kind = self.kind.previous();
                self.selected = 0;
                Ok(())
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if len > 0 {
                    self.selected = (self.selected + len - 1) % len;
                }
                Ok(())
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if len > 0 {
                    self.selected = (self.selected + 1) % len;
                }
                Ok(())
            }
            KeyCode::Char('f') if self.kind == Kind::Task => {
                self.completed = !self.completed;
                self.selected = 0;
                Ok(())
            }
            KeyCode::Char('a') | KeyCode::Char('+') => {
                self.completed = false;
                self.editor = Some(Editor::new(Entry::blank(self.kind)));
                Ok(())
            }
            KeyCode::Char('e') | KeyCode::Enter => {
                if let Some(e) = entry {
                    self.editor = Some(Editor::new(e));
                }
                Ok(())
            }
            KeyCode::Char(' ') => {
                if let Some(e) = entry {
                    store.modify(e.id, |e| e.done = !e.done)
                } else {
                    Ok(())
                }
            }
            KeyCode::Char('p') if self.kind != Kind::Character => {
                if let Some(e) = entry {
                    store.modify(e.id, |e| e.priority = e.priority.next())
                } else {
                    Ok(())
                }
            }
            KeyCode::Char('x') => {
                if let Some(e) = entry {
                    match store.modify(e.id, |e| e.archived = true) {
                        Ok(()) => {
                            self.archived = Some(e.id);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    Ok(())
                }
            }
            KeyCode::Char('u') => {
                if let Some(id) = self.archived {
                    match store.modify(id, |e| e.archived = false) {
                        Ok(()) => {
                            self.archived = None;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.error = Some(error);
        }
        self.selected = self
            .selected
            .min(self.entries(&store.data, date).len().saturating_sub(1));
        false
    }
}
pub struct Conversation {
    pub input: TextInput,
    pub messages: Vec<(String, String)>,
    pub pending: Option<String>,
    pub busy: bool,
    effort: Option<crate::ai::ThinkingEffort>,
    pub session: u64,
    pub scroll_back: usize,
}
impl Conversation {
    pub fn new(store: &Store, date: NaiveDate) -> Self {
        Self {
            input: TextInput::new(String::new()),
            messages: vec![(
                "Lofiji · local planner".into(),
                format!(
                    "{}\nWhat would you like to work on?",
                    store.data.daily_lines(date, 1).join("")
                ),
            )],
            pending: None,
            busy: false,
            effort: None,
            session: 0,
            scroll_back: 0,
        }
    }
    pub fn effort(&self, default: crate::ai::ThinkingEffort) -> crate::ai::ThinkingEffort {
        self.effort.unwrap_or(default)
    }

    pub fn cycle_effort(&mut self, ai: &crate::ai::Settings) {
        if !self.busy && ai.supports_effort() {
            self.effort = Some(self.effort(ai.thinking_effort).cycle(true));
        }
    }

    pub fn cancel_reply(&mut self) {
        if self.busy {
            self.busy = false;
            self.pending = None;
            self.messages
                .push(("Lofiji".into(), "Reply cancelled.".into()));
            self.scroll_back = 0;
            self.trim_history();
        }
    }
    pub fn trim_history(&mut self) {
        if self.messages.len() > 40 {
            self.messages.drain(..self.messages.len() - 40);
        }
    }
    pub fn history(&self) -> Vec<crate::ai::Message> {
        let all = self
            .messages
            .iter()
            .filter_map(|(who, text)| {
                let role = if who == "You" && !is_local_command(text) {
                    "user"
                } else if who.starts_with("AI · ") {
                    "assistant"
                } else {
                    return None;
                };
                Some(crate::ai::Message {
                    role: role.into(),
                    content: text.clone(),
                })
            })
            .collect::<Vec<_>>();
        let mut merged: Vec<crate::ai::Message> = Vec::new();
        for message in all {
            if let Some(last) = merged.last_mut().filter(|m| m.role == message.role) {
                last.content.push('\n');
                last.content.push_str(&message.content);
            } else {
                merged.push(message);
            }
        }
        let mut history = merged.into_iter().rev().take(13).collect::<Vec<_>>();
        history.reverse();
        while history.first().is_some_and(|m| m.role != "user") {
            history.remove(0);
        }
        history
    }
    pub fn key(&mut self, key: KeyEvent, store: &mut Store, ai: &crate::ai::Settings) -> bool {
        if key.code == KeyCode::Esc {
            return true;
        }
        match key.code {
            KeyCode::PageUp => {
                self.scroll_back = (self.scroll_back + 5).min(10000);
                return false;
            }
            KeyCode::PageDown => {
                self.scroll_back = self.scroll_back.saturating_sub(5);
                return false;
            }
            _ => {}
        }
        if key.code == KeyCode::Enter {
            let text = self.input.value.trim().to_string();
            if text.is_empty() {
                return false;
            }
            if self.busy {
                return false;
            }
            self.scroll_back = 0;
            self.messages.push(("You".into(), text.clone()));
            if ai.provider != crate::ai::Provider::Off
                && !is_local_command(&text)
                && store.error.is_none()
            {
                self.pending = Some(text);
                self.busy = true;
            } else {
                let reply = respond(store, &text, Local::now().date_naive());
                self.messages.push(("Lofiji · local planner".into(), reply));
            }
            self.trim_history();
            self.input = TextInput::new(String::new());
        } else if key.modifiers == KeyModifiers::NONE
            || key.modifiers == KeyModifiers::SHIFT
            || key.modifiers == KeyModifiers::CONTROL
        {
            self.input.key(key);
        }
        false
    }
}
pub fn is_local_command(text: &str) -> bool {
    let value = text.to_lowercase();
    [
        "task:",
        "add task ",
        "short goal:",
        "long goal:",
        "reminder:",
        "character:",
        "done ",
    ]
    .iter()
    .any(|p| value.starts_with(p))
}
pub fn respond(store: &mut Store, text: &str, date: NaiveDate) -> String {
    if let Some(error) = &store.error {
        return format!("I couldn't load your goals: {error}. Your file has been kept intact.");
    }
    let lower = text.to_lowercase();
    for (prefix, kind) in [
        ("task:", Kind::Task),
        ("add task ", Kind::Task),
        ("short goal:", Kind::Short),
        ("long goal:", Kind::Long),
        ("reminder:", Kind::Character),
        ("character:", Kind::Character),
    ] {
        if lower.starts_with(prefix) {
            let mut entry = Entry::blank(kind);
            entry.title = text[prefix.len()..].trim().into();
            let title = entry.title.clone();
            return match store.put(entry) {
                Ok(id) => format!("Saved #{id}: {title}.\n{}", store.data.brief(date)),
                Err(e) => format!("I couldn't save that: {e}"),
            };
        }
    }
    if let Some(id) = lower
        .strip_prefix("done ")
        .and_then(|id| id.trim().trim_start_matches('#').parse::<u64>().ok())
    {
        if store
            .data
            .entry(id)
            .is_some_and(|e| e.kind == Kind::Character)
        {
            return "Character reminders are ongoing. Press h, then Space to pause or resume one."
                .into();
        }
        return match store.modify(id, |e| e.done = true) {
            Ok(()) => format!("Marked #{id} complete.\n{}", store.data.brief(date)),
            Err(e) => format!("I couldn't complete that: {e}"),
        };
    }
    if ["plan", "next", "what", "today", "goals", "help"]
        .iter()
        .any(|word| lower.contains(word))
    {
        return format!("{}\nI put overdue tasks first, then today's deadlines and priority. Choose one small step to begin.",store.data.brief(date));
    }
    "AI is off. I can show your plan ('what should I do?'), save 'task: …', 'short goal: …' or 'long goal: …', and complete 'done #ID'. Press g on the clock to edit links, dates and priorities.".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn character_reminders_are_not_tasks_or_linkable_objectives() {
        let mut store = Store::open(Path::new(""));
        let mut panel = Panel::character();
        panel.key(key(KeyCode::Char('+')), &mut store);
        panel.paste("Listen before responding 東京");
        panel.key(key(KeyCode::Tab), &mut store);
        assert_eq!(panel.editor.as_ref().unwrap().field, 0);
        panel.key(key(KeyCode::Enter), &mut store);
        assert_eq!(store.data.entries[0].kind, Kind::Character);
        assert!(store.data.valid_parents(Kind::Task).is_empty());
        assert!(store.data.pending(Local::now().date_naive()).is_empty());
        panel.key(key(KeyCode::Right), &mut store);
        assert_eq!(panel.kind, Kind::Character);
        panel.key(key(KeyCode::Char(' ')), &mut store);
        assert!(store.data.entries[0].done);
        panel.key(key(KeyCode::Char(' ')), &mut store);
        assert!(!store.data.entries[0].done);
        let data: Data = toml::from_str(&toml::to_string(&store.data).unwrap()).unwrap();
        assert_eq!(data, store.data);
        assert!(is_local_command("reminder: Keep small promises"));
        assert!(respond(
            &mut store,
            "reminder: Keep small promises",
            Local::now().date_naive()
        )
        .contains("Saved"));
        assert_eq!(store.data.entries[1].kind, Kind::Character);
        let mut invalid = crate::planner::Entry::blank(Kind::Task);
        invalid.title = "Not a valid parent".into();
        invalid.parent = Some(store.data.entries[0].id);
        assert!(store.put(invalid).is_err());
    }
    #[test]
    fn editor_preserves_unicode_completes_and_undoes_archives() {
        let mut store = Store::open(Path::new(""));
        let mut panel = Panel::quick_add();
        panel.paste("Review 東京 and café");
        panel.key(key(KeyCode::Enter), &mut store);
        assert!(panel.editor.is_none());
        assert_eq!(store.data.entries[0].title, "Review 東京 and café");
        panel.key(key(KeyCode::Char(' ')), &mut store);
        assert!(store.data.entries[0].done);
        panel.key(key(KeyCode::Char('f')), &mut store);
        panel.key(key(KeyCode::Char(' ')), &mut store);
        assert!(!store.data.entries[0].done);
        panel.key(key(KeyCode::Char('f')), &mut store);
        panel.key(key(KeyCode::Char('x')), &mut store);
        assert!(store.data.visible(Kind::Task).is_empty());
        panel.key(key(KeyCode::Char('u')), &mut store);
        assert_eq!(store.data.visible(Kind::Task).len(), 1);
        panel.key(key(KeyCode::Enter), &mut store);
        panel.paste(" discarded");
        panel.key(key(KeyCode::Esc), &mut store);
        assert!(!store.data.entries[0].title.contains("discarded"));
    }
    #[test]
    fn conversation_is_explicitly_local_and_saves_only_supported_commands() {
        let mut store = Store::open(Path::new(""));
        let date = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        assert!(respond(&mut store, "task: Review the proof", date).contains("Saved"));
        assert_eq!(store.data.entries.len(), 1);
        assert!(respond(&mut store, "What should I do?", date).contains("Review the proof"));
        assert!(respond(&mut store, "Tell me about quantum mechanics", date).contains("AI is off"));
        respond(&mut store, "done #1", date);
        assert!(store.data.entries[0].done);
    }
}
