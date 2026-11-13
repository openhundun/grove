use super::chat::InlineChat;
use super::gutter::Cell;
use super::layout::CHAT_ROWS;
use super::layout::Row;
use super::layout::Shown;
use super::state::CodeEditor;
use super::state::EditorEvent;
use crate::theme::ActiveTheme;
use crate::theme::TextSize;
use gpui::AnyElement;
use gpui::Context;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::ParentElement;
use gpui::Stateful;
use gpui::Styled;
use gpui::StyledText;
use gpui::Window;
use gpui::div;
use gpui::prelude::*;

/// How far a faded line recedes.
const FADED: f32 = 0.35;

impl CodeEditor {
    /// Row `ix` as the list draws it.
    pub(crate) fn row(&self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let handle = self.scroll_handle();
        // UniformList shifts the entire item. Undo that for the viewport and gutter;
        // each code layer applies the shared offset inside its own fixed clip.
        let base = div().id(("editor-row", ix)).absolute().left(-handle.offset().x).w(handle.bounds().size.width).h(self.frame.line).flex();
        let caret = self.buffer.line_of(self.primary().head);
        let base = match self.frame.rows[ix] {
            Row::Line(line) => base.debug_selector(move || format!("editor-line-{line}")),
            _ => base,
        };
        let row = match self.frame.rows[ix] {
            Row::Line(line) if self.options.focus_line && line != caret => self.line_row(base.opacity(FADED), ix, line, window, cx),
            Row::Line(line) => self.line_row(base, ix, line, window, cx),
            Row::Lens(at) => self.lens_row(base, ix, at, cx),
            Row::Removed(hunk, at) => self.removed_row(base, ix, hunk, at, cx),
            Row::Ghost(at) => self.ghost_row(base, ix, at, cx),
            Row::Chat(at) => self.chat_row(base, ix, at, cx),
            Row::Room => base.into_any_element(),
        };
        div().relative().w_full().min_w(self.content_width()).h(self.frame.line).child(row).into_any_element()
    }

    fn lens_row(&self, base: Stateful<gpui::Div>, ix: usize, at: usize, cx: &mut Context<Self>) -> AnyElement {
        let gutter = self.gutter(Cell::Blank, ix, cx);
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let lens = &self.marks.lenses[at];
        let indent = self.buffer.indent(lens.line);
        let items = lens.items.iter().enumerate().map(|(item, words)| {
            let line = lens.line;
            div()
                .id(("lens", item))
                .cursor_pointer()
                .hover(|style| style.text_color(colors.link))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(cx.listener(move |_, _, _, cx| {
                    log::info!("code lens: line {line}, item {item}");
                    cx.emit(EditorEvent::Lens(line, item));
                }))
                .child(words.clone())
        });
        let items: Vec<AnyElement> = items
            .enumerate()
            .flat_map(|(place, item)| {
                let gap = (place > 0).then(|| div().child("·").into_any_element());
                gap.into_iter().chain([item.into_any_element()])
            })
            .collect();
        base.child(gutter)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .pl(self.frame.advance * (indent + 1) as f32)
                    .font_family(theme.font_family.clone())
                    .text_size(theme.text_size(TextSize::Xs))
                    .text_color(colors.fg_subtle)
                    .children(items),
            )
            .into_any_element()
    }

    fn removed_row(&self, base: Stateful<gpui::Div>, ix: usize, hunk: usize, at: usize, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let shown = Shown::new(&self.marks.hunks[hunk].removed[at], Vec::new(), Vec::new()).with_tabs(self.options.tab);
        base.child(self.gutter(Cell::Removed, ix, cx))
            .child(
                div().flex_1().min_w_0().h_full().overflow_hidden().bg(colors.danger.opacity(0.08)).child(
                    div()
                        .relative()
                        .left(self.frame.advance + self.scroll_handle().offset().x)
                        .w(self.content_width())
                        .line_height(self.frame.line)
                        .whitespace_nowrap()
                        .text_color(colors.fg_muted)
                        .child(StyledText::new(shown.text).with_highlights(shown.styles)),
                ),
            )
            .into_any_element()
    }

    fn ghost_row(&self, base: Stateful<gpui::Div>, ix: usize, at: usize, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let shown = self.ghost_shown(at, cx);
        base.child(self.gutter(Cell::Blank, ix, cx))
            .child(
                div().flex_1().min_w_0().h_full().overflow_hidden().child(
                    div()
                        .relative()
                        .left(self.frame.advance + self.scroll_handle().offset().x)
                        .w(self.content_width())
                        .line_height(self.frame.line)
                        .whitespace_nowrap()
                        .text_color(colors.syntax.variable)
                        .child(StyledText::new(shown.text).with_highlights(shown.styles)),
                ),
            )
            .into_any_element()
    }

    fn chat_row(&self, base: Stateful<gpui::Div>, ix: usize, at: usize, cx: &mut Context<Self>) -> AnyElement {
        let gutter = self.gutter(Cell::Blank, ix, cx);
        let theme = cx.theme();
        let row = base.child(gutter);
        if at > 0 {
            return row.into_any_element();
        }
        let asked = cx.entity();
        let closed = cx.entity();
        let field = self.chat_field.clone();
        row.child(
            div().relative().flex_1().h_full().child(
                InlineChat::new(("inline-chat", ix), &self.chat_field)
                    .absolute()
                    .top_0()
                    .left(self.frame.advance)
                    .w(self.frame.advance * 72.0)
                    .h(self.frame.line * CHAT_ROWS as f32)
                    .font_family(theme.font_family.clone())
                    .on_ask(move |prompt, _, cx| {
                        field.update(cx, |field, cx| field.set_text("", cx));
                        asked.update(cx, |_, cx| cx.emit(EditorEvent::Asked(prompt)));
                    })
                    .on_close(move |window, cx| {
                        closed.update(cx, |editor, cx| editor.set_chat(None, window, cx));
                    }),
            ),
        )
        .into_any_element()
    }
}
