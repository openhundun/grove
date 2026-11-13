use super::*;
use crate::workspace_state::ListPanelState;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

/// Sorted, compact tree projection built on data arrival for Changes and Files.
/// Single-child directory chains share a row; real paths identify icons, accessibility
/// and folding, displayed depth drives indentation, and file indices retain input order.
#[derive(Default)]
pub(super) struct FileTree {
    node_list: Vec<FileTreeNode>,
    flat_index_list: Vec<usize>,
}

struct FileTreeNode {
    path: PathBuf,
    name: SharedString,
    display: SharedString,
    depth: usize,
    kind: FileTreeNodeKind,
}

#[derive(Clone, Copy)]
enum FileTreeNodeKind {
    Directory { subtree_end: usize },
    File { original_index: usize },
}

impl FileTreeNode {
    fn new(path: PathBuf, parent: &Path, depth: usize, kind: FileTreeNodeKind) -> Self {
        let name = path.strip_prefix(parent).unwrap_or(&path).to_string_lossy().into_owned().into();
        let display = path.to_string_lossy().into_owned().into();
        Self { path, name, display, depth, kind }
    }
}

#[derive(Default)]
struct FileTreeBuilder {
    directory_map: BTreeMap<OsString, FileTreeBuilder>,
    file_list: Vec<(usize, PathBuf)>,
}

impl FileTreeBuilder {
    fn append_to(mut self, parent: &Path, depth: usize, node_list: &mut Vec<FileTreeNode>) {
        for (name, mut directory) in self.directory_map {
            let mut path = parent.join(name);
            while directory.file_list.is_empty() && directory.directory_map.len() == 1 {
                let (name, child) = directory.directory_map.pop_first().unwrap();
                path.push(name);
                directory = child;
            }
            let index = node_list.len();
            node_list.push(FileTreeNode::new(path.clone(), parent, depth, FileTreeNodeKind::Directory { subtree_end: 0 }));
            directory.append_to(&path, depth + 1, node_list);
            let subtree_end = node_list.len();
            node_list[index].kind = FileTreeNodeKind::Directory { subtree_end };
        }
        self.file_list.sort_unstable_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));
        for (original_index, path) in self.file_list {
            node_list.push(FileTreeNode::new(path, parent, depth, FileTreeNodeKind::File { original_index }));
        }
    }
}

impl FileTree {
    pub(super) fn from_paths<'a>(path_list: impl IntoIterator<Item = &'a Path>) -> Self {
        let mut root = FileTreeBuilder::default();
        for (original_index, path) in path_list.into_iter().enumerate() {
            let mut directory = &mut root;
            if let Some(parent) = path.parent() {
                for component in parent.components() {
                    directory = directory.directory_map.entry(component.as_os_str().to_owned()).or_default();
                }
            }
            directory.file_list.push((original_index, path.to_owned()));
        }
        let mut node_list = Vec::new();
        root.append_to(Path::new(""), 0, &mut node_list);
        let mut flat_index_list: Vec<usize> = node_list
            .iter()
            .enumerate()
            .filter_map(|(index, node)| matches!(node.kind, FileTreeNodeKind::File { .. }).then_some(index))
            .collect();
        flat_index_list.sort_unstable_by(|left, right| node_list[*left].path.cmp(&node_list[*right].path).then(left.cmp(right)));
        Self { node_list, flat_index_list }
    }

    fn rows<'a>(&'a self, panel: &'a ListPanelState) -> FileTreeRows<'a> {
        FileTreeRows { tree: self, panel, next: 0, match_index: None }
    }
}

struct FileTreeRows<'a> {
    tree: &'a FileTree,
    panel: &'a ListPanelState,
    next: usize,
    match_index: Option<usize>,
}

impl<'a> Iterator for FileTreeRows<'a> {
    type Item = (usize, &'a FileTreeNode);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if !self.panel.tree {
                let index = *self.tree.flat_index_list.get(self.next)?;
                self.next += 1;
                let node = &self.tree.node_list[index];
                if self.panel.matches_query(node.display.as_ref()) {
                    return Some((index, node));
                }
                continue;
            }
            let index = self.next;
            let node = self.tree.node_list.get(index)?;
            if !self.panel.query.is_empty() {
                if self.match_index.is_none_or(|match_index| match_index < index) {
                    self.match_index = self.tree.node_list[index..]
                        .iter()
                        .position(|node| matches!(node.kind, FileTreeNodeKind::File { .. }) && self.panel.matches_query(node.display.as_ref()))
                        .map(|offset| index + offset);
                }
                let match_index = self.match_index?;
                self.next = index + 1;
                match node.kind {
                    FileTreeNodeKind::Directory { subtree_end } if match_index >= subtree_end => {
                        self.next = subtree_end;
                        continue;
                    }
                    FileTreeNodeKind::File { .. } if match_index != index => continue,
                    _ => {}
                }
            } else {
                self.next = match node.kind {
                    FileTreeNodeKind::Directory { subtree_end } if self.panel.collapsed_path_set.contains(&node.path) => subtree_end,
                    _ => index + 1,
                };
            }
            return Some((index, node));
        }
    }
}

impl Grove {
    pub(super) fn files(&self, cx: &mut Context<Self>) -> AnyElement {
        let workspace = self.active;
        let is_files = self.state().mode == Mode::Files;
        let panel = if is_files { &self.state().files_panel } else { &self.state().changes_panel };
        let tree = panel.tree;
        let file_tree = if is_files { &self.view().files_tree } else { &self.view().changes_tree };
        let typography = if is_files { &self.settings.files } else { &self.settings.changes };
        let indent_size = if is_files {
            self.settings.file_panel.indent_size
        } else {
            self.settings.changes_panel.indent_size
        };
        let row_height = typography.row_height();
        let title = if is_files {
            "Files"
        } else if self.state().compare {
            "PR changes"
        } else {
            "Changes"
        };
        let head = self.chrome(title).child(div().flex_1()).child(
            h_flex()
                .tab_group()
                .flex_shrink_0()
                .child(
                    self.icon_button(
                        if is_files { "files-layout" } else { "changes-layout" },
                        if tree { IconName::ListTree } else { IconName::List },
                        format!("{title}: {}; switch to {}", if tree { "path tree" } else { "flat list" }, if tree { "flat list" } else { "path tree" }),
                    )
                    .toggled(tree)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_panel_layout(workspace, if is_files { FontTarget::Files } else { FontTarget::Changes }, !tree, cx);
                    })),
                )
                .child(self.refresh_button(
                    if is_files { "refresh-files" } else { "refresh-changes" },
                    if is_files { "Refresh Files" } else { "Refresh Changes" },
                    RefreshTarget::Worktree,
                    cx,
                )),
        );
        let mut row_list = Vec::new();
        if !is_files && matches!(self.state().history, HistorySelection::Commit(_)) && panel.matches_query("COMMIT_MSG") {
            // Commit messages are virtual files and share the ordinary file-row presentation.
            row_list.push(
                ListItem::new("commit-message")
                    .accessibility_label("COMMIT_MSG")
                    .role(if tree { Role::TreeItem } else { Role::ListBoxOption })
                    .aria_level(1)
                    .aria_selected(self.state().file == FileSelection::Message)
                    .tab_index(0)
                    .selected(self.state().file == FileSelection::Message)
                    .h(px(row_height))
                    .flex_shrink_0()
                    .pl(px(if tree { 24. } else { 8. }))
                    .pr(px(8.))
                    .py_0()
                    .rounded_none()
                    .overflow_hidden()
                    .border_l_2()
                    .border_color(rgb(if self.state().file == FileSelection::Message { ACCENT } else { BG }))
                    .child(
                        h_flex()
                            .w_full()
                            .h_full()
                            .min_w_0()
                            .gap(px(6.))
                            .refine_style(&typography.style())
                            .child(div().flex_shrink_0().child(icon::file(Path::new("COMMIT_MSG"), 14.)))
                            .child(div().flex_shrink_0().whitespace_nowrap().child("COMMIT_MSG")),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state_mut().file = FileSelection::Message;
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.state_mut().file = FileSelection::Message;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .into_any_element(),
            );
        }
        for (node_index, node) in file_tree.rows(panel) {
            match node.kind {
                FileTreeNodeKind::Directory { .. } => {
                    let collapsed = panel.query.is_empty() && panel.collapsed_path_set.contains(&node.path);
                    row_list.push(
                        ListItem::new(("dir", node_index))
                            .accessibility_label(node.display.clone())
                            .role(Role::TreeItem)
                            .aria_level(node.depth + 1)
                            .aria_expanded(!collapsed)
                            .tab_index(0)
                            .h(px(row_height))
                            .flex_shrink_0()
                            .pl(px(8. + node.depth as f32 * indent_size))
                            .pr(px(8.))
                            .py_0()
                            .rounded_none()
                            .overflow_hidden()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap(px(6.))
                                    .refine_style(&typography.style())
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .child(icon_view(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown }, 10., MUTED)),
                                    )
                                    .child(div().flex_shrink_0().child(icon::folder(&node.path, !collapsed, false, 14.)))
                                    .child(div().flex_shrink_0().whitespace_nowrap().child(node.name.clone())),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.fold_directory(workspace, is_files, node_index, None, cx)))
                            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                let collapsed = match event.keystroke.key.as_str() {
                                    "enter" | "space" => None,
                                    "left" => Some(true),
                                    "right" => Some(false),
                                    _ => return,
                                };
                                this.fold_directory(workspace, is_files, node_index, collapsed, cx);
                                cx.stop_propagation();
                            }))
                            .into_any_element(),
                    );
                }
                FileTreeNodeKind::File { original_index: index } => {
                    let selected = if is_files {
                        self.state().fs_file == index
                    } else {
                        self.state().file == FileSelection::File(index)
                    };
                    let mut content = h_flex()
                        .w_full()
                        .h_full()
                        .min_w_0()
                        .gap(px(6.))
                        .refine_style(&typography.style())
                        .child(div().flex_shrink_0().child(icon::file(&node.path, 14.)))
                        .child(div().flex_shrink_0().whitespace_nowrap().child(if tree { node.name.clone() } else { node.display.clone() }));
                    if !is_files && let Some(change) = self.view().history_data.as_ref().and_then(|data| data.file_list.get(index)) {
                        if let Some(additions) = change.additions {
                            content = content.child(div().flex_shrink_0().whitespace_nowrap().text_color(rgb(0x1a7f37)).child(format!("+{additions}")));
                        }
                        if let Some(deletions) = change.deletions {
                            content = content.child(div().flex_shrink_0().whitespace_nowrap().text_color(rgb(0xb42318)).child(format!("−{deletions}")));
                        }
                        content = content.child(div().flex_shrink_0().whitespace_nowrap().text_color(rgb(0x8a4b00)).child(change.status.to_string()));
                    }
                    row_list.push(
                        ListItem::new(("file", index))
                            .accessibility_label(node.display.clone())
                            .role(if tree { Role::TreeItem } else { Role::ListBoxOption })
                            .aria_level(if tree { node.depth + 1 } else { 1 })
                            .aria_selected(selected)
                            .tab_index(0)
                            .selected(selected)
                            .h(px(row_height))
                            .flex_shrink_0()
                            .py_0()
                            .rounded_none()
                            .overflow_hidden()
                            .pl(px(if tree { 24. + node.depth as f32 * indent_size } else { 8. }))
                            .pr(px(8.))
                            .border_l_2()
                            .border_color(rgb(if selected { ACCENT } else { BG }))
                            .child(content)
                            .on_click(cx.listener(move |this, _, _, cx| this.select_file(index, cx)))
                            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    this.select_file(index, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .into_any_element(),
                    );
                }
            }
        }
        if row_list.is_empty() {
            row_list.push(self.notice(if self.view().loading {
                "Reading files…"
            } else if !panel.query.is_empty() {
                "No files match this search"
            } else {
                "No files in this selection"
            }));
        }
        v_flex()
            .size_full()
            .child(head)
            .child(self.search_bar(if is_files { SearchPanel::Files } else { SearchPanel::Changes }, cx))
            .child(
                v_flex()
                    .id("files-list")
                    .role(if tree { Role::Tree } else { Role::ListBox })
                    .aria_label(title)
                    .tab_group()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.view().file_scroll)
                    .children(row_list),
            )
            .into_any_element()
    }

    fn fold_directory(&mut self, workspace: usize, is_files: bool, node_index: usize, collapsed: Option<bool>, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[workspace];
        let file_tree = if is_files { &view.files_tree } else { &view.changes_tree };
        let Some(node) = file_tree.node_list.get(node_index) else {
            return;
        };
        let path = node.path.clone();
        let panel = if is_files { &mut view.state.files_panel } else { &mut view.state.changes_panel };
        match collapsed {
            Some(true) => {
                panel.collapsed_path_set.insert(path);
            }
            Some(false) => {
                panel.collapsed_path_set.remove(&path);
            }
            None => panel.toggle_path(path),
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_state::WorkspaceState;
    use core::prelude::v1::test;

    fn visible_path_list(tree: &FileTree, panel: &ListPanelState) -> Vec<PathBuf> {
        tree.rows(panel).map(|(_, node)| node.path.clone()).collect()
    }

    #[test]
    fn single_child_chain_joins_labels_and_uses_displayed_depth() {
        let tree = FileTree::from_paths(["src/components/目录/file.rs"].into_iter().map(Path::new));
        let row_list = tree
            .rows(&ListPanelState::new(true))
            .map(|(_, node)| (node.path.clone(), node.name.as_ref().to_owned(), node.depth))
            .collect::<Vec<_>>();
        assert_eq!(
            row_list,
            [
                (PathBuf::from("src/components/目录"), "src/components/目录".to_owned(), 0),
                (PathBuf::from("src/components/目录/file.rs"), "file.rs".to_owned(), 1),
            ]
        );
        assert_eq!(tree.node_list[0].display.as_ref(), "src/components/目录");
    }

    #[test]
    fn compaction_stops_at_branches_and_directories_with_files() {
        let tree = FileTree::from_paths(["src/left/deep/a.rs", "src/right/deep/b.rs", "src/right/own.rs"].into_iter().map(Path::new));
        let row_list = tree.rows(&ListPanelState::new(true)).map(|(_, node)| (node.name.as_ref().to_owned(), node.depth)).collect::<Vec<_>>();
        assert_eq!(
            row_list,
            [
                ("src".to_owned(), 0),
                ("left/deep".to_owned(), 1),
                ("a.rs".to_owned(), 2),
                ("right".to_owned(), 1),
                ("deep".to_owned(), 2),
                ("b.rs".to_owned(), 3),
                ("own.rs".to_owned(), 2),
            ]
        );
    }

    #[test]
    fn folding_parent_hides_every_descendant_and_preserves_child_fold() {
        let tree = FileTree::from_paths(["src/deep/nested/file.rs", "src/top.rs", "README.md"].into_iter().map(Path::new));
        let mut panel = ListPanelState::new(true);
        panel.toggle_path(PathBuf::from("src/deep/nested"));
        panel.toggle_path(PathBuf::from("src"));
        assert_eq!(visible_path_list(&tree, &panel), [PathBuf::from("src"), PathBuf::from("README.md")]);

        panel.toggle_path(PathBuf::from("src"));
        assert_eq!(
            visible_path_list(&tree, &panel),
            [PathBuf::from("src"), PathBuf::from("src/deep/nested"), PathBuf::from("src/top.rs"), PathBuf::from("README.md")]
        );
    }

    #[test]
    fn sorted_flat_and_tree_rows_keep_original_file_indices() {
        let path_list = ["z.rs", "src/z.rs", "src/nested/deeper/b.rs", "src/a.rs", "Cargo.toml"];
        let tree = FileTree::from_paths(path_list.iter().map(Path::new));
        let file_index_list = |panel: &ListPanelState| {
            tree.rows(panel)
                .filter_map(|(_, node)| match node.kind {
                    FileTreeNodeKind::File { original_index } => {
                        assert_eq!(node.path, Path::new(path_list[original_index]));
                        Some(original_index)
                    }
                    FileTreeNodeKind::Directory { .. } => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(file_index_list(&ListPanelState::new(false)), [4, 3, 2, 1, 0]);
        assert_eq!(file_index_list(&ListPanelState::new(true)), [2, 3, 1, 4, 0]);
    }

    #[test]
    fn folding_and_layout_belong_to_one_panel_in_one_workspace() {
        let tree = FileTree::from_paths(["src/components/file.rs"].into_iter().map(Path::new));
        let mut first = WorkspaceState::default();
        let second = WorkspaceState::default();
        first.changes_panel.tree = true;
        first.changes_panel.toggle_path(PathBuf::from("src/components"));
        first.repository_panel.tree = false;

        assert_eq!(visible_path_list(&tree, &first.changes_panel), [PathBuf::from("src/components")]);
        assert_eq!(visible_path_list(&tree, &first.files_panel), [PathBuf::from("src/components"), PathBuf::from("src/components/file.rs")]);
        assert_eq!(
            visible_path_list(&tree, &second.files_panel),
            [PathBuf::from("src/components"), PathBuf::from("src/components/file.rs")]
        );
        assert!(!second.changes_panel.tree);
        assert!(second.changes_panel.collapsed_path_set.is_empty());
        assert!(second.repository_panel.tree);
        assert!(first.repository_panel.collapsed_path_set.is_empty());
    }

    #[test]
    fn search_keeps_only_matching_ancestors_and_original_file_indices() {
        let path_list = ["src/sibling/skip.rs", "src/Target/deep/Needle.rs", "src/Target/keep.rs", "docs/guide.md", "zNEEDLE.txt"];
        let tree = FileTree::from_paths(path_list.iter().map(Path::new));
        let mut panel = ListPanelState::new(true);
        panel.query = "needle".into();
        assert_eq!(
            visible_path_list(&tree, &panel),
            [
                PathBuf::from("src"),
                PathBuf::from("src/Target"),
                PathBuf::from("src/Target/deep"),
                PathBuf::from("src/Target/deep/Needle.rs"),
                PathBuf::from("zNEEDLE.txt")
            ]
        );
        for tree_layout in [true, false] {
            panel.tree = tree_layout;
            let index_list = tree
                .rows(&panel)
                .filter_map(|(_, node)| match node.kind {
                    FileTreeNodeKind::File { original_index } => {
                        assert_eq!(node.path, Path::new(path_list[original_index]));
                        Some(original_index)
                    }
                    FileTreeNodeKind::Directory { .. } => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(index_list, [1, 4]);
        }
        panel.tree = true;
        panel.query = "target".into();
        assert_eq!(
            visible_path_list(&tree, &panel),
            [
                PathBuf::from("src"),
                PathBuf::from("src/Target"),
                PathBuf::from("src/Target/deep"),
                PathBuf::from("src/Target/deep/Needle.rs"),
                PathBuf::from("src/Target/keep.rs")
            ]
        );
    }

    #[test]
    fn search_temporarily_exposes_collapsed_matches_and_restores_panel_boundaries() {
        let tree = FileTree::from_paths(["src/deep/nested/Needle.rs", "src/top.rs", "README.md"].into_iter().map(Path::new));
        let mut first = WorkspaceState::default();
        let second = WorkspaceState::default();
        first.files_panel.toggle_path(PathBuf::from("src/deep/nested"));
        first.files_panel.toggle_path(PathBuf::from("src"));
        let folded_path_list = visible_path_list(&tree, &first.files_panel);
        let collapsed_path_set = first.files_panel.collapsed_path_set.clone();
        first.files_panel.query = "needle".into();
        assert_eq!(
            visible_path_list(&tree, &first.files_panel),
            [PathBuf::from("src"), PathBuf::from("src/deep/nested"), PathBuf::from("src/deep/nested/Needle.rs")]
        );
        assert_eq!(first.files_panel.collapsed_path_set, collapsed_path_set);
        assert!(first.changes_panel.query.is_empty());
        assert!(first.repository_panel.query.is_empty());
        assert!(second.files_panel.query.is_empty());
        assert_eq!(visible_path_list(&tree, &second.files_panel).len(), 5);
        first.files_panel.tree = false;
        assert_eq!(visible_path_list(&tree, &first.files_panel), [PathBuf::from("src/deep/nested/Needle.rs")]);
        first.files_panel.tree = true;
        first.files_panel.query.clear();
        assert_eq!(visible_path_list(&tree, &first.files_panel), folded_path_list);
        assert_eq!(first.files_panel.collapsed_path_set, collapsed_path_set);
        first.files_panel.query = "absent".into();
        assert!(visible_path_list(&tree, &first.files_panel).is_empty());
        first.files_panel.tree = false;
        assert!(visible_path_list(&tree, &first.files_panel).is_empty());
    }
}
