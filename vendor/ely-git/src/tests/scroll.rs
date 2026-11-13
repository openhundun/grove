use crate::DiffLayout;
use crate::DiffLine;
use crate::DiffScrollHandle;
use crate::DiffViewer;
use crate::LineKind;
use crate::Stretch;
use crate::theme::Theme;
use gpui::Context;
use gpui::FontWeight;
use gpui::IntoElement;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::Render;
use gpui::Styled;
use gpui::TestAppContext;
use gpui::VisualTestContext;
use gpui::Window;
use gpui::div;
use gpui::point;
use gpui::px;
use std::rc::Rc;

struct ScrollingDiff {
    stretches: Rc<Vec<Stretch>>,
    scroll: DiffScrollHandle,
    layout: DiffLayout,
    font_size: f32,
    font_weight: FontWeight,
    line_height: f32,
}

impl Render for ScrollingDiff {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(480.0)).h(px(240.0)).child(
            DiffViewer::from_stretches("scroll-test", "file.rs", self.stretches.clone())
                .headless()
                .layout(self.layout)
                .font_size(self.font_size)
                .font_weight(self.font_weight)
                .line_height(self.line_height)
                .track_scroll(&self.scroll),
        )
    }
}

fn lines(count: usize, long: bool) -> Rc<Vec<Stretch>> {
    let mut lines = Vec::new();
    for (kind, width) in [(LineKind::Removed, 240), (LineKind::Added, 500)] {
        for ix in 0..count {
            lines.push(DiffLine {
                kind,
                old: (kind == LineKind::Removed).then_some(ix + 1),
                new: (kind == LineKind::Added).then_some(ix + 1),
                text: if long && ix == count - 1 { format!("{}END", "x".repeat(width)).into() } else { "short".into() },
                words: Vec::new(),
            });
        }
    }
    Rc::new(vec![Stretch::Hunk { header: "@@ -1 +1 @@".into(), lines }])
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui::test]
fn unified_scrolls_and_clamps_both_axes(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (view, cx) = cx.add_window_view(|_, _| ScrollingDiff {
        stretches: lines(80, true),
        scroll: DiffScrollHandle::default(),
        layout: DiffLayout::Unified,
        font_size: 10.0,
        font_weight: FontWeight::BLACK,
        line_height: 1.0,
    });
    draw(cx);
    let handle = view.read_with(cx, |view, _| view.scroll.unified().0.borrow().base_handle.clone());
    assert!(handle.max_offset().x > px(1000.0));
    assert!(handle.max_offset().y > px(240.0));
    cx.simulate_event(gpui::ScrollEvent {
        position: handle.bounds().origin + point(px(100.0), px(10.0)),
        delta: gpui::ScrollDelta::Pixels(point(-px(150.0), Pixels::ZERO)),
        ..Default::default()
    });
    draw(cx);
    assert!(handle.offset().x < Pixels::ZERO);
    cx.simulate_event(gpui::ScrollEvent {
        position: handle.bounds().origin + point(px(100.0), px(10.0)),
        delta: gpui::ScrollDelta::Pixels(point(Pixels::ZERO, -px(100.0))),
        ..Default::default()
    });
    draw(cx);
    assert!(handle.offset().y < Pixels::ZERO);
    view.update(cx, |view, cx| {
        handle.set_offset(point(-px(100000.0), -px(100000.0)));
        view.stretches = lines(1, false);
        cx.notify();
    });
    draw(cx);
    assert_eq!(handle.max_offset(), point(Pixels::ZERO, Pixels::ZERO));
    assert_eq!(handle.offset(), point(Pixels::ZERO, Pixels::ZERO));
}

#[gpui::test]
fn split_links_both_axes_and_clamps_each_pane_without_feedback(cx: &mut TestAppContext) {
    cx.update(Theme::init);
    let (view, cx) = cx.add_window_view(|_, _| ScrollingDiff {
        stretches: lines(80, true),
        scroll: DiffScrollHandle::default(),
        layout: DiffLayout::Split,
        font_size: 10.0,
        font_weight: FontWeight::BLACK,
        line_height: 1.0,
    });
    draw(cx);
    let (left, right) = view.read_with(cx, |view, _| (view.scroll.left().0.borrow().base_handle.clone(), view.scroll.right().0.borrow().base_handle.clone()));
    assert!(left.max_offset().x > px(1000.0));
    assert!(right.max_offset().x > left.max_offset().x);
    assert!(left.max_offset().y > px(240.0));
    assert_eq!(left.max_offset().y, right.max_offset().y);
    let row_extent = left.max_offset().y;
    let compact_width = [left.max_offset().x, right.max_offset().x];
    for selector in ["diff-left-1", "diff-right-1"] {
        assert_eq!(cx.debug_bounds(selector).expect("both actual panes draw the paired row").size.height, px(10.0));
    }

    cx.simulate_event(gpui::ScrollEvent {
        position: left.bounds().origin + point(px(30.0), px(10.0)),
        delta: gpui::ScrollDelta::Pixels(point(-px(150.0), Pixels::ZERO)),
        ..Default::default()
    });
    draw(cx);
    assert!(left.offset().x < Pixels::ZERO);
    assert_eq!(right.offset().x, left.offset().x);
    cx.simulate_event(gpui::ScrollEvent {
        position: right.bounds().origin + point(px(30.0), px(10.0)),
        delta: gpui::ScrollDelta::Pixels(point(-px(300.0), Pixels::ZERO)),
        ..Default::default()
    });
    draw(cx);
    assert_eq!(left.offset().x, right.offset().x);

    // A wider source reaches its own end while the narrower receiver stops at
    // its edge. Repeated layout must not treat that clamp as new user input.
    view.update(cx, |_, cx| {
        right.set_offset(point(-right.max_offset().x, right.offset().y));
        cx.notify();
    });
    draw(cx);
    draw(cx);
    assert!((left.offset().x + left.max_offset().x).abs() < px(0.1));
    assert!((right.offset().x + right.max_offset().x).abs() < px(0.1));
    assert!(right.offset().x < left.offset().x);
    let (left_x, right_x) = (left.offset().x, right.offset().x);

    for handle in [&left, &right] {
        let previous_y = handle.offset().y;
        cx.simulate_event(gpui::ScrollEvent {
            position: handle.bounds().origin + point(px(30.0), px(10.0)),
            delta: gpui::ScrollDelta::Pixels(point(Pixels::ZERO, -px(100.0))),
            ..Default::default()
        });
        draw(cx);
        assert!(handle.offset().y < previous_y);
        assert_eq!(left.offset().y, right.offset().y);
        assert_eq!(left.offset().x, left_x);
        assert_eq!(right.offset().x, right_x);
    }

    for (handle, y) in [(&left, -px(350.0)), (&right, -px(550.0))] {
        view.update(cx, |_, cx| {
            handle.set_offset(point(handle.offset().x, y));
            cx.notify();
        });
        draw(cx);
        assert_eq!(left.offset().y, y);
        assert_eq!(right.offset().y, y);
        assert_eq!(left.offset().x, left_x);
        assert_eq!(right.offset().x, right_x);
    }

    for (handle, x) in [(&left, -px(125.0)), (&right, -px(225.0))] {
        view.update(cx, |_, cx| {
            handle.set_offset(point(x, handle.offset().y));
            cx.notify();
        });
        draw(cx);
        assert_eq!(left.offset().x, x);
        assert_eq!(right.offset().x, x);
        assert_eq!(left.offset().y, right.offset().y);
    }

    view.update(cx, |view, cx| {
        view.font_size = 16.0;
        view.font_weight = FontWeight::NORMAL;
        view.line_height = 1.5;
        cx.notify();
    });
    draw(cx);
    assert!(left.max_offset().x > compact_width[0]);
    assert!(right.max_offset().x > compact_width[1], "a font change remeasures each pane's own range");
    assert!(left.max_offset().y > row_extent);
    assert_eq!(left.max_offset().y, right.max_offset().y);
    assert_eq!(left.offset().y, right.offset().y);
    assert!(left.offset().x < Pixels::ZERO && right.offset().x < Pixels::ZERO, "live typography keeps the view scrolled");
    view.update(cx, |view, cx| {
        left.set_offset(point(-px(100000.0), -px(100000.0)));
        right.set_offset(point(-px(100000.0), -px(100000.0)));
        view.font_size = 10.0;
        view.font_weight = FontWeight::BLACK;
        view.line_height = 1.0;
        cx.notify();
    });
    draw(cx);
    draw(cx);
    for (index, handle) in [&left, &right].into_iter().enumerate() {
        assert_eq!(handle.max_offset().x, compact_width[index]);
        assert_eq!(handle.max_offset().y, row_extent);
        assert!((handle.offset().x + compact_width[index]).abs() < px(0.1));
        assert!((handle.offset().y + row_extent).abs() < px(0.1), "new typography clamps stale ranges without pane feedback");
    }
    let left_row = cx.debug_bounds("diff-left-80").expect("the last left row is visible");
    let right_row = cx.debug_bounds("diff-right-80").expect("the last right row is visible");
    assert_eq!(left_row.size.height, px(10.0));
    assert_eq!(right_row.size.height, px(10.0));
    assert_eq!(left_row.origin.y, right_row.origin.y);

    view.update(cx, |view, cx| {
        view.stretches = lines(80, false);
        cx.notify();
    });
    draw(cx);
    assert_eq!(left.max_offset().y, row_extent);
    assert_eq!(right.max_offset().y, row_extent);
    assert_eq!(left.max_offset().x, Pixels::ZERO);
    assert_eq!(right.max_offset().x, Pixels::ZERO);
    assert_eq!(left.offset().x, Pixels::ZERO);
    assert_eq!(right.offset().x, Pixels::ZERO);
    assert_eq!(left.offset().y, right.offset().y);

    view.update(cx, |view, cx| {
        view.stretches = lines(1, false);
        cx.notify();
    });
    draw(cx);
    assert_eq!(left.max_offset(), point(Pixels::ZERO, Pixels::ZERO));
    assert_eq!(right.max_offset(), point(Pixels::ZERO, Pixels::ZERO));
    assert_eq!(left.offset(), point(Pixels::ZERO, Pixels::ZERO));
    assert_eq!(right.offset(), point(Pixels::ZERO, Pixels::ZERO));
}
