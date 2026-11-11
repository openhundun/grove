//! Converts Git's supplied hunks into ELY rows without reconstructing unavailable context.
use ely_git::DiffLine;
use ely_git::LineKind;
use ely_git::Stretch;
use ely_git::mark_words;
use std::rc::Rc;

pub fn parse(patch: &str) -> Result<Rc<Vec<Stretch>>, &'static str> {
    let mut source = patch.split_terminator('\n').peekable();
    let mut stretch_list = Vec::new();
    while let Some(header) = source.next() {
        if !header.starts_with("@@ ") {
            continue;
        }
        let (mut old, old_count, mut new, new_count) = hunk_range(header)?;
        let (mut old_used, mut new_used) = (0, 0);
        let mut line_list: Vec<DiffLine> = Vec::new();
        while let Some(&line) = source.peek() {
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
        mark_words(&mut line_list);
        stretch_list.push(Stretch::Hunk { header: header.to_owned().into(), lines: line_list });
    }
    Ok(Rc::new(stretch_list))
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

    #[test]
    fn preserves_real_line_numbers_across_separate_hunks() {
        let stretches = parse("@@ -8,2 +8,2 @@\n keep\n-old\n+new\n@@ -42 +42,2 @@\n last\n+extra\n\\ No newline at end of file\n").unwrap();
        let Stretch::Hunk { lines, .. } = &stretches[1] else { panic!("expected supplied hunk") };
        assert_eq!((lines[0].old, lines[0].new), (Some(42), Some(42)));
        assert_eq!((lines[1].old, lines[1].new), (None, Some(43)));
        assert!(lines[1].text.contains("No newline at end of file"));
        assert!(parse("@@ -1,2 +1 @@\n-a\n+b\n").is_err());
    }
}
