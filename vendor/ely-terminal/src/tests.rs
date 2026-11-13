use super::Target;
use super::Terminal;
use crate::editor::FindOptions;
use crate::theme::Theme;
use gpui::Entity;
use gpui::TestAppContext;
use gpui::VisualTestContext;

fn replayed<'a>(bytes: &'static [u8], cx: &'a mut TestAppContext) -> (Entity<Terminal>, &'a mut VisualTestContext) {
    cx.update(Theme::init);
    let (terminal, cx) = cx.add_window_view(move |_, cx| Terminal::replay(bytes, 30, 4, cx));
    cx.run_until_parked();
    (terminal, cx)
}

#[gpui::test]
fn replayed_output_shows_without_its_codes(cx: &mut TestAppContext) {
    let (terminal, cx) = replayed(b"\x1b[32mok\x1b[0m done\r\n\x1b]0;build\x07next", cx);
    terminal.read_with(cx, |terminal, _| {
        assert_eq!(terminal.text(), "ok done\nnext");
        assert_eq!(terminal.title().as_ref(), "build", "OSC 0 names it");
        assert_eq!(terminal.shown.cursor, None, "a replay shows no cursor");
    });
}

#[gpui::test]
fn a_scroll_redraws_once(cx: &mut TestAppContext) {
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::grid::Scroll;
    use alacritty_terminal::index::Column;
    use alacritty_terminal::index::Line;
    use gpui::AppContext as _;
    use gpui::FontWeight;
    use gpui::ParentElement as _;
    use gpui::Styled as _;
    use gpui::px;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;

    struct Boxed(Entity<Terminal>);
    impl gpui::Render for Boxed {
        fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
            gpui::div().w(px(240.0)).h(px(56.0)).child(self.0.clone())
        }
    }

    cx.update(Theme::init);
    let (view, cx) = cx.add_window_view(|_, cx| {
        Boxed(cx.new(|cx| {
            Terminal::replay(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7", 30, 4, cx)
                .font_size(10.0)
                .font_weight(FontWeight::BLACK)
                .line_height(1.0)
        }))
    });
    let terminal = view.read_with(cx, |view, _| view.0.clone());
    cx.run_until_parked();
    let grid = terminal.read_with(cx, |terminal, _| terminal.term.clone());
    let content = || {
        let term = grid.lock();
        let grid = term.grid();
        (grid.topmost_line().0..=grid.bottommost_line().0)
            .flat_map(|line| (0..grid.columns()).map(move |column| grid[Line(line)][Column(column)].c))
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
    };
    let before = content();
    assert_eq!(before, "1234567");
    terminal.read_with(cx, |terminal, _| assert_eq!(terminal.cell.height, px(10.0)));

    let notes = Rc::new(Cell::new(0));
    let counted = notes.clone();
    let _watch = cx.update(|_, cx| cx.observe(&terminal, move |_, _| counted.set(counted.get() + 1)));
    terminal.update(cx, |terminal, cx| terminal.scroll(Scroll::Delta(1), cx));
    cx.run_until_parked();
    assert_eq!(notes.get(), 1, "the grid's cursor note draws nothing");
    assert!(grid.lock().grid().display_offset() > 0);

    for (size, weight, ratio, height) in [(16.0, FontWeight::NORMAL, 1.5, 24.0), (10.0, FontWeight::BLACK, 1.0, 10.0)] {
        terminal.update(cx, |terminal, cx| {
            let family = crate::theme::ActiveTheme::theme(&**cx).mono_family.clone();
            terminal.set_typography(family, size, weight, ratio, cx);
        });
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        terminal.read_with(cx, |terminal, _| {
            assert!(Arc::ptr_eq(&grid, &terminal.term), "live typography retains the running terminal's grid");
            assert_eq!(terminal.cell.height, px(height));
        });
        assert_eq!(content(), before, "font-driven reflow must retain all output, including scrollback");
        assert!(grid.lock().grid().display_offset() > 0, "a typography change does not jump to live output");
    }
}

#[gpui::test]
fn find_counts_matches_and_steps_round(cx: &mut TestAppContext) {
    let (terminal, cx) = replayed(b"one two one\r\nthree one", cx);
    terminal.update(cx, |terminal, cx| {
        assert_eq!(terminal.find("one", FindOptions::default(), cx), Ok(3));
        assert_eq!(terminal.matches(), (3, Some(2)), "the newest match first");
        terminal.step(true, cx);
        assert_eq!(terminal.matches(), (3, Some(0)), "past the last comes the first");
        assert!(terminal.find("(", FindOptions { regex: true, ..FindOptions::default() }, cx).is_err());
    });
}

#[gpui::test]
fn tab_reaches_the_terminal(cx: &mut TestAppContext) {
    let (terminal, cx) = replayed(b"ok", cx);
    let focused = cx.update(|window, cx| {
        window.focus_next(cx);
        terminal.read(cx).focus.is_focused(window)
    });
    assert!(focused);
}

#[gpui::test]
fn clearing_drops_the_matches(cx: &mut TestAppContext) {
    let (terminal, cx) = replayed(b"one two one", cx);
    terminal.update(cx, |terminal, cx| {
        assert_eq!(terminal.find("one", FindOptions::default(), cx), Ok(2));
        terminal.clear(cx);
        assert_eq!(terminal.matches(), (0, None));
    });
}

#[gpui::test]
fn a_link_keeps_its_combining_marks(cx: &mut TestAppContext) {
    let (terminal, cx) = replayed("a https://e.dev/cafe\u{301}/x".as_bytes(), cx);
    terminal.read_with(cx, |terminal, _| {
        let (_, columns, target) = terminal.link_at(0, 10).expect("a link under column 10");
        assert_eq!(columns, 2..22);
        assert_eq!(target, Target::Url("https://e.dev/cafe\u{301}/x".into()));
    });
}

#[gpui::test]
fn a_pixel_size_query_of_a_wide_grid_answers(cx: &mut TestAppContext) {
    use gpui::AppContext as _;
    cx.update(Theme::init);
    for columns in [80, 8192] {
        let terminal = cx.new(|cx| Terminal::replay(b"\x1b[14t", columns, 1, cx));
        cx.run_until_parked();
        terminal.read_with(cx, |terminal, _| assert_eq!(terminal.title().as_ref(), "Output"));
    }
}

#[cfg(windows)]
#[test]
fn more_cells_than_a_windows_pty_holds_resize_it() {
    use alacritty_terminal::event::OnResize;
    use alacritty_terminal::term::test::TermSize;
    use alacritty_terminal::tty;
    let options = tty::Options {
        shell: Some(tty::Shell::new("cmd.exe".into(), vec!["/c".into(), "exit".into()])),
        escape_args: true,
        ..tty::Options::default()
    };
    let size = |columns| super::pty::window_size(&TermSize::new(columns, 24), (8, 16));
    let mut pty = tty::new(&options, size(80), 0).expect("a pty starts");
    pty.on_resize(size(80));
    pty.on_resize(size(70_000));
}
