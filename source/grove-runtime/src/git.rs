//! 只读 Git CLI 适配器；参数结构化，不暴露 shell 或修改命令。
//!
//! 文本侧预览上限为 2 MiB。完整上下文 diff 的显示上限为 12 MiB，覆盖两侧文本、逐行前缀及元数据。
//! Git 输出不截断；超限、二进制和不支持的冲突比较通过可见提示说明。
use crate::fs;
use grove_core::data::*;
use grove_platform::git_path_bytes;
use grove_platform::path_from_git_bytes;
use grove_platform::read_only_git_command;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::Output;
use std::sync::Arc;

const MAX_DIFF_BYTES: u64 = 6 * fs::MAX_TEXT_BYTES;

fn output(path: &Path, argument_list: &[&OsStr]) -> Result<Output, String> {
    read_only_git_command(path).args(argument_list).output().map_err(|error| format!("git: {error}"))
}
fn run(path: &Path, argument_list: &[&str]) -> Result<Vec<u8>, String> {
    let argument_list: Vec<&OsStr> = argument_list.iter().map(OsStr::new).collect();
    checked(output(path, &argument_list)?)
}
fn checked(output: Output) -> Result<Vec<u8>, String> {
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}
fn text(byte_list: Vec<u8>) -> Result<String, String> {
    String::from_utf8(byte_list).map_err(|_| "Git metadata is not UTF-8".into())
}
fn line(path: &Path, argument_list: &[&str]) -> Result<String, String> {
    Ok(text(run(path, argument_list)?)?.trim_end_matches('\n').to_owned())
}

pub fn discover(root: &Path) -> Result<WorkspaceSnapshot, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let (candidate_list, mut warning_list) = fs::repository_candidates(&root)?;
    let mut repository_map = BTreeMap::<PathBuf, RepositorySnapshot>::new();
    for candidate in candidate_list {
        let common = match line(&candidate, &["rev-parse", "--path-format=absolute", "--git-common-dir"]) {
            Ok(common) => PathBuf::from(common),
            Err(error) => {
                warning_list.push(format!("{}: {error}", candidate.display()));
                continue;
            }
        };
        let common = match fs::within(&root, &common) {
            Ok(common) => common,
            Err(error) => {
                warning_list.push(format!("{}: {error}", candidate.display()));
                continue;
            }
        };
        if repository_map.contains_key(&common) {
            continue;
        }
        let listing = match run(&candidate, &["worktree", "list", "--porcelain", "-z"]) {
            Ok(listing) => listing,
            Err(error) => {
                warning_list.push(error);
                continue;
            }
        };
        let mut worktree_list = parse_worktrees(&listing);
        worktree_list.retain(|worktree| worktree.path.canonicalize().is_ok_and(|path| path.starts_with(&root)));
        for worktree in &mut worktree_list {
            worktree.path = worktree.path.canonicalize().map_err(|error| error.to_string())?;
        }
        if worktree_list.is_empty() {
            continue;
        }
        let path = worktree_list[0].path.clone();
        let workspace_root = worktree_list.iter().any(|worktree| worktree.path == root);
        let name = if workspace_root {
            root.file_name().unwrap_or_default().to_string_lossy().into_owned()
        } else {
            let directory = common.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy();
            directory.strip_suffix("--main").unwrap_or(&directory).to_owned()
        };
        repository_map.insert(common.clone(), RepositorySnapshot { common_dir: common, path, name, workspace_root, worktree_list });
    }
    let mut repository_list: Vec<_> = repository_map.into_values().collect();
    repository_list.sort_by(|left, right| right.workspace_root.cmp(&left.workspace_root).then(left.name.cmp(&right.name)));
    Ok(WorkspaceSnapshot { root, repository_list, warning_list })
}

fn parse_worktrees(byte_list: &[u8]) -> Vec<WorktreeSnapshot> {
    let mut worktree_list = Vec::new();
    let mut current: Option<WorktreeSnapshot> = None;
    for field in byte_list.split(|byte| *byte == 0) {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            if let Some(worktree) = current.take() {
                worktree_list.push(worktree);
            }
            current = Some(WorktreeSnapshot { path: path_from_git_bytes(path), branch: None, head: None, locked: false });
        } else if let Some(worktree) = &mut current {
            if let Some(head) = field.strip_prefix(b"HEAD ")
                && !head.iter().all(|byte| *byte == b'0')
            {
                worktree.head = Some(String::from_utf8_lossy(head).into_owned());
            }
            if let Some(branch) = field.strip_prefix(b"branch refs/heads/") {
                worktree.branch = Some(String::from_utf8_lossy(branch).into_owned());
            }
            if field.starts_with(b"locked") {
                worktree.locked = true;
            }
        }
    }
    if let Some(worktree) = current {
        worktree_list.push(worktree);
    }
    worktree_list
}

pub fn worktree(root: &Path, path: &Path) -> Result<WorktreeData, String> {
    let path = fs::within(root, path)?;
    let raw = run(&path, &["status", "--porcelain=v2", "--branch", "--untracked-files=all", "-z"])?;
    let mut data = parse_status(&raw, path.clone())?;
    if data.head.is_some() {
        data.commit_list = commits(&path)?;
        data.base_ref_list = line(&path, &["for-each-ref", "--format=%(refname:short)", "refs/heads", "refs/remotes"])?
            .lines()
            .filter(|reference| !reference.ends_with("/HEAD"))
            .map(str::to_owned)
            .collect();
    }
    let files = run(&path, &["ls-files", "--cached", "--others", "--exclude-standard", "-z"])?;
    let mut path_set = HashSet::new();
    data.file_list = files
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(path_from_git_bytes)
        .filter(|file| fs::relative(file).is_ok() && path_set.insert(file.clone()))
        .collect();
    data.file_list.sort();
    apply_numstat(&mut data.unstaged_list, &run(&path, &["diff", "--no-ext-diff", "--no-textconv", "--find-renames", "--numstat", "-z"])?);
    apply_numstat(
        &mut data.staged_list,
        &run(&path, &["diff", "--cached", "--no-ext-diff", "--no-textconv", "--find-renames", "--numstat", "-z"])?,
    );
    Ok(data)
}

fn parse_status(byte_list: &[u8], path: PathBuf) -> Result<WorktreeData, String> {
    let mut data = WorktreeData {
        path,
        branch: None,
        head: None,
        ahead: None,
        behind: None,
        unstaged_list: Vec::new(),
        staged_list: Vec::new(),
        commit_list: Vec::new(),
        base_ref_list: Vec::new(),
        file_list: Vec::new(),
        warning_list: Vec::new(),
    };
    let mut field_iter = byte_list.split(|byte| *byte == 0);
    while let Some(field) = field_iter.next() {
        if let Some(value) = field.strip_prefix(b"# branch.oid ") {
            if value != b"(initial)" {
                data.head = Some(String::from_utf8_lossy(value).into_owned());
            }
            continue;
        }
        if let Some(value) = field.strip_prefix(b"# branch.head ") {
            if value != b"(detached)" {
                data.branch = Some(String::from_utf8_lossy(value).into_owned());
            }
            continue;
        }
        if let Some(value) = field.strip_prefix(b"# branch.ab ") {
            let value = String::from_utf8_lossy(value);
            let mut part_iter = value.split_whitespace();
            data.ahead = part_iter.next().and_then(|value| value.trim_start_matches('+').parse().ok());
            data.behind = part_iter.next().and_then(|value| value.trim_start_matches('-').parse().ok());
            continue;
        }
        if let Some(value) = field.strip_prefix(b"? ") {
            data.unstaged_list
                .push(FileChange { path: path_from_git_bytes(value), old_path: None, status: '?', additions: None, deletions: None });
            continue;
        }
        let field_count = match field.first() {
            Some(b'1') => 9,
            Some(b'2') => 10,
            Some(b'u') => 11,
            _ => continue,
        };
        let part_list: Vec<_> = field.splitn(field_count, |byte| *byte == b' ').collect();
        if part_list.len() != field_count || part_list[1].len() != 2 {
            return Err("Invalid Git porcelain v2 record".into());
        }
        let path = path_from_git_bytes(part_list[field_count - 1]);
        let old_path = if field[0] == b'2' {
            Some(path_from_git_bytes(field_iter.next().ok_or("Missing rename source")?))
        } else {
            None
        };
        for (column, list) in [(0, &mut data.staged_list), (1, &mut data.unstaged_list)] {
            let status = part_list[1][column] as char;
            if status != '.' {
                list.push(FileChange {
                    path: path.clone(),
                    old_path: old_path.clone(),
                    status: if field[0] == b'u' { 'U' } else { status },
                    additions: None,
                    deletions: None,
                });
            }
        }
    }
    Ok(data)
}

fn commits(path: &Path) -> Result<Vec<CommitSummary>, String> {
    let raw = run(
        path,
        &[
            "log",
            "-150",
            "--topo-order",
            "--no-show-signature",
            "--format=%H%x00%P%x00%an%x00%ad%x00%s%x00",
            "--date=short",
            "HEAD",
            "--",
        ],
    )?;
    let field_list: Vec<_> = raw.split(|byte| *byte == 0).collect();
    let mut commit_list = Vec::new();
    for part_list in field_list.chunks(5) {
        if part_list.len() < 5 {
            break;
        }
        commit_list.push(CommitSummary {
            oid: String::from_utf8_lossy(part_list[0]).trim().into(),
            parent_list: String::from_utf8_lossy(part_list[1]).split_whitespace().map(str::to_owned).collect(),
            author: String::from_utf8_lossy(part_list[2]).into_owned(),
            date: String::from_utf8_lossy(part_list[3]).into_owned(),
            subject: String::from_utf8_lossy(part_list[4]).into_owned(),
        });
    }
    Ok(commit_list)
}

fn diff_arguments(path: &Path, revision: &Revision) -> Result<Vec<String>, String> {
    let mut argument_list = vec![
        "diff".into(),
        "--no-ext-diff".into(),
        "--no-textconv".into(),
        "--find-renames".into(),
        "--no-color".into(),
        "--src-prefix=a/".into(),
        "--dst-prefix=b/".into(),
    ];
    match revision {
        Revision::Unstaged => {}
        Revision::Staged => argument_list.push("--cached".into()),
        Revision::Comparison(base) => {
            let base = resolve(path, base)?;
            let merge = line(path, &["merge-base", &base, "HEAD"])?;
            argument_list.extend([merge, "HEAD".into()]);
        }
        Revision::Commit(oid) => {
            let oid = resolve(path, oid)?;
            let parent = line(path, &["rev-list", "--parents", "-1", &oid])?;
            let mut part_iter = parent.split_whitespace();
            part_iter.next();
            if let Some(parent) = part_iter.next() {
                argument_list.extend([parent.to_owned(), oid]);
            } else {
                argument_list = vec![
                    "diff-tree".into(),
                    "--root".into(),
                    "--no-commit-id".into(),
                    "-r".into(),
                    "--no-ext-diff".into(),
                    "--no-textconv".into(),
                    "--find-renames".into(),
                    "--src-prefix=a/".into(),
                    "--dst-prefix=b/".into(),
                    oid,
                ];
            }
        }
    }
    Ok(argument_list)
}
fn resolve(path: &Path, reference: &str) -> Result<String, String> {
    line(path, &["rev-parse", "--verify", "--end-of-options", &format!("{reference}^{{commit}}")])
}
fn dynamic(path: &Path, argument_list: &[String], tail_list: &[&OsStr]) -> Result<Vec<u8>, String> {
    let mut list: Vec<&OsStr> = argument_list.iter().map(OsStr::new).collect();
    list.extend_from_slice(tail_list);
    checked(output(path, &list)?)
}

pub fn history(root: &Path, path: &Path, revision: Revision) -> Result<HistoryData, String> {
    let path = fs::within(root, path)?;
    match &revision {
        Revision::Unstaged | Revision::Staged => {
            let raw = run(&path, &["status", "--porcelain=v2", "--branch", "--untracked-files=all", "-z"])?;
            let data = parse_status(&raw, path.clone())?;
            let mut file_list = if revision == Revision::Staged { data.staged_list } else { data.unstaged_list };
            let mut argument_list = diff_arguments(&path, &revision)?;
            argument_list.extend(["--numstat".into(), "-z".into(), "--".into()]);
            apply_numstat(&mut file_list, &dynamic(&path, &argument_list, &[])?);
            Ok(HistoryData { revision, message: String::new(), file_list })
        }
        _ => {
            let mut argument_list = diff_arguments(&path, &revision)?;
            argument_list.extend(["--name-status".into(), "-z".into(), "--".into()]);
            let raw = dynamic(&path, &argument_list, &[])?;
            let mut file_list = Vec::new();
            let mut field_iter = raw.split(|byte| *byte == 0).filter(|field| !field.is_empty());
            while let Some(status) = field_iter.next() {
                let file = field_iter.next().ok_or("Missing changed file path")?;
                let old_path = if matches!(status[0], b'R' | b'C') { Some(path_from_git_bytes(file)) } else { None };
                let file = if old_path.is_some() { field_iter.next().ok_or("Missing rename destination")? } else { file };
                file_list.push(FileChange {
                    path: path_from_git_bytes(file),
                    old_path,
                    status: status[0] as char,
                    additions: None,
                    deletions: None,
                });
            }
            let mut argument_list = diff_arguments(&path, &revision)?;
            argument_list.extend(["--numstat".into(), "-z".into(), "--".into()]);
            apply_numstat(&mut file_list, &dynamic(&path, &argument_list, &[])?);
            let message = if let Revision::Commit(oid) = &revision {
                let oid = resolve(&path, oid)?;
                line(&path, &["show", "-s", "--no-show-signature", "--format=%B", &oid, "--"])?
            } else {
                "Local merge-base comparison; no remote request or fetch is performed.".into()
            };
            Ok(HistoryData { revision, message, file_list })
        }
    }
}
fn apply_numstat(file_list: &mut [FileChange], raw: &[u8]) {
    let mut field_iter = raw.split(|byte| *byte == 0);
    while let Some(field) = field_iter.next() {
        let part_list: Vec<_> = field.splitn(3, |byte| *byte == b'\t').collect();
        if part_list.len() != 3 {
            continue;
        }
        let file = if part_list[2].is_empty() {
            field_iter.next();
            field_iter.next().unwrap_or_default()
        } else {
            part_list[2]
        };
        let path = path_from_git_bytes(file);
        if let Some(file) = file_list.iter_mut().find(|file| file.path == path) {
            file.additions = std::str::from_utf8(part_list[0]).ok().and_then(|value| value.parse().ok());
            file.deletions = std::str::from_utf8(part_list[1]).ok().and_then(|value| value.parse().ok());
        }
    }
}

pub fn file(root: &Path, path: &Path, revision: Option<Revision>, file: &Path) -> Result<FileData, String> {
    let path = fs::within(root, path)?;
    fs::relative(file)?;
    let mut content = match &revision {
        Some(Revision::Commit(oid)) => {
            let oid = resolve(&path, oid)?;
            blob(&path, OsString::from(format!("{oid}:")), file)
        }
        Some(Revision::Staged) => blob(&path, OsString::from(":"), file),
        Some(Revision::Comparison(_)) => blob(&path, OsString::from("HEAD:"), file),
        _ => fs::read_file(root, &path, file),
    };
    let mut patch = None;
    let mut notice = None;
    if let Some(revision) = &revision {
        if matches!(content, FileContent::Binary) {
            notice = Some("Binary or non-UTF-8 content cannot be displayed as a text diff.".into());
        } else if matches!(&content, FileContent::Unavailable(message) if message.contains("2 MiB")) {
            notice = Some("Full diff is not loaded for a file above the 2 MiB text preview limit.".into());
        } else {
            let history = history(root, &path, revision.clone())?;
            let change = history.file_list.iter().find(|change| change.path == file);
            let unmerged = change.is_some_and(|change| change.status == 'U');
            if unmerged && *revision == Revision::Staged {
                notice = Some("The index is unmerged and has no staged text side. View Unstaged to compare our branch with the working file, or resolve and stage the conflict.".into());
            } else {
                let mut argument_list = diff_arguments(&path, revision)?;
                argument_list.extend(["--patch".into(), "--unified=2147483647".into(), "--diff-merges=off".into()]);
                if unmerged && *revision == Revision::Unstaged {
                    argument_list.push("--ours".into());
                }
                argument_list.push("--".into());
                let mut tail_list = vec![file.as_os_str()];
                if let Some(old_path) = change.and_then(|change| change.old_path.as_ref()) {
                    tail_list.push(old_path.as_os_str());
                }
                let raw = dynamic(&path, &argument_list, &tail_list)?;
                if raw.len() as u64 > MAX_DIFF_BYTES {
                    notice = Some("Full diff exceeds the 12 MiB display limit. Inspect this revision with a local Git diff tool.".into());
                } else if raw.contains(&0)
                    || raw
                        .split(|byte| *byte == b'\n')
                        .any(|line| (line.starts_with(b"Binary files ") && line.ends_with(b" differ")) || line == b"GIT binary patch")
                {
                    notice = Some("Binary content cannot be displayed as a text diff.".into());
                } else if raw.split(|byte| *byte == b'\n').any(|line| line.starts_with(b"@@@")) {
                    notice = Some("Combined conflict diffs are unsupported. Inspect the merge stages with a local Git tool, or resolve and stage the conflict.".into());
                } else if unmerged && !raw.split(|byte| *byte == b'\n').any(|line| line.starts_with(b"@@ ")) {
                    notice = Some("No text diff is available against our branch (index stage 2). Inspect the merge stages with a local Git tool, or resolve and stage the conflict.".into());
                } else if !raw.is_empty() {
                    match String::from_utf8(raw) {
                        Ok(text) => {
                            patch = Some(Arc::from(text));
                            if unmerged {
                                notice = Some("Conflict diff compares our branch (index stage 2) with the working file. Resolve and stage the conflict when ready.".into());
                            }
                        }
                        Err(_) => notice = Some("Patch contains non-UTF-8 text and cannot be displayed".into()),
                    }
                } else if *revision == Revision::Unstaged {
                    let tracked = output(&path, &[OsStr::new("ls-files"), OsStr::new("--error-unmatch"), OsStr::new("--"), file.as_os_str()])?
                        .status
                        .success();
                    if !tracked && let FileContent::Text(source) = &content {
                        patch = Some(Arc::from(added_patch(file, source)));
                    }
                }
            }
        }
        if patch.is_some() && matches!(content, FileContent::Unavailable(_)) {
            content = FileContent::Unavailable("This revision has no readable text on the selected side; inspect the diff.".into());
        }
    }
    Ok(FileData { path: file.to_path_buf(), content, patch, notice })
}
fn blob(path: &Path, mut prefix: OsString, file: &Path) -> FileContent {
    prefix.push(file);
    let size = match output(path, &[OsStr::new("cat-file"), OsStr::new("-s"), &prefix]).and_then(checked).and_then(text) {
        Ok(size) => size.trim().parse::<u64>().unwrap_or(u64::MAX),
        Err(error) => return FileContent::Unavailable(error),
    };
    if size > fs::MAX_TEXT_BYTES {
        return FileContent::Unavailable("Content exceeds the 2 MiB text preview limit".into());
    }
    match output(path, &[OsStr::new("cat-file"), OsStr::new("blob"), &prefix]).and_then(checked) {
        Ok(byte_list) => fs::content(byte_list),
        Err(error) => FileContent::Unavailable(error),
    }
}
fn added_patch(file: &Path, source: &str) -> String {
    fn quote(prefix: &str, path: &Path) -> String {
        let mut result = String::from("\"");
        result.push_str(prefix);
        for byte in git_path_bytes(path) {
            match byte {
                b'"' => result.push_str("\\\""),
                b'\\' => result.push_str("\\\\"),
                32..=126 => result.push(*byte as char),
                _ => result.push_str(&format!("\\{byte:03o}")),
            }
        }
        result.push('"');
        result
    }
    let old_name = quote("a/", file);
    let new_name = quote("b/", file);
    let mut patch = format!("diff --git {old_name} {new_name}\nnew file mode 100644\n--- /dev/null\n+++ {new_name}\n");
    let count = source.split_terminator('\n').count();
    if count > 0 {
        patch.push_str(&format!("@@ -0,0 +1,{count} @@\n"));
        for line in source.split_terminator('\n') {
            patch.push('+');
            patch.push_str(line);
            patch.push('\n');
        }
        if !source.ends_with('\n') {
            patch.push_str("\\ No newline at end of file\n");
        }
    }
    patch
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
