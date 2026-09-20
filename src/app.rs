use anyhow::Result;
use crossterm::event::{
    Event::Key,
    KeyCode::{self},
    KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::{DefaultTerminal, widgets::ListState};

use crate::{
    event::{AppEvent, Event, EventHandler},
    todo::Todo,
    ui,
};

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
    pub events: EventHandler,
    pub mode: Mode,
    pub todos: Vec<Todo>,
    pub state: ListState,
    pub input: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            should_quit: false,
            events: EventHandler::new(),
            mode: Mode::default(),
            todos: vec![Todo::new("read docs"), Todo::new("build apps")],
            state: ListState::default().with_selected(Some(0)),
            input: String::new(),
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|f| ui::draw(f, &mut self))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn handle_events(&mut self) -> Result<()> {
        match self.events.next()? {
            Event::Tick => self.tick(),
            Event::Crossterm(event) => match event {
                Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key_event(key_event)?
                }
                _ => {}
            },
            Event::App(app_event) => match app_event {
                AppEvent::Quit => self.quit(),
            },
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        if ctrl && matches!(key.code, KeyCode::Char('c' | 'C')) {
            self.events.send(AppEvent::Quit);
            return Ok(());
        }

        match self.mode {
            Mode::Normal => self.normal_key(key),
            Mode::Add | Mode::Edit => self.input_key(key),
            Mode::ConfirmDelete => self.confirm_key(key),
        }
        Ok(())
    }

    fn normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.events.send(AppEvent::Quit),
            KeyCode::Char('j') | KeyCode::Down => self.state.select_next(),
            KeyCode::Char('k') | KeyCode::Up => self.state.select_previous(),
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Char('a') => {
                self.input.clear();
                self.mode = Mode::Add;
            }
            KeyCode::Char('e') => {
                if let Some(i) = self.state.selected() {
                    self.input = self.todos[i].title.clone();
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
                let title = self.input.trim().to_string();
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
                self.input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
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

    fn tick(&self) {}

    fn quit(&mut self) {
        self.should_quit = true;
    }

    fn toggle(&mut self) {
        if let Some(i) = self.state.selected() {
            self.todos[i].done = !self.todos[i].done
        }
    }
}
