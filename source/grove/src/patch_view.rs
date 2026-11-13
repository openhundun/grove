//! 按 Git 提供的行序和真实行号折叠上下文，不重建未提供的行。
use ely_git::DiffLine;
use ely_git::LineKind;
use ely_git::Stretch;
use ely_git::mark_words;
use std::rc::Rc;

pub fn parse(patch: &str) -> Result<Rc<Vec<Stretch>>, &'static str> {
    let mut source = patch.split_terminator('\n').peekable();
    let mut stretch_list = Vec::new();
    while let Some(header) = source.next() {
        if header.starts_with("@@@") {
            return Err("combined diff hunks are not supported");
        }
        if !header.starts_with("@@ ") {
            continue;
        }
        let (old_start, old_count, new_start, new_count) = hunk_range(header)?;
        let (mut old, mut new) = (old_start, new_start);
        let (mut old_used, mut new_used) = (0, 0);
        let mut line_list: Vec<DiffLine> = Vec::new();
        while let Some(&line) = source.peek() {
            if line.starts_with("@@@") {
                return Err("combined diff hunks are not supported");
            }
            if line == "\\ No newline at end of file" {
                let previous = line_list.last_mut().ok_or("orphan newline marker")?;
                previous.text = format!("{}  ⟵ No newline at end of file", previous.text).into();
                source.next();
                continue;
            }
            if old_used == old_count && new_used == new_count {
                break;
            }
            let text = line.get(1..).ok_or("invalid hunk body")?;
            let (kind, old_line, new_line) = match line.as_bytes().first() {
                Some(b' ') if old_used < old_count && new_used < new_count => {
                    old_used += 1;
                    new_used += 1;
                    old += 1;
                    new += 1;
                    (LineKind::Same, Some(old - 1), Some(new - 1))
                }
                Some(b'-') if old_used < old_count => {
                    old_used += 1;
                    old += 1;
                    (LineKind::Removed, Some(old - 1), None)
                }
                Some(b'+') if new_used < new_count => {
                    new_used += 1;
                    new += 1;
                    (LineKind::Added, None, Some(new - 1))
                }
                _ => return Err("hunk body does not match declared line counts"),
            };
            line_list.push(DiffLine { kind, old: old_line, new: new_line, text: text.to_owned().into(), words: Vec::new() });
            source.next();
        }
        if old_used != old_count || new_used != new_count {
            return Err("hunk ended before its declared line counts");
        }
        let at = (old_start - usize::from(old_count > 0), new_start - usize::from(new_count > 0));
        partition_hunk(line_list, at, &mut stretch_list);
    }
    Ok(Rc::new(stretch_list))
}

/// 无文本变化时将实际文件内容作为一个折叠段，保留 CR 和真实行号。
pub fn unchanged(source: &str) -> Rc<Vec<Stretch>> {
    let line_list: Vec<DiffLine> = source
        .split_terminator('\n')
        .enumerate()
        .map(|(index, text)| DiffLine {
            kind: LineKind::Same,
            old: Some(index + 1),
            new: Some(index + 1),
            text: text.to_owned().into(),
            words: Vec::new(),
        })
        .collect();
    Rc::new(if line_list.is_empty() { Vec::new() } else { vec![Stretch::Folded(line_list)] })
}

fn partition_hunk(line_list: Vec<DiffLine>, mut at: (usize, usize), stretch_list: &mut Vec<Stretch>) {
    const CONTEXT: usize = 3;
    let mut fold_list = Vec::new();
    let mut index = 0;
    while index < line_list.len() {
        if line_list[index].kind != LineKind::Same {
            index += 1;
            continue;
        }
        let start = index;
        while index < line_list.len() && line_list[index].kind == LineKind::Same {
            index += 1;
        }
        let before = if start > 0 { CONTEXT.min(index - start) } else { 0 };
        let after = if index < line_list.len() { CONTEXT.min(index - start) } else { 0 };
        let hidden = start + before..index - after;
        if hidden.start < hidden.end {
            fold_list.push(hidden);
        }
    }
    if fold_list.is_empty() {
        push_hunk(line_list, &mut at, stretch_list);
        return;
    }
    if fold_list.len() == 1 && fold_list[0].len() == line_list.len() {
        stretch_list.push(Stretch::Folded(line_list));
        return;
    }
    let mut source = line_list.into_iter();
    let mut consumed = 0;
    for hidden in fold_list {
        push_hunk(source.by_ref().take(hidden.start - consumed).collect(), &mut at, stretch_list);
        let folded: Vec<DiffLine> = source.by_ref().take(hidden.len()).collect();
        at.0 += folded.len();
        at.1 += folded.len();
        stretch_list.push(Stretch::Folded(folded));
        consumed = hidden.end;
    }
    push_hunk(source.collect(), &mut at, stretch_list);
}

fn push_hunk(mut line_list: Vec<DiffLine>, at: &mut (usize, usize), stretch_list: &mut Vec<Stretch>) {
    if line_list.is_empty() {
        return;
    }
    let (old_count, new_count) = line_list
        .iter()
        .fold((0, 0), |(old, new), line| (old + usize::from(line.old.is_some()), new + usize::from(line.new.is_some())));
    let (old_start, new_start) = (at.0 + usize::from(old_count > 0), at.1 + usize::from(new_count > 0));
    let header = format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@");
    for changed in line_list.split_mut(|line| line.kind == LineKind::Same) {
        if changed.iter().any(|line| line.kind == LineKind::Removed) && changed.iter().any(|line| line.kind == LineKind::Added) {
            mark_words(changed);
        }
    }
    at.0 += old_count;
    at.1 += new_count;
    stretch_list.push(Stretch::Hunk { header: header.into(), lines: line_list });
}

fn hunk_range(header: &str) -> Result<(usize, usize, usize, usize), &'static str> {
    let body = header
        .strip_prefix("@@ ")
        .and_then(|body| body.split_once(" @@").map(|(range, _)| range))
        .ok_or("invalid unified hunk header")?;
    let mut field_list = body.split_whitespace();
    let old = field_list.next().and_then(|field| field.strip_prefix('-')).ok_or("missing original hunk range")?;
    let new = field_list.next().and_then(|field| field.strip_prefix('+')).ok_or("missing modified hunk range")?;
    if field_list.next().is_some() {
        return Err("invalid unified hunk range");
    }
    fn range(field: &str) -> Result<(usize, usize), &'static str> {
        let (start, count) = field.split_once(',').unwrap_or((field, "1"));
        let start = start.parse::<usize>().map_err(|_| "invalid hunk start")?;
        let count = count.parse::<usize>().map_err(|_| "invalid hunk count")?;
        if count > 0 && start == 0 || start.checked_add(count.max(1)).is_none() {
            return Err("invalid hunk extent");
        }
        Ok((start, count))
    }
    let (old_start, old_count) = range(old)?;
    let (new_start, new_count) = range(new)?;
    Ok((old_start, old_count, new_start, new_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn flattened(stretch_list: &[Stretch]) -> Vec<(LineKind, Option<usize>, Option<usize>, String)> {
        stretch_list
            .iter()
            .flat_map(|stretch| match stretch {
                Stretch::Hunk { lines, .. } | Stretch::Folded(lines) => lines,
            })
            .map(|line| (line.kind, line.old, line.new, line.text.to_string()))
            .collect()
    }
    #[test]
    fn folds_leading_middle_and_trailing_context_without_changing_git_rows() {
        let mut patch = String::from("@@ -1,26 +1,27 @@ supplied function\n");
        let mut expected = Vec::new();
        for old in 1..=26 {
            match old {
                8 => {
                    patch.push_str("-let 值 = 1;\r\n+let 值 = 2;\r\n+新增\r\n");
                    expected.extend([
                        (LineKind::Removed, Some(8), None, "let 值 = 1;\r".to_owned()),
                        (LineKind::Added, None, Some(8), "let 值 = 2;\r".to_owned()),
                        (LineKind::Added, None, Some(9), "新增\r".to_owned()),
                    ]);
                }
                19 => {
                    patch.push_str("-old tail\r\n+new tail\r\n");
                    expected.extend([(LineKind::Removed, Some(19), None, "old tail\r".to_owned()), (LineKind::Added, None, Some(20), "new tail\r".to_owned())]);
                }
                _ => {
                    let text = format!("line {old}\r");
                    patch.push_str(&format!(" {text}\n"));
                    expected.push((LineKind::Same, Some(old), Some(old + usize::from(old > 8)), text));
                }
            }
        }
        let stretch_list = parse(&patch).unwrap();
        assert_eq!(flattened(&stretch_list), expected);
        let [
            Stretch::Folded(leading),
            Stretch::Hunk { header: first_header, lines: first },
            Stretch::Folded(middle),
            Stretch::Hunk { header: last_header, lines: last },
            Stretch::Folded(trailing),
        ] = stretch_list.as_slice()
        else {
            panic!("expected two hunks with three folds: {stretch_list:?}");
        };
        assert_eq!((leading.len(), middle.len(), trailing.len()), (4, 4, 4));
        assert_eq!(first_header.as_ref(), "@@ -5,7 +5,8 @@");
        assert_eq!(last_header.as_ref(), "@@ -16,7 +17,7 @@");
        assert_eq!((first.len(), last.len()), (9, 8));
        assert_eq!(
            first.iter().filter(|line| line.kind == LineKind::Same).map(|line| (line.old, line.new)).collect::<Vec<_>>(),
            [
                (Some(5), Some(5)),
                (Some(6), Some(6)),
                (Some(7), Some(7)),
                (Some(9), Some(10)),
                (Some(10), Some(11)),
                (Some(11), Some(12)),
            ]
        );
        assert_eq!(
            last.iter().filter(|line| line.kind == LineKind::Same).map(|line| (line.old, line.new)).collect::<Vec<_>>(),
            [
                (Some(16), Some(17)),
                (Some(17), Some(18)),
                (Some(18), Some(19)),
                (Some(20), Some(21)),
                (Some(21), Some(22)),
                (Some(22), Some(23)),
            ]
        );
        assert_eq!(first[3].words.iter().map(|range| &first[3].text[range.clone()]).collect::<Vec<_>>(), ["1"]);
        assert_eq!(first[4].words.iter().map(|range| &first[4].text[range.clone()]).collect::<Vec<_>>(), ["2"]);
        assert!(leading.iter().chain(middle).chain(trailing).all(|line| line.kind == LineKind::Same && line.words.is_empty()));
    }
    #[test]
    fn merges_change_blocks_separated_by_at_most_six_context_lines() {
        for gap in [0, 1, 3, 6, 7] {
            let count = gap + 2;
            let mut patch = format!("@@ -10,{count} +30,{count} @@\n-old first\n+new first\n");
            let mut expected = vec![(LineKind::Removed, Some(10), None, "old first".to_owned()), (LineKind::Added, None, Some(30), "new first".to_owned())];
            for index in 0..gap {
                let text = format!("keep {index}");
                patch.push_str(&format!(" {text}\n"));
                expected.push((LineKind::Same, Some(11 + index), Some(31 + index), text));
            }
            patch.push_str("-old last\n+new last\n");
            expected.extend([
                (LineKind::Removed, Some(11 + gap), None, "old last".to_owned()),
                (LineKind::Added, None, Some(31 + gap), "new last".to_owned()),
            ]);
            let stretch_list = parse(&patch).unwrap();
            assert_eq!(flattened(&stretch_list), expected);
            if gap <= 6 {
                let [Stretch::Hunk { header, lines }] = stretch_list.as_slice() else {
                    panic!("short gaps must share one hunk")
                };
                assert_eq!(header.as_ref(), format!("@@ -10,{count} +30,{count} @@"));
                assert_eq!(lines.len(), gap + 4);
            } else {
                let [Stretch::Hunk { header: first, lines: before }, Stretch::Folded(hidden), Stretch::Hunk { header: last, lines: after }] = stretch_list.as_slice() else {
                    panic!("seven context lines must hide exactly the center line");
                };
                assert_eq!((before.len(), hidden.len(), after.len()), (5, 1, 5));
                assert_eq!((hidden[0].old, hidden[0].new), (Some(14), Some(34)));
                assert_eq!(first.as_ref(), "@@ -10,4 +30,4 @@");
                assert_eq!(last.as_ref(), "@@ -15,4 +35,4 @@");
            }
        }
    }
    #[test]
    fn folds_edge_context_only_beyond_three_lines() {
        for context in [0, 1, 3, 4] {
            let count = context * 2 + 1;
            let mut patch = format!("@@ -1,{count} +1,{count} @@\n");
            let mut expected = Vec::new();
            for number in 1..=count {
                if number == context + 1 {
                    patch.push_str("-old\n+new\n");
                    expected.extend([(LineKind::Removed, Some(number), None, "old".to_owned()), (LineKind::Added, None, Some(number), "new".to_owned())]);
                } else {
                    let text = format!("line {number}");
                    patch.push_str(&format!(" {text}\n"));
                    expected.push((LineKind::Same, Some(number), Some(number), text));
                }
            }
            let stretch_list = parse(&patch).unwrap();
            assert_eq!(flattened(&stretch_list), expected);
            if context <= 3 {
                let [Stretch::Hunk { header, lines }] = stretch_list.as_slice() else {
                    panic!("short edge context must remain visible")
                };
                assert_eq!(header.as_ref(), format!("@@ -1,{count} +1,{count} @@"));
                assert_eq!(lines.len(), count + 1);
            } else {
                let [Stretch::Folded(leading), Stretch::Hunk { header, lines }, Stretch::Folded(trailing)] = stretch_list.as_slice() else {
                    panic!("long edges must fold")
                };
                assert_eq!((leading.len(), lines.len(), trailing.len()), (1, 8, 1));
                assert_eq!(header.as_ref(), "@@ -2,7 +2,7 @@");
            }
        }
    }
    #[test]
    fn preserves_zero_length_ranges_for_only_additions_or_deletions() {
        for before in [0, 41] {
            let start = before + 1;
            let addition = parse(&format!("@@ -{before},0 +{start},2 @@\n+é\r\n+\n")).unwrap();
            let deletion = parse(&format!("@@ -{start},2 +{before},0 @@\n-é\r\n-\n")).unwrap();
            assert_eq!(
                flattened(&addition),
                [(LineKind::Added, None, Some(start), "é\r".to_owned()), (LineKind::Added, None, Some(start + 1), String::new())]
            );
            assert_eq!(
                flattened(&deletion),
                [(LineKind::Removed, Some(start), None, "é\r".to_owned()), (LineKind::Removed, Some(start + 1), None, String::new())]
            );
            let [Stretch::Hunk { header: added_header, lines: added }] = addition.as_slice() else {
                panic!("expected addition hunk")
            };
            let [Stretch::Hunk { header: removed_header, lines: removed }] = deletion.as_slice() else {
                panic!("expected deletion hunk")
            };
            assert_eq!(added_header.as_ref(), format!("@@ -{before},0 +{start},2 @@"));
            assert_eq!(removed_header.as_ref(), format!("@@ -{start},2 +{before},0 @@"));
            assert!(added.iter().chain(removed).all(|line| line.words.is_empty()));
        }
    }
    #[test]
    fn unchanged_fallback_folds_actual_content_with_one_based_numbers() {
        let stretch_list = unchanged("one\r\n\n末尾");
        let [Stretch::Folded(lines)] = stretch_list.as_slice() else {
            panic!("unchanged content must be one fold")
        };
        assert_eq!(
            flattened(&stretch_list),
            [
                (LineKind::Same, Some(1), Some(1), "one\r".to_owned()),
                (LineKind::Same, Some(2), Some(2), String::new()),
                (LineKind::Same, Some(3), Some(3), "末尾".to_owned()),
            ]
        );
        assert!(lines.iter().all(|line| line.words.is_empty()));
        assert!(unchanged("").is_empty());
        assert_eq!(flattened(&unchanged("\n")), [(LineKind::Same, Some(1), Some(1), String::new())]);
        assert_eq!(
            flattened(&unchanged("one\n\n")),
            [(LineKind::Same, Some(1), Some(1), "one".to_owned()), (LineKind::Same, Some(2), Some(2), String::new())]
        );
    }
    #[test]
    fn supplied_unchanged_hunks_fold_without_inventing_missing_context() {
        let stretch_list = parse("@@ -8,3 +12,3 @@\n one\r\n \n 末尾\n").unwrap();
        let [Stretch::Folded(lines)] = stretch_list.as_slice() else {
            panic!("expected one unchanged fold")
        };
        assert_eq!(lines.len(), 3);
        assert_eq!(
            flattened(&stretch_list),
            [
                (LineKind::Same, Some(8), Some(12), "one\r".to_owned()),
                (LineKind::Same, Some(9), Some(13), String::new()),
                (LineKind::Same, Some(10), Some(14), "末尾".to_owned()),
            ]
        );
    }
    #[test]
    fn preserves_real_line_numbers_across_separate_hunks() {
        let stretches = parse("@@ -8,2 +8,2 @@\n keep\n-old\n+new\n@@ -42 +42,2 @@\n last\n+extra\n\\ No newline at end of file\n").unwrap();
        let Stretch::Hunk { lines, header } = &stretches[1] else { panic!("expected supplied hunk") };
        assert_eq!((lines[0].old, lines[0].new), (Some(42), Some(42)));
        assert_eq!((lines[1].old, lines[1].new), (None, Some(43)));
        assert!(lines[1].text.contains("No newline at end of file"));
        assert_eq!(header.as_ref(), "@@ -42,1 +42,2 @@");
        assert_eq!(
            flattened(&stretches),
            [
                (LineKind::Same, Some(8), Some(8), "keep".to_owned()),
                (LineKind::Removed, Some(9), None, "old".to_owned()),
                (LineKind::Added, None, Some(9), "new".to_owned()),
                (LineKind::Same, Some(42), Some(42), "last".to_owned()),
                (LineKind::Added, None, Some(43), "extra  ⟵ No newline at end of file".to_owned()),
            ]
        );
        assert!(parse("@@ -1,2 +1 @@\n-a\n+b\n").is_err());
    }
    #[test]
    fn newline_markers_stay_on_their_changed_or_folded_lines() {
        let changed = parse("@@ -1 +1 @@\n-before\n\\ No newline at end of file\n+after\n\\ No newline at end of file").unwrap();
        assert_eq!(
            flattened(&changed),
            [
                (LineKind::Removed, Some(1), None, "before  ⟵ No newline at end of file".to_owned()),
                (LineKind::Added, None, Some(1), "after  ⟵ No newline at end of file".to_owned()),
            ]
        );
        let trailing = parse("@@ -1,5 +1,5 @@\n-old\n+new\n a\n b\n c\n tail\n\\ No newline at end of file").unwrap();
        let [Stretch::Hunk { header, .. }, Stretch::Folded(hidden)] = trailing.as_slice() else {
            panic!("expected a folded final line")
        };
        assert_eq!(header.as_ref(), "@@ -1,4 +1,4 @@");
        assert_eq!(hidden.len(), 1);
        assert_eq!(
            flattened(&trailing),
            [
                (LineKind::Removed, Some(1), None, "old".to_owned()),
                (LineKind::Added, None, Some(1), "new".to_owned()),
                (LineKind::Same, Some(2), Some(2), "a".to_owned()),
                (LineKind::Same, Some(3), Some(3), "b".to_owned()),
                (LineKind::Same, Some(4), Some(4), "c".to_owned()),
                (LineKind::Same, Some(5), Some(5), "tail  ⟵ No newline at end of file".to_owned()),
            ]
        );
    }
    #[test]
    fn no_hunk_patches_are_empty_but_combined_hunks_are_rejected() {
        for patch in [
            "",
            "diff --git a/old b/new\nsimilarity index 100%\nrename from old\nrename to new\n",
            "diff --git a/file b/file\nBinary files a/file and b/file differ\n",
            "GIT binary patch\nliteral 1\nA\n",
        ] {
            assert!(parse(patch).unwrap().is_empty());
        }
        for patch in [
            "diff --cc file\n@@@ -1 -1 +1 @@@\n  same\n",
            "@@ -1 +1 @@\n-old\n+new\n@@@ -1 -1 +1 @@@\n",
            "@@ -1,2 +1,2 @@\n same\n@@@ -1 -1 +1 @@@\n",
        ] {
            assert_eq!(parse(patch).unwrap_err(), "combined diff hunks are not supported");
        }
    }
    #[test]
    fn folds_between_deletions_and_additions_with_independent_line_offsets() {
        let mut patch = String::from("@@ -1,20 +1,20 @@\n");
        let mut expected = Vec::new();
        for old in 1..=20 {
            if old == 16 {
                patch.push_str("+inserted\n");
                expected.push((LineKind::Added, None, Some(15), "inserted".to_owned()));
            }
            let text = format!("line {old}");
            if old == 8 {
                patch.push_str(&format!("-{text}\n"));
                expected.push((LineKind::Removed, Some(old), None, text));
            } else {
                patch.push_str(&format!(" {text}\n"));
                expected.push((LineKind::Same, Some(old), Some(old - usize::from((9..16).contains(&old))), text));
            }
        }
        let stretch_list = parse(&patch).unwrap();
        assert_eq!(flattened(&stretch_list), expected);
        let [
            Stretch::Folded(leading),
            Stretch::Hunk { header: first, lines: removed },
            Stretch::Folded(middle),
            Stretch::Hunk { header: last, lines: added },
            Stretch::Folded(trailing),
        ] = stretch_list.as_slice()
        else {
            panic!("expected separate removal and addition hunks with three folds");
        };
        assert_eq!((leading.len(), middle.len(), trailing.len()), (4, 1, 2));
        assert_eq!(first.as_ref(), "@@ -5,7 +5,6 @@");
        assert_eq!(last.as_ref(), "@@ -13,6 +12,7 @@");
        assert_eq!((removed.len(), added.len()), (7, 7));
        assert_eq!((middle[0].old, middle[0].new), (Some(12), Some(11)));
        assert!(removed.iter().all(|line| line.kind != LineKind::Added && line.words.is_empty()));
        assert!(added.iter().all(|line| line.kind != LineKind::Removed && line.words.is_empty()));
    }
}
