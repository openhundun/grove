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
