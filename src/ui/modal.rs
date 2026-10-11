use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Padding, Paragraph},
};

use crate::app::{HistoryView, Mode, Model};

use tui_input::Input;

use super::{Chrome, components, help, scroll_offset, theme::Theme};

/// Render the active modal. Returns the help overlay's clamped scroll offset
/// (only when help is open), so the caller stores it and the renderer never has
/// to mutate the model.
pub fn draw(f: &mut Frame, model: &Model, theme: &Theme, chrome: &Chrome) -> Option<usize> {
    match model.ui.mode {
        Mode::Add | Mode::Edit => {
            input_modal(f, model, theme);
            None
        }
        Mode::Completed => {
            completed_modal(f, model, theme);
            None
        }
        Mode::Help => Some(help::draw(f, model, theme, chrome)),
        Mode::Normal => None,
    }
}

fn input_modal(f: &mut Frame, model: &Model, theme: &Theme) {
    let area = centered(f.area(), 50, 3);
    f.render_widget(Clear, area);

    let title = if model.ui.mode == Mode::Add {
        " add "
    } else {
        " edit "
    };
    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(Padding::horizontal(1))
        .title(Span::styled(title, theme.accent()));
    input_field(
        f,
        area,
        block,
        &model.ui.input,
        model.ui.input.value(),
        theme.text(),
    );
}

fn completed_modal(f: &mut Frame, model: &Model, theme: &Theme) {
    let items = model.history_items();
    // A fixed default height so switching tabs never resizes the box.
    let height = 18u16.min(f.area().height.saturating_sub(2));
    let area = centered(f.area(), 60, height);
    f.render_widget(Clear, area);

    let block = Block::bordered()
        .border_style(theme.accent())
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [tabs_area, search_area, list_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Min(1),
    ])
    .areas(inner);

    f.render_widget(Paragraph::new(history_tabs(model, theme)), tabs_area);
    search_box(f, model, theme, search_area);

    if items.is_empty() {
        f.render_widget(
            Paragraph::new("\u{00af}\\_(._.)_/\u{00af}")
                .style(theme.muted())
                .centered(),
            list_area,
        );
        return;
    }

    let height = list_area.height as usize;
    let offset = scroll_offset(model.ui.done_cursor, items.len(), height);
    let lines: Vec<Line> = items
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(position, &index)| {
            let Some(task) = model.board.task(index) else {
                return Line::default();
            };
            components::selectable_row(
                task,
                theme,
                list_area.width,
                position == model.ui.done_cursor,
            )
        })
        .collect();
    f.render_widget(Paragraph::new(lines), list_area);
}

fn history_tabs(model: &Model, theme: &Theme) -> Line<'static> {
    let tab = |label: &'static str, active: bool| {
        let style = if active {
            Style::new().bg(theme.accent).fg(theme.sel_bg).bold()
        } else {
            theme.muted()
        };
        Span::styled(format!(" {label} "), style)
    };
    Line::from(vec![
        tab("DONE", model.ui.history_view == HistoryView::Done),
        Span::raw(" "),
        tab("DROPPED", model.ui.history_view == HistoryView::Dropped),
    ])
}

/// A bordered search field with a real cursor, fixed above the scrolling list.
fn search_box(f: &mut Frame, model: &Model, theme: &Theme, area: Rect) {
    let block = Block::bordered()
        .border_style(theme.disabled())
        .padding(Padding::horizontal(1));
    let input = &model.ui.done_filter;
    let (text, style) = if input.value().is_empty() {
        ("search", theme.muted())
    } else {
        (input.value(), theme.text())
    };
    input_field(f, area, block, input, text, style);
}

/// Draw a bordered, single-line input through `block` and put the terminal
/// cursor at its caret. `text` is what to show — the field's value, or a muted
/// placeholder when it is empty — and the caret always comes from `input`, so
/// the two can never disagree.
fn input_field(
    f: &mut Frame,
    area: Rect,
    block: Block<'static>,
    input: &Input,
    text: &str,
    style: Style,
) {
    let inner = block.inner(area);
    let scroll = input.visual_scroll(inner.width as usize);
    f.render_widget(
        Paragraph::new(text)
            .style(style)
            .scroll((0, scroll as u16))
            .block(block),
        area,
    );
    let x = inner.x + (input.visual_cursor().max(scroll) - scroll) as u16;
    f.set_cursor_position((x, inner.y));
}

pub(super) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    // Keep one blank row above and below so a tall modal never touches the
    // screen edges.
    let height = height.min(area.height.saturating_sub(2));
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}
