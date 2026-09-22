use ratatui::widgets::ListState;

pub struct Todo {
    pub title: String,
    pub done: bool,
}

impl Todo {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            done: false,
        }
    }

    pub fn toggle(&mut self) {
        self.done = !self.done;
    }
}

#[derive(Default)]
pub struct TodoList {
    items: Vec<Todo>,
    state: ListState,
}

impl TodoList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_items(items: Vec<Todo>) -> Self {
        let selected = (!items.is_empty()).then_some(0);
        Self {
            items,
            state: ListState::default().with_selected(selected),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn done_count(&self) -> usize {
        self.items.iter().filter(|todo| todo.done).count()
    }

    pub fn selected(&self) -> Option<usize> {
        self.state.selected()
    }

    fn selected_mut(&mut self) -> Option<&mut Todo> {
        self.selected().and_then(|index| self.items.get_mut(index))
    }

    pub fn selected_title(&self) -> Option<&str> {
        self.selected()
            .and_then(|index| self.items.get(index))
            .map(|todo| todo.title.as_str())
    }

    pub fn select_next(&mut self) {
        self.state.select_next();
    }

    pub fn select_previous(&mut self) {
        self.state.select_previous();
    }

    pub fn add(&mut self, title: impl Into<String>) {
        self.items.push(Todo::new(title));
        self.state.select(Some(self.len() - 1));
    }

    pub fn rename_selected(&mut self, title: impl Into<String>) {
        if let Some(todo) = self.selected_mut() {
            todo.title = title.into();
        }
    }

    pub fn toggle_selected(&mut self) {
        if let Some(todo) = self.selected_mut() {
            todo.toggle();
        }
    }

    pub fn remove_selected(&mut self) -> Option<Todo> {
        let index = self.selected()?;
        let removed = self.items.remove(index);
        if self.is_empty() {
            self.state.select(None);
        } else if index >= self.len() {
            self.state.select(Some(self.len() - 1));
        }
        Some(removed)
    }

    pub fn split_mut(&mut self) -> (&[Todo], &mut ListState) {
        (&self.items, &mut self.state)
    }
}
