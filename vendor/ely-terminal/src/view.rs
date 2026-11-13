use super::colors::Ink;
use super::frame::Frame;
use super::frame::frame;
use super::input;
use super::links::Target;
use super::paint;
use super::pty::Launch;
use super::pty::Listener;
use super::pty::Session;
use super::pty::{self};
use super::search::Search;
use crate::motion;
use crate::theme::ActiveTheme;
use crate::theme::TextSize;
use alacritty_terminal::event::Event;
use alacritty_terminal::event::Notify;
use alacritty_terminal::event::OnResize;
use alacritty_terminal::event_loop::Notifier;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::vte::ansi::ClearMode;
use alacritty_terminal::vte::ansi::Handler;
use alacritty_terminal::vte::ansi::Processor;
use futures::StreamExt;
use gpui::App;
use gpui::ClipboardItem;
use gpui::Context;
use gpui::EventEmitter;
use gpui::FocusHandle;
use gpui::Focusable;
use gpui::FontWeight;
use gpui::IntoElement;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::Point;
use gpui::Render;
use gpui::Role;
use gpui::SharedString;
use gpui::Size;
use gpui::Styled;
use gpui::Task;
use gpui::Window;
use gpui::div;
use gpui::font;
use gpui::prelude::*;
use gpui::px;
use gpui::size;
use std::borrow::Cow;
use std::process::ExitStatus;
use std::rc::Rc;
use std::sync::Arc;
use web_time::Instant;

/// Line height over the text size.
const LEADING: f32 = 1.35;

/// No program types into a replay, so it shows no cursor.
const HIDE_CURSOR: &[u8] = b"\x1b[?25l";

/// What a terminal tells its host.
///
/// Platforms: macOS, Windows, Linux.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalEvent {
    /// The program set its title.
    Title(SharedString),
    /// The program ended.
    Exited(ExitStatus),
    /// The program rang the bell.
    Bell,
    /// A path a Cmd-press opened; URLs open in the browser.
    Open(Target),
    /// Cmd-F: the host shows its find widget.
    Find,
}

/// A terminal: a program on a pseudo-terminal, or replayed output, drawn cell by cell. Type to it, drag to select, Cmd-press a link.
///
/// Platforms: macOS, Windows, Linux.
///
/// ```
/// use ely_terminal::Terminal;
/// use gpui::{App, AppContext, Entity};
///
/// fn replayed(cx: &mut App) -> Entity<Terminal> {
///     cx.new(|cx| Terminal::replay(b"$ ls\r\nCargo.toml  src\r\n", 80, 24, cx))
/// }
/// ```
pub struct Terminal {
    pub(crate) term: Arc<FairMutex<Term<Listener>>>,
    pub(crate) pty: Option<Notifier>,
    pub(crate) focus: FocusHandle,
    title: SharedString,
    exited: Option<ExitStatus>,
    font_family: Option<SharedString>,
    font_size: Option<f32>,
    font_weight: FontWeight,
    line_height: f32,
    /// Columns and lines.
    pub(crate) size: (usize, usize),
    pub(crate) cell: Size<Pixels>,
    resize_pending: bool,
    /// The grid's top-left in the window, from the last paint.
    pub(crate) origin: Point<Pixels>,
    pub(crate) shown: Rc<Frame>,
    pub(crate) search: Search,
    /// The link under the pointer while Cmd is held: its row, columns and target.
    pub(crate) hovered: Option<(usize, std::ops::Range<usize>, Target)>,
    /// Wheel pixels not yet a whole line.
    pub(crate) rest: Pixels,
    /// Whether a press here started the drag that selects.
    pub(crate) selecting: bool,
    bell: Option<Instant>,
    _events: Task<()>,
}

impl EventEmitter<TerminalEvent> for Terminal {}

impl Focusable for Terminal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Terminal {
    /// Starts `launch` on a pseudo-terminal; the first layout sizes its grid to the box.
    pub fn spawn(launch: Launch, cx: &mut Context<Self>) -> anyhow::Result<Self> {
        let size = (80, 24);
        let session = pty::spawn(&launch, TermSize::new(size.0, size.1), (8, 16))?;
        Ok(Self::new(session, size, "Terminal".into(), cx))
    }

    /// Output recorded elsewhere, escape codes and all, on a grid of `columns` by `lines` no process feeds.
    pub fn replay(bytes: &[u8], columns: usize, lines: usize, cx: &mut Context<Self>) -> Self {
        let session = pty::detached(TermSize::new(columns, lines));
        {
            let mut term = session.term.lock();
            let mut parser: Processor = Processor::new();
            parser.advance(&mut *term, bytes);
            parser.advance(&mut *term, HIDE_CURSOR);
        }
        Self::new(session, (columns, lines), "Output".into(), cx)
    }

    fn new(session: Session, size: (usize, usize), title: SharedString, cx: &mut Context<Self>) -> Self {
        let Session { term, pty, mut events } = session;
        let task = cx.spawn(async move |this, cx| {
            while let Some(first) = events.next().await {
                let mut batch = vec![first];
                while let Ok(more) = events.try_recv() {
                    batch.push(more);
                }
                if this.update(cx, |terminal, cx| terminal.handle(batch, cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            term,
            pty,
            focus: cx.focus_handle().tab_stop(true),
            title,
            exited: None,
            font_family: None,
            font_size: None,
            font_weight: FontWeight::NORMAL,
            line_height: LEADING,
            size,
            cell: size_of_cell(),
            resize_pending: true,
            origin: Point::default(),
            shown: Rc::default(),
            search: Search::default(),
            hovered: None,
            rest: Pixels::ZERO,
            selecting: false,
            bell: None,
            _events: task,
        }
    }

    /// This grid's font, leaving the global theme unchanged; otherwise the theme's mono font.
    pub fn font_family(mut self, family: impl Into<SharedString>) -> Self {
        self.font_family = Some(family.into());
        self.resize_pending = true;
        self
    }

    /// Grid text size in pixels; otherwise the theme's small text size.
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self.resize_pending = true;
        self
    }

    /// The base cell weight; ANSI bold adds weight relative to this face.
    pub fn font_weight(mut self, weight: FontWeight) -> Self {
        self.font_weight = weight;
        self.resize_pending = true;
        self
    }

    /// Cell height as a multiple of the text size; defaults to 1.35.
    pub fn line_height(mut self, multiplier: f32) -> Self {
        self.line_height = multiplier;
        self.resize_pending = true;
        self
    }

    /// Changes only this grid's font and refits the running program on the next paint.
    pub fn set_font_family(&mut self, family: impl Into<SharedString>, cx: &mut Context<Self>) {
        let family = family.into();
        if self.font_family.as_ref() == Some(&family) {
            return;
        }
        self.font_family = Some(family);
        self.resize_pending = true;
        cx.notify();
    }

    /// Refits the running grid on the next paint without restarting its PTY or replacing its text.
    pub fn set_typography(&mut self, family: SharedString, size: f32, weight: FontWeight, line_height: f32, cx: &mut Context<Self>) {
        if self.font_family.as_ref() == Some(&family) && self.font_size == Some(size) && self.font_weight == weight && self.line_height == line_height {
            return;
        }
        self.font_family = Some(family);
        self.font_size = Some(size);
        self.font_weight = weight;
        self.line_height = line_height;
        self.resize_pending = true;
        cx.notify();
    }

    /// The title the program set.
    pub fn title(&self) -> &SharedString {
        &self.title
    }

    /// How the program ended, once it has.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.exited
    }

    /// Releases this grid's PTY even while a rendered frame still holds the terminal entity.
    /// The reader's shutdown drops the platform PTY, which stops and reaps its child program.
    pub fn shutdown(&mut self) {
        if let Some(pty) = self.pty.take()
            && pty.0.send(alacritty_terminal::event_loop::Msg::Shutdown).is_err()
        {
            log::debug!("terminal: the program had already ended");
        }
    }

    /// The screen's text as it shows now, trailing spaces gone.
    pub fn text(&self) -> String {
        self.shown.text()
    }

    /// Sends bytes to the program, as typing would.
    pub fn write(&mut self, bytes: impl Into<Cow<'static, [u8]>>) {
        match &self.pty {
            Some(pty) => pty.notify(bytes),
            None => log::debug!("terminal: input to a replayed grid"),
        }
    }

    /// Clears the screen and the scrollback; a shell redraws its prompt.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.search = Search::default();
        {
            let mut term = self.term.lock();
            term.clear_screen(ClearMode::Saved);
            if self.pty.is_none() {
                term.clear_screen(ClearMode::All);
            }
        }
        if self.pty.is_some() {
            self.write(&b"\x0c"[..]);
        }
        log::info!("terminal: cleared");
        cx.notify();
    }

    pub(crate) fn scroll(&mut self, scroll: Scroll, cx: &mut Context<Self>) {
        self.term.lock().scroll_display(scroll);
        cx.notify();
    }

    /// Fits the grid to `columns` by `lines`, and tells the program.
    pub(crate) fn resize(&mut self, columns: usize, lines: usize) {
        let size = (columns.max(2), lines.max(1));
        if size == self.size && !self.resize_pending {
            return;
        }
        if size != self.size {
            self.size = size;
            self.term.lock().resize(TermSize::new(size.0, size.1));
        }
        self.resize_pending = false;
        let cell = (f32::from(self.cell.width).round() as u16, f32::from(self.cell.height).round() as u16);
        if let Some(pty) = &mut self.pty {
            pty.on_resize(pty::window_size(&TermSize::new(size.0, size.1), cell));
        }
        log::debug!("terminal: {columns} by {lines}");
    }

    fn handle(&mut self, events: Vec<Event>, cx: &mut Context<Self>) {
        let drawn = events.iter().any(|event| !matches!(event, Event::MouseCursorDirty | Event::CursorBlinkingChange));
        for event in events {
            match event {
                Event::Wakeup | Event::MouseCursorDirty | Event::CursorBlinkingChange => {}
                Event::Title(title) => {
                    self.title = title.into();
                    cx.emit(TerminalEvent::Title(self.title.clone()));
                }
                Event::ResetTitle => {
                    self.title = "Terminal".into();
                    cx.emit(TerminalEvent::Title(self.title.clone()));
                }
                Event::Bell => {
                    self.bell = Some(Instant::now());
                    cx.emit(TerminalEvent::Bell);
                }
                Event::ClipboardStore(_, text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
                Event::ClipboardLoad(_, reply) => {
                    let text = cx.read_from_clipboard().and_then(|item| item.text()).unwrap_or_default();
                    self.write(reply(&text).into_bytes());
                }
                Event::ColorRequest(index, reply) => {
                    let ink = Ink::new(&cx.theme().colors);
                    let rgb = ink.query(index, self.term.lock().colors());
                    self.write(reply(rgb).into_bytes());
                }
                Event::TextAreaSizeRequest(reply) => {
                    let cell = (f32::from(self.cell.width).round() as u16, f32::from(self.cell.height).round() as u16);
                    let size = TermSize::new(self.size.0, self.size.1);
                    self.write(reply(pty::window_size(&size, cell)).into_bytes());
                }
                Event::PtyWrite(text) => self.write(text.into_bytes()),
                Event::ChildExit(status) => {
                    log::info!("terminal: the program ended, {status}");
                    self.exited = Some(status);
                    cx.emit(TerminalEvent::Exited(status));
                }
                Event::Exit => log::info!("terminal: the grid closed"),
            }
        }
        if drawn {
            cx.notify();
        }
    }
}

/// A cell before the first layout measures one.
fn size_of_cell() -> Size<Pixels> {
    size(Pixels::from(8.0), Pixels::from(16.0))
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Render for Terminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let ink = Ink::new(&theme.colors);
        let pixels = self.font_size.map(px).unwrap_or_else(|| theme.text_size(TextSize::Sm).to_pixels(window.rem_size()));
        let family = self.font_family.clone().unwrap_or_else(|| theme.mono_family.clone());
        let mut mono = font(family);
        mono.weight = self.font_weight;
        let resolved = window.text_system().resolve_font(&mono);
        let advance = match window.text_system().advance(resolved, pixels, 'm') {
            Ok(advance) => advance.width,
            Err(error) => {
                log::error!("terminal: no advance for the code font: {error:#}; cells guessed");
                pixels * 0.6
            }
        };
        let cell = size(advance, (pixels * self.line_height).round());
        self.resize_pending |= self.cell != cell;
        self.cell = cell;
        let (found, current) = self.search.visible();
        self.shown = Rc::new(frame(&self.term.lock(), &ink, found, current));
        let flash = self.bell.and_then(|rang| {
            let lasting = motion::duration(motion::SLOW, cx);
            let spent = rang.elapsed();
            (spent < lasting).then(|| 1.0 - spent.as_secs_f32() / lasting.as_secs_f32())
        });
        if flash.is_some() {
            window.request_animation_frame();
        }
        let colors = theme.colors.clone();
        let look = paint::Look {
            font: mono,
            size: pixels,
            cell: self.cell,
            focused: self.focus.is_focused(window),
            caret: colors.focus,
            caret_width: theme.caret_width().to_pixels(window.rem_size()),
            rule: theme.underline_thickness(),
            selection: colors.selection,
            found: colors.warning.opacity(0.25),
            current: colors.warning.opacity(0.55),
            link: colors.link,
        };
        let root = div()
            .id(("terminal", cx.entity_id()))
            .role(Role::Terminal)
            .aria_label(self.title.clone())
            .key_context(input::CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .p_2()
            .bg(ink.bg)
            .when_some(flash, |root, left| root.bg(ink.bg.blend(colors.warning.opacity(0.12 * left))))
            .when(self.hovered.is_some(), |root| root.cursor_pointer())
            .child(paint::grid(cx.entity(), self.shown.clone(), self.hovered.clone(), look));
        input::listen(root, cx)
    }
}
