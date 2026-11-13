//! Native GitHub light Markdown presentation; CSS cannot be applied to GPUI elements.
//! Palette and proportions follow github-markdown-css, with Preview typography as the base.
use crate::settings::Typography;
use ely_core::forms::code_highlights_with;
use ely_core::forms::syntax_fenced;
use ely_core::theme::ActiveTheme as _;
use ely_core::theme::Syntax as SyntaxPalette;
use gpui_kit::base::StyledExt as _;
use gpui_kit::base::text::CodeBlock;
use gpui_kit::base::text::SelectionFormat;
use gpui_kit::base::text::TextView;
use gpui_kit::base::text::TextViewStyle;
use gpui_kit::*;
use markdown::mdast::Node;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

const FOREGROUND: u32 = 0x1f2328;
const MUTED: u32 = 0x59636e;
const LINK: u32 = 0x0969da;
const BORDER: u32 = 0xd1d9e0;
const CODE_BACKGROUND: u32 = 0xf6f8fa;
const BACKGROUND: u32 = 0xffffff;

pub(super) fn render(source: impl Into<SharedString>, typography: &Typography) -> impl IntoElement {
    let source = source.into();
    let label_style = typography.style();
    MarkdownPreview {
        source: source.clone(),
        view: TextView::markdown("message", source)
            .selectable(true)
            .selection_format(SelectionFormat::Source)
            .style(github_style(typography))
            .refine_style(&typography.style())
            .text_color(rgb(FOREGROUND))
            .code_block_actions(move |code, _, _| div().refine_style(&label_style).text_color(rgb(MUTED)).children(code.lang().filter(|lang| !lang.is_empty()))),
    }
}

struct MarkdownPreview {
    source: SharedString,
    view: TextView,
}

impl IntoElement for MarkdownPreview {
    type Element = ViewElement<Self>;

    fn into_element(self) -> Self::Element {
        ViewElement::new(self)
    }
}

impl RenderOnce for MarkdownPreview {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let cache = window.use_keyed_state("message-code-highlighter", cx, |_, cx| CodeHighlightCache::new(self.source.clone(), cx));
        if cache.read(cx).source != self.source || cache.read(cx).palette != cx.theme().colors.syntax {
            cache.update(cx, |cache, cx| *cache = CodeHighlightCache::new(self.source, cx));
        }
        self.view.shared_code_block_highlighter(cache.read(cx).highlighter.clone())
    }
}

type CodeHighlights = Vec<(Range<usize>, HighlightStyle)>;
type CodeHighlighter = dyn Fn(&CodeBlock) -> CodeHighlights + Send + Sync;

struct CodeHighlightCache {
    source: SharedString,
    palette: SyntaxPalette,
    highlighter: Arc<CodeHighlighter>,
}

impl CodeHighlightCache {
    fn new(source: SharedString, cx: &App) -> Self {
        let mut highlight_map = HashMap::new();
        // Base's highlighter callback has no App parameter. Resolve ELY's
        // language-aware lexer once per message/palette change, then retain
        // the callback identity so native code blocks reuse their own cache.
        if let Ok(root) = markdown::to_mdast(&source, &markdown::ParseOptions::gfm()) {
            collect_code_highlights(&root, &mut highlight_map, cx);
        }
        Self {
            source,
            palette: cx.theme().colors.syntax.clone(),
            highlighter: Arc::new(move |block| block.span.and_then(|span| highlight_map.get(&span.start)).cloned().unwrap_or_default()),
        }
    }
}

fn collect_code_highlights(node: &Node, highlight_map: &mut HashMap<usize, CodeHighlights>, cx: &App) {
    if let Node::Code(code) = node {
        if let Some(position) = &code.position
            && let Some(syntax) = code.lang.as_deref().and_then(syntax_fenced)
        {
            let highlight_list = code_highlights_with(&code.value, syntax, cx)
                .into_iter()
                .map(|(range, highlight)| {
                    (
                        range,
                        HighlightStyle {
                            color: Some(highlight.color),
                            background_color: highlight.background,
                            font_weight: highlight.weight,
                            ..Default::default()
                        },
                    )
                })
                .collect();
            highlight_map.insert(position.start.offset, highlight_list);
        }
    } else if let Some(child_list) = node.children() {
        for child in child_list {
            collect_code_highlights(child, highlight_map, cx);
        }
    }
}

fn github_style(typography: &Typography) -> TextViewStyle {
    let font_size = typography.font_size;
    let inline_code = StyleRefinement::default()
        .font_family(typography.font_family.clone())
        .text_size(px(font_size))
        .font_weight(FontWeight(f32::from(typography.font_weight)))
        .bg(rgb(CODE_BACKGROUND))
        .text_color(rgb(FOREGROUND))
        .rounded(px(6.))
        .px(px(font_size * 0.4))
        .py(px(font_size * 0.2));
    let mut code_block = typography
        .style()
        .bg(rgb(CODE_BACKGROUND))
        .text_color(rgb(FOREGROUND))
        .border_0()
        .rounded(px(6.))
        .p(px(16.))
        .pt(px(font_size * typography.line_height + 16.))
        .whitespace_nowrap();
    // The native actions hook supplies each code block's stable element id,
    // allowing its no-wrap content to scroll horizontally without a WebView.
    code_block.overflow.x = Some(Overflow::Scroll);
    let mut table = typography.style().bg(rgb(BACKGROUND)).border_1().border_color(rgb(BORDER)).rounded(px(6.));
    table.overflow.x = Some(Overflow::Scroll);

    TextViewStyle::default()
        .with_dark(false)
        .with_foreground(rgb(FOREGROUND).into())
        .with_muted_foreground(rgb(MUTED).into())
        .with_link(rgb(LINK).into())
        .with_selection(rgba(0x0969da33).into())
        .with_border(rgb(BORDER).into())
        .with_code_background(rgb(CODE_BACKGROUND).into())
        .with_paragraph_gap(rems(1.))
        .with_heading(move |level| {
            let scale = match level {
                1 => 2.,
                2 => 1.5,
                3 => 1.25,
                4 => 1.,
                5 => 0.875,
                _ => 0.85,
            };
            let mut heading = StyleRefinement::default()
                .text_size(px(font_size * scale))
                .font_weight(FontWeight::SEMIBOLD)
                .line_height(relative(1.25))
                .mb(px(font_size));
            if level <= 2 {
                heading = heading.border_b_1().border_color(rgb(BORDER)).pb(px(font_size * scale * 0.3));
            }
            heading
        })
        .with_inline_code_style(inline_code)
        .with_code_block(code_block)
        .with_blockquote_style(
            StyleRefinement::default()
                .text_color(rgb(MUTED))
                .border_l(px(font_size * 0.25))
                .border_color(rgb(BORDER))
                .px(px(font_size)),
        )
        .with_list_style(StyleRefinement::default().text_color(rgb(FOREGROUND)))
        .with_table(table)
        .with_table_head(StyleRefinement::default().bg(rgb(BACKGROUND)).font_weight(FontWeight::SEMIBOLD))
        .with_table_cell(StyleRefinement::default().px(px(13.)).py(px(6.)).border_color(rgb(BORDER)))
}
