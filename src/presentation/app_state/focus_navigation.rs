use super::{AppState, NO_DETAIL_NOTICE, PAGE_JUMP_ITEMS};
use crate::domain::widget::WidgetRecord;
use crate::presentation::key_input::KeyInput;

const DECIMAL_RADIX: u32 = 10;
const FIRST_TAB_NUMBER: usize = 1;
const FORWARD: isize = 1;
const BACKWARD: isize = -1;
const PAGE_STEP: isize = PAGE_JUMP_ITEMS as isize;

fn shifted(position: usize, delta: isize, last: usize) -> usize {
    position.saturating_add_signed(delta).min(last)
}

#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Backward,
}

impl AppState {
    pub(super) fn handle_navigation_key(&mut self, key: KeyInput) {
        if self.is_expanded() {
            self.handle_focus_key(key);
        } else {
            self.handle_grid_key(key);
        }
    }

    pub(super) fn handle_selection_character(&mut self, letter: char) {
        match letter {
            '1'..='9' if !self.is_expanded() => self.select_tab_by_digit(letter),
            'A'..='Z' => self.focus_shortcut(letter),
            _ => {}
        }
    }

    fn handle_grid_key(&mut self, key: KeyInput) {
        match key {
            KeyInput::Tab => self.cycle_tab(Direction::Forward),
            KeyInput::BackTab => self.cycle_tab(Direction::Backward),
            KeyInput::Up | KeyInput::Left => self.move_focus(BACKWARD),
            KeyInput::Down | KeyInput::Right => self.move_focus(FORWARD),
            KeyInput::Enter => self.expand_focused(),
            KeyInput::Escape
            | KeyInput::PageUp
            | KeyInput::PageDown
            | KeyInput::Home
            | KeyInput::End
            | KeyInput::Character(_) => {}
        }
    }

    fn handle_focus_key(&mut self, key: KeyInput) {
        match key {
            KeyInput::Up | KeyInput::Left => self.move_item(BACKWARD),
            KeyInput::Down | KeyInput::Right => self.move_item(FORWARD),
            KeyInput::PageUp => self.move_item(-PAGE_STEP),
            KeyInput::PageDown => self.move_item(PAGE_STEP),
            KeyInput::Home => self.selected_item = 0,
            KeyInput::End => self.selected_item = self.item_count().saturating_sub(1),
            KeyInput::Escape => self.collapse(),
            KeyInput::Tab | KeyInput::BackTab | KeyInput::Enter | KeyInput::Character(_) => {}
        }
    }

    fn move_focus(&mut self, delta: isize) {
        let last = self.active_entry_indexes().len().saturating_sub(1);
        self.focus = shifted(self.focus, delta, last);
    }

    fn move_item(&mut self, delta: isize) {
        let last = self.item_count().saturating_sub(1);
        self.selected_item = shifted(self.selected_item, delta, last);
    }

    fn cycle_tab(&mut self, direction: Direction) {
        let count = self.tabs().len();
        if count == 0 {
            return;
        }
        let next = match direction {
            Direction::Forward => (self.active_tab + 1) % count,
            Direction::Backward => (self.active_tab + count - 1) % count,
        };
        self.go_to_tab(next);
    }

    fn go_to_tab(&mut self, index: usize) {
        if index >= self.tabs().len() || index == self.active_tab {
            return;
        }
        self.active_tab = index;
        self.expanded = false;
        self.focus_position(0);
    }

    fn select_tab_by_digit(&mut self, digit: char) {
        let Some(number) = digit.to_digit(DECIMAL_RADIX) else {
            return;
        };
        let Some(index) = (number as usize).checked_sub(FIRST_TAB_NUMBER) else {
            return;
        };
        self.go_to_tab(index);
    }

    fn focus_shortcut(&mut self, letter: char) {
        let position = self
            .active_entry_indexes()
            .iter()
            .position(|&index| self.shortcut_of(index) == Some(letter));
        if let Some(position) = position {
            self.focus_position(position);
        }
    }

    fn focus_position(&mut self, position: usize) {
        self.focus = position;
        self.selected_item = 0;
    }

    fn expand_focused(&mut self) {
        if self.focused_entry().is_none() {
            return;
        }
        if !self.has_detail() && !self.is_focused_failing() {
            self.show_notice(NO_DETAIL_NOTICE.to_string());
            return;
        }
        self.expanded = true;
        self.selected_item = 0;
    }

    fn collapse(&mut self) {
        self.expanded = false;
        self.selected_item = 0;
    }

    fn is_focused_failing(&self) -> bool {
        self.focused_record().is_some_and(WidgetRecord::is_failing)
    }

    fn has_detail(&self) -> bool {
        self.focused_record()
            .and_then(|record| record.snapshot())
            .is_some_and(|snapshot| snapshot.detail.is_some())
    }
}
