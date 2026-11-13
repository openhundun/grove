use super::badges::DiffStat;
use super::diff::DiffLine;
use super::diff::LineKind;
use super::diff::Stretch;
use super::diff::diff;
use super::diff::side_numbers;
use super::rows::Place;
use super::rows::Shown;
use super::rows::code;
use super::rows::shown;
use super::rows::spoken;
use super::rows::washes;
use crate::buttons::Button;
use crate::buttons::ButtonVariant;
use crate::buttons::SegmentedControl;
use crate::forms::Syntax;
use crate::forms::syntax_of;
use crate::i18n;
use crate::primitives::tab_stop;
use crate::theme::ActiveTheme;
use crate::theme::ControlSize;
use crate::theme::TextSize;
use crate::typography::LEADING;
use gpui::AnyElement;
use gpui::App;
use gpui::BorderStyle;
use gpui::Bounds;
use gpui::ElementId;
use gpui::FocusHandle;
use gpui::FontWeight;
use gpui::Hsla;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::ListHorizontalSizingBehavior;
use gpui::MouseButton;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::RenderOnce;
use gpui::Role;
use gpui::SharedString;
use gpui::StatefulInteractiveElement;
use gpui::Styled;
use gpui::UniformListScrollHandle;
use gpui::Window;
use gpui::canvas;
use gpui::div;
use gpui::font;
use gpui::outline;
use gpui::point;
use gpui::prelude::*;
use gpui::px;
use gpui::size;
use gpui::uniform_list;
use std::cell::Cell;
use std::rc::Rc;

/// How a diff lays out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLayout {
    /// Old and new lines in one column.
    Unified,
    /// Old lines left, new lines right.
    Split,
}

/// Unchanged lines kept around each change.
const CONTEXT: usize = 3;

type OnIndex = Rc<dyn Fn(usize, &mut Window, &mut App)>;
type OnLayout = Rc<dyn Fn(DiffLayout, &mut Window, &mut App)>;
type OnLine = Rc<dyn Fn(Place, bool, &mut Window, &mut App)>;
type Scrollbars = Rc<dyn Fn(&UniformListScrollHandle, &mut Window, &mut App) -> AnyElement>;

/// Persistent diff viewports: Split links both axes, clamping each pane to its own range.
#[derive(Clone, Default)]
pub struct DiffScrollHandle(Rc<DiffScrollHandles>);

#[derive(Default)]
struct DiffScrollHandles {
    unified: UniformListScrollHandle,
    left: UniformListScrollHandle,
    right: UniformListScrollHandle,
    last: Cell<[gpui::Point<Pixels>; 2]>,
}

impl DiffScrollHandle {
    /// The single Unified viewport.
    pub fn unified(&self) -> &UniformListScrollHandle {
        &self.0.unified
    }

    /// The old-file Split viewport, linked to the right on both axes.
    pub fn left(&self) -> &UniformListScrollHandle {
        &self.0.left
    }

    /// The new-file Split viewport, linked to the left on both axes.
    pub fn right(&self) -> &UniformListScrollHandle {
        &self.0.right
    }

    fn sync(&self, max_x: [Pixels; 2], max_y: Pixels) {
        let left = self.left().0.borrow().base_handle.clone();
        let right = self.right().0.borrow().base_handle.clone();
        let clamp = |offset: gpui::Point<Pixels>, index: usize| point(offset.x.clamp(-max_x[index], Pixels::ZERO), offset.y.clamp(-max_y, Pixels::ZERO));
        // UniformList normalizes wheel input to hundredths of a pixel. Apply the
        // same precision before recording offsets so rounding cannot feed back.
        let rounded = |value: Pixels| (value * 100.0).round() / 100.0;
        let (requested_a, requested_b) = (left.offset(), right.offset());
        let (a, b) = (clamp(requested_a, 0), clamp(requested_b, 1));
        let last = self.0.last.get();
        let (last_a, last_b) = (clamp(last[0], 0), clamp(last[1], 1));
        // Compare bounded offsets so a range change is not fresh input, but pass
        // the requested coordinate on before clamping each pane independently.
        // Otherwise a shared out-of-range request loses the wider pane's extent.
        let x = if a.x != last_a.x {
            requested_a.x
        } else if b.x != last_b.x {
            requested_b.x
        } else {
            requested_a.x.min(requested_b.x)
        };
        let y = if a.y != last_a.y { a.y } else { b.y };
        let applied = [clamp(point(rounded(x), rounded(y)), 0), clamp(point(rounded(x), rounded(y)), 1)];
        left.set_offset(applied[0]);
        right.set_offset(applied[1]);
        self.0.last.set(applied);
    }
}

#[derive(Default)]
struct DiffScrollState {
    scroll: DiffScrollHandle,
    viewport: [Rc<Cell<Bounds<Pixels>>>; 3],
    measured: Option<MeasuredDiff>,
}

struct MeasuredDiff {
    stretches: Rc<Vec<Stretch>>,
    family: SharedString,
    size: Pixels,
    weight: FontWeight,
    line_height: f32,
    syntax: Option<&'static Syntax>,
    layout: DiffLayout,
    open: Vec<usize>,
    code: [Pixels; 3],
    header: Pixels,
}

fn text_width(text: impl IntoElement, family: SharedString, font_size: Pixels, weight: FontWeight, window: &mut Window, cx: &mut App) -> Pixels {
    div()
        .font_family(family)
        .text_size(font_size)
        .font_weight(weight)
        .whitespace_nowrap()
        .child(text)
        .into_any_element()
        .layout_as_root(size(gpui::AvailableSpace::MaxContent, gpui::AvailableSpace::MaxContent), window, cx)
        .width
}

fn content_width(viewport: Pixels, code: Pixels, header: Pixels, fixed: Pixels) -> Pixels {
    viewport.max(fixed + code).max(header)
}

/// Focus borders paint inside the fixed row slot without taking space from its line box.
fn focus_frame(focus: FocusHandle, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            if focus.is_focused(window) {
                window.paint_quad(outline(bounds, color, BorderStyle::Solid));
            }
        },
    )
    .absolute()
    .inset_0()
}

/// A file's changes, unified or split; fills its box.
///
/// ```
/// use ely_git::{DiffLayout, DiffViewer};
///
/// let change = DiffViewer::new("change", "src/lib.rs", "let a = 1;\n", "let a = 2;\n")
///     .layout(DiffLayout::Split)
///     .on_layout(|_layout, _, _| {});
/// ```
#[derive(IntoElement)]
pub struct DiffViewer {
    id: ElementId,
    path: SharedString,
    /// How its lines read, from the path.
    pub(super) syntax: Option<&'static Syntax>,
    stretches: Rc<Vec<Stretch>>,
    stat: (usize, usize),
    layout: DiffLayout,
    font_family: Option<SharedString>,
    font_size: Option<f32>,
    font_weight: FontWeight,
    line_height: Option<f32>,
    open: Vec<usize>,
    on_open: Option<OnIndex>,
    on_layout: Option<OnLayout>,
    headless: bool,
    selected: Rc<Vec<Place>>,
    on_line: Option<OnLine>,
    hunk_action: Option<(SharedString, OnIndex)>,
    scroll: Option<DiffScrollHandle>,
    scrollbars: Option<Scrollbars>,
}

impl DiffViewer {
    /// Views the change in `path` from `old` to `new`.
    pub fn new(id: impl Into<ElementId>, path: impl Into<SharedString>, old: &str, new: &str) -> Self {
        let path: SharedString = path.into();
        Self {
            id: id.into(),
            syntax: syntax_of(&path),
            path,
            stretches: Rc::new(diff(old, new, CONTEXT)),
            stat: super::diff::stat(old, new),
            layout: DiffLayout::Unified,
            font_family: None,
            font_size: None,
            font_weight: FontWeight::NORMAL,
            line_height: None,
            open: Vec::new(),
            on_open: None,
            on_layout: None,
            headless: false,
            selected: Rc::default(),
            on_line: None,
            hunk_action: None,
            scroll: None,
            scrollbars: None,
        }
    }

    /// Stretches made elsewhere, such as from Git's own diff.
    pub fn from_stretches(id: impl Into<ElementId>, path: impl Into<SharedString>, stretches: Rc<Vec<Stretch>>) -> Self {
        let stat = stretches
            .iter()
            .filter_map(|stretch| match stretch {
                Stretch::Hunk { lines, .. } => Some(lines),
                Stretch::Folded(_) => None,
            })
            .flatten()
            .fold((0, 0), |(added, removed), line| match line.kind {
                LineKind::Added => (added + 1, removed),
                LineKind::Removed => (added, removed + 1),
                LineKind::Same => (added, removed),
            });
        Self { stretches, stat, ..Self::new(id, path, "", "") }
    }

    /// Changed lines shown as picked.
    pub fn selected(mut self, places: impl IntoIterator<Item = Place>) -> Self {
        self.selected = Rc::new(places.into_iter().collect());
        self
    }

    /// Makes changed lines pressable; true when Shift was held.
    pub fn on_line(mut self, handler: impl Fn(Place, bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_line = Some(Rc::new(handler));
        self
    }

    /// A button on each hunk header, given its place.
    pub fn hunk_action(mut self, label: impl Into<SharedString>, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.hunk_action = Some((label.into(), Rc::new(handler)));
        self
    }

    /// Leaves header, path, counts and layout to the owner.
    pub fn headless(mut self) -> Self {
        self.headless = true;
        self
    }

    /// How its lines lay out.
    pub fn layout(mut self, layout: DiffLayout) -> Self {
        self.layout = layout;
        self
    }

    /// The code and gutter font for this viewer, without changing the theme.
    pub fn font_family(mut self, family: impl Into<SharedString>) -> Self {
        self.font_family = Some(family.into());
        self
    }

    /// Code and gutter text size in pixels; otherwise the theme's extra-small text size.
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }

    /// The base code and gutter weight, retaining semantic syntax highlights.
    pub fn font_weight(mut self, weight: FontWeight) -> Self {
        self.font_weight = weight;
        self
    }

    /// Row height as a multiple of the text size, without extra border padding.
    /// Unconfigured viewers retain the theme's leading and legacy 2px row allowance.
    pub fn line_height(mut self, multiplier: f32) -> Self {
        self.line_height = Some(multiplier);
        self
    }

    /// Shares the Unified viewport and two Split viewports linked on both axes.
    pub fn track_scroll(mut self, handle: &DiffScrollHandle) -> Self {
        self.scroll = Some(handle.clone());
        self
    }

    /// Renders the owner's scrollbar chrome inside each actual viewport.
    pub fn scrollbars(mut self, render: impl Fn(&UniformListScrollHandle, &mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.scrollbars = Some(Rc::new(render));
        self
    }

    /// Folded stretches shown in full, by their place.
    pub fn open(mut self, stretches: impl IntoIterator<Item = usize>) -> Self {
        self.open = stretches.into_iter().collect();
        self
    }

    /// Runs with the place of a folded stretch pressed open.
    pub fn on_open(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_open = Some(Rc::new(handler));
        self
    }

    /// Runs with the layout picked.
    pub fn on_layout(mut self, handler: impl Fn(DiffLayout, &mut Window, &mut App) + 'static) -> Self {
        self.on_layout = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for DiffViewer {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let rows = Rc::new(shown(&self.stretches, self.layout, &self.open));
        let layout = SegmentedControl::new(
            (self.id.clone(), "layout"),
            match self.layout {
                DiffLayout::Unified => "unified",
                DiffLayout::Split => "split",
            },
        )
        .size(ControlSize::Sm)
        .segment("unified", i18n::text(cx, "git.diff.unified", &[]), None)
        .segment("split", i18n::text(cx, "git.diff.split", &[]), None);
        let layout = match self.on_layout {
            Some(on_layout) => layout.on_change(move |key, window, cx| {
                let next = if key.as_ref() == "split" { DiffLayout::Split } else { DiffLayout::Unified };
                on_layout(next, window, cx)
            }),
            None => layout,
        };
        let digits = self
            .stretches
            .iter()
            .flat_map(|stretch| match stretch {
                Stretch::Hunk { lines, .. } | Stretch::Folded(lines) => lines.iter(),
            })
            .flat_map(|line| [line.old, line.new])
            .flatten()
            .max()
            .unwrap_or(1)
            .to_string()
            .len()
            .max(2);
        let family = self.font_family.clone().unwrap_or_else(|| theme.mono_family.clone());
        let font_size = self.font_size.map(px).unwrap_or_else(|| theme.text_size(TextSize::Xs).to_pixels(window.rem_size()));
        let font_weight = self.font_weight;
        let mut code_font = font(family.clone());
        code_font.weight = font_weight;
        let resolved = window.text_system().resolve_font(&code_font);
        let advance = match window.text_system().advance(resolved, font_size, '0') {
            Ok(advance) => advance.width,
            Err(error) => {
                log::error!("diff viewer: no advance for the code font: {error:#}; gutter guessed");
                font_size * 0.62
            }
        };
        let leading = self.line_height.unwrap_or(LEADING);
        let line_height = (font_size * leading).ceil();
        let outer_size = theme.text_size(TextSize::Sm);
        let button_height = theme.control_height(ControlSize::Sm).to_pixels(window.rem_size());
        let gap = window.rem_size() * 0.5;
        let number_width = advance * digits as f32 + font_size * 0.8;
        let state = window.use_keyed_state((self.id.clone(), "scroll-state"), cx, |_, _| DiffScrollState::default());
        let (scroll, viewport) = {
            let state = state.read(cx);
            (self.scroll.clone().unwrap_or_else(|| state.scroll.clone()), state.viewport.clone())
        };
        let cached = state.read(cx).measured.as_ref().is_some_and(|cached| {
            Rc::ptr_eq(&cached.stretches, &self.stretches)
                && cached.family == family
                && cached.size == font_size
                && cached.weight == font_weight
                && cached.line_height == leading
                && cached.syntax.map(|syntax| syntax as *const Syntax) == self.syntax.map(|syntax| syntax as *const Syntax)
                && cached.layout == self.layout
                && cached.open == self.open
        });
        if !cached {
            let mut code_width = [Pixels::ZERO; 3];
            let mut header_width = Pixels::ZERO;
            for row in rows.iter() {
                match row {
                    Shown::Line(_, line) => code_width[0] = code_width[0].max(text_width(code(line, &colors, self.syntax, cx), family.clone(), font_size, font_weight, window, cx)),
                    Shown::Pair(left, right) => {
                        for (index, side) in [left, right].into_iter().enumerate() {
                            if let Some((_, line)) = side {
                                code_width[index + 1] = code_width[index + 1].max(text_width(code(line, &colors, self.syntax, cx), family.clone(), font_size, font_weight, window, cx));
                            }
                        }
                    }
                    Shown::Header(_, header) => header_width = header_width.max(text_width(header.clone(), family.clone(), font_size, font_weight, window, cx)),
                    Shown::Fold(_, _) => {}
                }
            }
            state.update(cx, |state, _| {
                state.measured = Some(MeasuredDiff {
                    stretches: self.stretches.clone(),
                    family: family.clone(),
                    size: font_size,
                    weight: font_weight,
                    line_height: leading,
                    syntax: self.syntax,
                    layout: self.layout,
                    open: self.open.clone(),
                    code: code_width,
                    header: header_width,
                });
            });
        }
        let (code_width, header_width) = {
            let state = state.read(cx);
            let measured = state.measured.as_ref().expect("the diff was measured above");
            (measured.code, measured.header + gap * 2.0 + px(26.0))
        };
        let sign_width = text_width("−", family.clone(), font_size, font_weight, window, cx);
        let fixed = if self.layout == DiffLayout::Split {
            number_width + gap * 3.0 + px(26.0)
        } else {
            number_width * 2.0 + gap * 5.0 + sign_width + px(26.0)
        };
        let syntax = self.syntax;
        let button = self.hunk_action.is_some();
        let split = self.layout == DiffLayout::Split;
        // Headers, code, empty partners and focus borders occupy one shared slot.
        let row_height = if self.line_height.is_some() { line_height } else { (font_size * leading + px(2.0)).ceil() };
        let row_height = if button { row_height.max(button_height) } else { row_height };
        let (selected, on_line, hunk_action) = (self.selected.clone(), self.on_line.clone(), self.hunk_action.clone());
        let pane_count = if split { 2 } else { 1 };
        let mut pane_list = Vec::with_capacity(pane_count);
        for pane_index in 0..pane_count {
            let slot = if split { pane_index + 1 } else { 0 };
            let pane_id = (
                self.id.clone(),
                match slot {
                    1 => "left",
                    2 => "right",
                    _ => "unified",
                },
            );
            let pane_scroll = match slot {
                1 => scroll.left(),
                2 => scroll.right(),
                _ => scroll.unified(),
            }
            .clone();
            let pane_viewport = viewport[slot].clone();
            let code_width = code_width[slot];
            let rows = rows.clone();
            let (selected, on_line, hunk_action) = (selected.clone(), on_line.clone(), hunk_action.clone());
            let (id, on_open) = (ElementId::from(pane_id.clone()), self.on_open.clone());
            let list_scroll = pane_scroll.clone();
            let list_viewport = pane_viewport.clone();
            let list = uniform_list((id.clone(), "rows"), rows.len(), move |range, window, cx| {
                let colors = cx.theme().colors.clone();
                let viewport_width = list_viewport.get().size.width;
                let offset = list_scroll.0.borrow().base_handle.offset().x;
                let extent = content_width(viewport_width, code_width, header_width, fixed);
                let number = |value: Option<usize>| {
                    div()
                        .flex_none()
                        .w(number_width)
                        .text_right()
                        .text_color(colors.fg_subtle)
                        .children(value.map(|value| value.to_string()))
                };
                // Changed lines toggle when the owner picks lines.
                let pick = |element: gpui::Div, own: ElementId, place: Place, line: &DiffLine, window: &mut Window, cx: &mut App| -> gpui::Stateful<gpui::Div> {
                    let picked = selected.contains(&place);
                    let element = element.when(picked, |element| element.bg(colors.selection));
                    match (&on_line, line.kind) {
                        (Some(on_line), LineKind::Added | LineKind::Removed) => {
                            let on_line = on_line.clone();
                            let own = ElementId::from((id.clone(), format!("line-{}-{}", place.0, place.1)));
                            let focus = tab_stop((own.clone(), "focus").into(), true, window, cx);
                            element
                                .id(own)
                                .role(Role::ListItem)
                                .aria_label(spoken(line))
                                .aria_selected(picked)
                                .tab_index(0)
                                .track_focus(&focus)
                                .child(focus_frame(focus, colors.focus))
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                                .on_click(move |event, window, cx| on_line(place, event.modifiers().shift, window, cx))
                        }
                        _ => element.id(own).role(Role::Label).aria_label(spoken(line)),
                    }
                };
                let side = |line: Option<&(Place, DiffLine)>, shown: Option<usize>, at: &str, window: &mut Window, cx: &mut App| -> AnyElement {
                    let (wash, _) = line.map_or((Some(colors.hover.opacity(0.5)), colors.hover), |(_, line)| washes(line.kind, &colors));
                    let own = ElementId::from(SharedString::from(at.to_string()));
                    let selector = format!("diff-{at}");
                    let code_selector = format!("diff-code-{at}");
                    let element = div()
                        .debug_selector(move || selector)
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .items_center()
                        .gap_2()
                        .overflow_hidden()
                        .px_2()
                        .when_some(wash, |side, wash| side.bg(wash))
                        .child(number(shown))
                        .children(line.map(|(_, line)| {
                            div().relative().flex_1().min_w_0().h_full().overflow_hidden().debug_selector(move || code_selector).child(
                                div()
                                    .absolute()
                                    .left(offset)
                                    .top((row_height - line_height) / 2.0)
                                    .h(line_height)
                                    .whitespace_nowrap()
                                    .child(code(line, &colors, syntax, cx)),
                            )
                        }));
                    match line {
                        Some((place, line)) => pick(element, own, *place, line, window, cx).into_any_element(),
                        None => element.id(own).into_any_element(),
                    }
                };
                range
                    .map(|ix| {
                        let row = div()
                            .debug_selector(|| format!("diff-row-{ix}"))
                            .absolute()
                            .left(-offset)
                            .w(viewport_width)
                            .h(row_height)
                            .flex()
                            .whitespace_nowrap()
                            .line_height(line_height)
                            .when(split, |row| row.items_stretch())
                            .when(!split, |row| row.px_2())
                            .when(!split && button, |row| row.items_center());
                        let row = match &rows[ix] {
                            Shown::Header(stretch, header) => {
                                let action = hunk_action.clone().map(|(label, run)| {
                                    let stretch = *stretch;
                                    Button::new((id.clone(), format!("hunk-{stretch}")), label)
                                        .variant(ButtonVariant::Ghost)
                                        .size(ControlSize::Sm)
                                        .on_click(move |_, window, cx| run(stretch, window, cx))
                                });
                                row.id(ix)
                                    .when(split, |row| row.px_2())
                                    .role(Role::Heading)
                                    .aria_label(header.clone())
                                    .bg(colors.hover)
                                    .items_center()
                                    .text_color(colors.fg_muted)
                                    .child(div().flex_1().min_w_0().overflow_hidden().child(header.clone()))
                                    .children(action)
                                    .into_any_element()
                            }
                            Shown::Line(place, line) => {
                                let (wash, _) = washes(line.kind, &colors);
                                let sign = match line.kind {
                                    LineKind::Same => " ",
                                    LineKind::Added => "+",
                                    LineKind::Removed => "−",
                                };
                                let row = row.gap_2().when_some(wash, |row, wash| row.bg(wash));
                                pick(row, ElementId::from(ix), *place, line, window, cx)
                                    .child(number(line.old))
                                    .child(number(line.new))
                                    .child(div().flex_none().text_color(colors.fg_subtle).child(sign))
                                    .child(
                                        div().debug_selector(|| format!("diff-code-{ix}")).flex_1().min_w_0().overflow_hidden().relative().h_full().child(
                                            div()
                                                .absolute()
                                                .left(offset)
                                                .top((row_height - line_height) / 2.0)
                                                .h(line_height)
                                                .whitespace_nowrap()
                                                .child(code(line, &colors, syntax, cx)),
                                        ),
                                    )
                                    .into_any_element()
                            }
                            Shown::Pair(left, right) => {
                                let (old, new) = side_numbers(left.as_ref().map(|(_, line)| line), right.as_ref().map(|(_, line)| line));
                                row.id(ix)
                                    .child(if pane_index == 0 {
                                        side(left.as_ref(), old, &format!("left-{ix}"), window, cx)
                                    } else {
                                        side(right.as_ref(), new, &format!("right-{ix}"), window, cx)
                                    })
                                    .into_any_element()
                            }
                            Shown::Fold(stretch, count) => {
                                let (open, stretch) = (on_open.clone(), *stretch);
                                let own = ElementId::from((id.clone(), format!("fold-{stretch}")));
                                let said = i18n::text(cx, if *count == 1 { "git.diff.unchanged.one" } else { "git.diff.unchanged.other" }, &[("n", &count.to_string())]);
                                let role = if open.is_some() { Role::Button } else { Role::Label };
                                row.id(own.clone())
                                    .role(role)
                                    .aria_label(said.clone())
                                    .justify_center()
                                    .when(split, |row| row.px_2().items_center())
                                    .bg(colors.hover.opacity(0.5))
                                    .text_color(colors.fg_subtle)
                                    .when_some(open, |row, open| {
                                        let focus = tab_stop((own, "focus").into(), true, window, cx);
                                        row.tab_index(0)
                                            .track_focus(&focus)
                                            .child(focus_frame(focus, colors.focus))
                                            .cursor_pointer()
                                            .hover(|row| row.text_color(colors.fg))
                                            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                                            .on_click(move |_, window, cx| open(stretch, window, cx))
                                    })
                                    .child(format!("⋯ {said}"))
                                    .into_any_element()
                            }
                        };
                        div().relative().w_full().min_w(extent).h(row_height).child(row).into_any_element()
                    })
                    .collect()
            })
            .track_scroll(&pane_scroll)
            .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
            .pb(px(18.0))
            .size_full();
            let scrollbars = self.scrollbars.as_ref().map(|render| render(&pane_scroll, window, cx));
            pane_list.push(
                div()
                    .id(pane_id)
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .h_full()
                    .overflow_hidden()
                    .font_family(family.clone())
                    .text_size(font_size)
                    .font_weight(font_weight)
                    .line_height(line_height)
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                if pane_viewport.replace(bounds) != bounds {
                                    window.request_animation_frame();
                                }
                                let width = window.pixel_snap(content_width(bounds.size.width, code_width, header_width, fixed));
                                let max = (((width - bounds.size.width).max(Pixels::ZERO) * 100.0).round() / 100.0).max(Pixels::ZERO);
                                let handle = pane_scroll.0.borrow().base_handle.clone();
                                let offset = handle.offset();
                                if !split {
                                    handle.set_offset(point(offset.x.clamp(-max, Pixels::ZERO), offset.y));
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .child(list)
                    .children(scrollbars)
                    .into_any_element(),
            );
        }
        let row_count = rows.len();
        let sync_scroll = scroll.clone();
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_size(outer_size)
            .when(!self.headless, |view| {
                view.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_2()
                        .py_1p5()
                        .border_b_1()
                        .border_color(colors.border)
                        .child(div().flex_1().min_w_0().font_weight(FontWeight::MEDIUM).text_color(colors.fg).child(self.path))
                        .child(DiffStat::new(self.stat.0, self.stat.1))
                        .child(layout),
                )
            })
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .when(split, |body| body.gap_1())
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                if split {
                                    let height = row_height * row_count as f32 + px(18.0);
                                    let max = (((height - bounds.size.height).max(Pixels::ZERO) * 100.0).round() / 100.0).max(Pixels::ZERO);
                                    let pane_width = (bounds.size.width - window.rem_size() * 0.25) / 2.0;
                                    let max_x = [code_width[1], code_width[2]].map(|code| {
                                        let width = window.pixel_snap(content_width(pane_width, code, header_width, fixed));
                                        (((width - pane_width).max(Pixels::ZERO) * 100.0).round() / 100.0).max(Pixels::ZERO)
                                    });
                                    sync_scroll.sync(max_x, max);
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .children(pane_list),
            )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn stretches_made_elsewhere_count_their_lines() {
        let stretches = std::rc::Rc::new(super::diff("a\nb\nc\n", "a\nx\ny\nc\n", 1));
        let viewer = super::DiffViewer::from_stretches("counted", "a.txt", stretches);
        assert_eq!(viewer.stat, (2, 1));
    }
}
