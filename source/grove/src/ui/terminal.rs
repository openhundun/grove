//! The lower dock hosts the real terminal while the upper editor stays visible.

use super::*;

impl Grove {
    pub(super) fn terminal_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let key = self.view().session_key(self.state().session);
        let session = self.view().session_map.get(&key);
        let title = session.and_then(|session| session.terminal.as_ref()).map(|terminal| terminal.read(cx).title().clone());
        let mut head = self
            .chrome("Terminal")
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(rgb(MUTED))
                    .when(title.is_some(), |this| this.refine_style(&self.settings.terminal.style()))
                    .child(title.unwrap_or_else(|| "Terminal".into())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .refine_style(&self.settings.terminal.style())
                    .child(key.cwd.to_string_lossy().into_owned()),
            );
        if let Some(session) = session.filter(|session| session.status(cx) != "PTY") {
            let status = session.terminal.as_ref().and_then(|terminal| terminal.read(cx).exit_status());
            let label = status.as_ref().map_or_else(|| "Start failed".to_owned(), |status| format!("{status}"));
            head = head
                .child(
                    div()
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .text_color(rgb(MUTED))
                        .when(status.is_some(), |this| this.refine_style(&self.settings.terminal.style()))
                        .child(label),
                )
                .child(
                    self.icon_button("restart-terminal", IconName::RefreshCw, "Restart terminal session")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let view = &mut this.workspace_list[this.active];
                            view.session_map.remove(&key);
                            view.terminal_focus = true;
                            cx.notify();
                        })),
                );
        }
        head = head.child(self.refresh_button("refresh-terminal", "Refresh terminal", RefreshTarget::Terminal, cx));
        let body = if let Some(terminal) = session.and_then(|session| session.terminal.as_ref()) {
            div().size_full().min_w_0().min_h_0().child(terminal.clone()).into_any_element()
        } else {
            self.notice(session.and_then(|session| session.error.clone()).unwrap_or_else(|| "Starting terminal…".into()))
        };
        v_flex()
            .size_full()
            .child(head)
            .child(div().flex_1().min_h_0().min_w_0().overflow_hidden().child(body))
            .into_any_element()
    }

    pub(super) fn sessions(&self, cx: &mut Context<Self>) -> AnyElement {
        let typography = &self.settings.terminal;
        v_flex()
            .size_full()
            .child(
                self.chrome("Sessions")
                    .child(div().flex_1())
                    .child(self.refresh_button("refresh-sessions", "Refresh terminal sessions", RefreshTarget::Terminal, cx)),
            )
            .children(SESSION_LIST.into_iter().enumerate().map(|(index, label)| {
                ListItem::new(("session", index))
                    .accessibility_label(label)
                    .selected(self.state().session == index)
                    .h(px(typography.row_height()))
                    .flex_shrink_0()
                    .py_0()
                    .px(px(8.))
                    .rounded_none()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .w_full()
                            .h_full()
                            .min_w_0()
                            .items_center()
                            .gap(px(7.))
                            .refine_style(&typography.style())
                            .child(div().flex_shrink_0().child(icon_view(IconName::Terminal, 12., MUTED)))
                            .child(div().flex_shrink_0().whitespace_nowrap().child(label))
                            .child(div().flex_1())
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .whitespace_nowrap()
                                    .text_color(rgb(META))
                                    .child(self.view().session_map.get(&self.view().session_key(index)).map_or("Start", |session| session.status(cx))),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state_mut().session = index;
                        this.workspace_list[this.active].terminal_focus = true;
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }
}
