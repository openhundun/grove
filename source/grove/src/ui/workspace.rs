use super::*;

/// Removing an inactive tab preserves selection; an active tab selects its right neighbor, then left.
fn active_after_close(active: usize, closed: usize, count: usize) -> Option<usize> {
    if count == 1 {
        None
    } else if active == closed {
        Some(closed.min(count - 2))
    } else {
        Some(active - usize::from(closed < active))
    }
}

impl Grove {
    /// Browser-style workspace tabs share the native titlebar with traffic lights and empty drag space.
    pub(super) fn title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new().h(px(CHROME)).bg(rgb(SURFACE)).border_color(rgb(BORDER)).child(
            h_flex()
                .size_full()
                .items_center()
                .gap(px(3.))
                .child(
                    h_flex()
                        .min_w_0()
                        .max_w_full()
                        .gap(px(3.))
                        .child(
                            h_flex()
                                .id("workspace-tabs")
                                .min_w_0()
                                .gap(px(3.))
                                .overflow_x_scroll()
                                .children(self.workspace_list.iter().enumerate().map(|(index, workspace)| {
                                    let label = workspace.root.file_name().unwrap_or_default().to_string_lossy().into_owned();
                                    let workspace_id = workspace.id;
                                    let close_label = format!("Close workspace {label}");
                                    h_flex()
                                        .id(("workspace", workspace_id))
                                        .role(Role::Tab)
                                        .aria_label(label.clone())
                                        .aria_selected(index == self.active)
                                        .tab_stop(true)
                                        .h(px(CHROME))
                                        .min_w(px(72.))
                                        .max_w(px(200.))
                                        .px(px(8.))
                                        .gap(px(6.))
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .border_1()
                                        .border_color(rgb(if index == self.active { 0xd2d2d7 } else { SURFACE }))
                                        .when(index == self.active, |this| this.bg(rgb(BG)))
                                        .hover(|this| this.bg(rgb(0xebebef)))
                                        .child(div().size(px(6.)).flex_shrink_0().rounded_full().bg(rgb(if index == 0 { 0x58c4b0 } else { 0x9b8afb })))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .flex_1()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .refine_style(&self.settings.ui.style())
                                                .text_color(rgb(if index == self.active { FG } else { MUTED }))
                                                .child(label),
                                        )
                                        .child(
                                            div()
                                                .id(("close-workspace-control", workspace_id))
                                                .flex_shrink_0()
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                                .child(
                                                    Button::new(("close-workspace", workspace_id))
                                                        .child(icon_view(IconName::X, 12., SECONDARY))
                                                        .accessibility_label(close_label)
                                                        .ghost()
                                                        .xsmall()
                                                        .w(px(16.))
                                                        .h(px(16.))
                                                        .p_0()
                                                        .on_click(cx.listener(move |this, _, window, cx| {
                                                            cx.stop_propagation();
                                                            this.close_workspace(workspace_id, window, cx);
                                                        })),
                                                ),
                                        )
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            cx.stop_propagation();
                                            if let Some(index) = workspace_index(&this.workspace_list, workspace_id) {
                                                this.activate_workspace(index, window, cx);
                                            }
                                        }))
                                })),
                        )
                        .child(
                            div()
                                .id("add-workspace-control")
                                .flex_shrink_0()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(
                                    Button::new("add-workspace")
                                        .child(icon_view(IconName::Plus, 12., SECONDARY))
                                        .accessibility_label("Add workspace")
                                        .ghost()
                                        .xsmall()
                                        .w(px(24.))
                                        .h(px(CHROME))
                                        .p_0()
                                        .rounded_none()
                                        .flex_shrink_0()
                                        .refine_style(&self.settings.ui.style())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.add_workspace(window, cx);
                                        })),
                                ),
                        ),
                )
                .child(div().flex_1().min_w(px(32.)).h_full()),
        )
    }

    fn activate_workspace(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.active = index;
        self.workspace_list[index].terminal_focus = self.state().terminal_visible();
        self.drag_axis = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Closing is atomic after confirmation: cancellation leaves tab state and its PTYs untouched.
    fn close_workspace(&mut self, workspace_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = workspace_index(&self.workspace_list, workspace_id) else { return };
        if !self.workspace_list[index].session_list.iter().any(|session| session.is_live(cx)) {
            self.finish_close_workspace(workspace_id, window, cx);
            return;
        }
        let confirmation = window.prompt(
            PromptLevel::Warning,
            "Close workspace and end its terminal sessions?",
            Some("Running terminal programs in this workspace will be stopped."),
            &[PromptButton::cancel("Cancel"), PromptButton::ok("Close workspace")],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if confirmation.await == Ok(1) {
                let _ = this.update_in(cx, |this, window, cx| this.finish_close_workspace(workspace_id, window, cx));
            }
        })
        .detach();
    }

    fn finish_close_workspace(&mut self, workspace_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = workspace_index(&self.workspace_list, workspace_id) else { return };
        let next_active = active_after_close(self.active, index, self.workspace_list.len());
        let view = &mut self.workspace_list[index];
        view.task = None;
        for session in view.session_list.iter_mut() {
            session.dispose(cx);
        }
        // Keep the final view valid until the native window releases its root; never render an empty list.
        let Some(next_active) = next_active else {
            window.remove_window();
            return;
        };
        self.workspace_list.remove(index);
        if self.active == index {
            self.activate_workspace(next_active, window, cx);
        } else {
            self.active = next_active;
            cx.notify();
        }
    }

    /// A native directory chooser adds isolated workspace state; canonical duplicates reuse their tab.
    fn add_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source_workspace_id = self.view().id;
        let picker = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Add workspace".into()) });
        cx.spawn_in(window, async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(Some(path_list))) => {
                    let Some(root) = path_list.into_iter().next() else { return };
                    cx.background_spawn(async move {
                        let root = root.canonicalize().map_err(|error| format!("{}: {error}", root.display()))?;
                        if !root.is_dir() {
                            return Err(format!("{} is not a directory", root.display()));
                        }
                        Ok(root)
                    })
                    .await
                }
                Ok(Ok(None)) => return,
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(root) => {
                    let index = if let Some(index) = this.workspace_list.iter().position(|workspace| workspace.root == root) {
                        index
                    } else {
                        let index = this.workspace_list.len();
                        this.workspace_list.push(WorkspaceView::new(root, &mut this.next_workspace_id, &this.settings));
                        this.refresh(index, cx);
                        index
                    };
                    this.activate_workspace(index, window, cx);
                }
                Err(error) => {
                    if let Some(index) = workspace_index(&this.workspace_list, source_workspace_id) {
                        this.workspace_list[index].error = Some(error);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
    pub(super) fn rail(&self, upper_height: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let button = |index: usize, icon: IconName, label: &'static str, selected: bool| {
            div()
                .id(("mode", index))
                .role(Role::Button)
                .aria_label(label)
                .size(px(CHROME))
                .flex_shrink_0()
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
                        .border_color(rgb(if selected { 0xa9c8ed } else { SURFACE }))
                        .when(selected, |this| this.bg(rgb(SELECT)))
                        .child(icon_view(icon, 16., if selected { ACCENT } else { MUTED })),
                )
        };
        v_flex()
            .relative()
            .w(px(CHROME))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(BORDER))
            .children(
                [
                    (Mode::Changes, IconName::GitCompare, grove_platform::MODE_LABEL_LIST[0]),
                    (Mode::Files, IconName::Folder, grove_platform::MODE_LABEL_LIST[1]),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (mode, icon, label))| {
                    button(index, icon, label, self.state().mode == mode).on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.focus, cx);
                        this.set_mode(mode, cx);
                    }))
                }),
            )
            .child(
                v_flex()
                    .absolute()
                    .left_0()
                    .when(self.state().dock_visible, |this| this.top(px(upper_height)))
                    .when(!self.state().dock_visible, |this| this.bottom_0())
                    .children(
                        [
                            (DockMode::History, IconName::GitBranch, grove_platform::MODE_LABEL_LIST[3]),
                            (DockMode::Terminal, IconName::Terminal, grove_platform::MODE_LABEL_LIST[2]),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (mode, icon, label))| {
                            button(index + 2, icon, label, self.state().dock_visible && self.state().dock_mode == mode).on_click(cx.listener(move |this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.toggle_dock(mode, cx);
                                if this.state().terminal_visible() {
                                    this.ensure_terminal(window, cx);
                                }
                            }))
                        }),
                    ),
            )
    }
    pub(super) fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (memory_label, memory_accessibility_label) = match self.memory_bytes {
            Some(bytes) => {
                let label = format!("{:.1} MiB", bytes as f64 / (1024. * 1024.));
                let accessibility_label = format!("Current process resident memory: {label}");
                (label, accessibility_label)
            }
            None => ("— MiB".to_owned(), "Current process resident memory unavailable".to_owned()),
        };
        h_flex()
            .h(px(CHROME))
            .flex_shrink_0()
            .items_center()
            .pr(px(10.))
            .bg(rgb(SURFACE))
            .border_t_1()
            .border_color(rgb(BORDER))
            .refine_style(&self.settings.secondary_ui_style())
            .child(
                div().w(px(CHROME)).h(px(CHROME)).flex_shrink_0().flex().items_center().justify_center().child(
                    self.icon_button("settings", IconName::Settings, "Settings")
                        .selected(self.settings_open)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.settings_open {
                                this.close_settings(window, cx);
                            } else {
                                this.open_settings(window, cx);
                            }
                        })),
                ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("process-memory")
                    .role(Role::Label)
                    .aria_label(memory_accessibility_label)
                    .text_color(rgb(META))
                    .child(memory_label),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::active_after_close;

    #[test]
    fn closing_inactive_tab_preserves_selected_workspace() {
        assert_eq!(active_after_close(2, 0, 3), Some(1));
        assert_eq!(active_after_close(0, 2, 3), Some(0));
    }

    #[test]
    fn closing_active_tab_selects_adjacent_or_closes_window() {
        assert_eq!(active_after_close(0, 0, 3), Some(0));
        assert_eq!(active_after_close(1, 1, 3), Some(1));
        assert_eq!(active_after_close(2, 2, 3), Some(1));
        assert_eq!(active_after_close(0, 0, 1), None);
    }
}
