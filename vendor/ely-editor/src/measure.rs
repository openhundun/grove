//! Where drawn columns fall, as gpui lays them out.

use super::layout::Row;
use super::layout::Shown;
use super::state::CodeEditor;
use crate::theme::ActiveTheme;
use gpui::Bounds;
use gpui::Context;
use gpui::Font;
use gpui::FontStyle;
use gpui::HighlightStyle;
use gpui::Hsla;
use gpui::LineLayout;
use gpui::Pixels;
use gpui::SharedString;
use gpui::TextRun;
use gpui::Window;
use gpui::px;
use std::collections::BTreeSet;
use std::sync::Arc;

/// A laid-out line: wide characters keep their own width.
pub(crate) struct Measured {
    text: SharedString,
    layout: Arc<LineLayout>,
    advance: Pixels,
}

/// The unwrapped extent survives scrolling and caret blinks, but not text or font changes.
pub(crate) struct Extent {
    stamp: u64,
    font: Font,
    size: Pixels,
    line: Pixels,
    advance: Pixels,
    matched: Option<(usize, usize)>,
    folded: BTreeSet<usize>,
    width: Pixels,
}

impl Measured {
    /// `shown` in the configured base font at `size`, retaining semantic highlights.
    pub(crate) fn new(shown: &Shown, base: Font, size: Pixels, advance: Pixels, window: &Window) -> Self {
        let run = |len: usize, style: Option<&gpui::HighlightStyle>| {
            let mut font = base.clone();
            if let Some(style) = style {
                font.weight = style.font_weight.unwrap_or(font.weight);
                font.style = style.font_style.unwrap_or(font.style);
            }
            TextRun { len, font, color: Hsla::default(), background_color: None, underline: None, strikethrough: None }
        };
        let (text, mut runs, mut at) = (shown.text.clone(), Vec::new(), 0);
        for (range, style) in shown.styles.iter().filter(|(range, _)| !range.is_empty()) {
            if range.start > at {
                runs.push(run(range.start - at, None));
            }
            runs.push(run(range.len(), Some(style)));
            at = range.end;
        }
        if at < text.len() {
            runs.push(run(text.len() - at, None));
        }
        let layout = window.text_system().layout_line(&text, size, &runs, None);
        Self { text, layout, advance }
    }

    /// The same unwrapped width the text renderer uses.
    pub(crate) fn width(&self) -> Pixels {
        self.layout.width
    }

    /// Where `column` starts; past the end in whole advances.
    pub(crate) fn x(&self, column: usize) -> Pixels {
        match self.text.char_indices().nth(column) {
            Some((at, _)) => self.layout.x_for_index(at),
            None => {
                let past = column - self.text.chars().count();
                self.layout.width + self.advance * past as f32
            }
        }
    }

    /// The column starting nearest `x`, line end included.
    pub(crate) fn column(&self, x: Pixels) -> usize {
        let starts = self.layout.runs.iter().flat_map(|run| run.glyphs.iter().map(|glyph| (glyph.index, glyph.position.x)));
        let off = |edge: Pixels| f32::from((edge - x).abs());
        let at = starts
            .chain([(self.text.len(), self.layout.width)])
            .min_by(|a, b| off(a.1).total_cmp(&off(b.1)))
            .map(|(at, _)| at.min(self.text.len()))
            .expect("the line end is always a stop");
        self.text[..at].chars().count()
    }
}

impl CodeEditor {
    /// Reuses shaping until a source, decoration, or font metric changes.
    pub(crate) fn measure_extent(&mut self, window: &Window, cx: &Context<Self>) {
        let font = self.code_font(cx);
        if self.extent.as_ref().is_some_and(|extent| {
            extent.stamp == self.buffer.stamp()
                && extent.font == font
                && extent.size == self.metrics.size
                && extent.line == self.metrics.line
                && extent.advance == self.metrics.advance
                && extent.matched == self.frame.matched
                && extent.folded == self.folded
        }) {
            return;
        }
        let mut width = Pixels::ZERO;
        for row in &self.frame.rows {
            let shown = match *row {
                Row::Line(line) => {
                    let (_, measured) = self.measured(line, window, cx);
                    width = width.max(measured.width());
                    continue;
                }
                Row::Removed(hunk, at) => Shown::new(&self.marks.hunks[hunk].removed[at], Vec::new(), Vec::new()).with_tabs(self.options.tab),
                Row::Ghost(at) => self.ghost_shown(at, cx),
                _ => continue,
            };
            let measured = Measured::new(&shown, font.clone(), self.metrics.size, self.metrics.advance, window);
            width = width.max(measured.width());
        }
        self.extent = Some(Extent {
            stamp: self.buffer.stamp(),
            font,
            size: self.metrics.size,
            line: self.metrics.line,
            advance: self.metrics.advance,
            matched: self.frame.matched,
            folded: self.folded.clone(),
            width,
        });
    }

    /// Gutter, leading space, the widest row, and room for an end-of-line caret.
    pub(crate) fn content_width(&self) -> Pixels {
        let width = self.extent.as_ref().map_or(Pixels::ZERO, |extent| extent.width);
        self.metrics.advance * (self.gutter_columns() + 1) as f32 + width + (self.metrics.advance * 3.0).max(px(24.0))
    }

    /// A ghost continuation, including source text after its last line.
    pub(crate) fn ghost_shown(&self, at: usize, cx: &Context<Self>) -> Shown {
        let ghost = self.marks.ghost.as_ref().expect("a ghost row has its ghost");
        let mut lines = ghost.text.split('\n').skip(at);
        let mut text = lines.next().unwrap_or_default().to_string();
        let colors = &cx.theme().colors;
        let mut styles = vec![(
            0..text.len(),
            HighlightStyle { color: Some(colors.fg_subtle), font_style: Some(FontStyle::Italic), ..HighlightStyle::default() },
        )];
        if lines.next().is_none() {
            let line = self.buffer.line_of(ghost.offset);
            let rest = &self.buffer.text()[ghost.offset..self.buffer.line_range(line).end];
            let start = text.len();
            styles.extend(self.line_colors(rest, cx).into_iter().map(|(range, style)| (range.start + start..range.end + start, style)));
            text.push_str(rest);
        }
        Shown::new(&text, Vec::new(), styles).with_tabs(self.options.tab)
    }

    /// Runs before the list prepaints, so fixed row clips use its final X offset.
    pub(crate) fn prepare_scroll(&mut self, bounds: Bounds<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let handle = self.scroll_handle();
        let mut offset = handle.offset();
        // Taffy snaps the measured row width before GPUI rounds its scroll range.
        let width = window.pixel_snap(self.content_width());
        let max = (((width - bounds.size.width).max(Pixels::ZERO) * 100.0).round() / 100.0).max(Pixels::ZERO);
        offset.x = offset.x.clamp(-max, Pixels::ZERO);
        let advance = self.metrics.advance;
        if self.reveal_x && bounds.size.width > Pixels::ZERO && advance > Pixels::ZERO {
            let (line, column) = self.buffer.point(self.primary().head);
            let (shown, measured) = self.measured(line, window, cx);
            let column = shown.column(self.row_text(line), column.min(self.row_text(line).chars().count()));
            let left = advance + measured.x(column);
            let right = advance + measured.x(column + 1);
            let viewport = bounds.size.width - advance * self.gutter_columns() as f32;
            let margin = (advance * 2.0).max(px(16.0));
            if right + offset.x > viewport - margin {
                offset.x = viewport - margin - right;
            }
            if left + offset.x < advance {
                offset.x = advance - left;
            }
            offset.x = offset.x.clamp(-max, Pixels::ZERO);
            self.reveal_x = false;
        }
        if handle.offset() != offset {
            handle.set_offset(offset);
            // Sticky headers were built during render, before these layout bounds existed.
            cx.notify();
        }
        self.metrics.left = bounds.origin.x + advance * (self.gutter_columns() + 1) as f32 + offset.x;
    }

    /// The current code origin, even between an offset change and the next paint.
    pub(crate) fn text_left(&self) -> Pixels {
        let handle = self.scroll_handle();
        handle.bounds().origin.x + self.metrics.advance * (self.gutter_columns() + 1) as f32 + handle.offset().x
    }
}
