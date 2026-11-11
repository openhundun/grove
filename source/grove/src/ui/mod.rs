use crate::icon::view as icon_view;
use crate::icon::{self};
use crate::terminal_session::Session;
use crate::terminal_session::SessionKey;
use crate::workspace_state::FileSelection;
use crate::workspace_state::HistorySelection;
use crate::workspace_state::Mode;
use crate::workspace_state::WorkspaceState;
use ely_core::buttons::SegmentedControl;
use ely_core::primitives::FocusScope;
use ely_core::theme::ControlSize;
use ely_editor::CodeEditor;
use ely_editor::LineNumbers;
use ely_git::DiffLayout;
use ely_git::DiffViewer;
use ely_git::Stretch;
use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt as _;
use gpui_kit::base::h_flex;
use gpui_kit::base::v_flex;
use gpui_kit::component::Disableable;
use gpui_kit::component::Selectable;
use gpui_kit::component::Sizable;
use gpui_kit::component::TitleBar;
use gpui_kit::component::button::Button;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::diff::DiffFile;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::text::TextView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use grove_core::data::*;
use grove_runtime::bus;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

mod commit;
mod detail;
mod file;
mod repository;
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
const ROW_HEIGHT: f32 = 24.;
const MONO: &str = grove_platform::MONO_FONT;
const SESSION_LIST: [&str; 3] = ["Workspace", "Worktree", "Shell 2"];

actions!(grove, [ChangesMode, FilesMode, TerminalMode]);
#[derive(Clone, Copy)]
enum DragAxis {
    Column,
    Row,
}

type FileKey = (Option<Revision>, PathBuf);
struct FileView {
    data: FileData,
    editor: Option<Entity<CodeEditor>>,
    diff: Option<Rc<Vec<Stretch>>>,
    error: Option<String>,
    hydrated: bool,
}
struct WorkspaceView {
    root: PathBuf,
    state: WorkspaceState,
    snapshot: Option<WorkspaceSnapshot>,
    worktree_data: Option<WorktreeData>,
    graph_list: Vec<grove_core::commit_graph::GraphRow>,
    history_data: Option<HistoryData>,
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
    session_map: HashMap<SessionKey, Session>,
    terminal_focus: bool,
}
impl WorkspaceView {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            state: WorkspaceState::default(),
            snapshot: None,
            worktree_data: None,
            graph_list: Vec::new(),
            history_data: None,
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

pub struct Grove {
    workspace_list: Vec<WorkspaceView>,
    active: usize,
    focus: FocusHandle,
    body_bounds: Rc<Cell<Bounds<Pixels>>>,
    drag_axis: Option<DragAxis>,
}
impl Grove {
    pub fn new(root_list: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut this = Self {
            workspace_list: root_list.into_iter().map(WorkspaceView::new).collect(),
            active: 0,
            focus,
            body_bounds: Rc::new(Cell::new(Bounds::default())),
            drag_axis: None,
        };
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

    fn request(&mut self, index: usize, request: ReadRequest, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[index];
        view.generation += 1;
        let generation = view.generation;
        view.loading = true;
        view.error = None;
        view.pending_file = matches!(request, ReadRequest::File { .. });
        view.task = Some(cx.spawn(async move |this, cx| {
            let event = cx.background_spawn(async move { bus::execute(index, generation, request) }).await;
            let _ = this.update(cx, |this, cx| this.receive(event, cx));
        }));
        cx.notify();
    }
    fn refresh(&mut self, index: usize, cx: &mut Context<Self>) {
        let root = self.workspace_list[index].root.clone();
        self.request(index, ReadRequest::Discover { root }, cx);
    }
    fn receive(&mut self, event: ReadEvent, cx: &mut Context<Self>) {
        let index = event.workspace;
        if self.workspace_list[index].generation != event.generation {
            return;
        }
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
                view.history_data = Some(data);
                self.load_file(index, cx);
            }
            ReadResult::File(data) => {
                let view = &mut self.workspace_list[index];
                if let Some(key) = &view.file_key {
                    view.file_map.insert(key.clone(), FileView { data, editor: None, diff: None, error: None, hydrated: false });
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
        if view.state.mode == Mode::Terminal {
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
        self.state_mut().mode = mode;
        self.workspace_list[self.active].terminal_focus = mode == Mode::Terminal;
        self.load_file(self.active, cx);
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
        let terminal_mode = view.state.mode == Mode::Terminal;
        view.state.select_history(HistorySelection::Unstaged);
        if terminal_mode {
            view.state.mode = Mode::Terminal;
            view.terminal_focus = true;
        }
        view.state.compare = false;
        view.state.fs_file = 0;
        view.worktree_data = None;
        view.history_data = None;
        view.file_key = None;
        view.file_map.clear();
        let root = view.root.clone();
        self.request(self.active, ReadRequest::Worktree { root, path }, cx);
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
        if self.state().mode == Mode::Terminal {
            let key = self.view().session_key(self.state().session);
            let view = &mut self.workspace_list[self.active];
            let session = view.session_map.entry(key.clone()).or_insert_with(|| Session::spawn(&key, window, cx));
            if view.terminal_focus {
                if let Some(terminal) = &session.terminal {
                    window.focus(&terminal.focus_handle(cx), cx);
                }
                view.terminal_focus = false;
            }
            return;
        }
        let view = &mut self.workspace_list[self.active];
        if view.message_editor.is_none()
            && let Some(history) = &view.history_data
        {
            view.message_editor = Some(cx.new(|cx| CodeEditor::new(history.message.clone(), window, cx).language("markdown").line_numbers(LineNumbers::Hidden).read_only()));
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
                let editor = CodeEditor::new(source.to_string(), window, cx).language(language).read_only();
                if let Some(syntax) = syntax {
                    editor.highlighter(move |text, cx| ely_core::forms::code_highlights_with(text, syntax, cx))
                } else {
                    editor.highlighter(|_, _| Vec::new())
                }
            }));
        }
        if let Some(patch) = &file.data.patch {
            match DiffFile::parse(patch) {
                Ok(_) => match crate::patch_view::parse(patch) {
                    Ok(stretches) => file.diff = Some(stretches),
                    Err(error) => file.error = Some(error.into()),
                },
                Err(error) => file.error = Some(format!("Unable to display this patch: {error:?}")),
            }
        }
    }
    fn chrome(&self, title: &'static str) -> Div {
        h_flex()
            .h(px(28.))
            .flex_shrink_0()
            .px(px(8.))
            .gap(px(6.))
            .items_center()
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(BORDER))
            .text_size(px(11.))
            .text_color(rgb(SECONDARY))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
    }
    fn segment(&self, id: &'static str, label: impl Into<SharedString>, selected: bool) -> Button {
        Button::new(id).label(label).ghost().xsmall().selected(selected).h(px(20.)).px(px(6.)).text_size(px(10.5))
    }
    fn notice(&self, message: impl Into<SharedString>) -> AnyElement {
        div().p(px(16.)).text_size(px(11.)).text_color(rgb(MUTED)).child(message.into()).into_any_element()
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
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - RAIL_WIDTH).max(600.);
        let height = (f32::from(viewport.height) - TITLE_HEIGHT - STATUS_HEIGHT).max(300.);
        let col = (width * self.state().col_ratio).clamp(200., width - 360.);
        let row = (height * self.state().row_ratio).clamp(90., height - 90.);
        let bounds = self.body_bounds.clone();
        let lower_left = if self.state().mode == Mode::Terminal { self.sessions(cx) } else { self.files(cx) };
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
                    .child(div().h(px(row)).flex_shrink_0().overflow_hidden().child(self.repositories(cx)))
                    .child(div().flex_1().min_h_0().overflow_hidden().child(lower_left)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(div().h(px(row)).flex_shrink_0().overflow_hidden().child(self.commits(cx)))
                    .child(div().flex_1().min_h_0().overflow_hidden().child(self.detail(cx))),
            )
            .child(self.splitter(DragAxis::Column, col, cx))
            .child(self.splitter(DragAxis::Row, row, cx));
        div()
            .size_full()
            .key_context("Grove")
            .on_action(cx.listener(|this, _: &ChangesMode, _, cx| this.set_mode(Mode::Changes, cx)))
            .on_action(cx.listener(|this, _: &FilesMode, _, cx| this.set_mode(Mode::Files, cx)))
            .on_action(cx.listener(|this, _: &TerminalMode, _, cx| this.set_mode(Mode::Terminal, cx)))
            .child(
                FocusScope::new(&self.focus).root().size_full().child(
                    v_flex()
                        .id("grove")
                        .role(Role::Application)
                        .aria_label("Grove browser and terminal")
                        .size_full()
                        .bg(rgb(BG))
                        .text_color(rgb(FG))
                        .font_family(grove_platform::SYSTEM_FONT)
                        .text_size(px(11.5))
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
                        .child(h_flex().flex_1().min_h_0().overflow_hidden().child(self.rail(cx)).child(main))
                        .child(self.status_bar(cx)),
                ),
            )
    }
}
