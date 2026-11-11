//! Filesystem adapter: discovery and bounded reads only; no mutation API exists.
use grove_core::data::FileContent;
use std::fs;
use std::io::Read;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;

pub fn within(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let path = path.canonicalize().map_err(|error| format!("{}: {error}", path.display()))?;
    if !path.starts_with(&root) {
        return Err("Path is outside this workspace".into());
    }
    Ok(path)
}

pub fn relative(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err("Expected a repository-relative file path".into());
    }
    Ok(())
}

pub fn read_file(root: &Path, worktree: &Path, file: &Path) -> FileContent {
    if let Err(error) = relative(file) {
        return FileContent::Unavailable(error);
    }
    let path = match within(root, &worktree.join(file)) {
        Ok(path) => path,
        Err(error) => return FileContent::Unavailable(error),
    };
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => return FileContent::Unavailable(error.to_string()),
    };
    if !metadata.is_file() {
        return FileContent::Unavailable("Not a regular file; directory and submodule contents are not opened as text".into());
    }
    if metadata.len() > MAX_TEXT_BYTES {
        return FileContent::Unavailable("File exceeds the 2 MiB text preview limit".into());
    }
    match fs::File::open(path).and_then(|file| {
        let mut byte_list = Vec::new();
        file.take(MAX_TEXT_BYTES + 1).read_to_end(&mut byte_list)?;
        Ok(byte_list)
    }) {
        Ok(byte_list) => content(byte_list),
        Err(error) => FileContent::Unavailable(error.to_string()),
    }
}

pub fn content(byte_list: Vec<u8>) -> FileContent {
    if byte_list.len() as u64 > MAX_TEXT_BYTES {
        return FileContent::Unavailable("Content exceeds the 2 MiB text preview limit".into());
    }
    if byte_list.contains(&0) {
        return FileContent::Binary;
    }
    match String::from_utf8(byte_list) {
        Ok(text) => FileContent::Text(Arc::from(text)),
        Err(_) => FileContent::Binary,
    }
}

pub fn repository_candidates(root: &Path) -> Result<(Vec<PathBuf>, Vec<String>), String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let mut candidate_list = Vec::new();
    let mut warning_list = Vec::new();
    let mut pending_list = vec![root.clone()];
    while let Some(path) = pending_list.pop() {
        let is_repository = path.join(".git").exists();
        if is_repository {
            candidate_list.push(path.clone());
        }
        if is_repository && path != root {
            continue;
        }
        let entry_list = match fs::read_dir(&path) {
            Ok(entry_list) => entry_list,
            Err(error) => {
                warning_list.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        for entry in entry_list {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warning_list.push(error.to_string());
                    continue;
                }
            };
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(_) => continue,
            };
            if !kind.is_dir() || kind.is_symlink() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if matches!(
                name.as_ref(),
                ".git" | "node_modules" | "target" | "vendor" | ".venv" | "venv" | "build" | "dist" | ".idea" | ".cache" | "__pycache__"
            ) {
                continue;
            }
            if name.starts_with('.') && !matches!(name.as_ref(), ".agents" | ".worktrees" | ".worktree") {
                continue;
            }
            pending_list.push(entry.path());
        }
    }
    candidate_list.sort();
    Ok((candidate_list, warning_list))
}
