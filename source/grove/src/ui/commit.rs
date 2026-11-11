use super::*;

impl Grove {
    pub(super) fn history_row(&self, selection: HistorySelection, label: String, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id(("history", index))
            .h(px(26.))
            .flex_shrink_0()
            .items_center()
            .cursor_pointer()
            .border_b_1()
            .border_color(rgb(0xf1f1f4))
            .when(self.state().history == selection, |this| this.bg(rgb(SELECT)))
            .hover(|this| this.bg(rgb(SURFACE)))
            .child(div().w(px(36.)).text_center().text_color(rgb(META)).child("○"))
            .child(div().flex_1().truncate().child(label))
            .on_click(cx.listener(move |this, _, _, cx| this.select_history(selection, cx)))
    }
    pub(super) fn commits(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let compare = self.state().compare;
        let mut head = self
            .chrome("Commits")
            .child(div().flex_1())
            .child(self.segment("workspace-scope", "Workspace", !compare).on_click(cx.listener(|this, _, _, cx| {
                this.state_mut().compare = false;
                this.select_history(HistorySelection::Unstaged, cx);
            })))
            .child(self.segment("pr-scope", "PR", compare).on_click(cx.listener(|this, _, _, cx| {
                this.state_mut().compare = true;
                this.select_history(HistorySelection::Comparison, cx);
            })));
        if compare {
            let label = self
                .view()
                .worktree_data
                .as_ref()
                .and_then(|data| data.base_ref_list.get(self.state().base))
                .cloned()
                .unwrap_or_else(|| "No local base ref".into());
            head = head.child(self.segment("compare-base", label, true).on_click(cx.listener(|this, _, _, cx| {
                let count = this.view().worktree_data.as_ref().map_or(0, |data| data.base_ref_list.len());
                if count > 0 {
                    this.state_mut().base = (this.state().base + 1) % count;
                    this.select_history(HistorySelection::Comparison, cx);
                }
            })));
        }
        let mut body = v_flex().id("commit-list").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.view().commit_scroll);
        if let Some(data) = &self.view().worktree_data {
            if compare {
                body = body.child(self.history_row(HistorySelection::Comparison, "Local merge-base → HEAD · no remote PR metadata".into(), 2, cx));
            } else {
                body = body
                    .child(self.history_row(HistorySelection::Unstaged, format!("Unstaged changes ({})", data.unstaged_list.len()), 0, cx))
                    .child(self.history_row(HistorySelection::Staged, format!("Staged changes ({})", data.staged_list.len()), 1, cx));
                let graph_width = self.view().graph_list.iter().map(|row| row.lane_count).max().unwrap_or(1) as f32 * 12. + 20.;
                for (index, commit) in data.commit_list.iter().enumerate() {
                    body = body.child(
                        h_flex()
                            .id(("commit", index))
                            .h(px(26.))
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(8.))
                            .border_b_1()
                            .border_color(rgb(0xf1f1f4))
                            .cursor_pointer()
                            .when(self.state().history == HistorySelection::Commit(index), |this| this.bg(rgb(SELECT)))
                            .hover(|this| this.bg(rgb(SURFACE)))
                            .child(commit_graph(&self.view().graph_list[index], graph_width))
                            .child(div().flex_1().min_w_0().truncate().child(commit.subject.clone()))
                            .child(div().w(px(120.)).truncate().text_color(rgb(MUTED)).text_size(px(10.)).child(commit.author.clone()))
                            .child(div().w(px(76.)).text_color(rgb(META)).text_size(px(9.5)).child(commit.date.clone()))
                            .child(div().w(px(64.)).font_family(MONO).text_size(px(9.)).child(commit.oid[..commit.oid.len().min(8)].to_owned()))
                            .on_click(cx.listener(move |this, _, _, cx| this.select_history(HistorySelection::Commit(index), cx))),
                    );
                }
                if data.commit_list.is_empty() {
                    body = body.child(self.notice("No commits on this worktree (unborn HEAD)"));
                }
            }
        } else {
            body = body.child(self.notice("Select a worktree to read its history"));
        }
        v_flex().size_full().child(head).child(body)
    }
}

fn commit_graph(row: &grove_core::commit_graph::GraphRow, width: f32) -> impl IntoElement {
    let lane = row.lane;
    let incoming = row.incoming;
    let pass_list = row.pass_list.clone();
    let parent_lane_list = row.parent_lane_list.clone();
    div().w(px(width.max(36.))).h_full().flex_shrink_0().child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let x = |lane: usize| bounds.origin.x + px(12. + lane as f32 * 12.);
                let mid = bounds.origin.y + bounds.size.height / 2.;
                let mut path = PathBuilder::stroke(px(1.3));
                for (before, after) in &pass_list {
                    path.move_to(point(x(*before), bounds.origin.y));
                    path.cubic_bezier_to(point(x(*after), bounds.bottom()), point(x(*before), mid), point(x(*after), mid));
                }
                if incoming {
                    path.move_to(point(x(lane), bounds.origin.y));
                    path.line_to(point(x(lane), mid));
                }
                for parent in &parent_lane_list {
                    path.move_to(point(x(lane), mid));
                    path.cubic_bezier_to(point(x(*parent), bounds.bottom()), point(x(lane), mid + px(5.)), point(x(*parent), bounds.bottom() - px(5.)));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, rgb(0xc7c7cc));
                }
                window.paint_quad(quad(
                    Bounds::new(point(x(lane) - px(3.), mid - px(3.)), size(px(6.), px(6.))),
                    px(3.),
                    rgb(if parent_lane_list.len() > 1 { BG } else { ACCENT }),
                    px(if parent_lane_list.len() > 1 { 1.5 } else { 0. }),
                    rgb(ACCENT),
                    BorderStyle::Solid,
                ));
            },
        )
        .size_full(),
    )
}
