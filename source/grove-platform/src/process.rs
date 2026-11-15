//! Read the executable name for a live macOS process without launching a helper.
use std::ffi::c_void;
use std::io;
use std::path::Path;

#[link(name = "proc")]
unsafe extern "C" {
    fn proc_pidpath(pid: i32, buffer: *mut c_void, size: u32) -> i32;
    fn proc_name(pid: i32, buffer: *mut c_void, size: u32) -> i32;
}

pub fn process_name(pid: u32) -> io::Result<String> {
    let pid = i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid process ID"))?;
    let mut path = [0_u8; 4096];
    // SAFETY: libproc writes at most the supplied length into this owned byte buffer.
    let length = unsafe { proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
    if length > 0 {
        let end = path.iter().position(|byte| *byte == 0).unwrap_or(path.len());
        if let Ok(path) = std::str::from_utf8(&path[..end])
            && let Some(name) = Path::new(path).file_name().and_then(|name| name.to_str()).filter(|name| !name.is_empty())
        {
            return Ok(name.to_owned());
        }
    }
    let mut name = [0_u8; 256];
    // SAFETY: the process name is copied into this buffer within the provided bound.
    let length = unsafe { proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32) };
    if length <= 0 {
        return Err(io::Error::last_os_error());
    }
    let end = name.iter().position(|byte| *byte == 0).unwrap_or(name.len());
    let name = String::from_utf8_lossy(&name[..end]).into_owned();
    if name.is_empty() {
        Err(io::Error::new(io::ErrorKind::NotFound, "process has no executable name"))
    } else {
        Ok(name)
    }
}
