//! Native process, path, shell, and UI conventions. Only macOS is implemented.
use std::ffi::OsString;
use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
compile_error!("Grove currently supports macOS only; implement a native platform module before enabling another target.");

/// Launch settings only; a session's cwd does not restrict later shell commands.
pub struct ShellLaunch {
    pub program: OsString,
    pub argument_list: Vec<OsString>,
    pub cwd: PathBuf,
}
