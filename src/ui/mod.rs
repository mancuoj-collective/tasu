pub mod footer;
pub mod list;
pub mod modal;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::app::App;

const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 10;

pub fn draw(f: &mut Frame, app: &mut App) {
    let t = app.theme;
    f.render_widget(Block::new().style(t.root()), f.area());

    if f.area().width < MIN_WIDTH || f.area().height < MIN_HEIGHT {
        too_small(f, app);
        return;
    }

    let [list_area, footer_area] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
        .areas(f.area().inner(Margin::new(0, 1)));

    list::draw(f, app, list_area);
    footer::draw(f, app, footer_area);
    modal::draw(f, app);
}

fn too_small(f: &mut Frame, app: &App) {
    let t = app.theme;
    let (key_style, hint_style) = t.key_hint();
    let lines = vec![
        Line::from(Span::styled(
            format!("enlarge · min {MIN_WIDTH}x{MIN_HEIGHT}"),
            t.muted(),
        )),
        Line::from(vec![
            Span::styled("q", key_style),
            Span::styled(" to quit", hint_style),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), f.area());
}
