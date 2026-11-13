//! Ely's terminal for GPUI: a pseudo-terminal drawn cell by cell, colored output, command blocks, logs and processes.
#![deny(missing_docs)]

use ely_core::buttons;
use ely_core::i18n;
use ely_core::menus;
use ely_core::motion;
use ely_core::navigation;
use ely_core::primitives;
use ely_core::tables;
use ely_core::theme;
use ely_core::typography;
#[cfg(not(target_family = "wasm"))]
use ely_editor as editor;

mod ansi;
mod block;
mod colors;
#[cfg(not(target_family = "wasm"))]
mod frame;
mod history;
#[cfg(not(target_family = "wasm"))]
mod input;
#[cfg(not(target_family = "wasm"))]
mod keys;
mod links;
mod logs;
#[cfg(not(target_family = "wasm"))]
mod paint;
mod processes;
#[cfg(not(target_family = "wasm"))]
mod pty;
#[cfg(not(target_family = "wasm"))]
mod search;
mod shells;
#[cfg(all(test, feature = "test-support"))]
mod tests;
#[cfg(not(target_family = "wasm"))]
mod view;

pub use ansi::AnsiText;
pub use block::CommandBlock;
pub use block::CommandState;
pub use history::CommandHistory;
pub use history::PastCommand;
#[cfg(not(target_family = "wasm"))]
pub use input::bind_keys;
pub use links::Target;
pub use links::links;
pub use logs::LogLevel;
pub use logs::LogLine;
pub use logs::LogViewer;
pub use processes::Process;
pub use processes::ProcessList;
#[cfg(not(target_family = "wasm"))]
pub use pty::Launch;
pub use shells::ShellChoice;
pub use shells::ShellSelector;
pub use shells::TerminalToolbar;
#[cfg(not(target_family = "wasm"))]
pub use view::Terminal;
#[cfg(not(target_family = "wasm"))]
pub use view::TerminalEvent;
