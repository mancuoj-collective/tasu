pub mod footer;
pub mod header;
pub mod list;
pub mod modal;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::app::App;

pub fn draw(f: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    header::draw(f, header);
    list::draw(f, app, body);
    footer::draw(f, app, footer);
    modal::draw(f, app);
}
