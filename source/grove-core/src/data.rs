//! UI-independent read-only workspace data. Paths are identities; display labels are never lookup keys.
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct WorktreeSnapshot {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub locked: bool,
}
impl WorktreeSnapshot {
    pub fn label(&self) -> String {
        self.branch
            .clone()
            .unwrap_or_else(|| self.head.as_deref().map(|head| format!("detached {}", &head[..head.len().min(8)])).unwrap_or_else(|| "unborn".into()))
    }
}

#[derive(Clone, Debug)]
pub struct RepositorySnapshot {
    pub common_dir: PathBuf,
    pub path: PathBuf,
    pub name: String,
    pub workspace_root: bool,
    pub worktree_list: Vec<WorktreeSnapshot>,
}

#[derive(Clone, Debug)]
pub struct WorkspaceSnapshot {
    pub root: PathBuf,
    pub repository_list: Vec<RepositorySnapshot>,
    pub warning_list: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: PathBuf,
    pub old_path: Option<PathBuf>,
    pub status: char,
    pub additions: Option<usize>,
    pub deletions: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct CommitSummary {
    pub oid: String,
    pub parent_list: Vec<String>,
    pub subject: String,
    pub author: String,
    pub date: String,
}

#[derive(Clone, Debug)]
pub struct WorktreeData {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    pub unstaged_list: Vec<FileChange>,
    pub staged_list: Vec<FileChange>,
    pub commit_list: Vec<CommitSummary>,
    pub base_ref_list: Vec<String>,
    pub file_list: Vec<PathBuf>,
    pub warning_list: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Revision {
    Unstaged,
    Staged,
    Commit(String),
    Comparison(String),
}

#[derive(Clone, Debug)]
pub struct HistoryData {
    pub revision: Revision,
    pub message: String,
    pub file_list: Vec<FileChange>,
}

#[derive(Clone, Debug)]
pub enum FileContent {
    Text(Arc<str>),
    Binary,
    Unavailable(String),
}

#[derive(Clone, Debug)]
pub struct FileData {
    pub path: PathBuf,
    pub content: FileContent,
    pub patch: Option<Arc<str>>,
    pub notice: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ReadRequest {
    Discover { root: PathBuf },
    Worktree { root: PathBuf, path: PathBuf },
    History { root: PathBuf, path: PathBuf, revision: Revision },
    File { root: PathBuf, path: PathBuf, revision: Option<Revision>, file: PathBuf },
}

#[derive(Clone, Debug)]
pub enum ReadResult {
    Workspace(WorkspaceSnapshot),
    Worktree(WorktreeData),
    History(HistoryData),
    File(FileData),
}

#[derive(Clone, Debug)]
pub struct ReadEvent {
    pub workspace: usize,
    pub generation: u64,
    pub result: Result<ReadResult, String>,
}
