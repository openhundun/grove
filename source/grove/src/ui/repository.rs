use super::*;

impl Grove {
    pub(super) fn repositories(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.active;
        let panel = &self.state().repository_panel;
        let tree = panel.tree;
        let font_family = self.settings.repositories_font.clone();
        let head = self.chrome("Repositories").child(div().flex_1()).child(
            h_flex()
                .tab_group()
                .gap(px(2.))
                .child(
                    Button::new("repositories-flat")
                        .icon(IconName::List)
                        .accessibility_label("Repositories: flat worktree list")
                        .ghost()
                        .xsmall()
                        .selected(!tree)
                        .toggled(!tree)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.workspace_list[workspace].state.repository_panel.tree = false;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("repositories-tree")
                        .icon(IconName::ListTree)
                        .accessibility_label("Repositories: repository and worktree tree")
                        .ghost()
                        .xsmall()
                        .selected(tree)
                        .toggled(tree)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.workspace_list[workspace].state.repository_panel.tree = true;
                            cx.notify();
                        })),
                ),
        );
        let mut row_list = Vec::new();
        if let Some(snapshot) = &self.view().snapshot {
            for (repo_index, repo) in snapshot.repository_list.iter().enumerate() {
                if tree {
                    let collapsed = panel.collapsed_path_set.contains(&repo.common_dir);
                    row_list.push(
                        ListItem::new(("repo", repo_index))
                            .accessibility_label(format!("{} · {}", repo.path.display(), repo.common_dir.display()))
                            .role(Role::TreeItem)
                            .aria_level(1)
                            .aria_expanded(!collapsed)
                            .tab_index(0)
                            .h(px(ROW_HEIGHT))
                            .px(px(8.))
                            .py_0()
                            .rounded_none()
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap(px(6.))
                                    .items_center()
                                    .child(icon_view(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown }, 11., MUTED))
                                    .child(icon::folder(&repo.path, !collapsed, repo.workspace_root, 14.))
                                    .child(div().min_w_0().flex_1().truncate().font_family(font_family.clone()).text_size(px(10.)).child(repo.name.clone()))
                                    .when(repo.workspace_root, |this| {
                                        this.child(div().text_size(px(9.)).px(px(4.)).border_1().border_color(rgb(BORDER)).text_color(rgb(META)).child("Workspace"))
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
                    let label = worktree.label();
                    row_list.push(
                        ListItem::new(("worktree", repo_index * 100000 + worktree_index))
                            .accessibility_label(format!("{} · {label} · {}", repo.name, worktree.path.display()))
                            .role(if tree { Role::TreeItem } else { Role::ListBoxOption })
                            .aria_level(if tree { 2 } else { 1 })
                            .aria_selected(selected)
                            .tab_index(0)
                            .selected(selected)
                            .h(px(ROW_HEIGHT))
                            .pl(px(if tree { 28. } else { 8. }))
                            .pr(px(8.))
                            .py_0()
                            .rounded_none()
                            .border_l_2()
                            .border_color(rgb(if selected { ACCENT } else { BG }))
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap(px(6.))
                                    .items_center()
                                    .child(icon_view(IconName::GitBranch, 12., 0x1a7f6b))
                                    .child(
                                        h_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .gap(px(6.))
                                            .when(!tree, |this| {
                                                this.child(
                                                    div()
                                                        .min_w_0()
                                                        .truncate()
                                                        .font_family(font_family.clone())
                                                        .text_size(px(10.))
                                                        .text_color(rgb(MUTED))
                                                        .child(repo.name.clone()),
                                                )
                                            })
                                            .child(div().flex_1().min_w_0().truncate().font_family(font_family.clone()).text_size(px(10.)).child(label)),
                                    )
                                    .when(worktree.locked, |this| this.child(div().text_color(rgb(META)).child("locked")))
                                    .when_some(counts, |this, counts| {
                                        this.child(div().font_family(font_family.clone()).text_size(px(9.)).text_color(rgb(META)).child(counts))
                                    }),
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
                row_list.push(self.notice("No repositories found within this workspace"));
            }
        } else {
            row_list.push(self.notice("Discovering repositories and worktrees…"));
        }
        v_flex().size_full().child(head).child(
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
