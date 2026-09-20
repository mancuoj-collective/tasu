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
}
