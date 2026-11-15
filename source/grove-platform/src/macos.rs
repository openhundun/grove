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
pub const MODE_KEY_LIST: [&str; 4] = ["cmd-1", "cmd-2", "cmd-3", "cmd-4"];
pub const MODE_LABEL_LIST: [&str; 4] = ["Changes and diff · ⌘1", "Files and preview · ⌘2", "Terminal sessions · ⌘3", "Repositories and commits · ⌘4"];

#[derive(Default)]
#[repr(C)]
struct MachTimeValue {
    seconds: i32,
    microseconds: i32,
}

// Matches mach_task_basic_info in the macOS SDK's mach/task_info.h, including its pack(4).
#[derive(Default)]
#[repr(C, packed(4))]
struct MachTaskBasicInfo {
    virtual_size: u64,
    resident_size: u64,
    resident_size_max: u64,
    user_time: MachTimeValue,
    system_time: MachTimeValue,
    policy: i32,
    suspend_count: i32,
}

unsafe extern "C" {
    static mach_task_self_: u32;
    fn task_info(target_task: u32, flavor: u32, task_info_out: *mut i32, task_info_out_count: *mut u32) -> i32;
}

/// Returns the current process's resident memory in bytes, not its historical maximum.
pub fn resident_memory_bytes() -> Option<u64> {
    const MACH_TASK_BASIC_INFO: u32 = 20;
    const KERN_SUCCESS: i32 = 0;
    const INFO_COUNT: u32 = (std::mem::size_of::<MachTaskBasicInfo>() / std::mem::size_of::<u32>()) as u32;
    let mut info = MachTaskBasicInfo::default();
    let mut count = INFO_COUNT;
    // SAFETY: libSystem initializes mach_task_self_ for this process. The SDK-compatible
    // structure is writable for count 32-bit words, and task_info does not retain either pointer.
    let result = unsafe { task_info(mach_task_self_, MACH_TASK_BASIC_INFO, std::ptr::from_mut(&mut info).cast(), &mut count) };
    if result == KERN_SUCCESS && count == INFO_COUNT { Some(info.resident_size) } else { None }
}

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
