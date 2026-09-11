//! A small readline-style text input: cursor position, word-wise
//! delete/move, and kill-to-start/end — used by both the search box and
//! the preview's product filter.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Default, Clone)]
pub struct TextField {
    chars: Vec<char>,
    pub cursor: usize,
}

impl TextField {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn as_string(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
    }

    pub fn insert_char(&mut self, c: char) {
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.chars.remove(self.cursor - 1);
            self.cursor -= 1;
        }
    }

    pub fn delete_forward(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars.len());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.chars.len();
    }

    pub fn kill_to_start(&mut self) {
        self.chars.drain(0..self.cursor);
        self.cursor = 0;
    }

    pub fn kill_to_end(&mut self) {
        self.chars.truncate(self.cursor);
    }

    fn word_start_before(&self, mut i: usize) -> usize {
        while i > 0 && self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    }

    fn word_end_after(&self, mut i: usize) -> usize {
        let len = self.chars.len();
        while i < len && self.chars[i].is_whitespace() {
            i += 1;
        }
        while i < len && !self.chars[i].is_whitespace() {
            i += 1;
        }
        i
    }

    pub fn delete_word_backward(&mut self) {
        let start = self.word_start_before(self.cursor);
        self.chars.drain(start..self.cursor);
        self.cursor = start;
    }

    pub fn delete_word_forward(&mut self) {
        let end = self.word_end_after(self.cursor);
        self.chars.drain(self.cursor..end);
    }

    pub fn move_word_left(&mut self) {
        self.cursor = self.word_start_before(self.cursor);
    }

    pub fn move_word_right(&mut self) {
        self.cursor = self.word_end_after(self.cursor);
    }

    /// Splits the field around the cursor for rendering: text before the
    /// cursor, the character the cursor sits on (empty at end-of-line, so
    /// the caller can render a block cursor instead), and the rest.
    pub fn render_parts(&self) -> (String, String, String) {
        let before = self.chars[..self.cursor].iter().collect();
        let at_cursor = self.chars.get(self.cursor).map(|c| c.to_string()).unwrap_or_default();
        let after = if self.cursor < self.chars.len() {
            self.chars[self.cursor + 1..].iter().collect()
        } else {
            String::new()
        };
        (before, at_cursor, after)
    }

    /// Applies a key as a line-editing operation (insert, delete, cursor
    /// move). Returns whether the key was consumed — callers should still
    /// handle Enter/Esc/Tab themselves before falling back to this.
    ///
    /// Word-wise delete follows common terminal conventions: Alt+Backspace
    /// and Ctrl+Backspace both delete the previous word (terminals disagree
    /// on which modifier they report for that chord, so both are accepted).
    /// Ctrl+W deliberately deviates from the readline norm (delete-word) and
    /// clears the whole field instead — a quicker "start over" for a search
    /// box than a line editor.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Left if alt || ctrl => self.move_word_left(),
            KeyCode::Right if alt || ctrl => self.move_word_right(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.move_home(),
            KeyCode::End => self.move_end(),
            KeyCode::Backspace if alt || ctrl => self.delete_word_backward(),
            KeyCode::Delete if alt || ctrl => self.delete_word_forward(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete_forward(),
            KeyCode::Char('w') if ctrl => self.clear(),
            KeyCode::Char('d') if alt => self.delete_word_forward(),
            KeyCode::Char('a') if ctrl => self.move_home(),
            KeyCode::Char('e') if ctrl => self.move_end(),
            KeyCode::Char('u') if ctrl => self.kill_to_start(),
            KeyCode::Char('k') if ctrl => self.kill_to_end(),
            KeyCode::Char('b') if alt => self.move_word_left(),
            KeyCode::Char('f') if alt => self.move_word_right(),
            KeyCode::Char(c) if !ctrl && !alt => self.insert_char(c),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: crossterm::event::KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn alt_backspace_deletes_previous_word() {
        let mut f = TextField::new();
        for c in "search milo product".chars() {
            f.insert_char(c);
        }
        assert!(f.handle_key(key(KeyCode::Backspace, KeyModifiers::ALT)));
        assert_eq!(f.as_string(), "search milo ");

        assert!(f.handle_key(key(KeyCode::Backspace, KeyModifiers::CONTROL)));
        assert_eq!(f.as_string(), "search ");
    }

    #[test]
    fn ctrl_w_clears_the_whole_field() {
        let mut f = TextField::new();
        for c in "nestle milo".chars() {
            f.insert_char(c);
        }
        f.move_left();
        assert!(f.handle_key(key(KeyCode::Char('w'), KeyModifiers::CONTROL)));
        assert!(f.is_empty());
        assert_eq!(f.cursor, 0);
    }

    #[test]
    fn cursor_movement_and_mid_string_insert() {
        let mut f = TextField::new();
        for c in "abd".chars() {
            f.insert_char(c);
        }
        f.move_left();
        f.insert_char('c');
        assert_eq!(f.as_string(), "abcd");
        assert_eq!(f.cursor, 3);

        f.move_home();
        assert_eq!(f.cursor, 0);
        f.move_end();
        assert_eq!(f.cursor, 4);
    }

    #[test]
    fn kill_to_start_and_end() {
        let mut f = TextField::new();
        for c in "hello world".chars() {
            f.insert_char(c);
        }
        f.cursor = 5;
        f.kill_to_end();
        assert_eq!(f.as_string(), "hello");

        for c in ", friend".chars() {
            f.insert_char(c);
        }
        f.cursor = 5;
        f.kill_to_start();
        assert_eq!(f.as_string(), ", friend");
    }
}
