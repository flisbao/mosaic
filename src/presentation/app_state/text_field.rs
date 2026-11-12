use crate::presentation::key_input::KeyInput;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    text: String,
    cursor: usize,
}

impl TextField {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn handle_key(&mut self, key: KeyInput) -> bool {
        match key {
            KeyInput::Character(character) if !character.is_control() => {
                self.insert(character);
                true
            }
            KeyInput::Backspace => {
                self.backspace();
                true
            }
            KeyInput::Delete => {
                self.delete();
                true
            }
            KeyInput::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                true
            }
            KeyInput::Right => {
                self.cursor = (self.cursor + 1).min(self.length());
                true
            }
            KeyInput::Home => {
                self.cursor = 0;
                true
            }
            KeyInput::End => {
                self.cursor = self.length();
                true
            }
            _ => false,
        }
    }

    fn length(&self) -> usize {
        self.text.chars().count()
    }

    fn byte_index(&self, character_index: usize) -> usize {
        self.text
            .char_indices()
            .nth(character_index)
            .map_or(self.text.len(), |(index, _)| index)
    }

    fn insert(&mut self, character: char) {
        let index = self.byte_index(self.cursor);
        self.text.insert(index, character);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let index = self.byte_index(self.cursor);
        self.text.remove(index);
    }

    fn delete(&mut self) {
        if self.cursor < self.length() {
            let index = self.byte_index(self.cursor);
            self.text.remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> TextField {
        let mut field = TextField::default();
        for character in text.chars() {
            assert!(field.handle_key(KeyInput::Character(character)));
        }
        field
    }

    #[test]
    fn characters_are_inserted_at_the_cursor() {
        let mut field = typed("ac");
        field.handle_key(KeyInput::Left);
        field.handle_key(KeyInput::Character('b'));
        assert_eq!((field.text(), field.cursor()), ("abc", 2));
    }

    #[test]
    fn backspace_and_delete_remove_around_the_cursor() {
        let mut field = typed("abc");
        field.handle_key(KeyInput::Left);
        field.handle_key(KeyInput::Backspace);
        assert_eq!((field.text(), field.cursor()), ("ac", 1));
        field.handle_key(KeyInput::Delete);
        assert_eq!((field.text(), field.cursor()), ("a", 1));
        field.handle_key(KeyInput::Delete);
        assert_eq!(field.text(), "a");
    }

    #[test]
    fn editing_at_the_edges_changes_nothing() {
        let mut field = TextField::default();
        field.handle_key(KeyInput::Backspace);
        field.handle_key(KeyInput::Left);
        assert_eq!((field.text(), field.cursor()), ("", 0));
        let mut field = typed("ab");
        field.handle_key(KeyInput::Right);
        assert_eq!(field.cursor(), 2);
    }

    #[test]
    fn home_and_end_jump_to_the_edges() {
        let mut field = typed("abc");
        field.handle_key(KeyInput::Home);
        assert_eq!(field.cursor(), 0);
        field.handle_key(KeyInput::End);
        assert_eq!(field.cursor(), 3);
    }

    #[test]
    fn multibyte_characters_count_as_one_position() {
        let mut field = typed("açaí");
        assert_eq!(field.cursor(), 4);
        field.handle_key(KeyInput::Backspace);
        field.handle_key(KeyInput::Left);
        field.handle_key(KeyInput::Delete);
        assert_eq!((field.text(), field.cursor()), ("aç", 2));
    }

    #[test]
    fn vim_letters_are_plain_text_and_other_keys_are_not_consumed() {
        let field = typed("hjkl q");
        assert_eq!(field.text(), "hjkl q");
        let mut field = TextField::default();
        assert!(!field.handle_key(KeyInput::Enter));
        assert!(!field.handle_key(KeyInput::Escape));
        assert!(!field.handle_key(KeyInput::Up));
        assert!(!field.handle_key(KeyInput::Character('\n')));
    }

    #[test]
    fn clear_empties_the_field() {
        let mut field = typed("abc");
        field.clear();
        assert_eq!((field.text(), field.cursor()), ("", 0));
    }
}
