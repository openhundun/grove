//! Tree roots and worktrees share control columns; nesting offsets the entire child node.

use super::*;

impl Grove {
    pub(super) fn repositories(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.active;
        let panel = &self.state().repository_panel;
        let tree = panel.tree;
        let typography = &self.settings.repositories;
        let row_height = typography.row_height();
        let head = self.chrome("Repositories").child(div().flex_1()).child(
            h_flex()
                .tab_group()
                .flex_shrink_0()
                .child(
                    self.icon_button(
                        "repositories-layout",
                        if tree { IconName::ListTree } else { IconName::List },
                        if tree {
                            "Repositories: repository tree; switch to flat worktree list"
                        } else {
                            "Repositories: flat worktree list; switch to repository tree"
                        },
                    )
                    .toggled(tree)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_panel_layout(workspace, FontTarget::Repositories, !tree, cx);
                    })),
                )
                .child(self.refresh_button("refresh-repositories", "Refresh Repositories", RefreshTarget::Repositories, cx)),
        );
        let mut row_list = Vec::new();
        if let Some(snapshot) = &self.view().snapshot {
            for (repo_index, repo) in snapshot.repository_list.iter().enumerate() {
                let repo_matches = panel.matches_query(&repo.name) || panel.matches_query(repo.path.to_string_lossy().as_ref());
                if !repo_matches
                    && !repo
                        .worktree_list
                        .iter()
                        .any(|worktree| panel.matches_query(worktree.path.to_string_lossy().as_ref()) || panel.matches_query(&worktree.label()))
                {
                    continue;
                }
                if tree {
                    let collapsed = panel.query.is_empty() && panel.collapsed_path_set.contains(&repo.common_dir);
                    row_list.push(
                        ListItem::new(("repo", repo_index))
                            .accessibility_label(format!("{} · {}", repo.path.display(), repo.common_dir.display()))
                            .role(Role::TreeItem)
                            .aria_level(1)
                            .aria_expanded(!collapsed)
                            .tab_index(0)
                            .h(px(row_height))
                            .flex_shrink_0()
                            .px(px(8.))
                            .py_0()
                            .rounded_none()
                            .overflow_hidden()
                            .border_l_2()
                            .border_color(rgba(0x00000000))
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap(px(6.))
                                    .items_center()
                                    .refine_style(&typography.style())
                                    .child(div().flex().w(px(16.)).items_center().justify_center().flex_shrink_0().child(icon_view(
                                        if collapsed { IconName::ChevronRight } else { IconName::ChevronDown },
                                        11.,
                                        MUTED,
                                    )))
                                    .child(
                                        div()
                                            .flex()
                                            .w(px(16.))
                                            .flex_shrink_0()
                                            .items_center()
                                            .justify_center()
                                            .child(icon::folder(&repo.path, !collapsed, repo.workspace_root, 14.)),
                                    )
                                    .child(div().flex_shrink_0().whitespace_nowrap().child(repo.name.clone()))
                                    .when(repo.workspace_root, |this| {
                                        this.child(
                                            div()
                                                .flex_shrink_0()
                                                .whitespace_nowrap()
                                                .px(px(4.))
                                                .border_1()
                                                .border_color(rgb(BORDER))
                                                .text_color(rgb(META))
                                                .child("Workspace"),
                                        )
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.fold_repository(workspace, repo_index, None, cx)))
                            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                let collapsed = match event.keystroke.key.as_str() {
                                    "enter" | "space" => None,
                                    "left" => Some(true),
                                    "right" => Some(false),
                                    _ => return,
                                };
                                this.fold_repository(workspace, repo_index, collapsed, cx);
                                cx.stop_propagation();
                            }))
                            .into_any_element(),
                    );
                    if collapsed {
                        continue;
                    }
                }
                for (worktree_index, worktree) in repo.worktree_list.iter().enumerate() {
                    let label = worktree.label();
                    if !repo_matches && !panel.matches_query(worktree.path.to_string_lossy().as_ref()) && !panel.matches_query(&label) {
                        continue;
                    }
                    let selected = self.state().repo == repo_index && self.state().worktree == worktree_index;
                    let counts = if selected {
                        self.view().worktree_data.as_ref().map(|data| {
                            format!(
                                "↑{} ↓{}",
                                data.ahead.map(|value| value.to_string()).unwrap_or_else(|| "—".into()),
                                data.behind.map(|value| value.to_string()).unwrap_or_else(|| "—".into())
                            )
                        })
                    } else {
                        None
                    };
                    row_list.push(
                        ListItem::new(("worktree", repo_index * 100000 + worktree_index))
                            .accessibility_label(format!("{} · {label} · {}", repo.name, worktree.path.display()))
                            .role(if tree { Role::TreeItem } else { Role::ListBoxOption })
                            .aria_level(if tree { 2 } else { 1 })
                            .aria_selected(selected)
                            .tab_index(0)
                            .selected(selected)
                            .h(px(row_height))
                            .flex_shrink_0()
                            .px(px(8.))
                            .py_0()
                            .rounded_none()
                            .overflow_hidden()
                            .border_l_2()
                            .border_color(rgb(if selected { ACCENT } else { BG }))
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap(px(6.))
                                    .items_center()
                                    .refine_style(&typography.style())
                                    .when(tree, |this| this.pl(px(self.settings.repository_panel.indent_size)).child(div().w(px(16.)).flex_shrink_0()))
                                    .child(
                                        self.icon_button(
                                            ("open-worktree-files", repo_index * 100000 + worktree_index),
                                            IconName::GitBranch,
                                            format!("Open Files for {label} · {}", worktree.path.display()),
                                        )
                                        .tab_index(0)
                                        .w(px(16.))
                                        .h(px(row_height))
                                        .text_color(rgb(0x1a7f6b))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.open_worktree_files(repo_index, worktree_index, cx);
                                        }))
                                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                                cx.stop_propagation();
                                                this.open_worktree_files(repo_index, worktree_index, cx);
                                            }
                                        })),
                                    )
                                    .child(
                                        h_flex()
                                            .flex_shrink_0()
                                            .whitespace_nowrap()
                                            .gap(px(6.))
                                            .when(!tree, |this| this.child(div().flex_shrink_0().whitespace_nowrap().text_color(rgb(MUTED)).child(repo.name.clone())))
                                            .child(div().flex_shrink_0().whitespace_nowrap().child(label)),
                                    )
                                    .when(worktree.locked, |this| this.child(div().flex_shrink_0().whitespace_nowrap().text_color(rgb(META)).child("locked")))
                                    .when_some(counts, |this, counts| this.child(div().flex_shrink_0().whitespace_nowrap().text_color(rgb(META)).child(counts))),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.select_worktree(repo_index, worktree_index, cx)))
                            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    this.select_worktree(repo_index, worktree_index, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .into_any_element(),
                    );
                }
            }
            if row_list.is_empty() {
                row_list.push(self.notice(if panel.query.is_empty() {
                    "No repositories found within this workspace"
                } else {
                    "No repositories or worktrees match this search"
                }));
            }
        } else {
            row_list.push(self.notice("Discovering repositories and worktrees…"));
        }
        v_flex().size_full().child(head).child(self.search_bar(SearchPanel::Repositories, cx)).child(
            v_flex()
                .id("repository-list")
                .role(if tree { Role::Tree } else { Role::ListBox })
                .aria_label("Repositories")
                .tab_group()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.view().repo_scroll)
                .children(row_list),
        )
    }

    fn fold_repository(&mut self, workspace: usize, repo_index: usize, collapsed: Option<bool>, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[workspace];
        let Some(repo) = view.snapshot.as_ref().and_then(|snapshot| snapshot.repository_list.get(repo_index)) else {
            return;
        };
        let path = repo.common_dir.clone();
        let panel = &mut view.state.repository_panel;
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
