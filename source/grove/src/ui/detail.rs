use super::*;
use gpui_kit::component::text::TextViewStyle;

impl Grove {
    pub(super) fn detail(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.state().mode == Mode::Terminal {
            let key = self.view().session_key(self.state().session);
            let session = self.view().session_map.get(&key);
            let title = session.and_then(|session| session.terminal.as_ref()).map(|terminal| terminal.read(cx).title().clone());
            let mut head = self
                .chrome("Terminal")
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .text_size(px(10.))
                        .when(title.is_some(), |this| this.font_family(self.settings.terminal_font.clone()))
                        .child(title.unwrap_or_else(|| "Terminal".into())),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .truncate()
                        .font_family(self.settings.terminal_font.clone())
                        .text_size(px(9.))
                        .child(key.cwd.to_string_lossy().into_owned()),
                );
            if let Some(session) = session.filter(|session| session.status(cx) != "PTY") {
                let status = session.terminal.as_ref().and_then(|terminal| terminal.read(cx).exit_status());
                let label = status.as_ref().map_or_else(|| "Start failed".to_owned(), |status| format!("{status}"));
                head = head
                    .child(
                        div()
                            .text_size(px(9.))
                            .text_color(rgb(MUTED))
                            .when(status.is_some(), |this| this.font_family(self.settings.terminal_font.clone()))
                            .child(label),
                    )
                    .child(
                        Button::new("restart-terminal")
                            .label("Restart")
                            .icon(IconName::RefreshCw)
                            .ghost()
                            .xsmall()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let view = &mut this.workspace_list[this.active];
                                view.session_map.remove(&key);
                                view.terminal_focus = true;
                                cx.notify();
                            })),
                    );
            }
            let body = if let Some(terminal) = session.and_then(|session| session.terminal.as_ref()) {
                div().size_full().min_w_0().min_h_0().child(terminal.clone()).into_any_element()
            } else {
                self.notice(session.and_then(|session| session.error.clone()).unwrap_or_else(|| "Starting terminal…".into()))
            };
            return v_flex()
                .size_full()
                .child(head)
                .child(div().flex_1().min_h_0().min_w_0().overflow_hidden().child(body))
                .into_any_element();
        }
        if let Some(error) = &self.view().error {
            return self.notice(error.clone());
        }
        if self.state().mode == Mode::Changes && self.state().file == FileSelection::Message {
            let Some(history) = &self.view().history_data else {
                return self.notice("Reading commit…");
            };
            let preview = self.state().preview;
            let workspace = self.active;
            let head = self.chrome("Message").child(div().flex_1()).child(
                h_flex()
                    .tab_group()
                    .gap(px(2.))
                    .child(
                        Button::new("message-source")
                            .icon(IconName::FileCode)
                            .accessibility_label("Commit message source")
                            .ghost()
                            .xsmall()
                            .selected(!preview)
                            .toggled(!preview)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.workspace_list[workspace].state.preview = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("message-preview")
                            .icon(IconName::Eye)
                            .accessibility_label("Commit message preview")
                            .ghost()
                            .xsmall()
                            .selected(preview)
                            .toggled(preview)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.workspace_list[workspace].state.preview = true;
                                cx.notify();
                            })),
                    ),
            );
            let body = if preview {
                let font_family = self.settings.editor_font.clone();
                div()
                    .id("message-markdown")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(18.))
                    .child(
                        TextView::markdown("message", history.message.clone())
                            .selectable(true)
                            .font_family(font_family.clone())
                            .inline_code_style(StyleRefinement::default().font_family(font_family.clone()))
                            .style(TextViewStyle::default().code_block(StyleRefinement::default().font_family(font_family.clone()).pt(px(28.))))
                            .code_block_actions(move |code, _, cx| {
                                div()
                                    .font_family(font_family.clone())
                                    .text_size(gpui_kit::base::Theme::global(cx).tokens.typography.xs.size)
                                    .line_height(px(16.))
                                    .text_color(gpui_kit::component::ActiveTheme::theme(cx).muted_foreground)
                                    .children(code.lang().filter(|lang| !lang.is_empty()))
                            }),
                    )
                    .into_any_element()
            } else if let Some(editor) = &self.view().message_editor {
                editor_view(editor).into_any_element()
            } else {
                self.notice("Reading message…")
            };
            return v_flex().size_full().child(head).child(div().flex_1().min_h_0().overflow_hidden().child(body)).into_any_element();
        }
        let Some(file) = self.view().file_key.as_ref().and_then(|key| self.view().file_map.get(key)) else {
            return self.notice(if self.view().loading { "Reading selected file…" } else { "Select a file to inspect" });
        };
        if self.state().mode == Mode::Files {
            let head = self.chrome("Preview").child(div().flex_1()).child(
                div()
                    .truncate()
                    .font_family(self.settings.files_font.clone())
                    .text_size(px(9.5))
                    .child(file.data.path.to_string_lossy().into_owned()),
            );
            let body = file
                .editor
                .as_ref()
                .map(|editor| editor_view(editor).into_any_element())
                .unwrap_or_else(|| self.notice(content_notice(&file.data.content)));
            return v_flex().size_full().child(head).child(div().flex_1().min_h_0().overflow_hidden().child(body)).into_any_element();
        }
        let split = self.state().split;
        let workspace = self.active;
        let head = self.chrome("Diff").child(div().flex_1()).child(
            h_flex()
                .tab_group()
                .gap(px(2.))
                .child(
                    Button::new("diff-unified")
                        .icon(IconName::Rows2)
                        .accessibility_label("Unified diff")
                        .ghost()
                        .xsmall()
                        .selected(!split)
                        .toggled(!split)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.workspace_list[workspace].state.split = false;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("diff-split")
                        .icon(IconName::Columns2)
                        .accessibility_label("Side-by-side diff")
                        .ghost()
                        .xsmall()
                        .selected(split)
                        .toggled(split)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.workspace_list[workspace].state.split = true;
                            cx.notify();
                        })),
                ),
        );
        let body = file
            .diff
            .as_ref()
            .filter(|stretches| !stretches.is_empty())
            .map(|stretches| {
                DiffViewer::from_stretches("file-diff", file.data.path.to_string_lossy().into_owned(), stretches.clone())
                    .headless()
                    .font_family(self.settings.editor_font.clone())
                    .layout(if split { DiffLayout::Split } else { DiffLayout::Unified })
                    .into_any_element()
            })
            .unwrap_or_else(|| {
                if let Some(error) = file.error.as_ref().or(file.data.notice.as_ref()) {
                    self.notice(error.clone())
                } else if let Some(patch) = &file.data.patch {
                    div()
                        .p(px(16.))
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .font_family(self.settings.editor_font.clone())
                        .child(patch.to_string())
                        .into_any_element()
                } else {
                    self.notice(content_notice(&file.data.content))
                }
            });
        v_flex().size_full().child(head).child(div().flex_1().min_h_0().overflow_hidden().child(body)).into_any_element()
    }
}

fn editor_view(editor: &Entity<CodeEditor>) -> impl IntoElement {
    div().size_full().min_w_0().min_h_0().child(editor.clone())
}
fn content_notice(content: &FileContent) -> String {
    match content {
        FileContent::Text(_) => "No textual patch available".into(),
        FileContent::Binary => "Binary or non-UTF-8 content; text preview is unavailable".into(),
        FileContent::Unavailable(error) => error.clone(),
    }
}
