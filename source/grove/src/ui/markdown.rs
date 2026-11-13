use ely_core::forms::Highlight;
use ely_core::theme::ActiveTheme as _;
use gpui_kit::App;
use gpui_kit::FontWeight;
use markdown::mdast::Node;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ink {
    Heading,
    Marker,
    Quote,
    Emphasis,
    Strong,
    Code,
    Link,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Style {
    ink: Ink,
    bold: bool,
    italic: bool,
}

type Span = (Range<usize>, Style);

/// Parse immutable source once; only resolve theme colors when a line is painted.
///
/// The editor supplies text without a line index. Identical lines with different
/// Markdown contexts therefore remain plain, including lines repeated inside and
/// outside code fences. Fenced code uses one code color, without language lexing.
/// No source characters are replaced, decoded, or hidden.
pub(super) fn highlighter(source: &str) -> impl Fn(&str, &App) -> Vec<(Range<usize>, Highlight)> + 'static {
    let line_map = source_highlights(source);
    move |line, cx| {
        let Some(span_list) = line_map.get(line) else { return Vec::new() };
        let syntax = &cx.theme().colors.syntax;
        span_list
            .iter()
            .map(|(range, style)| {
                let color = match style.ink {
                    Ink::Heading | Ink::Marker | Ink::Strong => syntax.keyword,
                    Ink::Quote => syntax.comment,
                    Ink::Emphasis => syntax.type_name,
                    Ink::Code => syntax.string,
                    Ink::Link => syntax.function,
                };
                let highlight = Highlight { weight: style.bold.then_some(FontWeight::BOLD), italic: style.italic, ..Highlight::new(color) };
                (range.clone(), highlight)
            })
            .collect()
    }
}

fn source_highlights(source: &str) -> HashMap<String, Vec<Span>> {
    // CommonMark has no parse errors; only opt-in MDX can return an error.
    let root = markdown::to_mdast(source, &markdown::ParseOptions::default()).expect("CommonMark always parses");
    let mut span_list = Vec::new();
    collect(&root, source, None, &mut span_list);
    let span_list = flatten(span_list);
    let mut line_map: HashMap<String, Vec<Span>> = HashMap::new();
    let mut offset = 0;
    let mut first = 0;
    // Match the editor's buffer: only LF is removed, preserving a preceding CR.
    for line in source.split('\n') {
        let end = offset + line.len();
        while first < span_list.len() && span_list[first].0.end <= offset {
            first += 1;
        }
        let line_span_list = span_list[first..]
            .iter()
            .take_while(|(range, _)| range.start < end)
            .map(|(range, style)| (range.start.max(offset) - offset..range.end.min(end) - offset, *style))
            .collect::<Vec<_>>();
        if let Some(existing) = line_map.get_mut(line) {
            if *existing != line_span_list {
                // All occurrences share this entry; never invent a line index.
                existing.clear();
            }
        } else {
            line_map.insert(line.to_owned(), line_span_list);
        }
        offset = end + 1;
    }
    line_map
}

fn collect(node: &Node, source: &str, inherited: Option<Style>, span_list: &mut Vec<Span>) {
    let Some(position) = node.position() else { return };
    // markdown-rs positions are byte offsets into the original source.
    let range = position.start.offset..position.end.offset;
    let ink = match node {
        Node::Heading(_) => Some(Ink::Heading),
        Node::Emphasis(_) => Some(Ink::Emphasis),
        Node::Strong(_) => Some(Ink::Strong),
        Node::InlineCode(_) | Node::Code(_) => Some(Ink::Code),
        Node::Link(_) | Node::LinkReference(_) | Node::Image(_) | Node::ImageReference(_) | Node::Definition(_) => Some(Ink::Link),
        Node::ThematicBreak(_) => Some(Ink::Marker),
        _ => None,
    };
    let style = ink.map(|ink| Style {
        ink,
        bold: matches!(node, Node::Heading(_) | Node::Strong(_)) || inherited.is_some_and(|style| style.bold),
        italic: matches!(node, Node::Emphasis(_)) || inherited.is_some_and(|style| style.italic),
    });
    if let Some(style) = style {
        span_list.push((range.clone(), style));
    }
    match node {
        Node::ListItem(_) => {
            // The parser establishes that this is a list, including escape and
            // paragraph-interruption rules. Read only its literal marker width.
            let text = &source[range.clone()];
            let digits = text.bytes().take_while(u8::is_ascii_digit).count();
            span_list.push((range.start..range.start + digits + 1, Style { ink: Ink::Marker, bold: false, italic: false }));
        }
        Node::Blockquote(_) => {
            let mut offset = range.start;
            for line in source[range.clone()].split('\n') {
                for (at, byte) in line.bytes().enumerate() {
                    match byte {
                        b'>' => span_list.push((offset + at..offset + at + 1, Style { ink: Ink::Quote, bold: false, italic: false })),
                        b' ' | b'\t' => {}
                        _ => break,
                    }
                }
                offset += line.len() + 1;
            }
        }
        _ => {}
    }
    if let Some(child_list) = node.children() {
        for child in child_list {
            collect(child, source, style.or(inherited), span_list);
        }
    }
}

/// Later (deeper) AST styles win. Split once into sorted, disjoint byte runs.
fn flatten(span_list: Vec<Span>) -> Vec<Span> {
    let mut edge_list = Vec::with_capacity(span_list.len() * 2);
    for (index, (range, _)) in span_list.iter().enumerate() {
        if !range.is_empty() {
            edge_list.push((range.start, true, index));
            edge_list.push((range.end, false, index));
        }
    }
    edge_list.sort_unstable();
    let mut active_set: BTreeSet<usize> = BTreeSet::new();
    let mut result: Vec<Span> = Vec::new();
    let mut previous = 0;
    for (at, opening, index) in edge_list {
        if previous < at
            && let Some(&top) = active_set.last()
        {
            let style = span_list[top].1;
            if let Some((range, last_style)) = result.last_mut()
                && range.end == previous
                && *last_style == style
            {
                range.end = at;
            } else {
                result.push((previous..at, style));
            }
        }
        if opening {
            active_set.insert(index);
        } else {
            active_set.remove(&index);
        }
        previous = at;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_and_nested_styles_keep_original_byte_boundaries() {
        let source = "# 标题 **粗 *斜* 字** 与 `代码` [链接](https://例子.test)";
        let line_map = source_highlights(source);
        let span_list = &line_map[source];
        let mut previous = 0;
        for (range, _) in span_list {
            assert!(previous <= range.start);
            assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
            previous = range.end;
        }
        let style_at = |needle: &str| {
            let at = source.find(needle).unwrap();
            span_list.iter().find(|(range, _)| range.contains(&at)).unwrap().1
        };
        assert_eq!(style_at("标题").ink, Ink::Heading);
        assert!(style_at("粗").bold);
        assert!(style_at("斜").bold && style_at("斜").italic);
        assert_eq!(style_at("代码").ink, Ink::Code);
        assert_eq!(style_at("链接").ink, Ink::Link);
    }

    #[test]
    fn escaped_markdown_and_intraword_underscores_stay_literal() {
        let source = r"\# 标题 \*文字\* \`代码\` \[链接](url) file_name_value";
        assert!(source_highlights(source)[source].is_empty());
    }

    #[test]
    fn duplicate_lines_with_fence_context_do_not_receive_false_emphasis() {
        let source = "**相同**\n\n```text\n**相同**\n独有 *字面*\n```";
        let line_map = source_highlights(source);
        assert!(line_map["**相同**"].is_empty());
        assert!(line_map["独有 *字面*"].iter().all(|(_, style)| style.ink == Ink::Code && !style.italic));
    }
}
