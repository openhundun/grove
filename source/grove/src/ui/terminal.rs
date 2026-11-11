use super::*;

impl Grove {
    pub(super) fn sessions(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .child(self.chrome("Sessions"))
            .children(SESSION_LIST.into_iter().enumerate().map(|(index, label)| {
                ListItem::new(("session", index))
                    .accessibility_label(label)
                    .selected(self.state().session == index)
                    .h(px(26.))
                    .flex_shrink_0()
                    .py_0()
                    .px(px(8.))
                    .rounded_none()
                    .text_size(px(11.5))
                    .child(
                        h_flex()
                            .w_full()
                            .h_full()
                            .min_w_0()
                            .gap(px(7.))
                            .child(icon_view(IconName::Terminal, 12., MUTED))
                            .child(label)
                            .child(div().flex_1())
                            .child(
                                div()
                                    .text_size(px(9.))
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
