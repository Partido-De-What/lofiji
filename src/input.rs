//! Shared Unicode-safe single-line text editing.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
pub struct TextInput {
    pub value: String,
    pub cursor: usize,
}
impl TextInput {
    pub(crate) fn new(value: String) -> Self {
        let cursor = value.len();
        Self { value, cursor }
    }
    pub fn insert(&mut self, text: &str) {
        for ch in text.chars().filter(|c| !c.is_control()) {
            if self.value.len() + ch.len_utf8() > 4096 {
                break;
            }
            self.value.insert(self.cursor, ch);
            self.cursor += ch.len_utf8();
        }
    }
    pub(crate) fn key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => {
                self.cursor = self.value[..self.cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            }
            KeyCode::Right => {
                self.cursor += self.value[self.cursor..]
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(0)
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.value.len(),
            KeyCode::Backspace if self.cursor > 0 => {
                let before = self.cursor;
                self.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
                self.value.replace_range(self.cursor..before, "");
            }
            KeyCode::Delete => {
                if let Some(ch) = self.value[self.cursor..].chars().next() {
                    self.value
                        .replace_range(self.cursor..self.cursor + ch.len_utf8(), "");
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.value.clear();
                self.cursor = 0;
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(&ch.to_string())
            }
            _ => {}
        }
    }
}
