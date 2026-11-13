use crate::settings::DiffContextMode;
use crate::settings::DiffViewMode;
use crate::settings::PanelViewMode;
use crate::settings::Settings;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Changes,
    Files,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockMode {
    History,
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

pub struct ListPanelState {
    pub tree: bool,
    /// 已规范为小写并去除首尾空白；仅影响显示，不改变选择或折叠状态。
    pub query: String,
    pub collapsed_path_set: HashSet<PathBuf>,
}
impl ListPanelState {
    pub fn new(tree: bool) -> Self {
        Self { tree, query: String::new(), collapsed_path_set: HashSet::new() }
    }

    pub fn matches_query(&self, text: &str) -> bool {
        if self.query.is_empty() {
            return true;
        }
        let mut text_iter = text.chars().flat_map(char::to_lowercase);
        while let Some(first) = text_iter.next() {
            let mut query_iter = self.query.chars();
            if query_iter.next() != Some(first) {
                continue;
            }
            let mut suffix_iter = text_iter.clone();
            if query_iter.all(|expected| suffix_iter.next() == Some(expected)) {
                return true;
            }
        }
        false
    }

    pub fn toggle_path(&mut self, path: PathBuf) {
        if !self.collapsed_path_set.remove(&path) {
            self.collapsed_path_set.insert(path);
        }
    }
}

/// Owned by one workspace tab, including its upper editor and independent lower dock.
/// Domain identity is retained separately from these view indices.
pub struct WorkspaceState {
    pub mode: Mode,
    pub dock_mode: DockMode,
    pub dock_visible: bool,
    pub repo: usize,
    pub worktree: usize,
    pub history: HistorySelection,
    pub file: FileSelection,
    pub fs_file: usize,
    pub split: bool,
    pub diff_full: bool,
    pub repository_panel: ListPanelState,
    pub changes_panel: ListPanelState,
    pub files_panel: ListPanelState,
    pub preview: bool,
    pub compare: bool,
    pub base: usize,
    pub col_ratio: f32,
    pub row_ratio: f32,
    pub session: usize,
}
impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            mode: Mode::Changes,
            dock_mode: DockMode::History,
            dock_visible: true,
            repo: 0,
            worktree: 0,
            history: HistorySelection::Unstaged,
            file: FileSelection::File(0),
            fs_file: 0,
            split: false,
            diff_full: false,
            repository_panel: ListPanelState::new(true),
            changes_panel: ListPanelState::new(false),
            files_panel: ListPanelState::new(true),
            preview: false,
            compare: false,
            base: 0,
            col_ratio: 0.2,
            row_ratio: 0.5,
            session: 0,
        }
    }
}
impl WorkspaceState {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            split: settings.diff_panel.view_mode == DiffViewMode::Split,
            diff_full: settings.diff_panel.context_mode == DiffContextMode::Full,
            repository_panel: ListPanelState::new(settings.repository_panel.view_mode == PanelViewMode::Tree),
            changes_panel: ListPanelState::new(settings.changes_panel.view_mode == PanelViewMode::Tree),
            files_panel: ListPanelState::new(settings.file_panel.view_mode == PanelViewMode::Tree),
            ..Self::default()
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    pub fn toggle_dock(&mut self, mode: DockMode) {
        if self.dock_mode == mode && self.dock_visible {
            self.dock_visible = false;
        } else {
            self.dock_mode = mode;
            self.dock_visible = true;
        }
    }

    pub fn toggle_dock_visibility(&mut self) {
        self.dock_visible = !self.dock_visible;
    }

    pub fn terminal_visible(&self) -> bool {
        self.dock_visible && self.dock_mode == DockMode::Terminal
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_view_modes_seed_isolated_workspace_tabs() {
        let mut settings = Settings::default();
        settings.repository_panel.view_mode = PanelViewMode::Flat;
        settings.changes_panel.view_mode = PanelViewMode::Tree;
        settings.file_panel.view_mode = PanelViewMode::Flat;
        settings.diff_panel.view_mode = DiffViewMode::Split;
        settings.diff_panel.context_mode = DiffContextMode::Full;
        let mut first = WorkspaceState::from_settings(&settings);
        let second = WorkspaceState::from_settings(&settings);
        assert!(!first.repository_panel.tree);
        assert!(first.changes_panel.tree);
        assert!(!first.files_panel.tree);
        assert!(first.split && first.diff_full);

        first.set_mode(Mode::Files);
        first.toggle_dock(DockMode::Terminal);
        first.toggle_dock_visibility();
        first.row_ratio = 0.7;
        first.col_ratio = 0.3;
        first.session = 2;
        first.select_history(HistorySelection::Commit(3));
        assert_eq!(second.mode, Mode::Changes);
        assert_eq!(second.dock_mode, DockMode::History);
        assert!(second.dock_visible);
        assert!(!second.terminal_visible());
        assert_eq!(second.row_ratio, 0.5);
        assert_eq!(second.col_ratio, 0.2);
        assert_eq!(second.session, 0);
        assert_eq!(second.history, HistorySelection::Unstaged);
        assert_eq!(second.file, FileSelection::File(0));

        first.diff_full = false;
        first.split = false;
        first.repository_panel.tree = true;
        first.changes_panel.toggle_path(PathBuf::from("src/nested"));
        assert!(second.split && second.diff_full);
        assert!(!second.repository_panel.tree);
        assert!(second.changes_panel.collapsed_path_set.is_empty());
        assert_eq!(settings.diff_panel.context_mode, DiffContextMode::Full);
    }

    #[test]
    fn upper_mode_switches_preserve_the_dock() {
        let mut state = WorkspaceState::default();
        state.toggle_dock(DockMode::Terminal);
        state.set_mode(Mode::Files);
        assert_eq!(state.mode, Mode::Files);
        assert_eq!(state.dock_mode, DockMode::Terminal);
        assert!(state.terminal_visible());

        state.toggle_dock_visibility();
        state.set_mode(Mode::Changes);
        assert_eq!(state.mode, Mode::Changes);
        assert_eq!(state.dock_mode, DockMode::Terminal);
        assert!(!state.dock_visible);
        assert!(!state.terminal_visible());
    }

    #[test]
    fn dock_switches_and_visibility_preserve_editor_and_selections() {
        let mut state = WorkspaceState {
            mode: Mode::Files,
            row_ratio: 0.7,
            col_ratio: 0.3,
            session: 2,
            history: HistorySelection::Commit(3),
            file: FileSelection::File(4),
            fs_file: 5,
            ..WorkspaceState::default()
        };
        state.toggle_dock(DockMode::History);
        assert!(!state.dock_visible);
        state.toggle_dock(DockMode::History);
        assert!(state.dock_visible);
        state.toggle_dock(DockMode::Terminal);
        assert!(state.terminal_visible());
        state.toggle_dock(DockMode::Terminal);
        assert!(!state.terminal_visible());
        state.toggle_dock_visibility();
        assert!(state.terminal_visible());
        state.toggle_dock_visibility();
        state.toggle_dock(DockMode::History);
        assert_eq!(state.dock_mode, DockMode::History);
        assert!(state.dock_visible);
        assert!(!state.terminal_visible());
        assert_eq!(state.mode, Mode::Files);
        assert_eq!(state.row_ratio, 0.7);
        assert_eq!(state.col_ratio, 0.3);
        assert_eq!(state.session, 2);
        assert_eq!(state.history, HistorySelection::Commit(3));
        assert_eq!(state.file, FileSelection::File(4));
        assert_eq!(state.fs_file, 5);
    }

    #[test]
    fn history_selection_changes_the_editor_without_changing_the_dock() {
        let mut state = WorkspaceState::default();
        state.toggle_dock(DockMode::Terminal);
        state.set_mode(Mode::Files);
        state.select_history(HistorySelection::Commit(3));
        assert_eq!(state.mode, Mode::Changes);
        assert_eq!(state.history, HistorySelection::Commit(3));
        assert_eq!(state.file, FileSelection::Message);
        assert!(state.terminal_visible());

        state.toggle_dock_visibility();
        state.set_mode(Mode::Files);
        state.select_history(HistorySelection::Unstaged);
        assert_eq!(state.mode, Mode::Changes);
        assert_eq!(state.history, HistorySelection::Unstaged);
        assert_eq!(state.file, FileSelection::File(0));
        assert_eq!(state.dock_mode, DockMode::Terminal);
        assert!(!state.dock_visible);
    }
}
