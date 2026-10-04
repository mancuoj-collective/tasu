use std::time::Duration;

use chrono::{DateTime, Local};
use crossterm::event::{Event, KeyCode, KeyEvent};
use tui_input::backend::crossterm::EventHandler;

use crate::domain::settle;

use super::action::{Action, Effect};
use super::model::{HistoryView, Mode, Model};

/// The toast lingers this long before a `Tick` clears it.
const TOAST_TTL: Duration = Duration::from_secs(5);

/// Turn an action into state changes and a list of effects. Pure with respect
/// to I/O: the caller owns the clock, the disk and the terminal.
pub fn update(model: &mut Model, action: Action, now: DateTime<Local>) -> Vec<Effect> {
    model.now = now;
    match action {
        Action::Tick => tick(model, now),
        Action::Reload(board) => {
            model.board = board;
            settle(&mut model.board, now);
            model.clamp_cursor();
            Vec::new()
        }
        Action::Key(key) => match model.ui.mode {
            Mode::Normal => normal(model, key, now),
            Mode::Add | Mode::Edit => editing(model, key, now),
            Mode::Completed => completed(model, key, now),
            Mode::Help => {
                model.ui.mode = Mode::Normal;
                Vec::new()
            }
        },
    }
}

fn tick(model: &mut Model, now: DateTime<Local>) -> Vec<Effect> {
    if let Some(toast) = &model.ui.toast
        && now
            .signed_duration_since(toast.born)
            .to_std()
            .unwrap_or_default()
            > TOAST_TTL
    {
        model.ui.toast = None;
    }
    if settle(&mut model.board, now) {
        model.clamp_cursor();
        vec![Effect::Save]
    } else {
        Vec::new()
    }
}

fn normal(model: &mut Model, key: KeyEvent, now: DateTime<Local>) -> Vec<Effect> {
    let mut effects = Vec::new();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => return vec![Effect::Quit],
        KeyCode::Char('j') | KeyCode::Down => model.cursor_down(),
        KeyCode::Char('k') | KeyCode::Up => model.cursor_up(),
        KeyCode::Char('g') | KeyCode::Home => model.cursor_first(),
        KeyCode::Char('G') | KeyCode::End => model.cursor_last(),
        KeyCode::Char(' ') | KeyCode::Enter => {
            if let Some(index) = model.selected()
                && model.board.complete(index, now)
            {
                model.clamp_cursor();
                effects.push(Effect::Save);
            }
        }
        KeyCode::Char('a') => {
            model.ui.input.reset();
            model.ui.mode = Mode::Add;
        }
        KeyCode::Char('e') => {
            if let Some(index) = model.selected()
                && let Some(task) = model.board.task(index)
            {
                model.ui.input = tui_input::Input::new(task.title.clone());
                model.ui.mode = Mode::Edit;
            }
        }
        KeyCode::Char('t') => {
            if selected_mutates(model, |board, index| board.pin_today(index, now)) {
                effects.push(Effect::Save);
            }
        }
        KeyCode::Char('h') | KeyCode::Left => model.cursor_bucket(-1),
        KeyCode::Char('l') | KeyCode::Right => model.cursor_bucket(1),
        KeyCode::Char('[') => {
            if selected_mutates(model, |board, index| board.move_bucket(index, -1, now)) {
                effects.push(Effect::Save);
            }
        }
        KeyCode::Char(']') => {
            if selected_mutates(model, |board, index| board.move_bucket(index, 1, now)) {
                effects.push(Effect::Save);
            }
        }
        KeyCode::Char('x') => {
            if selected_mutates(model, |board, index| board.archive(index, now)) {
                effects.push(Effect::Save);
            }
        }
        KeyCode::Char('c') => {
            model.ui.mode = Mode::Completed;
            model.ui.done_cursor = 0;
            model.ui.done_filter.reset();
            model.ui.history_view = HistoryView::Done;
        }
        KeyCode::Char('z') => {
            model.ui.later_expanded = !model.ui.later_expanded;
            model.clamp_cursor();
        }
        KeyCode::Char('?') => model.ui.mode = Mode::Help,
        _ => {}
    }
    effects
}

/// Apply `change` to the selected task, clamping the cursor if the row vanished.
fn selected_mutates(
    model: &mut Model,
    change: impl FnOnce(&mut crate::domain::Board, usize) -> bool,
) -> bool {
    let Some(index) = model.selected() else {
        return false;
    };
    if change(&mut model.board, index) {
        model.clamp_cursor();
        true
    } else {
        false
    }
}

fn editing(model: &mut Model, key: KeyEvent, now: DateTime<Local>) -> Vec<Effect> {
    match key.code {
        KeyCode::Enter => {
            let title = model.ui.input.value().trim().to_string();
            if title.is_empty() {
                return Vec::new();
            }
            match model.ui.mode {
                Mode::Add => {
                    model.board.add(title, now);
                    model.ui.cursor = 0;
                    model.set_toast("saved to today");
                }
                Mode::Edit => {
                    if let Some(index) = model.selected() {
                        model.board.rename(index, title);
                    }
                }
                _ => {}
            }
            model.ui.input.reset();
            model.ui.mode = Mode::Normal;
            model.clamp_cursor();
            vec![Effect::Save]
        }
        KeyCode::Esc => {
            model.ui.input.reset();
            model.ui.mode = Mode::Normal;
            Vec::new()
        }
        _ => {
            model.ui.input.handle_event(&Event::Key(key));
            Vec::new()
        }
    }
}

fn completed(model: &mut Model, key: KeyEvent, now: DateTime<Local>) -> Vec<Effect> {
    match key.code {
        KeyCode::Esc => {
            if model.ui.done_filter.value().is_empty() {
                model.ui.mode = Mode::Normal;
            } else {
                model.ui.done_filter.reset();
                model.ui.done_cursor = 0;
            }
            Vec::new()
        }
        KeyCode::Tab => {
            model.ui.history_view = match model.ui.history_view {
                HistoryView::Done => HistoryView::Dropped,
                HistoryView::Dropped => HistoryView::Done,
            };
            model.ui.done_cursor = 0;
            Vec::new()
        }
        KeyCode::Down => {
            if model.ui.done_cursor + 1 < model.history_items().len() {
                model.ui.done_cursor += 1;
            }
            Vec::new()
        }
        KeyCode::Up => {
            model.ui.done_cursor = model.ui.done_cursor.saturating_sub(1);
            Vec::new()
        }
        KeyCode::Enter => {
            let Some(&index) = model.history_items().get(model.ui.done_cursor) else {
                return Vec::new();
            };
            let restored = match model.ui.history_view {
                HistoryView::Done => model.board.restore(index, now),
                HistoryView::Dropped => model.board.unarchive(index, now),
            };
            if !restored {
                return Vec::new();
            }
            model.ui.mode = Mode::Normal;
            model.ui.done_filter.reset();
            model.clamp_cursor();
            vec![Effect::Save]
        }
        // Everything else edits the search field; typing filters the list.
        _ => {
            model.ui.done_filter.handle_event(&Event::Key(key));
            model.ui.done_cursor = 0;
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::domain::{Bucket, test_time::at};

    fn press(code: KeyCode) -> Action {
        Action::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn model() -> Model {
        Model::new(crate::domain::Board::new(), at(2026, 10, 5))
    }

    fn typ(model: &mut Model, action: Action) {
        update(model, action, at(2026, 10, 5));
    }

    #[test]
    fn add_records_into_today_and_persists() {
        let mut model = model();
        typ(&mut model, press(KeyCode::Char('a')));
        for c in "学 GPUI".chars() {
            typ(&mut model, press(KeyCode::Char(c)));
        }
        let effects = update(&mut model, press(KeyCode::Enter), at(2026, 10, 5));

        assert_eq!(effects, vec![Effect::Save]);
        assert_eq!(model.board.task(0).unwrap().title, "学 GPUI");
        assert_eq!(model.board.task(0).unwrap().bucket, Bucket::Today);
        assert!(model.ui.toast.is_some());
    }

    #[test]
    fn completing_hides_the_task_from_the_list() {
        let mut model = model();
        model.board.add("done me", at(2026, 10, 5));
        let effects = update(&mut model, press(KeyCode::Char(' ')), at(2026, 10, 5));
        assert_eq!(effects, vec![Effect::Save]);
        assert_eq!(model.selectable_len(), 0);
    }

    #[test]
    fn promote_and_demote_move_between_buckets() {
        let mut model = model();
        model.board.add("move me", at(2026, 10, 5));
        update(&mut model, press(KeyCode::Char(']')), at(2026, 10, 5));
        assert_eq!(model.board.task(0).unwrap().bucket, Bucket::Week);
        update(&mut model, press(KeyCode::Char('t')), at(2026, 10, 5));
        assert_eq!(model.board.task(0).unwrap().bucket, Bucket::Today);
    }

    #[test]
    fn quit_is_requested_by_effect() {
        let mut model = model();
        let effects = update(&mut model, press(KeyCode::Char('q')), at(2026, 10, 5));
        assert_eq!(effects, vec![Effect::Quit]);
    }

    #[test]
    fn tick_settles_and_saves_only_when_something_moved() {
        let mut model = model();
        model.board.add("stale", at(2026, 10, 5));
        assert!(
            update(&mut model, Action::Tick, at(2026, 10, 5)).is_empty(),
            "same day: no movement"
        );
        assert_eq!(
            update(&mut model, Action::Tick, at(2026, 10, 6)),
            vec![Effect::Save]
        );
        assert_eq!(model.board.task(0).unwrap().bucket, Bucket::Week);
    }

    #[test]
    fn h_and_l_move_the_cursor_between_buckets() {
        let mut model = model();
        model.board.add("today task", at(2026, 10, 5));
        model.board.add("later task", at(2026, 10, 5));
        model.board.move_bucket(1, 2, at(2026, 10, 5));

        update(&mut model, press(KeyCode::Char('l')), at(2026, 10, 5));
        assert_eq!(model.selected(), Some(1), "l should jump to Later");
        update(&mut model, press(KeyCode::Char('h')), at(2026, 10, 5));
        assert_eq!(model.selected(), Some(0), "h should jump back to Today");
    }

    #[test]
    fn later_collapses_then_expands() {
        use crate::app::Row;
        let mut model = model();
        for i in 0..7 {
            model.board.add(format!("task {i}"), at(2026, 10, 5));
        }
        for i in 0..7 {
            model.board.move_bucket(i, 2, at(2026, 10, 5));
        }

        assert_eq!(model.selectable_len(), Model::LATER_VISIBLE);
        assert!(model.rows().iter().any(|row| matches!(row, Row::Fold(2))));

        update(&mut model, press(KeyCode::Char('z')), at(2026, 10, 5));
        assert_eq!(model.selectable_len(), 7);
    }

    #[test]
    fn completed_search_filters_then_restores() {
        use crate::domain::TaskState;
        let mut model = model();
        model.board.add("买猫粮", at(2026, 10, 5));
        model.board.add("学 GPUI", at(2026, 10, 5));
        model.board.complete(0, at(2026, 10, 5));
        model.board.complete(1, at(2026, 10, 5));

        update(&mut model, press(KeyCode::Char('c')), at(2026, 10, 5));
        assert_eq!(model.history_items().len(), 2);
        for c in "gpui".chars() {
            update(&mut model, press(KeyCode::Char(c)), at(2026, 10, 5));
        }
        assert_eq!(model.history_items().len(), 1);

        let effects = update(&mut model, press(KeyCode::Enter), at(2026, 10, 5));
        assert_eq!(effects, vec![Effect::Save]);
        assert_eq!(model.board.task(1).unwrap().state, TaskState::Open);
    }

    #[test]
    fn dropped_view_lists_and_restores_archived_tasks() {
        use crate::domain::TaskState;
        let mut model = model();
        model.board.add("abandoned", at(2026, 10, 5));
        model.board.archive(0, at(2026, 10, 5));

        update(&mut model, press(KeyCode::Char('c')), at(2026, 10, 5));
        assert!(model.history_items().is_empty(), "done view is empty");

        update(&mut model, press(KeyCode::Tab), at(2026, 10, 5));
        assert_eq!(model.history_items().len(), 1);

        let effects = update(&mut model, press(KeyCode::Enter), at(2026, 10, 5));
        assert_eq!(effects, vec![Effect::Save]);
        assert_eq!(model.board.task(0).unwrap().state, TaskState::Open);
    }
}
