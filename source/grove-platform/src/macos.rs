use crate::ShellLaunch;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

pub const SYSTEM_FONT: &str = ".AppleSystemUIFont";
pub const MONO_FONT: &str = "Menlo";
pub const QUIT_KEY: &str = "cmd-q";
pub const MODE_KEY_LIST: [&str; 3] = ["cmd-1", "cmd-2", "cmd-3"];
pub const MODE_LABEL_LIST: [&str; 3] = ["Changes and diff · ⌘1", "Files and preview · ⌘2", "Terminal sessions · ⌘3"];

pub fn read_only_git_command(path: &Path) -> Command {
    let mut command = Command::new("/usr/bin/sandbox-exec");
    command.args([
        "-p",
        "(version 1)(allow default)(deny file-write*)(allow file-write* (literal \"/dev/null\"))(deny network*)",
        "/usr/bin/git",
    ]);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_COUNT",
        "GIT_EXTERNAL_DIFF",
    ] {
        command.env_remove(key);
    }
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_PAGER", "cat")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("LC_ALL", "C")
        .arg("--no-optional-locks")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "core.quotePath=false",
            "-c",
            "color.ui=false",
            "-c",
            "diff.ignoreSubmodules=none",
        ])
        .arg("-C")
        .arg(path);
    command
}

pub fn path_from_git_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(bytes.to_vec()))
}

pub fn git_path_bytes(path: &Path) -> &[u8] {
    path.as_os_str().as_bytes()
}

pub fn shell_launch(cwd: &Path) -> ShellLaunch {
    ShellLaunch {
        program: std::env::var_os("SHELL").filter(|shell| !shell.is_empty()).unwrap_or_else(|| OsStr::new("/bin/zsh").to_owned()),
        argument_list: vec![OsString::from("-l"), OsString::from("-i")],
        cwd: cwd.to_path_buf(),
    }
}
