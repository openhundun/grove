use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Changes,
    Files,
    Terminal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistorySelection {
    Commit(usize),
    Unstaged,
    Staged,
    Comparison,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileSelection {
    Message,
    File(usize),
}

/// Owned by one workspace tab. Domain identity is retained separately from these view indices.
pub struct WorkspaceState {
    pub mode: Mode,
    pub repo: usize,
    pub worktree: usize,
    pub history: HistorySelection,
    pub file: FileSelection,
    pub fs_file: usize,
    pub split: bool,
    pub tree: bool,
    pub preview: bool,
    pub compare: bool,
    pub base: usize,
    pub col_ratio: f32,
    pub row_ratio: f32,
    pub collapsed_repo_set: HashSet<usize>,
    pub collapsed_dir_set: HashSet<String>,
    pub session: usize,
}
impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            mode: Mode::Changes,
            repo: 0,
            worktree: 0,
            history: HistorySelection::Unstaged,
            file: FileSelection::File(0),
            fs_file: 0,
            split: false,
            tree: false,
            preview: false,
            compare: false,
            base: 0,
            col_ratio: 0.2,
            row_ratio: 0.5,
            collapsed_repo_set: HashSet::new(),
            collapsed_dir_set: HashSet::new(),
            session: 0,
        }
    }
}
impl WorkspaceState {
    pub fn select_history(&mut self, history: HistorySelection) {
        self.history = history;
        self.mode = Mode::Changes;
        self.file = if matches!(history, HistorySelection::Commit(_)) {
            FileSelection::Message
        } else {
            FileSelection::File(0)
        };
    }
}
