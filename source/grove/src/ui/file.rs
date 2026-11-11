use super::*;

impl Grove {
    pub(super) fn files(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_files = self.state().mode == Mode::Files;
        let tree = is_files || self.state().tree;
        let mut head = self
            .chrome(if is_files {
                "Files"
            } else if self.state().compare {
                "PR changes"
            } else {
                "Changes"
            })
            .child(div().flex_1());
        if !is_files {
            let owner = cx.entity().downgrade();
            head = head.child(
                SegmentedControl::new("file-layout", if tree { "tree" } else { "flat" })
                    .size(ControlSize::Sm)
                    .segment("flat", "Flat", None)
                    .segment("tree", "Tree", None)
                    .on_change(move |value, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.state_mut().tree = value.as_ref() == "tree";
                            cx.notify();
                        });
                    }),
            );
        }
        let mut row_list = Vec::new();
        if !is_files && matches!(self.state().history, HistorySelection::Commit(_)) {
            row_list.push(
                h_flex()
                    .id("commit-message")
                    .h(px(24.))
                    .px(px(8.))
                    .gap(px(6.))
                    .items_center()
                    .cursor_pointer()
                    .when(self.state().file == FileSelection::Message, |this| this.bg(rgb(SELECT)))
                    .child(icon_view(IconName::MessageSquare, 12., ACCENT))
                    .child("COMMIT_MSG")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state_mut().file = FileSelection::Message;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        let file_list: Vec<(&std::path::Path, Option<&FileChange>)> = if is_files {
            self.view()
                .worktree_data
                .as_ref()
                .map(|data| data.file_list.iter().map(|file| (file.as_path(), None)).collect())
                .unwrap_or_default()
        } else {
            self.view()
                .history_data
                .as_ref()
                .map(|data| data.file_list.iter().map(|file| (file.path.as_path(), Some(file))).collect())
                .unwrap_or_default()
        };
        let mut directory_set = std::collections::HashSet::new();
        for (index, (path, change)) in file_list.iter().enumerate() {
            let display = path.to_string_lossy().into_owned();
            let directory = path.parent().filter(|path| !path.as_os_str().is_empty()).map(|path| path.to_string_lossy().into_owned());
            if tree && let Some(directory) = directory {
                let collapsed = self.state().collapsed_dir_set.contains(&directory);
                if directory_set.insert(directory.clone()) {
                    row_list.push(
                        h_flex()
                            .id(("dir", index))
                            .h(px(ROW_HEIGHT))
                            .px(px(8.))
                            .gap(px(6.))
                            .items_center()
                            .cursor_pointer()
                            .child(icon_view(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown }, 10., MUTED))
                            .child(icon::folder(std::path::Path::new(&directory), !collapsed, false, 14.))
                            .child(div().truncate().child(directory.clone()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.state_mut().collapsed_dir_set.remove(&directory) {
                                    this.state_mut().collapsed_dir_set.insert(directory.clone());
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    );
                }
                if collapsed {
                    continue;
                }
            }
            let selected = if is_files {
                self.state().fs_file == index
            } else {
                self.state().file == FileSelection::File(index)
            };
            let name = if tree {
                path.file_name().unwrap_or_default().to_string_lossy().into_owned()
            } else {
                display.clone()
            };
            let mut content = h_flex()
                .w_full()
                .h_full()
                .min_w_0()
                .gap(px(6.))
                .child(icon::file(path, 14.))
                .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(10.)).child(name));
            if let Some(change) = change {
                if let Some(additions) = change.additions {
                    content = content.child(div().text_size(px(9.)).text_color(rgb(0x1a7f37)).child(format!("+{additions}")));
                }
                if let Some(deletions) = change.deletions {
                    content = content.child(div().text_size(px(9.)).text_color(rgb(0xb42318)).child(format!("−{deletions}")));
                }
                content = content.child(div().text_size(px(9.)).text_color(rgb(0x8a4b00)).child(change.status.to_string()));
            }
            row_list.push(
                ListItem::new(("file", index))
                    .accessibility_label(display)
                    .selected(selected)
                    .h(px(ROW_HEIGHT))
                    .flex_shrink_0()
                    .py_0()
                    .rounded_none()
                    .overflow_hidden()
                    .pl(px(if tree && path.parent().is_some_and(|path| !path.as_os_str().is_empty()) { 28. } else { 8. }))
                    .pr(px(8.))
                    .border_l_2()
                    .border_color(rgb(if selected { ACCENT } else { BG }))
                    .child(content)
                    .on_click(cx.listener(move |this, _, _, cx| this.select_file(index, cx)))
                    .into_any_element(),
            );
        }
        if file_list.is_empty() {
            row_list.push(self.notice(if self.view().loading { "Reading files…" } else { "No files in this selection" }));
        }
        v_flex()
            .size_full()
            .child(head)
            .child(
                v_flex()
                    .id("files-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.view().file_scroll)
                    .children(row_list),
            )
            .into_any_element()
    }
}
