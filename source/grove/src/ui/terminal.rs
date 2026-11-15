//! User-created terminal sessions live with their workspace, independently of dock rendering.

use super::*;
use ely_core::menus::ContextMenu;
use ely_core::menus::Menu;
use ely_core::menus::MenuItem;
use ely_terminal::TerminalEvent;

fn session_index(session_list: &[Session], id: usize) -> Option<usize> {
    session_list.iter().position(|session| session.id == id)
}

impl Grove {
    pub(super) fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open {
            return;
        }
        let view = &mut self.workspace_list[self.active];
        for session in &mut view.session_list {
            session.rename_subscription = None;
            session.rename_input = None;
            session.rename_return_focus = None;
        }
        let workspace_id = view.id;
        let session_id = view.next_session_id;
        view.next_session_id = session_id.checked_add(1).expect("terminal identity exhausted");
        let cwd = view.selected_worktree.clone().unwrap_or_else(|| view.root.clone());
        let session = Session::spawn(session_id, cwd, &self.settings.terminal, cx);
        view.session_list.push(session);
        view.state.session = Some(session_id);
        view.state.dock_mode = DockMode::Terminal;
        view.state.dock_visible = true;
        view.terminal_focus = true;
        self.attach_terminal(workspace_id, session_id, window, cx);
        self.hydrate_terminals(window, cx);
        cx.notify();
    }

    /// Only user dock-opening actions call this; hydration never creates a PTY.
    pub(super) fn ensure_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.settings_open && self.state().terminal_visible() && self.view().session_list.is_empty() {
            self.new_terminal(window, cx);
        }
    }

    pub(super) fn hydrate_terminals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = &mut self.workspace_list[self.active];
        let requested = std::mem::take(&mut view.terminal_focus);
        if !requested || self.settings_open || !view.state.terminal_visible() || view.session_list.iter().any(|session| session.rename_input.is_some()) {
            return;
        }
        if let Some(terminal) = view
            .state
            .session
            .and_then(|id| view.session_list.iter().find(|session| session.id == id))
            .and_then(|session| session.terminal.as_ref())
        {
            window.focus(&terminal.focus_handle(cx), cx);
        }
    }

    fn attach_terminal(&mut self, workspace_id: usize, session_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = workspace_index(&self.workspace_list, workspace_id) else { return };
        let Some(position) = session_index(&self.workspace_list[index].session_list, session_id) else {
            return;
        };
        let Some(terminal) = self.workspace_list[index].session_list[position].terminal.clone() else {
            return;
        };
        let subscription = cx.subscribe_in(&terminal, window, move |this, terminal, event, window, cx| {
            let Some(index) = workspace_index(&this.workspace_list, workspace_id) else { return };
            let Some(position) = session_index(&this.workspace_list[index].session_list, session_id) else {
                return;
            };
            let session = &mut this.workspace_list[index].session_list[position];
            if session.terminal.as_ref().map(Entity::entity_id) != Some(terminal.entity_id()) {
                return;
            }
            match event {
                TerminalEvent::Exited(_) => this.close_terminal(workspace_id, session_id, window, cx),
                TerminalEvent::Title(title) => {
                    if session.observe_title(title.clone(), cx) {
                        cx.notify();
                    }
                }
                _ => {}
            }
        });
        let executor = cx.background_executor().clone();
        let task = cx.spawn(async move |this, cx| {
            loop {
                executor.timer(std::time::Duration::from_secs(2)).await;
                if !matches!(
                    this.update(cx, |this, cx| {
                        let Some(index) = workspace_index(&this.workspace_list, workspace_id) else { return false };
                        let Some(position) = session_index(&this.workspace_list[index].session_list, session_id) else {
                            return false;
                        };
                        let session = &mut this.workspace_list[index].session_list[position];
                        if !session.is_live(cx) {
                            return false;
                        }
                        if session.poll_process(cx) {
                            cx.notify();
                        }
                        true
                    }),
                    Ok(true)
                ) {
                    break;
                }
            }
        });
        let session = &mut self.workspace_list[index].session_list[position];
        session.subscription = Some(subscription);
        session.process_task = Some(task);
    }

    fn select_terminal(&mut self, workspace_id: usize, session_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open || self.view().id != workspace_id {
            return;
        }
        let view = &mut self.workspace_list[self.active];
        let Some(position) = session_index(&view.session_list, session_id) else { return };
        if view.session_list[position].rename_input.is_some() {
            return;
        }
        view.state.session = Some(session_id);
        view.terminal_focus = true;
        self.hydrate_terminals(window, cx);
        cx.notify();
    }

    fn close_terminal(&mut self, workspace_id: usize, session_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = workspace_index(&self.workspace_list, workspace_id) else { return };
        let view = &mut self.workspace_list[index];
        let Some(position) = session_index(&view.session_list, session_id) else { return };
        let session = &view.session_list[position];
        let terminal_focus = session.terminal.as_ref().map(|terminal| terminal.focus_handle(cx));
        let rename_focus = session.rename_input.as_ref().map(|input| input.focus_handle(cx));
        let removed_focused = terminal_focus.as_ref().is_some_and(|focus| focus.is_focused(window)) || rename_focus.as_ref().is_some_and(|focus| focus.is_focused(window));
        if self
            .settings_return_focus
            .as_ref()
            .is_some_and(|focus| terminal_focus.as_ref() == Some(focus) || rename_focus.as_ref() == Some(focus))
        {
            self.settings_return_focus = Some(self.focus.clone());
        }
        for session in &mut view.session_list {
            if session
                .rename_return_focus
                .as_ref()
                .is_some_and(|focus| terminal_focus.as_ref() == Some(focus) || rename_focus.as_ref() == Some(focus))
            {
                session.rename_return_focus = Some(self.focus.clone());
            }
        }
        let selected = view.state.session == Some(session_id);
        view.session_list[position].dispose(cx);
        view.session_list.remove(position);
        if selected {
            view.state.session = view.session_list.get(position).or_else(|| view.session_list.last()).map(|session| session.id);
            view.terminal_focus = false;
        }
        if view.session_list.is_empty() {
            view.state.session = None;
            view.state.dock_mode = DockMode::History;
            view.terminal_focus = false;
        }
        if index == self.active && removed_focused && !self.settings_open {
            if selected && view.state.terminal_visible() {
                view.terminal_focus = true;
                self.hydrate_terminals(window, cx);
            } else {
                window.focus(&self.focus, cx);
            }
        }
        cx.notify();
    }

    fn retry_terminal(&mut self, workspace_id: usize, session_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open || self.view().id != workspace_id {
            return;
        }
        let view = &mut self.workspace_list[self.active];
        let Some(position) = session_index(&view.session_list, session_id) else { return };
        let session = &mut view.session_list[position];
        if session.terminal.is_some() || session.error.is_none() {
            return;
        }
        let custom = session.has_custom_title().then(|| session.display_title().to_owned());
        let cwd = session.launch_cwd.clone();
        session.dispose(cx);
        let mut replacement = Session::spawn(session_id, cwd, &self.settings.terminal, cx);
        if let Some(name) = custom {
            replacement.rename(&name);
        }
        view.session_list[position] = replacement;
        view.terminal_focus = view.state.session == Some(session_id);
        self.attach_terminal(workspace_id, session_id, window, cx);
        self.hydrate_terminals(window, cx);
        cx.notify();
    }

    fn rename_terminal(&mut self, workspace_id: usize, session_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open || self.view().id != workspace_id {
            return;
        }
        let view = &mut self.workspace_list[self.active];
        let Some(position) = session_index(&view.session_list, session_id) else { return };
        let title = view.session_list[position].display_title().to_owned();
        for session in &mut view.session_list {
            session.rename_subscription = None;
            session.rename_input = None;
            session.rename_return_focus = None;
        }
        let return_focus = window.focused(cx);
        let input = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).label("Terminal session name");
            input.set_text(title, cx);
            input
        });
        let subscription = cx.subscribe_in(&input, window, move |this, input, event, window, cx| {
            if !matches!(event, InputEvent::Submit | InputEvent::Blur) {
                return;
            }
            let Some(index) = workspace_index(&this.workspace_list, workspace_id) else { return };
            let Some(position) = session_index(&this.workspace_list[index].session_list, session_id) else {
                return;
            };
            if this.workspace_list[index].session_list[position].rename_input.as_ref().map(Entity::entity_id) == Some(input.entity_id()) {
                this.finish_terminal_rename(workspace_id, session_id, *event == InputEvent::Submit, window, cx);
            }
        });
        let session = &mut view.session_list[position];
        session.rename_input = Some(input.clone());
        session.rename_subscription = Some(subscription);
        session.rename_return_focus = return_focus;
        view.terminal_focus = false;
        window.focus(&input.focus_handle(cx), cx);
        cx.notify();
    }

    fn finish_terminal_rename(&mut self, workspace_id: usize, session_id: usize, commit: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = workspace_index(&self.workspace_list, workspace_id) else { return };
        let Some(position) = session_index(&self.workspace_list[index].session_list, session_id) else {
            return;
        };
        let session = &mut self.workspace_list[index].session_list[position];
        let Some(input) = session.rename_input.clone() else { return };
        let focused = input.focus_handle(cx).is_focused(window);
        if commit && !session.rename(input.read(cx).text()) {
            return;
        }
        session.rename_subscription = None;
        session.rename_input = None;
        let return_focus = session.rename_return_focus.take();
        if self.settings_return_focus.as_ref().is_some_and(|focus| *focus == input.focus_handle(cx)) {
            self.settings_return_focus = Some(self.focus.clone());
        }
        if index == self.active && focused && !self.settings_open {
            window.focus(return_focus.as_ref().unwrap_or(&self.focus), cx);
        }
        cx.notify();
    }

    fn reset_terminal_title(&mut self, workspace_id: usize, session_id: usize, cx: &mut Context<Self>) {
        if self.settings_open {
            return;
        }
        let Some(index) = workspace_index(&self.workspace_list, workspace_id) else { return };
        let Some(position) = session_index(&self.workspace_list[index].session_list, session_id) else {
            return;
        };
        self.workspace_list[index].session_list[position].reset_title();
        cx.notify();
    }

    pub(super) fn terminal_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = self.view();
        let session = view.state.session.and_then(|id| view.session_list.iter().find(|session| session.id == id));
        if let Some(terminal) = session.and_then(|session| session.terminal.as_ref()) {
            return div().size_full().min_w_0().min_h_0().overflow_hidden().child(terminal.clone()).into_any_element();
        }
        let Some(session) = session else { return self.notice("No terminal session selected") };
        let workspace_id = view.id;
        let session_id = session.id;
        v_flex()
            .size_full()
            .child(self.notice(session.error.clone().unwrap_or_else(|| "Terminal unavailable".into())))
            .child(
                h_flex()
                    .items_center()
                    .child(
                        self.icon_button(("retry-terminal", session_id), IconName::RefreshCw, "Retry terminal session")
                            .on_click(cx.listener(move |this, _, window, cx| this.retry_terminal(workspace_id, session_id, window, cx))),
                    )
                    .child(div().refine_style(&self.settings.ui.style()).child("Retry terminal session")),
            )
            .into_any_element()
    }

    pub(super) fn sessions(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = self.view();
        let workspace_id = view.id;
        let typography = &self.settings.terminal;
        let row_list = view.session_list.iter().filter(|session| {
            session.rename_input.is_some() || view.state.sessions_panel.matches_query(session.display_title()) || view.state.sessions_panel.matches_query(&session.launch_cwd.to_string_lossy())
        });
        v_flex()
            .size_full()
            .min_w_0()
            .child(
                self.chrome("Sessions").child(div().flex_1()).child(
                    self.icon_button("new-terminal", IconName::Plus, "New terminal session")
                        .on_click(cx.listener(|this, _, window, cx| this.new_terminal(window, cx))),
                ),
            )
            .child(self.search_bar(SearchPanel::Sessions, cx))
            .child(v_flex().id("session-list").flex_1().min_h_0().overflow_y_scroll().children(row_list.map(|session| {
                let session_id = session.id;
                let title = session.display_title().to_owned();
                let content = if let Some(input) = &session.rename_input {
                    div()
                        .flex_1()
                        .min_w_0()
                        .refine_style(&typography.style())
                        .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "escape" {
                                cx.stop_propagation();
                                this.finish_terminal_rename(workspace_id, session_id, false, window, cx);
                            }
                        }))
                        .child(input.clone())
                        .into_any_element()
                } else {
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(6.))
                        .overflow_hidden()
                        .child(div().min_w_0().overflow_hidden().whitespace_nowrap().child(title.clone()))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_color(rgb(META))
                                .refine_style(&self.settings.secondary_ui_style())
                                .child(session.launch_cwd.to_string_lossy().into_owned()),
                        )
                        .into_any_element()
                };
                let rename = cx.weak_entity();
                let reset = rename.clone();
                let close = rename.clone();
                let menu = Menu::new()
                    .item(MenuItem::new("Rename…").on_click(move |window, cx| {
                        let _ = rename.update(cx, |this, cx| this.rename_terminal(workspace_id, session_id, window, cx));
                    }))
                    .item(MenuItem::new("Reset automatic title").disabled(!session.has_custom_title()).on_click(move |_, cx| {
                        let _ = reset.update(cx, |this, cx| this.reset_terminal_title(workspace_id, session_id, cx));
                    }))
                    .separator()
                    .item(MenuItem::new("Close").on_click(move |window, cx| {
                        let _ = close.update(cx, |this, cx| this.close_terminal(workspace_id, session_id, window, cx));
                    }));
                ContextMenu::new(("session-menu", session_id), menu).w_full().flex_shrink_0().child(
                    h_flex()
                        .w_full()
                        .h(px(typography.row_height().max(CHROME)))
                        .min_w_0()
                        .items_center()
                        .child(
                            ListItem::new(("session", session_id))
                                .accessibility_label(title)
                                .selected(view.state.session == Some(session_id))
                                .flex_1()
                                .min_w_0()
                                .h_full()
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
                                        .gap(px(4.))
                                        .refine_style(&typography.style())
                                        .child(div().flex_shrink_0().child(icon_view(IconName::Terminal, 12., MUTED)))
                                        .child(content),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| this.select_terminal(workspace_id, session_id, window, cx))),
                        )
                        .child(
                            self.icon_button(("close-terminal", session_id), IconName::X, "Close terminal session")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.close_terminal(workspace_id, session_id, window, cx);
                                })),
                        ),
                )
            })))
            .into_any_element()
    }
}
