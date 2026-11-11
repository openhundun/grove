use super::*;
use crate::workspace_state::ListPanelState;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

/// Sorted UI projection; file indices always refer to the original Git result.
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
    fn new(path: PathBuf, depth: usize, kind: FileTreeNodeKind) -> Self {
        let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned().into();
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
        for (name, directory) in self.directory_map {
            let path = parent.join(name);
            let index = node_list.len();
            node_list.push(FileTreeNode::new(path.clone(), depth, FileTreeNodeKind::Directory { subtree_end: 0 }));
            directory.append_to(&path, depth + 1, node_list);
            let subtree_end = node_list.len();
            node_list[index].kind = FileTreeNodeKind::Directory { subtree_end };
        }
        self.file_list.sort_unstable_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));
        for (original_index, path) in self.file_list {
            node_list.push(FileTreeNode::new(path, depth, FileTreeNodeKind::File { original_index }));
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
        FileTreeRows { tree: self, panel, next: 0 }
    }
}

struct FileTreeRows<'a> {
    tree: &'a FileTree,
    panel: &'a ListPanelState,
    next: usize,
}

impl<'a> Iterator for FileTreeRows<'a> {
    type Item = (usize, &'a FileTreeNode);

    fn next(&mut self) -> Option<Self::Item> {
        if !self.panel.tree {
            let index = *self.tree.flat_index_list.get(self.next)?;
            self.next += 1;
            return Some((index, &self.tree.node_list[index]));
        }
        let index = self.next;
        let node = self.tree.node_list.get(index)?;
        self.next = match node.kind {
            FileTreeNodeKind::Directory { subtree_end } if self.panel.collapsed_path_set.contains(&node.path) => subtree_end,
            _ => index + 1,
        };
        Some((index, node))
    }
}

impl Grove {
    pub(super) fn files(&self, cx: &mut Context<Self>) -> AnyElement {
        let workspace = self.active;
        let is_files = self.state().mode == Mode::Files;
        let panel = if is_files { &self.state().files_panel } else { &self.state().changes_panel };
        let tree = panel.tree;
        let file_tree = if is_files { &self.view().files_tree } else { &self.view().changes_tree };
        let font_family = if is_files { self.settings.files_font.clone() } else { self.settings.changes_font.clone() };
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
                .gap(px(2.))
                .child(
                    Button::new(if is_files { "files-flat" } else { "changes-flat" })
                        .icon(IconName::List)
                        .accessibility_label(format!("{title}: flat list"))
                        .ghost()
                        .xsmall()
                        .selected(!tree)
                        .toggled(!tree)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let state = &mut this.workspace_list[workspace].state;
                            let panel = if is_files { &mut state.files_panel } else { &mut state.changes_panel };
                            panel.tree = false;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new(if is_files { "files-tree" } else { "changes-tree" })
                        .icon(IconName::ListTree)
                        .accessibility_label(format!("{title}: path tree"))
                        .ghost()
                        .xsmall()
                        .selected(tree)
                        .toggled(tree)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let state = &mut this.workspace_list[workspace].state;
                            let panel = if is_files { &mut state.files_panel } else { &mut state.changes_panel };
                            panel.tree = true;
                            cx.notify();
                        })),
                ),
        );
        let mut row_list = Vec::new();
        if !is_files && matches!(self.state().history, HistorySelection::Commit(_)) {
            row_list.push(
                ListItem::new("commit-message")
                    .accessibility_label("Commit message")
                    .role(if tree { Role::TreeItem } else { Role::ListBoxOption })
                    .aria_level(1)
                    .tab_index(0)
                    .selected(self.state().file == FileSelection::Message)
                    .h(px(ROW_HEIGHT))
                    .px(px(8.))
                    .py_0()
                    .rounded_none()
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(icon_view(IconName::MessageSquare, 12., ACCENT))
                            .child(div().font_family(font_family.clone()).child("COMMIT_MSG")),
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
                    let collapsed = panel.collapsed_path_set.contains(&node.path);
                    row_list.push(
                        ListItem::new(("dir", node_index))
                            .accessibility_label(node.display.clone())
                            .role(Role::TreeItem)
                            .aria_level(node.depth + 1)
                            .aria_expanded(!collapsed)
                            .tab_index(0)
                            .h(px(ROW_HEIGHT))
                            .pl(px(8. + node.depth as f32 * 16.))
                            .pr(px(8.))
                            .py_0()
                            .rounded_none()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .gap(px(6.))
                                    .child(icon_view(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown }, 10., MUTED))
                                    .child(icon::folder(&node.path, !collapsed, false, 14.))
                                    .child(div().min_w_0().truncate().font_family(font_family.clone()).text_size(px(10.)).child(node.name.clone())),
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
                    let mut content = h_flex().w_full().h_full().min_w_0().gap(px(6.)).child(icon::file(&node.path, 14.)).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(font_family.clone())
                            .text_size(px(10.))
                            .child(if tree { node.name.clone() } else { node.display.clone() }),
                    );
                    if !is_files && let Some(change) = self.view().history_data.as_ref().and_then(|data| data.file_list.get(index)) {
                        if let Some(additions) = change.additions {
                            content = content.child(div().font_family(font_family.clone()).text_size(px(9.)).text_color(rgb(0x1a7f37)).child(format!("+{additions}")));
                        }
                        if let Some(deletions) = change.deletions {
                            content = content.child(div().font_family(font_family.clone()).text_size(px(9.)).text_color(rgb(0xb42318)).child(format!("−{deletions}")));
                        }
                        content = content.child(div().font_family(font_family.clone()).text_size(px(9.)).text_color(rgb(0x8a4b00)).child(change.status.to_string()));
                    }
                    row_list.push(
                        ListItem::new(("file", index))
                            .accessibility_label(node.display.clone())
                            .role(if tree { Role::TreeItem } else { Role::ListBoxOption })
                            .aria_level(if tree { node.depth + 1 } else { 1 })
                            .aria_selected(selected)
                            .tab_index(0)
                            .selected(selected)
                            .h(px(ROW_HEIGHT))
                            .flex_shrink_0()
                            .py_0()
                            .rounded_none()
                            .overflow_hidden()
                            .pl(px(if tree { 24. + node.depth as f32 * 16. } else { 8. }))
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
        if file_tree.flat_index_list.is_empty() {
            row_list.push(self.notice(if self.view().loading { "Reading files…" } else { "No files in this selection" }));
        }
        v_flex()
            .size_full()
            .child(head)
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
    fn folding_parent_hides_every_descendant_and_preserves_child_fold() {
        let tree = FileTree::from_paths(["src/deep/file.rs", "src/top.rs", "README.md"].into_iter().map(Path::new));
        let mut panel = ListPanelState::new(true);
        panel.toggle_path(PathBuf::from("src/deep"));
        panel.toggle_path(PathBuf::from("src"));
        assert_eq!(visible_path_list(&tree, &panel), [PathBuf::from("src"), PathBuf::from("README.md")]);

        panel.toggle_path(PathBuf::from("src"));
        assert_eq!(
            visible_path_list(&tree, &panel),
            [PathBuf::from("src"), PathBuf::from("src/deep"), PathBuf::from("src/top.rs"), PathBuf::from("README.md")]
        );
    }

    #[test]
    fn sorted_flat_and_tree_rows_keep_original_file_indices() {
        let path_list = ["z.rs", "src/z.rs", "src/nested/b.rs", "src/a.rs", "Cargo.toml"];
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
        let tree = FileTree::from_paths(["src/file.rs"].into_iter().map(Path::new));
        let mut first = WorkspaceState::default();
        let second = WorkspaceState::default();
        first.changes_panel.tree = true;
        first.changes_panel.toggle_path(PathBuf::from("src"));
        first.repository_panel.tree = false;

        assert_eq!(visible_path_list(&tree, &first.changes_panel), [PathBuf::from("src")]);
        assert_eq!(visible_path_list(&tree, &first.files_panel), [PathBuf::from("src"), PathBuf::from("src/file.rs")]);
        assert_eq!(visible_path_list(&tree, &second.files_panel), [PathBuf::from("src"), PathBuf::from("src/file.rs")]);
        assert!(!second.changes_panel.tree);
        assert!(second.changes_panel.collapsed_path_set.is_empty());
        assert!(second.repository_panel.tree);
        assert!(first.repository_panel.collapsed_path_set.is_empty());
    }
}
