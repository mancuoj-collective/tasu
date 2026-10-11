//! The keys the footer and the help overlay advertise. They live in one file so
//! a key shown in the footer is always listed in the help, and a test holds
//! them to that.

use crate::app::Mode;

/// The footer's hint pairs `(key, label)` for a mode.
pub fn hints(mode: Mode) -> &'static [(&'static str, &'static str)] {
    match mode {
        Mode::Normal => &[
            ("?", "help"),
            ("q", "quit"),
            ("a", "add"),
            ("c", "history"),
            ("[]", "move"),
        ],
        Mode::Add | Mode::Edit => &[("enter", "save"), ("esc", "cancel")],
        Mode::Completed => &[
            ("\u{2190}\u{2192}", "switch"),
            ("enter", "restore"),
            ("\u{2191}\u{2193}", "move"),
            ("esc", "back"),
        ],
        Mode::Help => &[("\u{2191}\u{2193}", "scroll"), ("esc", "close")],
    }
}

/// The help overlay's rows `(keys, description)`. A `/` separates alternatives
/// that share one description.
pub const HELP: &[(&str, &str)] = &[
    ("j / k / \u{2193} / \u{2191}", "move"),
    ("h / l / \u{2190} / \u{2192}", "previous / next section"),
    ("g / G / home / end", "top / bottom"),
    ("space / enter", "done"),
    ("a", "add (to today)"),
    ("e", "edit title"),
    ("t", "move to today"),
    ("[]", "move between buckets"),
    ("x", "drop (archive)"),
    ("c", "history"),
    ("?", "this help"),
    ("q / esc", "quit"),
    ("ctrl+c", "quit anywhere"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_footer_key_in_normal_mode_is_listed_in_help() {
        for (key, _) in hints(Mode::Normal) {
            assert!(
                HELP.iter()
                    .any(|(keys, _)| keys.split(" / ").any(|listed| listed == *key)),
                "footer key {key:?} is missing from the help overlay"
            );
        }
    }
}
