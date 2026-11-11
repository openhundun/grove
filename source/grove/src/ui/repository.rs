use super::*;

impl Grove {
    pub(super) fn repositories(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row_list = Vec::new();
        if let Some(snapshot) = &self.view().snapshot {
            for (repo_index, repo) in snapshot.repository_list.iter().enumerate() {
                let collapsed = self.state().collapsed_repo_set.contains(&repo_index);
                row_list.push(
                    h_flex()
                        .id(("repo", repo_index))
                        .h(px(ROW_HEIGHT))
                        .px(px(8.))
                        .gap(px(6.))
                        .items_center()
                        .cursor_pointer()
                        .hover(|this| this.bg(rgb(SURFACE)))
                        .child(icon_view(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown }, 11., MUTED))
                        .child(icon::folder(&repo.path, !collapsed, repo.workspace_root, 14.))
                        .child(div().min_w_0().flex_1().truncate().child(repo.name.clone()))
                        .aria_label(format!("{} · {}", repo.path.display(), repo.common_dir.display()))
                        .when(repo.workspace_root, |this| {
                            this.child(div().text_size(px(9.)).px(px(4.)).border_1().border_color(rgb(BORDER)).text_color(rgb(META)).child("Workspace"))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.state_mut().collapsed_repo_set.remove(&repo_index) {
                                this.state_mut().collapsed_repo_set.insert(repo_index);
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                if collapsed {
                    continue;
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
                    row_list.push(
                        h_flex()
                            .id(("worktree", repo_index * 100000 + worktree_index))
                            .role(Role::Button)
                            .aria_label(worktree.path.to_string_lossy().into_owned())
                            .h(px(ROW_HEIGHT))
                            .pl(px(28.))
                            .pr(px(8.))
                            .gap(px(6.))
                            .items_center()
                            .cursor_pointer()
                            .border_l_2()
                            .border_color(rgb(if selected { ACCENT } else { BG }))
                            .when(selected, |this| this.bg(rgb(SELECT)))
                            .hover(|this| this.bg(rgb(SURFACE)))
                            .child(icon_view(IconName::GitBranch, 12., 0x1a7f6b))
                            .child(div().flex_1().min_w_0().truncate().child(worktree.label()))
                            .when(worktree.locked, |this| this.child(div().text_color(rgb(META)).child("locked")))
                            .when_some(counts, |this, counts| this.child(div().text_size(px(9.)).text_color(rgb(META)).child(counts)))
                            .on_click(cx.listener(move |this, _, _, cx| this.select_worktree(repo_index, worktree_index, cx)))
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
        v_flex().size_full().child(self.chrome("Repositories")).child(
            v_flex()
                .id("repository-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.view().repo_scroll)
                .children(row_list),
        )
    }
}
