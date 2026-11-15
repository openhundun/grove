use super::*;

#[test]
fn status_keeps_index_worktree_rename_and_unusual_paths_distinct() {
    let data = parse_status(b"# branch.oid abcdef\0# branch.head feature\0# branch.ab +3 -2\x001 MM N... 100644 100644 100644 a b space\tname.rs\x002 R. N... 100644 100644 100644 a b R100 renamed.rs\0old name.rs\0? new\nfile.rs\0u UU N... 100644 100644 100644 100644 a b c conflict.rs\0", PathBuf::from("/workspace")).unwrap();
    assert_eq!(data.branch.as_deref(), Some("feature"));
    assert_eq!((data.ahead, data.behind), (Some(3), Some(2)));
    assert_eq!(
        data.staged_list.iter().map(|file| (file.path.clone(), file.status)).collect::<Vec<_>>(),
        vec![(PathBuf::from("space\tname.rs"), 'M'), (PathBuf::from("renamed.rs"), 'R'), (PathBuf::from("conflict.rs"), 'U')]
    );
    assert_eq!(data.staged_list[1].old_path, Some(PathBuf::from("old name.rs")));
    assert_eq!(data.unstaged_list[1].path, PathBuf::from("new\nfile.rs"));
    assert_eq!(data.unstaged_list[1].status, '?');
    assert_eq!(data.unstaged_list[2].status, 'U');
}

#[test]
fn worktree_registry_preserves_detached_and_locked_identity() {
    let tree_list = parse_worktrees(b"worktree /workspace/main\0HEAD abcdef\0branch refs/heads/main\0\0worktree /workspace/detached\0HEAD 123456\0detached\0locked review\0\0");
    assert_eq!(tree_list[0].branch.as_deref(), Some("main"));
    assert_eq!(tree_list[1].branch, None);
    assert_eq!(tree_list[1].head.as_deref(), Some("123456"));
    assert!(tree_list[1].locked);
    assert_eq!(tree_list[1].path, PathBuf::from("/workspace/detached"));
}

#[test]
fn added_patch_preserves_carriage_returns_and_missing_final_newline() {
    let patch = added_patch(Path::new("new.rs"), "a\r\nb");
    assert!(patch.contains("@@ -0,0 +1,2 @@\n+a\r\n+b\n\\ No newline at end of file\n"));
    assert_eq!(added_patch(Path::new("empty"), "").lines().filter(|line| line.starts_with("@@")).count(), 0);
}

#[test]
fn relative_file_paths_cannot_escape_worktree() {
    assert!(fs::relative(Path::new("../outside")).is_err());
    assert!(fs::relative(Path::new("/absolute")).is_err());
    assert!(fs::relative(Path::new("src/file.rs")).is_ok());
}

#[test]
fn rename_numstat_consumes_both_paths_without_corrupting_next_record() {
    let mut file_list = vec![
        FileChange {
            path: "new name".into(),
            old_path: Some("old name".into()),
            status: 'R',
            additions: None,
            deletions: None,
        },
        FileChange { path: "next".into(), old_path: None, status: 'M', additions: None, deletions: None },
    ];
    apply_numstat(&mut file_list, b"2\t1\t\0old name\0new name\x003\t4\tnext\0");
    assert_eq!((file_list[0].additions, file_list[0].deletions), (Some(2), Some(1)));
    assert_eq!((file_list[1].additions, file_list[1].deletions), (Some(3), Some(4)));
}

struct GitFixture {
    path: PathBuf,
}

impl GitFixture {
    fn new() -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("grove-git-test-{}-{timestamp}-{sequence}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let fixture = Self { path };
        fixture.git(&["init", "--quiet", "--initial-branch=main", "--template="]);
        fixture.git(&["config", "core.autocrlf", "false"]);
        fixture.git(&["config", "core.attributesFile", "/dev/null"]);
        fixture
    }
    fn command(&self, argument_list: &[&str]) -> Output {
        let mut command = std::process::Command::new("git");
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CONFIG",
            "GIT_CONFIG_COUNT",
        ] {
            command.env_remove(key);
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Grove Fixture")
            .env("GIT_AUTHOR_EMAIL", "grove-fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Grove Fixture")
            .env("GIT_COMMITTER_EMAIL", "grove-fixture@example.invalid")
            .env("GIT_AUTHOR_DATE", "@0 +0000")
            .env("GIT_COMMITTER_DATE", "@0 +0000")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .arg("-C")
            .arg(&self.path)
            .args(["-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "-c", "merge.conflictStyle=merge"])
            .args(argument_list)
            .output()
            .unwrap()
    }
    fn git(&self, argument_list: &[&str]) -> String {
        let output = self.command(argument_list);
        assert!(output.status.success(), "git {argument_list:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap().trim_end_matches('\n').to_owned()
    }
    fn write(&self, name: &str, source: impl AsRef<[u8]>) {
        std::fs::write(self.path.join(name), source).unwrap();
    }
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "--all"]);
        self.git(&["commit", "--quiet", "--message", message]);
        self.git(&["rev-parse", "HEAD"])
    }
    fn file(&self, revision: Revision, name: &str) -> FileData {
        file(&self.path, &self.path, Some(revision), Path::new(name)).unwrap()
    }
}

impl Drop for GitFixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}

fn numbered_source() -> String {
    (1..=96).map(|index| format!("line {index:03}\n")).collect()
}

fn assert_full_patch(data: &FileData, before: &str, after: &str) {
    let patch = data.patch.as_deref().expect("readable full-context patch");
    let mut before_source = String::new();
    let mut after_source = String::new();
    let mut hunk_count = 0;
    let mut previous_side = (false, false);
    for line in patch.split_terminator('\n') {
        if line.starts_with("@@ ") {
            hunk_count += 1;
        } else if hunk_count > 0 {
            match line.as_bytes().first() {
                Some(b' ' | b'-' | b'+') => {
                    previous_side = (!line.starts_with('+'), !line.starts_with('-'));
                    if previous_side.0 {
                        before_source.push_str(&line[1..]);
                        before_source.push('\n');
                    }
                    if previous_side.1 {
                        after_source.push_str(&line[1..]);
                        after_source.push('\n');
                    }
                }
                Some(b'\\') => {
                    assert_eq!(line, "\\ No newline at end of file");
                    if previous_side.0 {
                        assert_eq!(before_source.pop(), Some('\n'));
                    }
                    if previous_side.1 {
                        assert_eq!(after_source.pop(), Some('\n'));
                    }
                }
                _ => panic!("unexpected patch line: {line:?}"),
            }
        }
    }
    assert_eq!(hunk_count, 1);
    assert_eq!(before_source, before);
    assert_eq!(after_source, after);
}

#[test]
fn full_context_keeps_staged_and_unstaged_sides_isolated() {
    let fixture = GitFixture::new();
    let head_source = format!("{}last line", numbered_source().replace('\n', "\r\n"));
    let index_source = head_source.replace("line 025\r\n", "staged first\r\n").replace("line 065\r\n", "staged second\r\n");
    let worktree_source = index_source.replace("line 035\r\n", "unstaged first\r\n").replace("line 075\r\n", "unstaged second\r\n");
    fixture.write("file.txt", &head_source);
    fixture.commit("root");
    fixture.write("file.txt", &index_source);
    fixture.git(&["add", "--", "file.txt"]);
    fixture.write("file.txt", &worktree_source);
    for (revision, before, after) in [
        (Revision::Staged, head_source.as_str(), index_source.as_str()),
        (Revision::Unstaged, index_source.as_str(), worktree_source.as_str()),
    ] {
        let data = fixture.file(revision, "file.txt");
        assert_full_patch(&data, before, after);
        assert!(matches!(&data.content, FileContent::Text(source) if source.as_ref() == after));
        assert!(data.notice.is_none(), "{:?}", data.notice);
    }
}

#[test]
fn full_context_uses_root_commit_first_parent_and_comparison_merge_base() {
    let fixture = GitFixture::new();
    let root_source = numbered_source();
    let selected_source = root_source.replace("line 025\n", "selected first\n").replace("line 065\n", "selected second\n");
    let base_source = root_source.replace("line 045\n", "comparison branch\n");
    fixture.write("file.txt", &root_source);
    let root_oid = fixture.commit("root");
    fixture.git(&["branch", "comparison-base"]);
    fixture.write("file.txt", &selected_source);
    let selected_oid = fixture.commit("selected");
    fixture.git(&["checkout", "--quiet", "comparison-base"]);
    fixture.write("file.txt", &base_source);
    fixture.commit("diverged base");
    fixture.git(&["checkout", "--quiet", "main"]);
    fixture.write("file.txt", selected_source.replace("line 035\n", "dirty index\n"));
    fixture.git(&["add", "--", "file.txt"]);
    fixture.write("file.txt", selected_source.replace("line 075\n", "dirty worktree\n"));
    for (revision, before, after) in [
        (Revision::Commit(root_oid), "", root_source.as_str()),
        (Revision::Commit(selected_oid), root_source.as_str(), selected_source.as_str()),
        (Revision::Comparison("comparison-base".into()), root_source.as_str(), selected_source.as_str()),
    ] {
        let data = fixture.file(revision, "file.txt");
        assert_full_patch(&data, before, after);
        assert!(matches!(&data.content, FileContent::Text(source) if source.as_ref() == after));
        assert!(data.notice.is_none(), "{:?}", data.notice);
    }
    fixture.write("file.txt", &selected_source);
    fixture.git(&["add", "--", "file.txt"]);
    fixture.git(&["merge", "--quiet", "--no-ff", "comparison-base", "--message", "merge"]);
    let merge_oid = fixture.git(&["rev-parse", "HEAD"]);
    let merged_source = selected_source.replace("line 045\n", "comparison branch\n");
    fixture.write("file.txt", "working file is not the selected commit\n");
    let data = fixture.file(Revision::Commit(merge_oid), "file.txt");
    assert_full_patch(&data, &selected_source, &merged_source);
    assert!(matches!(&data.content, FileContent::Text(source) if source.as_ref() == merged_source));
}

#[test]
fn full_context_preserves_renamed_deleted_and_untracked_content_and_metadata() {
    let fixture = GitFixture::new();
    let source = numbered_source();
    let renamed_source = source.replace("line 025\n", "renamed first\n").replace("line 065\n", "renamed second\n");
    let deleted_source = source.replace("line", "deleted line");
    let pure_source = source.replace("line", "pure line");
    fixture.write("old name.txt", &source);
    fixture.write("deleted.txt", &deleted_source);
    fixture.write("old pure.txt", &pure_source);
    fixture.write("mode.txt", "unchanged mode text\n");
    fixture.commit("root");
    std::fs::rename(fixture.path.join("old name.txt"), fixture.path.join("new name.txt")).unwrap();
    std::fs::rename(fixture.path.join("old pure.txt"), fixture.path.join("new pure.txt")).unwrap();
    std::fs::remove_file(fixture.path.join("deleted.txt")).unwrap();
    fixture.write("new name.txt", &renamed_source);
    fixture.git(&["add", "--all"]);
    fixture.git(&["update-index", "--chmod=+x", "--", "mode.txt"]);
    for revision in [Revision::Staged, Revision::Commit("HEAD".into())] {
        if matches!(&revision, Revision::Commit(_)) {
            fixture.git(&["commit", "--quiet", "--message", "rename delete and mode"]);
        }
        let data = fixture.file(revision.clone(), "new name.txt");
        assert_full_patch(&data, &source, &renamed_source);
        let patch = data.patch.as_deref().unwrap();
        assert!(patch.contains("rename from old name.txt\n"));
        assert!(patch.contains("rename to new name.txt\n"));
        let deleted = fixture.file(revision.clone(), "deleted.txt");
        assert_full_patch(&deleted, &deleted_source, "");
        assert!(matches!(deleted.content, FileContent::Unavailable(_)));
        let pure = fixture.file(revision.clone(), "new pure.txt");
        assert!(pure.patch.as_deref().unwrap().contains("similarity index 100%\n"));
        assert!(!pure.patch.as_deref().unwrap().contains("@@"));
        assert!(matches!(&pure.content, FileContent::Text(source) if source.as_ref() == pure_source));
        let mode = fixture.file(revision, "mode.txt");
        assert!(mode.patch.as_deref().unwrap().contains("old mode 100644\nnew mode 100755\n"));
        assert!(!mode.patch.as_deref().unwrap().contains("@@"));
        assert!(matches!(&mode.content, FileContent::Text(source) if source.as_ref() == "unchanged mode text\n"));
    }
    let untracked_source = format!("{}last untracked line", source.replace('\n', "\r\n"));
    fixture.write("new\tfile.txt", &untracked_source);
    let data = fixture.file(Revision::Unstaged, "new\tfile.txt");
    assert_full_patch(&data, "", &untracked_source);
    assert!(data.notice.is_none());
}

#[test]
fn full_context_accepts_patch_above_the_text_side_limit() {
    let fixture = GitFixture::new();
    let side_bytes = fs::MAX_TEXT_BYTES as usize / 2 + 16;
    let before = format!("{}\n", "a".repeat(side_bytes));
    let after = format!("{}\n", "b".repeat(side_bytes));
    fixture.write("long.txt", &before);
    fixture.commit("root");
    fixture.write("long.txt", &after);
    let data = fixture.file(Revision::Unstaged, "long.txt");
    assert!(data.patch.as_deref().unwrap().len() as u64 > fs::MAX_TEXT_BYTES);
    assert!(data.notice.is_none(), "{:?}", data.notice);
    assert_full_patch(&data, &before, &after);
}

#[test]
fn unmerged_diff_compares_ours_and_explains_missing_staged_or_ours_sides() {
    let fixture = GitFixture::new();
    let source = numbered_source();
    let ours_source = source.replace("line 045\n", "our branch\n");
    let theirs_source = source.replace("line 045\n", "their branch\n");
    fixture.write("conflict.txt", &source);
    fixture.write("deleted-by-ours.txt", "base of deleted file\n");
    fixture.commit("root");
    fixture.git(&["branch", "theirs"]);
    fixture.write("conflict.txt", &ours_source);
    std::fs::remove_file(fixture.path.join("deleted-by-ours.txt")).unwrap();
    fixture.commit("ours");
    fixture.git(&["checkout", "--quiet", "theirs"]);
    fixture.write("conflict.txt", &theirs_source);
    fixture.write("deleted-by-ours.txt", "their modified deleted file\n");
    fixture.commit("theirs");
    fixture.git(&["checkout", "--quiet", "main"]);
    let merge = fixture.command(&["merge", "--quiet", "theirs"]);
    assert!(!merge.status.success());
    let working_source = std::fs::read_to_string(fixture.path.join("conflict.txt")).unwrap();
    assert!(working_source.contains("<<<<<<< HEAD\n"));
    let data = fixture.file(Revision::Unstaged, "conflict.txt");
    assert_full_patch(&data, &ours_source, &working_source);
    assert!(!data.patch.as_deref().unwrap().contains("@@@"));
    assert!(data.notice.as_deref().unwrap().contains("index stage 2"));
    let staged = fixture.file(Revision::Staged, "conflict.txt");
    assert!(staged.patch.is_none());
    assert!(staged.notice.as_deref().unwrap().contains("resolve and stage"));
    let missing_ours = fixture.file(Revision::Unstaged, "deleted-by-ours.txt");
    assert!(missing_ours.patch.is_none());
    assert!(missing_ours.notice.as_deref().unwrap().contains("index stage 2"));
    fixture.write("conflict.txt", &ours_source);
    let unchanged_ours = fixture.file(Revision::Unstaged, "conflict.txt");
    assert!(unchanged_ours.patch.is_none());
    assert!(unchanged_ours.notice.as_deref().unwrap().contains("resolve and stage"));
}

#[test]
fn binary_sides_and_forced_binary_patches_are_not_displayed_as_text() {
    let fixture = GitFixture::new();
    fixture.write(".gitattributes", "forced.bin diff\n");
    fixture.write("current.bin", "old text\n");
    fixture.write("previous.bin", b"old\0binary\n");
    fixture.write("forced.bin", b"forced\0binary\n");
    fixture.commit("root");
    fixture.write("current.bin", b"new\0binary\n");
    fixture.write("previous.bin", "new text\n");
    fixture.write("forced.bin", "new forced text\n");
    fixture.write("untracked.bin", b"untracked\0binary\n");
    for name in ["current.bin", "previous.bin", "forced.bin", "untracked.bin"] {
        let data = fixture.file(Revision::Unstaged, name);
        assert!(data.patch.is_none(), "{name}");
        assert!(data.notice.as_deref().unwrap().contains("Binary"), "{name}");
    }
}

#[test]
fn filesystem_blame_maps_committed_and_modified_utf8_working_lines() {
    let fixture = GitFixture::new();
    let name = "space 名\nfile.txt";
    let original = "unchanged α\nreplace me\nunchanged 終\nlast without newline";
    fixture.write(name, original);
    let oid = fixture.commit("initial UTF-8 summary 摘要");
    let unchanged = file(&fixture.path, &fixture.path, None, Path::new(name)).unwrap();
    assert!(unchanged.blame_notice.is_none(), "{:?}", unchanged.blame_notice);
    assert_eq!(unchanged.blame.iter().map(|line| line.line).collect::<Vec<_>>(), vec![1, 2, 3, 4]);
    for line in &unchanged.blame {
        assert_eq!(line.oid, oid);
        assert_eq!(line.author, "Grove Fixture");
        assert_eq!(line.summary, "initial UTF-8 summary 摘要");
        assert_eq!(line.time, 0);
        assert!(!line.uncommitted);
    }
    let modified = "inserted 新\nunchanged α\nmodified β\nunchanged 終\nlast without newline";
    fixture.write(name, modified);
    let data = file(&fixture.path, &fixture.path, None, Path::new(name)).unwrap();
    assert!(matches!(&data.content, FileContent::Text(source) if source.as_ref() == modified));
    assert!(data.patch.is_none());
    assert!(data.notice.is_none());
    assert!(data.blame_notice.is_none(), "{:?}", data.blame_notice);
    assert_eq!(data.blame.iter().map(|line| line.line).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
    for (index, line) in data.blame.iter().enumerate() {
        let uncommitted = matches!(index, 0 | 2);
        assert_eq!(line.uncommitted, uncommitted);
        if uncommitted {
            assert_eq!(line.oid, "0".repeat(oid.len()));
            assert_eq!(line.author, "Not Committed Yet");
        } else {
            assert_eq!(line.oid, oid);
            assert_eq!(line.author, "Grove Fixture");
            assert_eq!(line.summary, "initial UTF-8 summary 摘要");
            assert_eq!(line.time, 0);
        }
    }
    for revision in [Revision::Unstaged, Revision::Staged, Revision::Commit(oid), Revision::Comparison("HEAD".into())] {
        let data = fixture.file(revision, name);
        assert!(data.blame.is_empty());
        assert!(data.blame_notice.is_none());
    }
}

#[test]
fn filesystem_blame_explains_unborn_and_untracked_files_without_attribution() {
    let fixture = GitFixture::new();
    let name = "new file 名.txt";
    fixture.write(name, "no historical author\n");
    fixture.git(&["add", "--", name]);
    let unborn = file(&fixture.path, &fixture.path, None, Path::new(name)).unwrap();
    assert!(unborn.blame.is_empty());
    assert!(unborn.blame_notice.as_deref().unwrap().contains("HEAD commit"));
    fixture.commit("root");
    fixture.write("untracked name.txt", "not committed\n");
    let untracked = file(&fixture.path, &fixture.path, None, Path::new("untracked name.txt")).unwrap();
    assert!(untracked.blame.is_empty());
    assert!(untracked.blame_notice.as_deref().unwrap().contains("untracked"));
    assert!(matches!(untracked.content, FileContent::Text(_)));
}

#[test]
fn filesystem_blame_follows_indexed_renames_and_explains_untracked_renames() {
    let fixture = GitFixture::new();
    let source = "first tracked line\nsecond tracked line\n";
    fixture.write("old name.txt", source);
    let oid = fixture.commit("rename origin");
    std::fs::rename(fixture.path.join("old name.txt"), fixture.path.join("new 名 name.txt")).unwrap();
    let untracked = file(&fixture.path, &fixture.path, None, Path::new("new 名 name.txt")).unwrap();
    assert!(untracked.blame.is_empty());
    assert!(untracked.blame_notice.as_deref().unwrap().contains("renames not recorded in the index"));
    fixture.git(&["add", "--all"]);
    let indexed = file(&fixture.path, &fixture.path, None, Path::new("new 名 name.txt")).unwrap();
    assert!(indexed.blame_notice.is_none(), "{:?}", indexed.blame_notice);
    assert_eq!(indexed.blame.len(), 2);
    for line in &indexed.blame {
        assert_eq!(line.oid, oid);
        assert_eq!(line.summary, "rename origin");
        assert!(!line.uncommitted);
    }
}

#[test]
fn filesystem_blame_explains_binary_deleted_and_oversize_content() {
    let fixture = GitFixture::new();
    fixture.write("binary.bin", b"binary\0content\n");
    fixture.write("deleted.txt", "deleted working text\n");
    fixture.commit("root");
    std::fs::remove_file(fixture.path.join("deleted.txt")).unwrap();
    fixture.write("large.txt", vec![b'x'; fs::MAX_TEXT_BYTES as usize + 1]);
    for (name, notice) in [("binary.bin", "binary"), ("deleted.txt", "deleted"), ("large.txt", "2 MiB")] {
        let data = file(&fixture.path, &fixture.path, None, Path::new(name)).unwrap();
        assert!(data.blame.is_empty(), "{name}");
        assert!(data.blame_notice.as_deref().unwrap().contains(notice), "{name}: {:?}", data.blame_notice);
    }
}

#[test]
fn filesystem_blame_uses_loaded_stdin_text_and_handles_a_near_limit_line() {
    let fixture = GitFixture::new();
    fixture.write("captured.txt", "unchanged\nold text\n");
    let oid = fixture.commit("root");
    fixture.write("captured.txt", "a later filesystem version\n");
    let captured = "unchanged\ncaptured UTF-8 新 text\n";
    let data = blame(&fixture.path, Path::new("captured.txt"), captured).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0].oid, oid);
    assert!(!data[0].uncommitted);
    assert_eq!(data[1].line, 2);
    assert!(data[1].uncommitted);
    let long_source = "x".repeat(fs::MAX_TEXT_BYTES as usize - 1);
    fixture.write("captured.txt", &long_source);
    let data = file(&fixture.path, &fixture.path, None, Path::new("captured.txt")).unwrap();
    assert!(data.blame_notice.is_none(), "{:?}", data.blame_notice);
    assert_eq!(data.blame.len(), 1);
    assert_eq!(data.blame[0].line, 1);
    assert!(data.blame[0].uncommitted);
}

#[test]
fn blame_parser_preserves_crlf_and_rejects_transformed_or_incomplete_mapping() {
    let oid = "1".repeat(40);
    let record = format!("{oid} 9 1 1\nauthor UTF-8 作者\nauthor-time 123\nsummary retained summary 摘要\nfilename \"space\\nname.txt\"\n\tfirst\r\n");
    let data = parse_blame(&mut record.as_bytes(), "first\r\n").unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].line, 1);
    assert_eq!(data[0].oid, oid);
    assert_eq!(data[0].author, "UTF-8 作者");
    assert_eq!(data[0].summary, "retained summary 摘要");
    assert_eq!(data[0].time, 123);
    assert!(parse_blame(&mut record.as_bytes(), "first\n").unwrap_err().contains("transformed"));
    assert!(parse_blame(&mut record.as_bytes(), "first\r\nsecond\n").unwrap_err().contains("fewer lines"));
    let shifted = record.replacen(" 9 1 1\n", " 9 2 1\n", 1);
    assert!(parse_blame(&mut shifted.as_bytes(), "first\r\n").unwrap_err().contains("line numbers"));
}
