use super::CodeEditor;
use crate::cursor::Motion;
use crate::layout::Row;
use crate::theme::Theme;
use gpui::AppContext as _;
use gpui::Bounds;
use gpui::Entity;
use gpui::EntityInputHandler;
use gpui::FontWeight;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::Styled as _;
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::point;
use gpui::px;

struct Boxed {
    editor: Entity<CodeEditor>,
    width: Pixels,
    height: Pixels,
}

impl gpui::Render for Boxed {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        gpui::div().w(self.width).h(self.height).child(self.editor.clone())
    }
}

fn boxed<'a>(text: String, cx: &'a mut TestAppContext) -> (Entity<Boxed>, Entity<CodeEditor>, &'a mut VisualTestContext) {
    cx.update(Theme::init);
    let (view, cx) = cx.add_window_view(move |window, cx| Boxed {
        editor: cx.new(|cx| CodeEditor::new(text, window, cx).font_size(10.0).font_weight(FontWeight::BLACK).line_height(1.0)),
        width: px(480.0),
        height: px(240.0),
    });
    let editor = view.read_with(cx, |view, _| view.editor.clone());
    draw(cx);
    (view, editor, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui::test]
fn both_axes_share_extent_and_clamp_without_wrapping(cx: &mut TestAppContext) {
    let long = format!("{}END", "a".repeat(500));
    let mut lines = vec!["short".to_string(); 80];
    lines[60] = long;
    let (view, editor, cx) = boxed(lines.join("\n"), cx);
    let handle = editor.read_with(cx, |editor, _| editor.scroll_handle());
    assert!(handle.max_offset().x > px(1000.0), "an offscreen long line supplies the width");
    assert!(handle.max_offset().y > px(240.0));
    let initial = cx.debug_bounds("editor-line-0").expect("the first line is drawn");
    assert_eq!(initial.size.height, px(10.0), "configured pixels and leading define the actual row slot");
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.frame.rows.iter().filter(|row| matches!(row, Row::Line(_))).count(), 80);
    });
    cx.simulate_event(gpui::ScrollEvent {
        position: initial.origin + point(px(200.0), px(10.0)),
        delta: gpui::ScrollDelta::Pixels(point(-px(150.0), Pixels::ZERO)),
        ..Default::default()
    });
    draw(cx);
    assert!(handle.offset().x < Pixels::ZERO, "a horizontal wheel updates the shared handle");
    editor.update(cx, |_, cx| {
        handle.set_offset(point(-handle.max_offset().x, Pixels::ZERO));
        cx.notify();
    });
    draw(cx);
    assert_eq!(cx.debug_bounds("editor-line-0"), Some(initial), "the gutter and code clip stay in place at maximum X");
    editor.update(cx, |editor, cx| {
        editor.scroll_line_to_top(60);
        cx.notify();
    });
    draw(cx);
    assert!(handle.offset().y < Pixels::ZERO);
    assert!(
        (handle.offset().x + handle.max_offset().x).abs() < px(0.1),
        "Y navigation preserves X: offset={:?}, max={:?}",
        handle.offset(),
        handle.max_offset(),
    );
    let compact_extent = handle.max_offset();
    let selection = editor.read_with(cx, |editor, _| editor.selections().to_vec());
    editor.update(cx, |editor, cx| {
        let family = editor.code_font(cx).family;
        editor.set_typography(family, 16.0, FontWeight::NORMAL, 1.5, cx);
    });
    draw(cx);
    assert!(handle.max_offset().x > compact_extent.x, "a live font resize reshapes the cached longest line");
    assert!(handle.max_offset().y > compact_extent.y, "line-height updates the vertical extent too");
    assert!(handle.offset().x < Pixels::ZERO && handle.offset().y < Pixels::ZERO, "changing typography does not reset scrolling");
    editor.read_with(cx, |editor, _| assert_eq!(editor.selections(), selection.as_slice()));
    editor.update(cx, |editor, cx| {
        handle.set_offset(point(-px(100000.0), -px(100000.0)));
        let family = editor.code_font(cx).family;
        editor.set_typography(family, 10.0, FontWeight::BLACK, 1.0, cx);
    });
    draw(cx);
    assert_eq!(handle.max_offset(), compact_extent, "shrinking the font restores both measured ranges");
    assert!((handle.offset().x + compact_extent.x).abs() < px(0.1));
    assert!((handle.offset().y + compact_extent.y).abs() < px(0.1), "stale offsets clamp against the new row height");
    editor.update(cx, |_, cx| {
        handle.set_offset(-handle.max_offset());
        cx.notify();
    });
    draw(cx);
    let last = cx.debug_bounds("editor-line-79").expect("the final line is drawn");
    assert!(last.bottom() <= handle.bounds().bottom() - px(15.0), "the last row clears a horizontal track");
    editor.update(cx, |editor, cx| {
        handle.set_offset(point(-px(100000.0), -px(100000.0)));
        editor.set_text("x", cx);
    });
    draw(cx);
    assert_eq!(handle.max_offset(), point(Pixels::ZERO, Pixels::ZERO));
    assert_eq!(handle.offset(), point(Pixels::ZERO, Pixels::ZERO), "both stale offsets clamp after content shrink");
    editor.update(cx, |editor, cx| editor.set_text("b".repeat(200), cx));
    draw(cx);
    editor.update(cx, |_, cx| {
        handle.set_offset(point(-handle.max_offset().x, Pixels::ZERO));
        cx.notify();
    });
    view.update(cx, |view, cx| {
        view.width = px(10000.0);
        cx.notify();
    });
    draw(cx);
    assert_eq!(handle.max_offset().x, Pixels::ZERO);
    assert_eq!(handle.offset().x, Pixels::ZERO, "a wider viewport clamps X");
}

#[gpui::test]
fn end_and_home_reveal_the_caret_without_mutating_read_only_text(cx: &mut TestAppContext) {
    let text = format!("{}END", "a".repeat(500));
    let (_, editor, cx) = boxed(text.clone(), cx);
    editor.update(cx, |editor, cx| {
        editor.set_read_only(true, cx);
        editor.motion(Motion::LineEnd, false, cx);
    });
    draw(cx);
    editor.update_in(cx, |editor, window, cx| {
        let handle = editor.scroll_handle();
        assert!(handle.offset().x < Pixels::ZERO);
        let end = text.encode_utf16().count();
        let caret = editor.bounds_for_range(end..end, Bounds::default(), window, cx).expect("the end caret is drawn");
        assert!(caret.right() <= handle.bounds().right() - px(15.0), "the end caret clears a scrollbar overlay: {caret:?}");
        let gutter_right = handle.bounds().left() + editor.metrics.advance * editor.gutter_columns() as f32;
        assert!(caret.left() >= gutter_right);
        editor.type_text("changed", cx);
        editor.undo(cx);
        assert_eq!(editor.text(), text);
        editor.motion(Motion::LineStart, false, cx);
    });
    draw(cx);
    assert_eq!(editor.read_with(cx, |editor, _| editor.scroll_handle().offset().x), Pixels::ZERO);
}

#[gpui::test]
fn scrolled_wide_tabbed_text_keeps_pointer_selection_and_ime_aligned(cx: &mut TestAppContext) {
    let prefix = "a".repeat(100);
    let text = format!("{prefix}\t测😄e\u{301}XYZ");
    let (_, editor, cx) = boxed(text.clone(), cx);
    editor.update_in(cx, |editor, window, cx| {
        // Preview readers can navigate and select without an insertion caret.
        editor.options.show_caret = false;
        editor.set_read_only(true, cx);
        window.focus(&editor.focus.clone(), cx);
        editor.motion(Motion::LineEnd, false, cx);
    });
    draw(cx);
    let (start, end, y) = editor.update_in(cx, |editor, window, cx| {
        let (shown, _) = editor.measured(0, window, cx);
        assert!(!shown.text.contains('\t'), "tabs use the same expanded text for shaping and painting");
        for (at, _) in text.char_indices().chain([(text.len(), '\0')]) {
            let column = text[..at].chars().count();
            let drawn = shown.column(&text, column);
            assert_eq!(shown.back(&text, drawn), column, "the source column round-trips at byte {at}");
        }
        let from = prefix.len() + 1;
        let to = from + "测😄".len();
        let from_utf16 = text[..from].encode_utf16().count();
        let to_utf16 = text[..to].encode_utf16().count();
        let a = editor.bounds_for_range(from_utf16..from_utf16, Bounds::default(), window, cx).unwrap();
        let b = editor.bounds_for_range(to_utf16..to_utf16, Bounds::default(), window, cx).unwrap();
        let y = a.origin.y + editor.metrics.line / 2.0;
        assert_eq!(editor.character_index_for_point(point(a.origin.x, y), window, cx), Some(from_utf16));
        assert_eq!(editor.offset_in(0, a.origin.x, window, cx), Some(from));
        assert_eq!(editor.offset_in(0, b.origin.x, window, cx), Some(to));
        (a.origin.x, b.origin.x, y)
    });
    cx.simulate_mouse_down(point(start, y), gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(point(end, y), Some(gpui::MouseButton::Left), gpui::Modifiers::none());
    cx.simulate_mouse_up(point(end, y), gpui::MouseButton::Left, gpui::Modifiers::none());
    editor.update(cx, |editor, cx| {
        assert_eq!(&editor.text()[editor.primary().range()], "测😄");
        assert!(!editor.caret_on);
        editor.type_text("changed", cx);
        assert_eq!(editor.text(), text, "read-only selection cannot mutate the preview");
        editor.copy(cx);
        assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), Some("测😄".to_string()));
    });
}
