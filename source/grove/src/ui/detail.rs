use super::*;

impl Grove {
    pub(super) fn detail(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.state().mode == Mode::Terminal {
            let key = self.view().session_key(self.state().session);
            let session = self.view().session_map.get(&key);
            let title = session
                .and_then(|session| session.terminal.as_ref())
                .map(|terminal| terminal.read(cx).title().clone())
                .unwrap_or_else(|| "Terminal".into());
            let mut head = self
                .chrome("Terminal")
                .child(div().text_color(rgb(MUTED)).text_size(px(10.)).child(title))
                .child(div().flex_1())
                .child(div().truncate().text_size(px(9.)).child(key.cwd.to_string_lossy().into_owned()));
            if let Some(session) = session.filter(|session| session.status(cx) != "PTY") {
                let label = session
                    .terminal
                    .as_ref()
                    .and_then(|terminal| terminal.read(cx).exit_status())
                    .map_or_else(|| "Start failed".to_owned(), |status| format!("{status}"));
                head = head.child(div().text_size(px(9.)).text_color(rgb(MUTED)).child(label)).child(
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
            let owner = cx.entity().downgrade();
            let head = self.chrome("Message").child(div().flex_1()).child(
                SegmentedControl::new("message-layout", if preview { "preview" } else { "source" })
                    .size(ControlSize::Sm)
                    .segment("source", "Source", None)
                    .segment("preview", "Preview", None)
                    .on_change(move |value, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.state_mut().preview = value.as_ref() == "preview";
                            cx.notify();
                        });
                    }),
            );
            let body = if preview {
                div()
                    .id("message-markdown")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(18.))
                    .child(TextView::markdown("message", history.message.clone()).selectable(true))
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
            let head = self
                .chrome("Preview")
                .child(div().flex_1())
                .child(div().truncate().text_size(px(9.5)).child(file.data.path.to_string_lossy().into_owned()));
            let body = file
                .editor
                .as_ref()
                .map(|editor| editor_view(editor).into_any_element())
                .unwrap_or_else(|| self.notice(content_notice(&file.data.content)));
            return v_flex().size_full().child(head).child(div().flex_1().min_h_0().overflow_hidden().child(body)).into_any_element();
        }
        let split = self.state().split;
        let owner = cx.entity().downgrade();
        let head = self.chrome("Diff").child(div().flex_1()).child(
            SegmentedControl::new("diff-layout", if split { "split" } else { "unified" })
                .size(ControlSize::Sm)
                .segment("unified", "Unified", None)
                .segment("split", "Split", None)
                .on_change(move |value, _, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.state_mut().split = value.as_ref() == "split";
                        cx.notify();
                    });
                }),
        );
        let body = file
            .diff
            .as_ref()
            .filter(|stretches| !stretches.is_empty())
            .map(|stretches| {
                DiffViewer::from_stretches("file-diff", file.data.path.to_string_lossy().into_owned(), stretches.clone())
                    .headless()
                    .layout(if split { DiffLayout::Split } else { DiffLayout::Unified })
                    .into_any_element()
            })
            .unwrap_or_else(|| {
                if let Some(error) = file.error.as_ref().or(file.data.notice.as_ref()) {
                    self.notice(error.clone())
                } else if let Some(patch) = &file.data.patch {
                    self.notice(patch.to_string())
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
