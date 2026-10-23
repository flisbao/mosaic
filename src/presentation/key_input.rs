use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::presentation::shortcuts::RESERVED_SHORTCUT_LETTERS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyInput {
    Tab,
    BackTab,
    Up,
    Down,
    Enter,
    Escape,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Character(char),
}

pub fn from_crossterm(event: KeyEvent) -> Option<KeyInput> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    match event.code {
        KeyCode::Tab => Some(KeyInput::Tab),
        KeyCode::BackTab => Some(KeyInput::BackTab),
        KeyCode::Up => Some(KeyInput::Up),
        KeyCode::Down => Some(KeyInput::Down),
        KeyCode::Enter => Some(KeyInput::Enter),
        KeyCode::Esc => Some(KeyInput::Escape),
        KeyCode::Left => Some(KeyInput::Left),
        KeyCode::Right => Some(KeyInput::Right),
        KeyCode::PageUp => Some(KeyInput::PageUp),
        KeyCode::PageDown => Some(KeyInput::PageDown),
        KeyCode::Home => Some(KeyInput::Home),
        KeyCode::End => Some(KeyInput::End),
        KeyCode::Char(letter)
            if event.modifiers.contains(KeyModifiers::ALT)
                && !event.modifiers.contains(KeyModifiers::CONTROL)
                && letter.is_ascii_alphabetic() =>
        {
            let shortcut = letter.to_ascii_uppercase();
            (!RESERVED_SHORTCUT_LETTERS.contains(&shortcut))
                .then_some(KeyInput::Character(shortcut))
        }
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(KeyInput::Character('q'))
        }
        KeyCode::Char('j') => Some(KeyInput::Down),
        KeyCode::Char('k') => Some(KeyInput::Up),
        KeyCode::Char('h') => Some(KeyInput::Left),
        KeyCode::Char('l') => Some(KeyInput::Right),
        KeyCode::Char(character) => Some(KeyInput::Character(character)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn navigation_keys_are_mapped() {
        assert_eq!(from_crossterm(press(KeyCode::Tab)), Some(KeyInput::Tab));
        assert_eq!(
            from_crossterm(press(KeyCode::BackTab)),
            Some(KeyInput::BackTab)
        );
        assert_eq!(from_crossterm(press(KeyCode::Up)), Some(KeyInput::Up));
        assert_eq!(from_crossterm(press(KeyCode::Down)), Some(KeyInput::Down));
        assert_eq!(from_crossterm(press(KeyCode::Enter)), Some(KeyInput::Enter));
        assert_eq!(from_crossterm(press(KeyCode::Esc)), Some(KeyInput::Escape));
    }

    #[test]
    fn vim_keys_become_vertical_movement() {
        assert_eq!(
            from_crossterm(press(KeyCode::Char('j'))),
            Some(KeyInput::Down)
        );
        assert_eq!(
            from_crossterm(press(KeyCode::Char('k'))),
            Some(KeyInput::Up)
        );
    }

    #[test]
    fn arrow_page_home_end_keys_are_mapped() {
        assert_eq!(from_crossterm(press(KeyCode::Left)), Some(KeyInput::Left));
        assert_eq!(from_crossterm(press(KeyCode::Right)), Some(KeyInput::Right));
        assert_eq!(
            from_crossterm(press(KeyCode::PageUp)),
            Some(KeyInput::PageUp)
        );
        assert_eq!(
            from_crossterm(press(KeyCode::PageDown)),
            Some(KeyInput::PageDown)
        );
        assert_eq!(from_crossterm(press(KeyCode::Home)), Some(KeyInput::Home));
        assert_eq!(from_crossterm(press(KeyCode::End)), Some(KeyInput::End));
    }

    #[test]
    fn h_and_l_move_horizontally() {
        assert_eq!(
            from_crossterm(press(KeyCode::Char('h'))),
            Some(KeyInput::Left)
        );
        assert_eq!(
            from_crossterm(press(KeyCode::Char('l'))),
            Some(KeyInput::Right)
        );
    }

    #[test]
    fn uppercase_h_and_l_are_not_horizontal_movement() {
        assert_eq!(
            from_crossterm(press(KeyCode::Char('H'))),
            Some(KeyInput::Character('H'))
        );
        assert_eq!(
            from_crossterm(press(KeyCode::Char('L'))),
            Some(KeyInput::Character('L'))
        );
    }

    #[test]
    fn other_characters_are_passed_through() {
        assert_eq!(
            from_crossterm(press(KeyCode::Char('a'))),
            Some(KeyInput::Character('a'))
        );
    }

    fn alt(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::ALT)
    }

    #[test]
    fn alt_letter_becomes_the_uppercase_shortcut() {
        for (typed, expected) in [('a', 'A'), ('b', 'B'), ('h', 'H'), ('j', 'J')] {
            assert_eq!(
                from_crossterm(alt(typed)),
                Some(KeyInput::Character(expected))
            );
        }
    }

    #[test]
    fn alt_with_shift_is_the_same_shortcut() {
        let event = KeyEvent::new(KeyCode::Char('B'), KeyModifiers::ALT | KeyModifiers::SHIFT);
        assert_eq!(from_crossterm(event), Some(KeyInput::Character('B')));
    }

    #[test]
    fn alt_on_a_reserved_letter_is_ignored() {
        for reserved in RESERVED_SHORTCUT_LETTERS {
            assert_eq!(
                from_crossterm(alt(reserved.to_ascii_lowercase())),
                None,
                "{reserved}"
            );
        }
    }

    #[test]
    fn control_c_quits() {
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(from_crossterm(event), Some(KeyInput::Character('q')));
    }

    #[test]
    fn key_release_and_unmapped_keys_are_ignored() {
        let mut release = press(KeyCode::Char('a'));
        release.kind = KeyEventKind::Release;
        assert_eq!(from_crossterm(release), None);
        assert_eq!(from_crossterm(press(KeyCode::F(5))), None);
    }
}
