use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyInput {
    Tab,
    BackTab,
    Up,
    Down,
    Enter,
    Escape,
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
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(KeyInput::Character('q'))
        }
        KeyCode::Char('j') => Some(KeyInput::Down),
        KeyCode::Char('k') => Some(KeyInput::Up),
        KeyCode::Char(character) => Some(KeyInput::Character(character)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

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
    fn other_characters_are_passed_through() {
        assert_eq!(
            from_crossterm(press(KeyCode::Char('a'))),
            Some(KeyInput::Character('a'))
        );
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
