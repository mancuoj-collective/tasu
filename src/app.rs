use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tui_input::{Input, backend::crossterm::EventHandler};

use crate::{store::Store, theme::Theme, todo::TodoList};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Normal,
    Add,
    Edit,
    ConfirmDelete,
}

pub struct App {
    pub should_quit: bool,
    pub theme: Theme,
    pub mode: Mode,
    pub todos: TodoList,
    pub input: Input,
}

impl Default for App {
    fn default() -> Self {
        Self {
            should_quit: false,
            theme: Theme::detect(),
            mode: Mode::default(),
            todos: TodoList::with_items(Store::load().todos),
            input: Input::default(),
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit();
            return;
        }

        match self.mode {
            Mode::Normal => self.normal_key(key),
            Mode::Add | Mode::Edit => self.input_key(key),
            Mode::ConfirmDelete => self.confirm_key(key),
        }
    }

    fn normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.quit(),
            KeyCode::Char('j') | KeyCode::Down => self.todos.select_next(),
            KeyCode::Char('k') | KeyCode::Up => self.todos.select_previous(),
            KeyCode::Char('g') | KeyCode::Home => self.todos.select_first(),
            KeyCode::Char('G') | KeyCode::End => self.todos.select_last(),
            KeyCode::Char(' ') => {
                self.todos.toggle_selected();
                self.persist();
            }
            KeyCode::Char('a') => {
                self.input.reset();
                self.mode = Mode::Add;
            }
            KeyCode::Char('e') => {
                if let Some(title) = self.todos.selected_title() {
                    self.input = Input::new(title.to_string());
                    self.mode = Mode::Edit;
                }
            }
            KeyCode::Char('d') if self.todos.selected().is_some() => {
                self.mode = Mode::ConfirmDelete;
            }
            _ => {}
        }
    }

    fn input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let title = self.input.value().trim().to_string();
                if !title.is_empty() {
                    match self.mode {
                        Mode::Add => self.todos.add(title),
                        Mode::Edit => self.todos.rename_selected(title),
                        _ => {}
                    }
                    self.persist();
                }
                self.input.reset();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.input.reset();
                self.mode = Mode::Normal;
            }
            _ => {
                self.input.handle_event(&Event::Key(key));
            }
        }
    }

    fn confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') => {
                self.todos.remove_selected();
                self.mode = Mode::Normal;
                self.persist();
            }
            KeyCode::Char('n') | KeyCode::Esc => self.mode = Mode::Normal,
            _ => {}
        }
    }

    fn persist(&self) {
        Store {
            todos: self.todos.items().to_vec(),
        }
        .save();
    }

    fn quit(&mut self) {
        self.should_quit = true;
    }
}
