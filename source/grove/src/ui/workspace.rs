use super::*;

impl Grove {
    pub(super) fn title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new().h(px(TITLE_HEIGHT)).bg(rgb(SURFACE)).border_color(rgb(BORDER)).child(
            h_flex()
                .size_full()
                .items_center()
                .gap(px(10.))
                .child(div().text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).child("Grove"))
                .child(h_flex().flex_1().min_w_0().gap(px(3.)).children(self.workspace_list.iter().enumerate().map(|(index, workspace)| {
                    let label = workspace.root.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    h_flex()
                        .id(("workspace", index))
                        .role(Role::Tab)
                        .aria_label(label.clone())
                        .tab_stop(true)
                        .h(px(22.))
                        .px(px(8.))
                        .gap(px(6.))
                        .rounded(px(4.))
                        .cursor_pointer()
                        .border_1()
                        .border_color(rgb(if index == self.active { 0xd2d2d7 } else { SURFACE }))
                        .when(index == self.active, |this| this.bg(rgb(BG)))
                        .hover(|this| this.bg(rgb(0xebebef)))
                        .child(div().size(px(6.)).rounded_full().bg(rgb(if index == 0 { 0x58c4b0 } else { 0x9b8afb })))
                        .child(div().text_size(px(11.)).truncate().text_color(rgb(if index == self.active { FG } else { MUTED })).child(label))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.active = index;
                            this.workspace_list[index].terminal_focus = this.state().mode == Mode::Terminal;
                            this.drag_axis = None;
                            window.focus(&this.focus, cx);
                            cx.notify();
                        }))
                })))
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(META))
                        .child(if self.view().loading { "Reading… · Git read-only" } else { "Local Git · Git read-only" }),
                )
                .child(
                    Button::new("refresh-workspace")
                        .icon(IconName::RefreshCw)
                        .ghost()
                        .xsmall()
                        .tooltip("Re-read this workspace; no fetch or Git writes")
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(this.active, cx))),
                )
                .child(div().w(px(8.))),
        )
    }
    pub(super) fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(px(RAIL_WIDTH)).h_full().flex_shrink_0().bg(rgb(SURFACE)).border_r_1().border_color(rgb(BORDER)).children(
            [
                (Mode::Changes, IconName::GitCompare, grove_platform::MODE_LABEL_LIST[0]),
                (Mode::Files, IconName::Folder, grove_platform::MODE_LABEL_LIST[1]),
                (Mode::Terminal, IconName::Terminal, grove_platform::MODE_LABEL_LIST[2]),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (mode, icon, label))| {
                div()
                    .id(("mode", index))
                    .role(Role::Button)
                    .aria_label(label)
                    .size(px(RAIL_WIDTH))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(3.))
                            .border_1()
                            .border_color(rgb(if self.state().mode == mode { 0xa9c8ed } else { SURFACE }))
                            .when(self.state().mode == mode, |this| this.bg(rgb(SELECT)))
                            .child(icon_view(icon, 15., if self.state().mode == mode { ACCENT } else { MUTED })),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.focus, cx);
                        this.set_mode(mode, cx);
                    }))
            }),
        )
    }
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.view();
        let path = view.selected_worktree.as_ref().unwrap_or(&view.root).clone();
        let label = view
            .worktree_data
            .as_ref()
            .and_then(|data| data.branch.clone())
            .or_else(|| view.worktree().map(WorktreeSnapshot::label))
            .unwrap_or_default();
        let count = view.snapshot.as_ref().map_or(0, |data| data.repository_list.len());
        let warning_count = view.snapshot.as_ref().map_or(0, |data| data.warning_list.len()) + view.worktree_data.as_ref().map_or(0, |data| data.warning_list.len());
        h_flex()
            .h(px(STATUS_HEIGHT))
            .flex_shrink_0()
            .items_center()
            .gap(px(8.))
            .pr(px(10.))
            .bg(rgb(SURFACE))
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(Button::new("settings").icon(IconName::Settings).ghost().xsmall().disabled(true).tooltip("Settings are not implemented"))
            .child(icon_view(IconName::GitBranch, 12., ACCENT))
            .child(label)
            .child(
                div()
                    .id("copy-worktree-path")
                    .cursor_pointer()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(MONO)
                    .text_size(px(9.5))
                    .text_color(rgb(META))
                    .child(path.to_string_lossy().into_owned())
                    .on_click(cx.listener(move |_, _, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(path.to_string_lossy().into_owned())))),
            )
            .child(
                div()
                    .text_size(px(9.5))
                    .text_color(rgb(META))
                    .child(format!("{count} repositories · {warning_count} warnings · Git read-only · Terminal")),
            )
    }
}
