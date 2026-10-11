//! Fixtures and render helpers shared by the integration test targets. Each
//! target uses a subset, so the module allows dead code (the usual pattern for
//! a shared test module).
#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Local, TimeZone};
use ratatui::{Terminal, backend::TestBackend};

use tasu::app::{Action, Model, update};
use tasu::ui::{self, Chrome, theme::Theme};

/// A fixed local 09:00 on October `day`, 2026, independent of the machine.
pub fn dt(day: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 10, day, 9, 0, 0)
        .single()
        .unwrap()
}

/// Draw one frame at `width`×`height` and return it as text, one line per row,
/// together with the help overlay's clamped scroll offset when help is open.
pub fn draw_text(
    model: &Model,
    width: u16,
    height: u16,
    data_path: &Path,
    remote: Option<&str>,
) -> (String, Option<usize>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let chrome = Chrome { data_path, remote };
    let mut overlay_scroll = None;
    terminal
        .draw(|frame| {
            overlay_scroll = ui::draw(frame, model, &Theme::DARK, &chrome);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let area = buffer.area();
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buffer.cell((x, y)).map_or(" ", |cell| cell.symbol()));
        }
        out.push('\n');
    }
    (out, overlay_scroll)
}

pub fn press(code: crossterm::event::KeyCode) -> Action {
    Action::Key(crossterm::event::KeyEvent::new(
        code,
        crossterm::event::KeyModifiers::NONE,
    ))
}

pub fn render(model: &mut Model, width: u16, height: u16) -> String {
    let (text, overlay_scroll) = draw_text(
        model,
        width,
        height,
        Path::new("/tmp/tasu/todos.json"),
        None,
    );
    if let Some(offset) = overlay_scroll {
        model.ui.overlay_scroll = offset;
    }
    text
}

pub fn type_str(model: &mut Model, text: &str, now: DateTime<Local>) {
    for c in text.chars() {
        update(model, press(crossterm::event::KeyCode::Char(c)), now);
    }
}

pub fn clone(remote: &Path, into: &Path) {
    let status = Command::new("git")
        .args(["clone", "--quiet"])
        .arg(remote)
        .arg(into)
        .status()
        .unwrap();
    assert!(status.success(), "clone failed");
}

/// Run git in `repo`, panicking on failure. The commit identity is fixed so the
/// test does not depend on the machine's global git config.
pub fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

pub fn commit_all(repo: &Path, message: &str) {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

/// A bare repository standing in for the remote, with `main` as its default
/// branch so every clone checks it out. Uses the real git binary end to end.
pub fn bare_remote(dir: &Path) -> std::path::PathBuf {
    let remote = dir.join("remote.git");
    assert!(
        Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&remote)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["symbolic-ref", "HEAD", "refs/heads/main"])
            .status()
            .unwrap()
            .success()
    );
    remote
}

// --- the real binary, end to end -------------------------------------------

/// Run the built `tasu` binary against an isolated data dir and config home, so
/// the developer's own settings never leak into the test.
pub fn tasu(data: &Path, config_home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tasu"));
    cmd.env("TASU_DATA", data);
    cmd.env("XDG_CONFIG_HOME", config_home);
    cmd.env_remove("TASU_REMOTE");
    cmd
}
