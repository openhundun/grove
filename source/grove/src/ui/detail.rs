use super::*;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::scroll::ScrollbarMode;

impl Grove {
    pub(super) fn detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let message = self.state().mode == Mode::Changes && self.state().file == FileSelection::Message;
        let (title, refresh_id, refresh_label, refresh_target) = if message {
            ("Message", "refresh-message", "Refresh commit message", RefreshTarget::History)
        } else if self.state().mode == Mode::Files {
            ("Preview", "refresh-preview", "Refresh file preview", RefreshTarget::File)
        } else {
            ("Diff", "refresh-diff", "Refresh diff", RefreshTarget::File)
        };
        if let Some(notice) = &self.view().error {
            let head = self.chrome(title).child(div().flex_1()).child(self.refresh_button(refresh_id, refresh_label, refresh_target, cx));
            return v_flex().size_full().child(head).child(self.notice(notice.clone())).into_any_element();
        }
        if message {
            let preview = self.state().preview;
            let workspace = self.active;
            let head = self
                .chrome("Message")
                .child(div().flex_1())
                .child(
                    // The same toolbar position represents the current source/preview mode.
                    self.icon_button(
                        "message-view",
                        if preview { IconName::Eye } else { IconName::FileCode },
                        if preview {
                            "Commit message preview; switch to source"
                        } else {
                            "Commit message source; switch to preview"
                        },
                    )
                    .toggled(preview)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace_list[workspace].state.preview = !preview;
                        cx.notify();
                    })),
                )
                .child(self.refresh_button("refresh-message", "Refresh commit message", RefreshTarget::History, cx));
            let Some(history) = &self.view().history_data else {
                return v_flex().size_full().child(head).child(self.notice("Reading commit…")).into_any_element();
            };
            let body = if preview {
                div()
                    .id("message-markdown")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(18.))
                    .bg(rgb(0xffffff))
                    .child(super::markdown_theme::render(history.message.clone(), &self.settings.preview))
                    .into_any_element()
            } else if let Some(editor) = &self.view().message_editor {
                editor_view(editor, cx).into_any_element()
            } else {
                self.notice("Reading message…")
            };
            return v_flex().size_full().child(head).child(div().flex_1().min_h_0().overflow_hidden().child(body)).into_any_element();
        }
        let Some(file) = self.view().file_key.as_ref().and_then(|key| self.view().file_map.get(key)) else {
            let head = self.chrome(title).child(div().flex_1()).child(self.refresh_button(refresh_id, refresh_label, refresh_target, cx));
            return v_flex()
                .size_full()
                .child(head)
                .child(self.notice(if self.view().loading { "Reading selected file…" } else { "Select a file to inspect" }))
                .into_any_element();
        };
        if self.state().mode == Mode::Files {
            let head = self
                .chrome("Preview")
                .child(div().flex_1())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .refine_style(&self.settings.files.style())
                        .child(file.data.path.to_string_lossy().into_owned()),
                )
                .child(self.refresh_button("refresh-preview", "Refresh file preview", RefreshTarget::File, cx));
            let body = file
                .editor
                .as_ref()
                .map(|editor| editor_view(editor, cx).into_any_element())
                .unwrap_or_else(|| self.notice(content_notice(&file.data.content)));
            return v_flex().size_full().child(head).child(div().flex_1().min_h_0().overflow_hidden().child(body)).into_any_element();
        }
        let split = self.state().split;
        let full = self.state().diff_full;
        let workspace_id = self.view().id;
        let file_key = self.view().file_key.as_ref().unwrap().clone();
        let owner = cx.entity().downgrade();
        let workspace = self.active;
        let head = self
            .chrome("Diff")
            .child(div().flex_1())
            .child(
                self.icon_button(
                    "diff-layout",
                    if split { IconName::Columns2 } else { IconName::Rows2 },
                    if split {
                        "Side-by-side diff; switch to unified diff"
                    } else {
                        "Unified diff; switch to side-by-side diff"
                    },
                )
                .toggled(split)
                .on_click(cx.listener(move |this, _, _, cx| this.set_diff_layout(workspace, !split, cx))),
            )
            .child(
                self.icon_button(
                    "diff-context",
                    if full { IconName::ChevronUp } else { IconName::ChevronDown },
                    if full { "Full diff; collapse unchanged lines" } else { "Collapsed diff; expand unchanged lines" },
                )
                .toggled(full)
                .on_click(cx.listener(move |this, _, _, cx| this.set_diff_context(workspace, !full, cx))),
            )
            .child(self.refresh_button("refresh-diff", "Refresh diff", RefreshTarget::File, cx));
        let body = file
            .diff
            .as_ref()
            .filter(|stretches| !stretches.is_empty())
            .map(|stretches| {
                // Split links both axes across the two actual pane scroll handles; each pane owns its bars.
                div()
                    .relative()
                    .size_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        DiffViewer::from_stretches("file-diff", file.data.path.to_string_lossy().into_owned(), stretches.clone())
                            .headless()
                            .open(
                                stretches
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(index, stretch)| (matches!(stretch, Stretch::Folded(_)) && (full || file.diff_open.contains(&index))).then_some(index)),
                            )
                            .on_open(move |stretch, _, cx| {
                                let _ = owner.update(cx, |this, cx| this.open_diff_context(workspace_id, &file_key, stretch, cx));
                            })
                            .font_family(self.settings.editor.font_family.clone())
                            .font_size(self.settings.editor.font_size)
                            .font_weight(FontWeight(f32::from(self.settings.editor.font_weight)))
                            .line_height(self.settings.editor.line_height)
                            .layout(if split { DiffLayout::Split } else { DiffLayout::Unified })
                            .track_scroll(&file.diff_scroll)
                            .scrollbars(|handle, _, _| Scrollbar::new(handle).id("file-diff-scrollbar").mode(ScrollbarMode::Always).into_any_element()),
                    )
                    .into_any_element()
            })
            .unwrap_or_else(|| {
                if let Some(error) = file.error.as_ref().or(file.data.notice.as_ref()) {
                    self.notice(error.clone())
                } else if let Some(patch) = &file.data.patch {
                    div()
                        .id("patch-fallback")
                        .size_full()
                        .overflow_scroll()
                        .whitespace_nowrap()
                        .p(px(16.))
                        .text_color(rgb(MUTED))
                        .refine_style(&self.settings.editor.style())
                        .child(patch.to_string())
                        .into_any_element()
                } else {
                    self.notice(content_notice(&file.data.content))
                }
            });
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(head)
            .children(file.data.notice.as_ref().filter(|_| file.diff.as_ref().is_some_and(|diff| !diff.is_empty())).map(|notice| {
                div()
                    .flex_shrink_0()
                    .px(px(8.))
                    .py(px(4.))
                    .refine_style(&self.settings.secondary_ui_style())
                    .text_color(rgb(MUTED))
                    .child(notice.clone())
            }))
            .child(div().flex_1().min_w_0().min_h_0().overflow_hidden().child(body))
            .into_any_element()
    }
}

fn editor_view(editor: &Entity<CodeEditor>, cx: &App) -> impl IntoElement {
    let scroll_handle = editor.read(cx).scroll_handle();
    div().relative().size_full().min_w_0().min_h_0().child(editor.clone()).child(
        div()
            .absolute()
            .inset_0()
            .child(Scrollbar::new(&scroll_handle).id(("editor-scrollbar", editor.entity_id())).mode(ScrollbarMode::Always)),
    )
}
fn content_notice(content: &FileContent) -> String {
    match content {
        FileContent::Text(_) => "No textual patch available".into(),
        FileContent::Binary => "Binary or non-UTF-8 content; text preview is unavailable".into(),
        FileContent::Unavailable(error) => error.clone(),
    }
}
