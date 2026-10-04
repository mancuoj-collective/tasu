#![forbid(unsafe_code)]

//! tasu's internals: the aging domain, persistence, git sync and the TUI.

pub mod app;
pub mod cli;
pub mod command;
pub mod config;
pub mod domain;
pub mod store;
pub mod sync;
pub mod ui;
