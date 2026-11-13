use crate::icon::view as icon_view;
use crate::icon::{self};
use crate::settings::DiffContextMode;
use crate::settings::DiffViewMode;
use crate::settings::FontTarget;
use crate::settings::PanelViewMode;
use crate::settings::Typography;
use crate::terminal_session::Session;
use crate::terminal_session::SessionKey;
use crate::workspace_state::DockMode;
use crate::workspace_state::FileSelection;
use crate::workspace_state::HistorySelection;
use crate::workspace_state::Mode;
use crate::workspace_state::WorkspaceState;
use ely_core::forms::InputEvent;
use ely_core::forms::TextInput;
use ely_core::primitives::FocusScope;
use ely_editor::CodeEditor;
use ely_editor::LineNumbers;
use ely_git::DiffLayout;
use ely_git::DiffScrollHandle;
use ely_git::DiffViewer;
use ely_git::Stretch;
use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt as _;
use gpui_kit::base::StyledExt as _;
use gpui_kit::base::h_flex;
use gpui_kit::base::v_flex;
use gpui_kit::component::Selectable;
use gpui_kit::component::Sizable;
use gpui_kit::component::TitleBar;
use gpui_kit::component::button::Button;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::list::ListItem;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use grove_core::data::*;
use grove_runtime::bus;
use std::cell::Cell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

mod commit;
mod detail;
mod file;
mod markdown;
mod markdown_theme;
mod repository;
mod settings;
mod terminal;
mod workspace;

const BG: u32 = 0xffffff;
const SURFACE: u32 = 0xf5f5f7;
const FG: u32 = 0x1d1d1f;
const SECONDARY: u32 = 0x424245;
const MUTED: u32 = 0x6e6e73;
const META: u32 = 0x86868b;
const BORDER: u32 = 0xe8e8ed;
const ACCENT: u32 = 0x0071e3;
const SELECT: u32 = 0xeaf2fd;
const TITLE_HEIGHT: f32 = 34.;
const RAIL_WIDTH: f32 = 28.;
const STATUS_HEIGHT: f32 = 28.;
const SESSION_LIST: [&str; 3] = ["Workspace", "Worktree", "Shell 2"];

actions!(grove, [ChangesMode, FilesMode, TerminalMode, HistoryMode, ToggleDock]);
#[derive(Clone, Copy)]
enum DragAxis {
    Column,
    Row,
}

#[derive(Clone, Copy)]
enum SearchPanel {
    Repositories,
    Changes,
    Files,
}

impl SearchPanel {
    fn index(self) -> usize {
        match self {
            Self::Repositories => 0,
            Self::Changes => 1,
            Self::Files => 2,
        }
    }
}

#[derive(Clone, Copy)]
enum RefreshTarget {
    Repositories,
    Worktree,
    History,
    File,
    Terminal,
}

type FileKey = (Option<Revision>, PathBuf);
struct FileView {
    data: FileData,
    editor: Option<Entity<CodeEditor>>,
    diff: Option<Rc<Vec<Stretch>>>,
    diff_open: HashSet<usize>,
    diff_scroll: DiffScrollHandle,
    error: Option<String>,
    hydrated: bool,
}
struct WorkspaceView {
    // A tab keeps its identity across Vec removals; a reopened root never reuses an old event target.
    id: usize,
    root: PathBuf,
    state: WorkspaceState,
    snapshot: Option<WorkspaceSnapshot>,
    worktree_data: Option<WorktreeData>,
    graph_list: Vec<grove_core::commit_graph::GraphRow>,
    history_data: Option<HistoryData>,
    files_tree: file::FileTree,
    changes_tree: file::FileTree,
    file_map: HashMap<FileKey, FileView>,
    file_key: Option<FileKey>,
    pending_file: bool,
    message_editor: Option<Entity<CodeEditor>>,
    selected_worktree: Option<PathBuf>,
    selected_commit: Option<String>,
    selected_file: Option<PathBuf>,
    selected_fs_file: Option<PathBuf>,
    generation: u64,
    loading: bool,
    error: Option<String>,
    task: Option<Task<()>>,
    repo_scroll: ScrollHandle,
    commit_scroll: ScrollHandle,
    file_scroll: ScrollHandle,
    search_input_list: [Option<Entity<TextInput>>; 3],
    search_subscription_list: Vec<Subscription>,
    session_map: HashMap<SessionKey, Session>,
    terminal_focus: bool,
}
impl WorkspaceView {
    fn new(root: PathBuf, next_id: &mut usize, settings: &crate::settings::Settings) -> Self {
        let id = *next_id;
        *next_id = id.checked_add(1).expect("workspace identity exhausted");
        Self {
            id,
            root,
            state: WorkspaceState::from_settings(settings),
            snapshot: None,
            worktree_data: None,
            graph_list: Vec::new(),
            history_data: None,
            files_tree: file::FileTree::default(),
            changes_tree: file::FileTree::default(),
            file_map: HashMap::new(),
            file_key: None,
            pending_file: false,
            message_editor: None,
            selected_worktree: None,
            selected_commit: None,
            selected_file: None,
            selected_fs_file: None,
            generation: 0,
            loading: false,
            error: None,
            task: None,
            repo_scroll: ScrollHandle::new(),
            commit_scroll: ScrollHandle::new(),
            file_scroll: ScrollHandle::new(),
            search_input_list: [None, None, None],
            search_subscription_list: Vec::new(),
            session_map: HashMap::new(),
            terminal_focus: false,
        }
    }
    fn worktree(&self) -> Option<&WorktreeSnapshot> {
        self.snapshot.as_ref()?.repository_list.get(self.state.repo)?.worktree_list.get(self.state.worktree)
    }
    fn revision(&self) -> Option<Revision> {
        match self.state.history {
            HistorySelection::Unstaged => Some(Revision::Unstaged),
            HistorySelection::Staged => Some(Revision::Staged),
            HistorySelection::Commit(index) => Some(Revision::Commit(self.worktree_data.as_ref()?.commit_list.get(index)?.oid.clone())),
            HistorySelection::Comparison => Some(Revision::Comparison(self.worktree_data.as_ref()?.base_ref_list.get(self.state.base)?.clone())),
        }
    }
    fn session_key(&self, slot: usize) -> SessionKey {
        SessionKey {
            cwd: if slot == 0 {
                self.root.clone()
            } else {
                self.selected_worktree.clone().unwrap_or_else(|| self.root.clone())
            },
            slot,
        }
    }
}

fn workspace_index(workspace_list: &[WorkspaceView], id: usize) -> Option<usize> {
    workspace_list.iter().position(|view| view.id == id)
}

fn read_event_index(workspace_list: &[WorkspaceView], event: &ReadEvent) -> Option<usize> {
    workspace_index(workspace_list, event.workspace).filter(|&index| workspace_list[index].generation == event.generation)
}

pub struct Grove {
    workspace_list: Vec<WorkspaceView>,
    active: usize,
    next_workspace_id: usize,
    focus: FocusHandle,
    settings_focus: FocusHandle,
    settings_return_focus: Option<FocusHandle>,
    body_bounds: Rc<Cell<Bounds<Pixels>>>,
    drag_axis: Option<DragAxis>,
    settings: crate::settings::Settings,
    settings_panel: Entity<settings::SettingsPanel>,
    _settings_subscription: Subscription,
    settings_open: bool,
}
impl Grove {
    pub fn new(root_list: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let (settings, error) = crate::settings::Settings::load(cx);
        let settings_panel = cx.new(|cx| settings::SettingsPanel::new(&settings, error, window, cx));
        let settings_subscription = cx.subscribe(&settings_panel, |this, _, event: &settings::SettingsChange, cx| match event {
            settings::SettingsChange::Typography { target, value } => this.set_typography(*target, value.clone(), cx),
            settings::SettingsChange::Navigation { target, value } => {
                match target {
                    FontTarget::Repositories => this.settings.repository_panel = *value,
                    FontTarget::Changes => this.settings.changes_panel = *value,
                    FontTarget::Files => this.settings.file_panel = *value,
                    _ => return,
                }
                this.persist_settings(cx);
            }
        });
        let mut next_workspace_id = 0;
        let workspace_list = root_list.into_iter().map(|root| WorkspaceView::new(root, &mut next_workspace_id, &settings)).collect();
        let mut this = Self {
            workspace_list,
            active: 0,
            next_workspace_id,
            focus,
            settings_focus: cx.focus_handle(),
            settings_return_focus: None,
            body_bounds: Rc::new(Cell::new(Bounds::default())),
            drag_axis: None,
            settings,
            settings_panel,
            _settings_subscription: settings_subscription,
            settings_open: false,
        };
        this.apply_ui_theme(cx);
        for index in 0..this.workspace_list.len() {
            this.refresh(index, cx);
        }
        this
    }
    fn view(&self) -> &WorkspaceView {
        &self.workspace_list[self.active]
    }
    fn state(&self) -> &WorkspaceState {
        &self.view().state
    }
    fn state_mut(&mut self) -> &mut WorkspaceState {
        &mut self.workspace_list[self.active].state
    }

    fn set_typography(&mut self, target: FontTarget, value: Typography, cx: &mut Context<Self>) {
        self.settings.set_typography(target, value.clone());
        match target {
            FontTarget::Editor => {
                for view in &self.workspace_list {
                    if let Some(editor) = &view.message_editor {
                        editor.update(cx, |editor, cx| {
                            editor.set_typography(value.font_family.clone(), value.font_size, FontWeight(value.font_weight as f32), value.line_height, cx)
                        });
                    }
                    for file in view.file_map.values() {
                        if let Some(editor) = &file.editor {
                            editor.update(cx, |editor, cx| {
                                editor.set_typography(value.font_family.clone(), value.font_size, FontWeight(value.font_weight as f32), value.line_height, cx)
                            });
                        }
                    }
                }
            }
            FontTarget::Terminal => {
                for view in &self.workspace_list {
                    for session in view.session_map.values() {
                        if let Some(terminal) = &session.terminal {
                            terminal.update(cx, |terminal, cx| {
                                terminal.set_typography(value.font_family.clone(), value.font_size, FontWeight(value.font_weight as f32), value.line_height, cx)
                            });
                        }
                    }
                }
            }
            FontTarget::Ui | FontTarget::Description => self.apply_ui_theme(cx),
            FontTarget::Preview | FontTarget::Repositories | FontTarget::Changes | FontTarget::Files | FontTarget::Commits => {}
        }
        self.persist_settings(cx);
    }

    fn persist_settings(&mut self, cx: &mut Context<Self>) {
        let error = self.settings.save().err();
        self.settings_panel.update(cx, |panel, cx| panel.sync_panel_preferences(&self.settings, cx));
        self.settings_panel.update(cx, |panel, cx| panel.set_error(error, cx));
        cx.notify();
    }

    fn apply_ui_theme(&self, cx: &mut Context<Self>) {
        let ui = &self.settings.ui;
        {
            let theme = cx.global_mut::<ely_core::theme::Theme>();
            theme.font_family = ui.font_family.clone();
            // ELY fields use 16px rems while the root rem follows the primary UI size.
            theme.font_scale = 16. * self.settings.description.font_size / (13. * ui.font_size);
        }
        gpui_kit::component::Theme::update(cx, |theme| {
            theme.font_family = ui.font_family.clone();
            theme.font_size = px(ui.font_size);
        });
    }

    fn set_panel_layout(&mut self, workspace: usize, target: FontTarget, tree: bool, cx: &mut Context<Self>) {
        let state = &mut self.workspace_list[workspace].state;
        let (panel, preference) = match target {
            FontTarget::Repositories => (&mut state.repository_panel, &mut self.settings.repository_panel),
            FontTarget::Changes => (&mut state.changes_panel, &mut self.settings.changes_panel),
            FontTarget::Files => (&mut state.files_panel, &mut self.settings.file_panel),
            _ => return,
        };
        panel.tree = tree;
        preference.view_mode = if tree { PanelViewMode::Tree } else { PanelViewMode::Flat };
        self.persist_settings(cx);
    }

    fn set_diff_layout(&mut self, workspace: usize, split: bool, cx: &mut Context<Self>) {
        self.workspace_list[workspace].state.split = split;
        self.settings.diff_panel.view_mode = if split { DiffViewMode::Split } else { DiffViewMode::Unified };
        self.persist_settings(cx);
    }

    fn set_diff_context(&mut self, workspace: usize, full: bool, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[workspace];
        view.state.diff_full = full;
        if !full {
            for file in view.file_map.values_mut() {
                file.diff_open.clear();
            }
        }
        self.settings.diff_panel.context_mode = if full { DiffContextMode::Full } else { DiffContextMode::Collapsed };
        self.persist_settings(cx);
    }

    fn open_diff_context(&mut self, workspace_id: usize, key: &FileKey, stretch: usize, cx: &mut Context<Self>) {
        let Some(workspace) = workspace_index(&self.workspace_list, workspace_id) else {
            return;
        };
        let Some(file) = self.workspace_list[workspace].file_map.get_mut(key) else {
            return;
        };
        if matches!(file.diff.as_ref().and_then(|diff| diff.get(stretch)), Some(Stretch::Folded(_))) && file.diff_open.insert(stretch) {
            cx.notify();
        }
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open {
            return;
        }
        self.settings_return_focus = window.focused(cx);
        self.settings_open = true;
        self.drag_axis = None;
        window.focus(&self.settings_focus, cx);
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_open = false;
        let focus = self.settings_return_focus.take().unwrap_or_else(|| self.focus.clone());
        window.focus(&focus, cx);
        cx.notify();
    }

    fn settings_modal(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - 80.).clamp(400., 960.);
        let height = (f32::from(viewport.height) - 100.).clamp(360., 720.);
        div()
            .id("settings-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000055))
            .refine_style(&self.settings.secondary_ui_style())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_settings(window, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.close_settings(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                FocusScope::new(&self.settings_focus)
                    .trap()
                    .node("settings-dialog", Role::Dialog, "Settings", None)
                    .w(px(width))
                    .h(px(height))
                    .child(
                        v_flex()
                            .size_full()
                            .rounded(px(10.))
                            .shadow_lg()
                            .bg(rgb(BG))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .overflow_hidden()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                self.chrome("Settings").h(px(40.)).child(div().flex_1()).child(
                                    self.icon_button("close-settings", IconName::X, "Close Settings")
                                        .on_click(cx.listener(|this, _, window, cx| this.close_settings(window, cx))),
                                ),
                            )
                            .child(div().flex_1().min_h_0().min_w_0().child(self.settings_panel.clone())),
                    ),
            )
            .into_any_element()
    }

    fn request(&mut self, index: usize, request: ReadRequest, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[index];
        view.generation += 1;
        let generation = view.generation;
        let workspace_id = view.id;
        view.loading = true;
        view.error = None;
        view.pending_file = matches!(request, ReadRequest::File { .. });
        view.task = Some(cx.spawn(async move |this, cx| {
            let event = cx.background_spawn(async move { bus::execute(workspace_id, generation, request) }).await;
            let _ = this.update(cx, |this, cx| this.receive(event, cx));
        }));
        cx.notify();
    }
    fn refresh(&mut self, index: usize, cx: &mut Context<Self>) {
        let root = self.workspace_list[index].root.clone();
        self.request(index, ReadRequest::Discover { root }, cx);
    }
    fn refresh_panel(&mut self, target: RefreshTarget, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.active;
        match target {
            RefreshTarget::Repositories => self.refresh(index, cx),
            RefreshTarget::Worktree => {
                let view = &mut self.workspace_list[index];
                view.file_map.clear();
                view.message_editor = None;
                if let Some(path) = view.selected_worktree.clone() {
                    let root = view.root.clone();
                    self.request(index, ReadRequest::Worktree { root, path }, cx);
                } else {
                    self.refresh(index, cx);
                }
            }
            RefreshTarget::History => {
                let view = &mut self.workspace_list[index];
                let revision = view.revision();
                view.file_map.retain(|(key, _), _| key != &revision);
                view.message_editor = None;
                self.load_history(index, cx);
            }
            RefreshTarget::File => {
                let view = &mut self.workspace_list[index];
                if let Some(key) = &view.file_key {
                    view.file_map.remove(key);
                }
                self.load_file(index, cx);
            }
            RefreshTarget::Terminal => {
                let view = &mut self.workspace_list[index];
                view.terminal_focus = true;
                let key = view.session_key(view.state.session);
                if let Some(terminal) = view.session_map.get(&key).and_then(|session| session.terminal.as_ref()) {
                    terminal.update(cx, |_, cx| cx.notify());
                }
                window.refresh();
            }
        }
        cx.notify();
    }
    fn receive(&mut self, event: ReadEvent, cx: &mut Context<Self>) {
        let Some(index) = read_event_index(&self.workspace_list, &event) else {
            return;
        };
        self.workspace_list[index].loading = false;
        self.workspace_list[index].pending_file = false;
        let result = match event.result {
            Ok(result) => result,
            Err(error) => {
                self.workspace_list[index].error = Some(error);
                cx.notify();
                return;
            }
        };
        match result {
            ReadResult::Workspace(snapshot) => {
                let view = &mut self.workspace_list[index];
                if let Some(selected) = &view.selected_worktree {
                    if let Some((repo, worktree)) = snapshot
                        .repository_list
                        .iter()
                        .enumerate()
                        .find_map(|(repo, data)| data.worktree_list.iter().position(|worktree| &worktree.path == selected).map(|worktree| (repo, worktree)))
                    {
                        view.state.repo = repo;
                        view.state.worktree = worktree;
                    } else {
                        view.state.repo = 0;
                        view.state.worktree = 0;
                    }
                }
                view.snapshot = Some(snapshot);
                view.root = view.snapshot.as_ref().unwrap().root.clone();
                view.file_map.clear();
                if let Some(worktree) = view.worktree() {
                    let path = worktree.path.clone();
                    view.selected_worktree = Some(path.clone());
                    let root = view.root.clone();
                    self.request(index, ReadRequest::Worktree { root, path }, cx);
                }
            }
            ReadResult::Worktree(data) => {
                let view = &mut self.workspace_list[index];
                if view.selected_worktree.as_ref() != Some(&data.path) {
                    return;
                }
                if let Some(oid) = &view.selected_commit {
                    if let Some(commit) = data.commit_list.iter().position(|commit| &commit.oid == oid) {
                        view.state.history = HistorySelection::Commit(commit);
                    } else {
                        view.state.select_history(HistorySelection::Unstaged);
                        view.selected_commit = None;
                    }
                }
                if view.state.base >= data.base_ref_list.len() {
                    view.state.base = 0;
                }
                view.files_tree = file::FileTree::from_paths(data.file_list.iter().map(|path| path.as_path()));
                view.worktree_data = Some(data);
                view.graph_list = grove_core::commit_graph::project(&view.worktree_data.as_ref().unwrap().commit_list);
                self.load_history(index, cx);
            }
            ReadResult::History(data) => {
                let view = &mut self.workspace_list[index];
                if view.revision().as_ref() != Some(&data.revision) {
                    return;
                }
                view.message_editor = None;
                let selected = view.selected_file.as_ref().and_then(|path| data.file_list.iter().position(|file| &file.path == path)).unwrap_or(0);
                if !matches!(view.state.file, FileSelection::Message) {
                    view.state.file = FileSelection::File(selected);
                }
                view.changes_tree = file::FileTree::from_paths(data.file_list.iter().map(|file| file.path.as_path()));
                view.history_data = Some(data);
                self.load_file(index, cx);
            }
            ReadResult::File(data) => {
                let view = &mut self.workspace_list[index];
                if let Some(key) = &view.file_key {
                    view.file_map.insert(
                        key.clone(),
                        FileView {
                            data,
                            editor: None,
                            diff: None,
                            diff_open: HashSet::new(),
                            diff_scroll: DiffScrollHandle::default(),
                            error: None,
                            hydrated: false,
                        },
                    );
                }
            }
        }
        cx.notify();
    }
    fn load_history(&mut self, index: usize, cx: &mut Context<Self>) {
        let view = &self.workspace_list[index];
        let (Some(revision), Some(path)) = (view.revision(), view.selected_worktree.clone()) else {
            return;
        };
        let root = view.root.clone();
        self.request(index, ReadRequest::History { root, path, revision }, cx);
    }
    fn load_file(&mut self, index: usize, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[index];
        if view.loading && !view.pending_file {
            return;
        }
        let (revision, file) = if view.state.mode == Mode::Files {
            let Some(data) = &view.worktree_data else {
                return;
            };
            let selected = view
                .selected_fs_file
                .as_ref()
                .and_then(|path| data.file_list.iter().position(|file| file == path))
                .unwrap_or(view.state.fs_file);
            view.state.fs_file = selected;
            (None, data.file_list.get(selected).cloned())
        } else {
            let FileSelection::File(selected) = view.state.file else {
                return;
            };
            (view.revision(), view.history_data.as_ref().and_then(|data| data.file_list.get(selected)).map(|file| file.path.clone()))
        };
        let Some(file) = file else {
            view.file_key = None;
            return;
        };
        let key = (revision.clone(), file.clone());
        view.file_key = Some(key.clone());
        if view.loading && view.pending_file {
            view.generation += 1;
            view.task = None;
            view.loading = false;
            view.pending_file = false;
        }
        if view.file_map.contains_key(&key) {
            return;
        }
        let Some(path) = view.selected_worktree.clone() else {
            return;
        };
        let root = view.root.clone();
        self.request(index, ReadRequest::File { root, path, revision, file }, cx);
    }
    fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.state_mut().set_mode(mode);
        self.workspace_list[self.active].terminal_focus = false;
        self.load_file(self.active, cx);
        cx.notify();
    }

    fn toggle_dock(&mut self, mode: DockMode, cx: &mut Context<Self>) {
        self.state_mut().toggle_dock(mode);
        self.workspace_list[self.active].terminal_focus = self.state().terminal_visible();
        self.drag_axis = None;
        cx.notify();
    }

    fn toggle_dock_visibility(&mut self, cx: &mut Context<Self>) {
        self.state_mut().toggle_dock_visibility();
        self.workspace_list[self.active].terminal_focus = self.state().terminal_visible();
        self.drag_axis = None;
        cx.notify();
    }
    fn select_worktree(&mut self, repo: usize, worktree: usize, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[self.active];
        view.state.repo = repo;
        view.state.worktree = worktree;
        let Some(path) = view.worktree().map(|worktree| worktree.path.clone()) else {
            return;
        };
        view.selected_worktree = Some(path.clone());
        view.selected_commit = None;
        view.selected_file = None;
        view.selected_fs_file = None;
        let mode = view.state.mode;
        view.state.select_history(HistorySelection::Unstaged);
        view.state.set_mode(mode);
        view.terminal_focus = view.state.terminal_visible();
        view.state.compare = false;
        view.state.fs_file = 0;
        view.worktree_data = None;
        view.history_data = None;
        view.files_tree = file::FileTree::default();
        view.changes_tree = file::FileTree::default();
        view.file_key = None;
        view.file_map.clear();
        let root = view.root.clone();
        self.request(self.active, ReadRequest::Worktree { root, path }, cx);
    }
    fn open_worktree_files(&mut self, repo: usize, worktree: usize, cx: &mut Context<Self>) {
        if self.state().repo != repo || self.state().worktree != worktree {
            self.select_worktree(repo, worktree, cx);
        }
        self.set_mode(Mode::Files, cx);
    }
    fn select_history(&mut self, selection: HistorySelection, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[self.active];
        view.state.select_history(selection);
        view.selected_commit = if let HistorySelection::Commit(index) = selection {
            view.worktree_data.as_ref().and_then(|data| data.commit_list.get(index)).map(|commit| commit.oid.clone())
        } else {
            None
        };
        view.selected_file = None;
        view.history_data = None;
        view.changes_tree = file::FileTree::default();
        view.file_key = None;
        self.load_history(self.active, cx);
        cx.notify();
    }
    fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[self.active];
        if view.state.mode == Mode::Files {
            view.state.fs_file = index;
            view.selected_fs_file = view.worktree_data.as_ref().and_then(|data| data.file_list.get(index)).cloned();
        } else {
            view.state.file = FileSelection::File(index);
            view.selected_file = view.history_data.as_ref().and_then(|data| data.file_list.get(index)).map(|file| file.path.clone());
        }
        self.load_file(self.active, cx);
        cx.notify();
    }
    fn hydrate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (panel, label) in [
            (SearchPanel::Repositories, "Search Repositories"),
            (SearchPanel::Changes, "Search Changes"),
            (SearchPanel::Files, "Search Files"),
        ] {
            if self.view().search_input_list[panel.index()].is_some() {
                continue;
            }
            let workspace_id = self.view().id;
            let input = cx.new(|cx| TextInput::new(window, cx).placeholder(label).label(label));
            let subscription = cx.subscribe(&input, move |this, input, event, cx| {
                if *event != InputEvent::Changed {
                    return;
                }
                let Some(index) = workspace_index(&this.workspace_list, workspace_id) else {
                    return;
                };
                let query = input.read(cx).text().trim().to_lowercase();
                let state = &mut this.workspace_list[index].state;
                let panel = match panel {
                    SearchPanel::Repositories => &mut state.repository_panel,
                    SearchPanel::Changes => &mut state.changes_panel,
                    SearchPanel::Files => &mut state.files_panel,
                };
                panel.query = query;
                cx.notify();
            });
            let view = &mut self.workspace_list[self.active];
            view.search_input_list[panel.index()] = Some(input);
            view.search_subscription_list.push(subscription);
        }
        if self.state().terminal_visible() {
            let key = self.view().session_key(self.state().session);
            let view = &mut self.workspace_list[self.active];
            let typography = self.settings.terminal.clone();
            let session = view.session_map.entry(key.clone()).or_insert_with(|| Session::spawn(&key, &typography, window, cx));
            if view.terminal_focus && !self.settings_open {
                if let Some(terminal) = &session.terminal {
                    window.focus(&terminal.focus_handle(cx), cx);
                }
                view.terminal_focus = false;
            }
        }
        let view = &mut self.workspace_list[self.active];
        if view.message_editor.is_none()
            && let Some(history) = &view.history_data
        {
            let typography = &self.settings.editor;
            // Preserve literal Markdown source and line numbers while coloring its syntax.
            view.message_editor = Some(cx.new(|cx| {
                CodeEditor::new(history.message.clone(), window, cx)
                    .language("markdown")
                    .line_numbers(LineNumbers::Absolute)
                    .highlighter(markdown::highlighter(&history.message))
                    .font_family(typography.font_family.clone())
                    .font_size(typography.font_size)
                    .font_weight(FontWeight(typography.font_weight as f32))
                    .line_height(typography.line_height)
                    .read_only()
                    .show_caret(false)
            }));
        }
        let Some(file) = view.file_key.as_ref().and_then(|key| view.file_map.get_mut(key)) else {
            return;
        };
        if file.hydrated {
            return;
        }
        file.hydrated = true;
        if let FileContent::Text(source) = &file.data.content {
            let language = match file.data.path.extension().and_then(|extension| extension.to_str()) {
                Some("rs") => "rust",
                Some("json") => "json",
                Some("js" | "ts" | "tsx") => "typescript",
                Some("py") => "python",
                Some("java") => "java",
                Some("md") => "markdown",
                _ => "text",
            };
            let syntax = ely_core::forms::syntax_of(&file.data.path.to_string_lossy());
            file.editor = Some(cx.new(|cx| {
                let editor = CodeEditor::new(source.to_string(), window, cx)
                    .language(language)
                    .font_family(self.settings.editor.font_family.clone())
                    .font_size(self.settings.editor.font_size)
                    .font_weight(FontWeight(self.settings.editor.font_weight as f32))
                    .line_height(self.settings.editor.line_height)
                    .read_only()
                    .show_caret(false);
                if let Some(syntax) = syntax {
                    editor.highlighter(move |text, cx| ely_core::forms::code_highlights_with(text, syntax, cx))
                } else {
                    editor.highlighter(|_, _| Vec::new())
                }
            }));
        }
        if let Some(patch) = &file.data.patch {
            match crate::patch_view::parse(patch) {
                Ok(stretches) if stretches.is_empty() => {
                    let unchanged_metadata = patch.contains("similarity index 100%\n") || patch.contains("\nold mode ");
                    let binary = patch.contains("\nBinary files ") || patch.contains("\nGIT binary patch\n");
                    if unchanged_metadata
                        && !binary
                        && let FileContent::Text(source) = &file.data.content
                    {
                        file.diff = Some(crate::patch_view::unchanged(source));
                    }
                }
                Ok(stretches) => file.diff = Some(stretches),
                Err(error) => file.error = Some(format!("Unable to display this patch: {error}")),
            }
        }
    }
    fn chrome(&self, title: &'static str) -> Div {
        h_flex()
            .h(px(RAIL_WIDTH))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .pl(px(8.))
            .items_center()
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(BORDER))
            .refine_style(&self.settings.ui.style())
            .text_color(rgb(SECONDARY))
            .child(div().flex_shrink_0().whitespace_nowrap().child(title))
    }
    fn icon_button(&self, id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Button {
        Button::new(id)
            .icon(icon)
            .accessibility_label(label)
            .ghost()
            .xsmall()
            .size(px(RAIL_WIDTH))
            .p_0()
            .rounded_none()
            .flex_shrink_0()
            .refine_style(&self.settings.ui.style())
    }

    fn refresh_button(&self, id: &'static str, label: impl Into<SharedString>, target: RefreshTarget, cx: &mut Context<Self>) -> Button {
        self.icon_button(id, IconName::RefreshCw, label)
            .on_click(cx.listener(move |this, _, window, cx| this.refresh_panel(target, window, cx)))
    }

    fn search_bar(&self, panel: SearchPanel, cx: &mut Context<Self>) -> AnyElement {
        let input = self.view().search_input_list[panel.index()].as_ref().expect("search inputs hydrate before rendering").clone();
        let focus = input.read(cx).focus_handle(cx);
        h_flex()
            .h(px(RAIL_WIDTH))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .border_b_1()
            .border_color(rgb(BORDER))
            .refine_style(&self.settings.secondary_ui_style())
            .child(
                div()
                    .size(px(RAIL_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon_view(IconName::Search, 12., MUTED)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .refine_style(&if input.read(cx).is_empty() {
                        self.settings.secondary_ui_style()
                    } else {
                        self.settings.editor.style()
                    })
                    .child(input.clone()),
            )
            .child(self.icon_button(("clear-search", panel.index()), IconName::X, "Clear search").on_click(move |_, window, cx| {
                input.update(cx, |input, cx| input.set_text("", cx));
                window.focus(&focus, cx);
            }))
            .into_any_element()
    }
    fn segment(&self, id: &'static str, label: impl Into<SharedString>, selected: bool) -> Button {
        Button::new(id)
            .child(div().refine_style(&self.settings.secondary_ui_style()).child(label.into()))
            .ghost()
            .xsmall()
            .selected(selected)
            .h(px(RAIL_WIDTH))
            .px(px(6.))
            .refine_style(&self.settings.secondary_ui_style())
            .flex_shrink_0()
    }
    fn notice(&self, message: impl Into<SharedString>) -> AnyElement {
        div()
            .p(px(16.))
            .refine_style(&self.settings.secondary_ui_style())
            .text_color(rgb(MUTED))
            .child(message.into())
            .into_any_element()
    }
    fn splitter(&self, axis: DragAxis, position: f32, cx: &mut Context<Self>) -> AnyElement {
        let column = matches!(axis, DragAxis::Column);
        div()
            .id(if column { "column-divider" } else { "row-divider" })
            .role(Role::Splitter)
            .absolute()
            .when(column, |this| this.left(px(position - 4.)).top_0().w(px(9.)).h_full().cursor_col_resize())
            .when(!column, |this| this.top(px(position - 4.)).left_0().h(px(9.)).w_full().cursor_row_resize())
            .child(
                div()
                    .absolute()
                    .bg(rgb(BORDER))
                    .when(column, |this| this.left(px(4.)).top_0().w(px(1.)).h_full())
                    .when(!column, |this| this.top(px(4.)).left_0().h(px(1.)).w_full()),
            )
            .hover(|this| this.bg(rgba(0x0071e318)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if event.click_count == 2 {
                        if column {
                            this.state_mut().col_ratio = 0.2;
                        } else {
                            this.state_mut().row_ratio = 0.5;
                        }
                        this.drag_axis = None;
                    } else {
                        this.drag_axis = Some(axis);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }
}
impl Render for Grove {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.hydrate(window, cx);
        let body = {
            let viewport = window.viewport_size();
            let width = (f32::from(viewport.width) - RAIL_WIDTH).max(600.);
            let height = (f32::from(viewport.height) - TITLE_HEIGHT - STATUS_HEIGHT).max(300.);
            let col = (width * self.state().col_ratio).clamp(200., width - 360.);
            let dock_visible = self.state().dock_visible;
            let row = if dock_visible { (height * self.state().row_ratio).clamp(90., height - 90.) } else { height };
            let bounds = self.body_bounds.clone();
            let lower = dock_visible.then(|| match self.state().dock_mode {
                DockMode::History => (self.repositories(cx).into_any_element(), self.commits(cx).into_any_element()),
                DockMode::Terminal => (self.sessions(cx), self.terminal_detail(cx)),
            });
            let (lower_left, lower_right) = lower.unzip();
            let main = h_flex()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_hidden()
                .on_prepaint(move |value, _, _| bounds.set(value))
                .child(
                    v_flex()
                        .w(px(col))
                        .h_full()
                        .flex_shrink_0()
                        .min_w_0()
                        .overflow_hidden()
                        .child(
                            div()
                                .id("upper-navigation")
                                .role(Role::Group)
                                .aria_label("Editor navigation")
                                .h(px(row))
                                .flex_shrink_0()
                                .overflow_hidden()
                                .child(self.files(cx)),
                        )
                        .children(lower_left.map(|panel| {
                            div()
                                .id("lower-navigation")
                                .role(Role::Group)
                                .aria_label("Bottom panel navigation")
                                .flex_1()
                                .min_h_0()
                                .overflow_hidden()
                                .child(panel)
                        })),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .overflow_hidden()
                        .child(
                            div()
                                .id("upper-detail")
                                .role(Role::Group)
                                .aria_label("Editor content")
                                .h(px(row))
                                .flex_shrink_0()
                                .overflow_hidden()
                                .child(self.detail(cx)),
                        )
                        .children(lower_right.map(|panel| {
                            div()
                                .id("lower-detail")
                                .role(Role::Group)
                                .aria_label("Bottom panel content")
                                .flex_1()
                                .min_h_0()
                                .overflow_hidden()
                                .child(panel)
                        })),
                )
                .child(self.splitter(DragAxis::Column, col, cx))
                .children(dock_visible.then(|| self.splitter(DragAxis::Row, row, cx)));
            h_flex().flex_1().min_h_0().overflow_hidden().child(self.rail(row, cx)).child(main).into_any_element()
        };
        div()
            .relative()
            .size_full()
            .key_context("Grove")
            .on_action(cx.listener(|this, _: &ChangesMode, window, cx| {
                if !this.settings_open {
                    window.focus(&this.focus, cx);
                    this.set_mode(Mode::Changes, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FilesMode, window, cx| {
                if !this.settings_open {
                    window.focus(&this.focus, cx);
                    this.set_mode(Mode::Files, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &TerminalMode, window, cx| {
                if !this.settings_open {
                    window.focus(&this.focus, cx);
                    this.toggle_dock(DockMode::Terminal, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &HistoryMode, window, cx| {
                if !this.settings_open {
                    window.focus(&this.focus, cx);
                    this.toggle_dock(DockMode::History, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleDock, window, cx| {
                if !this.settings_open {
                    window.focus(&this.focus, cx);
                    this.toggle_dock_visibility(cx);
                }
            }))
            .child(
                FocusScope::new(&self.focus).root().size_full().child(
                    v_flex()
                        .id("grove")
                        .role(Role::Application)
                        .aria_label("Grove browser and terminal")
                        .size_full()
                        .bg(rgb(BG))
                        .text_color(rgb(FG))
                        .refine_style(&self.settings.secondary_ui_style())
                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                            let Some(axis) = this.drag_axis else {
                                return;
                            };
                            let bounds = this.body_bounds.get();
                            match axis {
                                DragAxis::Column => {
                                    let width = f32::from(bounds.size.width);
                                    this.state_mut().col_ratio = f32::from(event.position.x - bounds.origin.x).clamp(200., width - 360.) / width;
                                }
                                DragAxis::Row => {
                                    let height = f32::from(bounds.size.height);
                                    this.state_mut().row_ratio = f32::from(event.position.y - bounds.origin.y).clamp(90., height - 90.) / height;
                                }
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.drag_axis = None))
                        .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, _| this.drag_axis = None))
                        .child(self.title_bar(cx))
                        .child(body)
                        .child(self.status_bar(cx)),
                ),
            )
            .children(self.settings_open.then(|| self.settings_modal(window, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::WorkspaceView;
    use super::read_event_index;
    use grove_core::data::ReadEvent;
    use std::path::PathBuf;

    #[test]
    fn read_events_follow_identity_after_preceding_tab_closes() {
        let mut next_id = 0;
        let settings = crate::settings::Settings::default();
        let mut workspace_list = vec![
            WorkspaceView::new(PathBuf::from("first"), &mut next_id, &settings),
            WorkspaceView::new(PathBuf::from("second"), &mut next_id, &settings),
        ];
        let mut event = ReadEvent { workspace: workspace_list[1].id, generation: 0, result: Err("read failed".into()) };
        assert_eq!(read_event_index(&workspace_list, &event), Some(1));
        workspace_list.remove(0);
        assert_eq!(read_event_index(&workspace_list, &event), Some(0));
        workspace_list[0].generation = 1;
        assert_eq!(read_event_index(&workspace_list, &event), None);
        event.generation = 1;
        assert_eq!(read_event_index(&workspace_list, &event), Some(0));
    }

    #[test]
    fn reopening_same_root_never_accepts_closed_tabs_events() {
        let root = PathBuf::from("workspace");
        let mut next_id = 0;
        let settings = crate::settings::Settings::default();
        let mut workspace_list = vec![WorkspaceView::new(root.clone(), &mut next_id, &settings)];
        let event = ReadEvent { workspace: workspace_list[0].id, generation: 0, result: Err("late read failed".into()) };
        workspace_list.remove(0);
        assert_eq!(read_event_index(&workspace_list, &event), None);
        workspace_list.push(WorkspaceView::new(root, &mut next_id, &settings));
        assert_ne!(workspace_list[0].id, event.workspace);
        assert_eq!(read_event_index(&workspace_list, &event), None);
        let reopened_event = ReadEvent { workspace: workspace_list[0].id, generation: 0, result: Err("current read failed".into()) };
        assert_eq!(read_event_index(&workspace_list, &reopened_event), Some(0));
    }
}
