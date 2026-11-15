//! The rows a diff viewer draws, and their colors.

use super::diff::DiffLine;
use super::diff::LineKind;
use super::diff::Stretch;
use super::diff::pair_places;
use super::viewer::DiffLayout;
use crate::editor::code_colors_with;
use crate::editor::stack;
use crate::forms::Syntax;
use crate::theme::Palette;
use gpui::App;
use gpui::HighlightStyle;
use gpui::Hsla;
use gpui::SharedString;
use gpui::StyledText;
use std::ops::Range;

/// A line's place: its stretch, then its line there.
pub type Place = (usize, usize);

/// A folded stretch keeps its identity and actual unchanged source ranges.
#[derive(Clone, Debug)]
pub(super) struct Fold {
    pub stretch: usize,
    pub description: SharedString,
    pub expanded: bool,
}

impl Fold {
    fn new(stretch: usize, lines: &[DiffLine], open: &[usize]) -> Self {
        let range = |old: bool| {
            let mut number_list = lines.iter().filter_map(|line| if old { line.old } else { line.new });
            match number_list.next() {
                Some(first) => match number_list.next_back() {
                    Some(last) if last != first => format!("{first}–{last}"),
                    _ => first.to_string(),
                },
                None => "∅".to_owned(),
            }
        };
        Self {
            stretch,
            description: format!("Unchanged −{} +{} · {} {}", range(true), range(false), lines.len(), if lines.len() == 1 { "line" } else { "lines" }).into(),
            expanded: open.contains(&stretch),
        }
    }
}

/// A row as the viewer draws it.
#[derive(Clone)]
pub(super) enum Shown {
    /// A real hunk range, optionally controlling its preceding unchanged stretch.
    Header(usize, SharedString, Option<Fold>),
    Line(Place, DiffLine),
    Pair(Option<(Place, DiffLine)>, Option<(Place, DiffLine)>),
    /// Trailing or entirely unchanged context has no following hunk header.
    Fold(Fold),
}

fn unchanged_rows(rows: &mut Vec<Shown>, stretch: usize, lines: &[DiffLine], layout: DiffLayout) {
    rows.extend(lines.iter().enumerate().map(|(at, line)| {
        let place = (stretch, at);
        match layout {
            DiffLayout::Unified => Shown::Line(place, line.clone()),
            DiffLayout::Split => Shown::Pair(Some((place, line.clone())), Some((place, line.clone()))),
        }
    }));
}

pub(super) fn shown(stretches: &[Stretch], layout: DiffLayout, open: &[usize]) -> Vec<Shown> {
    let mut rows = Vec::new();
    for (ix, stretch) in stretches.iter().enumerate() {
        match stretch {
            Stretch::Hunk { header, lines } => {
                let preceding = ix.checked_sub(1).and_then(|previous| match &stretches[previous] {
                    Stretch::Folded(lines) => Some((previous, lines)),
                    Stretch::Hunk { .. } => None,
                });
                rows.push(Shown::Header(ix, header.clone(), preceding.map(|(previous, lines)| Fold::new(previous, lines, open))));
                if let Some((previous, lines)) = preceding.filter(|(previous, _)| open.contains(previous)) {
                    unchanged_rows(&mut rows, previous, lines, layout);
                }
                let at = |line: usize| ((ix, line), lines[line].clone());
                match layout {
                    DiffLayout::Unified => rows.extend((0..lines.len()).map(|line| {
                        let (place, line) = at(line);
                        Shown::Line(place, line)
                    })),
                    DiffLayout::Split => rows.extend(pair_places(lines).into_iter().map(|(left, right)| Shown::Pair(left.map(at), right.map(at)))),
                }
            }
            Stretch::Folded(_) if matches!(stretches.get(ix + 1), Some(Stretch::Hunk { .. })) => {}
            Stretch::Folded(lines) => {
                rows.push(Shown::Fold(Fold::new(ix, lines, open)));
                if open.contains(&ix) {
                    unchanged_rows(&mut rows, ix, lines, layout);
                }
            }
        }
    }
    rows
}

/// The wash behind a line, and behind its changed words.
pub(super) fn washes(kind: LineKind, colors: &Palette) -> (Option<Hsla>, Hsla) {
    match kind {
        LineKind::Same => (None, colors.hover),
        LineKind::Added => (Some(colors.success.opacity(0.08)), colors.success.opacity(0.25)),
        LineKind::Removed => (Some(colors.danger.opacity(0.08)), colors.danger.opacity(0.25)),
    }
}

/// A line's syntax colors; none for prose.
pub(super) fn colors_of(text: &str, syntax: Option<&Syntax>, cx: &App) -> Vec<(Range<usize>, HighlightStyle)> {
    syntax.map_or_else(Vec::new, |syntax| code_colors_with(text, syntax, cx))
}

/// A line colored as its file reads, changed words washed.
pub(super) fn code(line: &DiffLine, colors: &Palette, syntax: Option<&Syntax>, cx: &App) -> StyledText {
    code_highlighted(line, colors, colors_of(&line.text, syntax, cx))
}

pub(super) fn code_highlighted(line: &DiffLine, colors: &Palette, mut styles: Vec<(Range<usize>, HighlightStyle)>) -> StyledText {
    let (_, word) = washes(line.kind, colors);
    styles.extend(
        line.words
            .iter()
            .map(|range| (range.clone(), HighlightStyle { background_color: Some(word), ..HighlightStyle::default() })),
    );
    styles.sort_by_key(|(range, _)| range.start);
    StyledText::new(line.text.clone()).with_highlights(stack(styles))
}

/// A line as assistive tech reads it: number, change, text.
pub(super) fn spoken(line: &DiffLine) -> SharedString {
    let (number, sign) = match line.kind {
        LineKind::Same => (line.new, ""),
        LineKind::Added => (line.new, "+ "),
        LineKind::Removed => (line.old, "− "),
    };
    let number = number.map_or_else(String::new, |n| format!("{n} "));
    format!("{number}{sign}{}", line.text).into()
}

#[cfg(test)]
mod tests {
    #[gpui::test]
    fn each_file_colors_as_its_kind_reads(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::init_for_tests(cx);
            let syntax = crate::theme::ActiveTheme::theme(cx).colors.syntax.clone();
            let color = |text: &str, path: &str, cx: &gpui::App| {
                let syntax = crate::forms::syntax_of(path);
                super::colors_of(text, syntax, cx).into_iter().map(|(range, style)| (range, style.color)).collect::<Vec<_>>()
            };
            for path in ["docs/README.md", "notes.MARKDOWN", "LICENSE", "a/b.txt", "data.csv"] {
                assert!(color("# return 42", path, cx).is_empty(), "{path} reads plain");
            }
            for path in ["script.py", "config.toml", "run.sh", ".gitignore", "Makefile", "CMakeLists.txt", "build.cmake"] {
                assert_eq!(color("# return 42", path, cx), [(0..11, Some(syntax.comment))], "{path}");
            }
            assert_eq!(color("-- return 42", "q.sql", cx), [(0..12, Some(syntax.comment))]);
            let rust = color("return 42 // done", "src/main.rs", cx);
            assert_eq!(rust[0], (0..6, Some(syntax.keyword)), "{rust:?}");
            assert_eq!(rust.last(), Some(&(10..17, Some(syntax.comment))));
            // Each language's own words.
            let python = color("assert ready and True", "a.py", cx);
            assert_eq!([python[0].1, python[2].1, python[3].1], [Some(syntax.keyword), Some(syntax.keyword), Some(syntax.constant)]);
            assert_ne!(color("return = 42", "config.toml", cx)[0].1, Some(syntax.keyword), "a TOML key");
            // Words belong to their own language.
            assert_ne!(color("let from = 42;", "main.rs", cx)[1].1, Some(syntax.keyword), "Rust's from");
            assert_eq!(color("def hello", "a.rb", cx)[0].1, Some(syntax.keyword), "Ruby's def");
            assert_eq!(color("-- return 42", "Main.hs", cx), [(0..12, Some(syntax.comment))]);
            assert_eq!(color("% return 42", "main.erl", cx), [(0..11, Some(syntax.comment))]);
            // A comment may follow code at once outside Shell.
            assert_eq!(color("answer = 42# return 99", "config.toml", cx).last(), Some(&(11..22, Some(syntax.comment))));
            assert_eq!(color("set(x 1)# return 42", "CMakeLists.txt", cx).last(), Some(&(8..19, Some(syntax.comment))));
            // Shell comments start words only.
            let shell = color("echo foo#bar 42", "run.sh", cx);
            assert!(shell.iter().all(|(_, c)| *c != Some(syntax.comment)), "{shell:?}");
            assert_eq!(color("echo x # note", "run.sh", cx).last(), Some(&(7..13, Some(syntax.comment))));
            // An operator ends a Shell word too.
            for line in [
                "echo x;# return 42",
                "echo x&# return 42",
                "(echo x)# return 4",
                "(echo $(a))# return 4",
                "(case a in a) echo y;; esac)# return 4",
                "case a in a)# return 42",
                r"echo x\\;# return 4",
            ] {
                assert_eq!(color(line, "run.sh", cx).last(), Some(&(line.find('#').expect("a mark")..line.len(), Some(syntax.comment))), "{line}");
            }
            // Escaped, quoted or in a word, it does not.
            for line in [
                r"echo x\;# return 42",
                r"echo x\&# return 42",
                r"echo x\ # return 42",
                r"echo x\\\;# return 42",
                r##"echo "a;"# return 42"##,
                "echo 1 $(echo a)# return 42",
                "echo 2 $((1+2))# return 42",
                "cat <(echo 4 p)# return 42",
                "tee >(cat)# return 42",
                "echo 5 $(case a in a) echo y;; esac)# return 42",
                "echo 6 $(case a in (a) echo y;; esac)# return 42",
                "echo 7 $(case a in a) case b in b) echo z;; esac;; esac)# return 42",
                "echo 8 $(case a in @(a|b)) echo y;; esac)# return 42",
                "echo 9 $(if true; then case a in a) echo y;& b) echo w;; esac; fi)# return 42",
                "echo 10 $(echo case)# return 42",
                "echo A $(case case in a) echo n;; case) echo y;; esac)# return 42",
                "echo B $(case case in b|case) echo y;; esac)# return 42",
                "echo C $(case case in (case) echo y;; esac)# return 42",
                "echo D $(case a in a) if true; then echo y; fi esac)# return 42",
                "echo E $(case a in a) { echo y; } esac)# return 42",
                "echo F $(case a in a) for i in 1; do echo $i; done esac)# return 42",
                "echo G $(case a in x-esac) echo n;; a) echo y;; esac)# return 42",
                "echo H $(case a in a) echo done esac;; esac)# return 42",
                r"echo \(x\)# return 42",
            ] {
                let shell = color(line, "run.sh", cx);
                assert!(shell.iter().all(|(_, c)| *c != Some(syntax.comment)), "{line}: {shell:?}");
            }
            let yaml = color("key: a;# b", "c.yaml", cx);
            assert!(yaml.iter().all(|(_, c)| *c != Some(syntax.comment)), "YAML wants a space: {yaml:?}");
        });
    }

    #[test]
    fn a_viewer_reads_its_file_once_built() {
        let viewer = |path: &str| super::super::viewer::DiffViewer::new("d", path.to_string(), "a\n", "b\n").syntax;
        assert_eq!(viewer("notes.md"), None);
        assert_eq!(viewer("a.py"), crate::forms::syntax_of("a.py"));
        assert_eq!(viewer("CMakeLists.txt"), crate::forms::syntax_of("Makefile"));
    }

    #[test]
    fn a_line_speaks_its_number_change_and_text() {
        let stretches = super::super::diff::diff("a\nb\n", "a\nc\n", 1);
        let super::Stretch::Hunk { lines, .. } = &stretches[0] else { panic!("a hunk") };
        let said: Vec<String> = lines.iter().map(|line| super::spoken(line).to_string()).collect();
        assert_eq!(said, ["1 a", "2 − b", "2 + c"]);
    }

    #[test]
    fn folded_headers_keep_real_ranges_and_identical_controls_after_expansion() {
        use super::DiffLayout;
        use super::Shown;
        use super::Stretch;
        let old = (1..=30).map(|number| format!("line {number}\n")).collect::<String>();
        let new = old.replacen("line 10\n", "new 10\n", 1).replacen("line 20\n", "new 20\n", 1);
        let stretch_list = super::super::diff::diff(&old, &new, 1);
        let fold_list = stretch_list
            .iter()
            .enumerate()
            .filter_map(|(index, stretch)| matches!(stretch, Stretch::Folded(_)).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(fold_list.len(), 3);
        for layout in [DiffLayout::Unified, DiffLayout::Split] {
            let closed = super::shown(&stretch_list, layout, &[]);
            let expanded = super::shown(&stretch_list, layout, &fold_list);
            let controls = |row_list: &[Shown]| {
                row_list
                    .iter()
                    .filter_map(|row| match row {
                        Shown::Header(_, _, Some(fold)) | Shown::Fold(fold) => Some((fold.stretch, fold.description.clone())),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(controls(&closed), controls(&expanded));
            assert_eq!(controls(&closed).iter().map(|(index, _)| *index).collect::<Vec<_>>(), fold_list);
            assert!(controls(&closed)[0].1.contains("−1–8 +1–8"));
            assert!(controls(&closed)[2].1.contains("−22–30 +22–30"));
            assert!(expanded.len() > closed.len());
            assert!(matches!(&expanded[0], Shown::Header(_, header, Some(fold)) if header.starts_with("@@ ") && fold.expanded));
        }
    }

    #[test]
    fn entirely_unchanged_stretch_retains_a_non_hunk_close_header() {
        use super::DiffLayout;
        use super::Shown;
        let stretch_list = super::super::diff::diff("one\ntwo\n", "one\ntwo\n", 0);
        for layout in [DiffLayout::Unified, DiffLayout::Split] {
            let closed = super::shown(&stretch_list, layout, &[]);
            let expanded = super::shown(&stretch_list, layout, &[0]);
            assert_eq!(closed.len(), 1);
            assert_eq!(expanded.len(), 3);
            assert!(matches!(&expanded[0], Shown::Fold(fold) if fold.stretch == 0 && fold.expanded && fold.description.as_ref() == "Unchanged −1–2 +1–2 · 2 lines"));
        }
    }
}
