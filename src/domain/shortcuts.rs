pub const RESERVED_SHORTCUT_LETTERS: [char; 1] = ['R'];

const FIRST_SHORTCUT_LETTER: char = 'A';
const LAST_SHORTCUT_LETTER: char = 'Z';

pub fn assign_shortcuts(widget_count: usize) -> Vec<Option<char>> {
    let mut free_letters = (FIRST_SHORTCUT_LETTER..=LAST_SHORTCUT_LETTER)
        .filter(|letter| !RESERVED_SHORTCUT_LETTERS.contains(letter));
    (0..widget_count).map(|_| free_letters.next()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDGET_COUNT_EXCEEDING_ALPHABET: usize = 30;
    const FREE_LETTER_COUNT: usize = 25;
    const LETTER_Q_INDEX: usize = 16;
    const LETTER_AFTER_RESERVED_R_INDEX: usize = 17;
    const LAST_FREE_LETTER_INDEX: usize = 24;
    const INDEX_BEYOND_FREE_LETTERS: usize = 25;
    const LAST_WIDGET_INDEX: usize = 29;

    #[test]
    fn letters_start_at_a_and_skip_reserved() {
        let shortcuts = assign_shortcuts(WIDGET_COUNT_EXCEEDING_ALPHABET);
        assert_eq!(shortcuts[LETTER_Q_INDEX], Some('Q'));
        assert_eq!(shortcuts[LETTER_AFTER_RESERVED_R_INDEX], Some('S'));
        assert_eq!(shortcuts[LAST_FREE_LETTER_INDEX], Some('Z'));
        assert_eq!(shortcuts[INDEX_BEYOND_FREE_LETTERS], None);
        assert_eq!(shortcuts[LAST_WIDGET_INDEX], None);
    }

    #[test]
    fn reserved_letters_are_never_assigned() {
        let shortcuts = assign_shortcuts(WIDGET_COUNT_EXCEEDING_ALPHABET);
        for reserved in RESERVED_SHORTCUT_LETTERS {
            assert!(!shortcuts.contains(&Some(reserved)));
        }
    }

    #[test]
    fn zero_widgets_yield_empty() {
        assert!(assign_shortcuts(0).is_empty());
    }

    #[test]
    fn shortcut_count_equals_widget_count() {
        for widget_count in [
            1,
            FREE_LETTER_COUNT,
            FREE_LETTER_COUNT + 1,
            WIDGET_COUNT_EXCEEDING_ALPHABET,
        ] {
            assert_eq!(assign_shortcuts(widget_count).len(), widget_count);
        }
    }
}
