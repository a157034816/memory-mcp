use super::*;

#[test]
fn namespace_dir_should_prevent_traversal() {
    let root = PathBuf::from("C:/tmp/root");
    let dir = resolve_namespace_dir(&root, "../a/..//b");
    let s = dir.to_string_lossy().replace('\\', "/");
    assert!(!s.contains("../"));
}

#[test]
fn remember_and_recall_by_keyword_and_time() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u1/p1").unwrap();
    let mut state = NamespaceState::open(paths).unwrap();

    state
        .append_memory(RememberArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec!["项目".to_string(), "ERP".to_string()],
            slice: "我们一起做过 ERP 项目".to_string(),
            diary: "今天我们推进了项目里程碑。".to_string(),
            occurred_at: None,
            importance: Some(3),
            source: Some("test".to_string()),
        })
        .unwrap();

    state
        .append_memory(RememberArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec!["病".to_string(), "药".to_string()],
            slice: "2025 年生了一场病，后来找到救命的药".to_string(),
            diary: "那段时间很艰难，但最终有了转机。".to_string(),
            occurred_at: Some("2025-05-01".to_string()),
            importance: Some(5),
            source: None,
        })
        .unwrap();

    let recalled = state
        .recall(RecallArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec!["项目".to_string()],
            start: None,
            end: None,
            query: None,
            limit: 20,
            include_diary: false,
        })
        .unwrap();

    assert_eq!(recalled.items.len(), 1);
    assert!(recalled.items[0].slice.contains("ERP"));
    assert!(recalled.items[0].diary.is_none());

    let recalled_2025 = state
        .recall(RecallArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec!["药".to_string()],
            start: Some("2025-01-01".to_string()),
            end: Some("2025-12-31".to_string()),
            query: None,
            limit: 20,
            include_diary: true,
        })
        .unwrap();

    assert_eq!(recalled_2025.items.len(), 1);
    assert!(recalled_2025.items[0].slice.contains("药"));
    assert!(recalled_2025.items[0].diary.is_some());
}

#[test]
fn invalid_jsonl_line_should_be_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u2/p2").unwrap();
    let mut state = NamespaceState::open(paths.clone()).unwrap();

    state
        .append_memory(RememberArgs {
            namespace: "u2/p2".to_string(),
            keywords: vec!["x".to_string()],
            slice: "slice".to_string(),
            diary: "diary".to_string(),
            occurred_at: None,
            importance: None,
            source: None,
        })
        .unwrap();

    // 注入坏行
    {
        let mut f = OpenOptions::new()
            .append(true)
            .open(&paths.memories_path)
            .unwrap();
        f.write_all(b"not json\n").unwrap();
        f.flush().unwrap();
    }

    // 重新打开，触发增量索引
    let mut reopened = NamespaceState::open(paths).unwrap();
    let recalled = reopened
        .recall(RecallArgs {
            namespace: "u2/p2".to_string(),
            keywords: vec!["x".to_string()],
            start: None,
            end: None,
            query: None,
            limit: 20,
            include_diary: false,
        })
        .unwrap();

    assert_eq!(recalled.items.len(), 1);
}

#[test]
fn remember_empty_keywords_should_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u3/p3").unwrap();
    let mut state = NamespaceState::open(paths).unwrap();

    let err = state
        .append_memory(RememberArgs {
            namespace: "u3/p3".to_string(),
            keywords: vec!["  ".to_string()],
            slice: "slice".to_string(),
            diary: "diary".to_string(),
            occurred_at: None,
            importance: None,
            source: None,
        })
        .err()
        .expect("should error");

    assert!(err.contains("keywords"));
}
