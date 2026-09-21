use anyhow::{Context, Result};
use crossterm::event::{
    self, Event,
    KeyCode::{self},
    KeyEvent, KeyModifiers,
};
use ratatui::{DefaultTerminal, widgets::ListState};
use tui_input::{Input, backend::crossterm::EventHandler};

use crate::{theme::Theme, todo::Todo, ui};

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
    pub todos: Vec<Todo>,
    pub state: ListState,
    pub input: Input,
}

impl Default for App {
    fn default() -> Self {
        Self {
            should_quit: false,
            theme: Theme::detect(),
            mode: Mode::default(),
            todos: vec![Todo::new("read docs"), Todo::new("build apps")],
            state: ListState::default().with_selected(Some(0)),
            input: Input::default(),
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|f| ui::draw(f, self))?;
            let event = event::read().context("failed to read crossterm event")?;
            if let Some(key) = event.as_key_press_event() {
                self.on_key(key);
            }
        }
        Ok(())
    }

    fn on_key(&mut self, key: KeyEvent) {
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
            KeyCode::Char('j') | KeyCode::Down => self.state.select_next(),
            KeyCode::Char('k') | KeyCode::Up => self.state.select_previous(),
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Char('a') => {
                self.input.reset();
                self.mode = Mode::Add;
            }
            KeyCode::Char('e') => {
                if let Some(i) = self.state.selected() {
                    self.input = Input::new(self.todos[i].title.clone());
                    self.mode = Mode::Edit;
                }
            }
            KeyCode::Char('d') => {
                if self.state.selected().is_some() {
                    self.mode = Mode::ConfirmDelete;
                }
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
                        Mode::Add => {
                            self.todos.push(Todo::new(title));
                            self.state.select(Some(self.todos.len() - 1));
                        }
                        Mode::Edit => {
                            if let Some(i) = self.state.selected() {
                                self.todos[i].title = title;
                            }
                        }
                        _ => {}
                    }
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
                if let Some(i) = self.state.selected() {
                    self.todos.remove(i);
                    if self.todos.is_empty() {
                        self.state.select(None);
                    } else if i >= self.todos.len() {
                        self.state.select(Some(self.todos.len() - 1));
                    }
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Esc => self.mode = Mode::Normal,
            _ => {}
        }
    }

    fn quit(&mut self) {
        self.should_quit = true;
    }

    fn toggle(&mut self) {
        if let Some(i) = self.state.selected() {
            self.todos[i].done = !self.todos[i].done
        }
    }
}
