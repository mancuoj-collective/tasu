pub mod footer;
pub mod list;
pub mod modal;

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    text::{Line, Span},
    widgets::{Block, BorderType, Padding, Paragraph},
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

    let [_, body, footer, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let done = app.todos.iter().filter(|todo| todo.done).count();
    let title = Line::from(vec![
        Span::styled(format!(" {} ", env!("CARGO_PKG_NAME")), t.title(true)),
        Span::styled(format!("· {done}/{} ", app.todos.len()), t.muted()),
    ]);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(t.border(false))
        .padding(Padding::new(1, 1, 1, 1))
        .title(title);

    let inner = block.inner(body);
    f.render_widget(block, body);
    list::draw(f, app, inner);

    footer::draw(f, app, footer);
    modal::draw(f, app);
}

fn too_small(f: &mut Frame, app: &App) {
    let t = app.theme;
    let lines = vec![
        Line::from(Span::styled(
            format!("enlarge · min {MIN_WIDTH}x{MIN_HEIGHT}"),
            t.muted(),
        )),
        Line::from(vec![
            Span::styled("q", t.key_hint().0),
            Span::styled(" to quit", t.key_hint().1),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), f.area());
}
