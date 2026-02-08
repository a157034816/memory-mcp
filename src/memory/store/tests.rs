use super::*;

#[test]
fn namespace_dir_should_prevent_traversal() {
    let root = PathBuf::from("C:/tmp/root");
    let dir = resolve_namespace_dir(&root, "../a/..//b");
    let s = dir.to_string_lossy().replace('\\', "/");
    assert!(!s.contains("../"));
}

#[test]
fn single_level_namespace_should_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let err = StorePaths::new(root, "proj1").err().expect("should error");
    assert!(err.contains("{userId}/{projectId}"), "unexpected err: {err}");
}

#[test]
fn three_level_namespace_should_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let err = StorePaths::new(root, "t/u/p").err().expect("should error");
    assert!(err.contains("{userId}/{projectId}"), "unexpected err: {err}");
}

#[test]
fn namespace_should_be_canonicalized() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u1\\p1//").unwrap();
    assert_eq!(paths.namespace, "u1/p1");

    let s = paths.namespace_dir.to_string_lossy().replace('\\', "/");
    assert!(s.ends_with("/u1/p1"), "unexpected dir: {s}");
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

#[test]
fn recall_query_time_expr_should_filter() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u1/p1").unwrap();
    let mut state = NamespaceState::open(paths).unwrap();

    state
        .append_memory(RememberArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec!["a".to_string()],
            slice: "older".to_string(),
            diary: "diary".to_string(),
            occurred_at: Some("2025-04-01".to_string()),
            importance: None,
            source: None,
        })
        .unwrap();

    state
        .append_memory(RememberArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec!["b".to_string()],
            slice: "newer".to_string(),
            diary: "diary".to_string(),
            occurred_at: Some("2025-05-01".to_string()),
            importance: None,
            source: None,
        })
        .unwrap();

    let recalled = state
        .recall(RecallArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec![],
            start: None,
            end: None,
            query: Some("time>=2025-05-01".to_string()),
            limit: 20,
            include_diary: false,
        })
        .unwrap();

    assert_eq!(recalled.items.len(), 1);
    assert_eq!(recalled.items[0].slice, "newer");
}

#[test]
fn recall_query_time_range_expr_should_filter() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u1/p1").unwrap();
    let mut state = NamespaceState::open(paths).unwrap();

    for (slice, occurred_at) in [("d1", "2025-01-15"), ("d2", "2025-02-20"), ("d3", "2025-03-10")]
    {
        state
            .append_memory(RememberArgs {
                namespace: "u1/p1".to_string(),
                keywords: vec!["x".to_string()],
                slice: slice.to_string(),
                diary: "diary".to_string(),
                occurred_at: Some(occurred_at.to_string()),
                importance: None,
                source: None,
            })
            .unwrap();
    }

    let recalled = state
        .recall(RecallArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec![],
            start: None,
            end: None,
            query: Some("time=2025-02-01..2025-02-28".to_string()),
            limit: 20,
            include_diary: false,
        })
        .unwrap();

    assert_eq!(recalled.items.len(), 1);
    assert_eq!(recalled.items[0].slice, "d2");
}

#[test]
fn remember_should_drop_time_like_keywords() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u1/p1").unwrap();
    let mut state = NamespaceState::open(paths).unwrap();

    let recorded = state
        .append_memory(RememberArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec![
                "项目".to_string(),
                "2025-08-20".to_string(),
                "2025 年 8 月 20 日".to_string(),
                "8月".to_string(),
                "2025年".to_string(),
                "20日".to_string(),
                "2025-08-20T10:00:00Z".to_string(),
                "2025-08-20t10:00:00z".to_string(),
            ],
            slice: "slice".to_string(),
            diary: "diary".to_string(),
            occurred_at: None,
            importance: None,
            source: None,
        })
        .unwrap();

    assert_eq!(recorded.keywords, vec!["项目".to_string()]);

    let keywords = state.list_keywords().unwrap();
    assert_eq!(keywords, vec!["项目".to_string()]);
}

#[test]
fn remember_only_time_keywords_should_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u1/p1").unwrap();
    let mut state = NamespaceState::open(paths).unwrap();

    let err = state
        .append_memory(RememberArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec!["2025-08-20".to_string()],
            slice: "slice".to_string(),
            diary: "diary".to_string(),
            occurred_at: None,
            importance: None,
            source: None,
        })
        .err()
        .expect("should error");

    assert!(err.contains("keywords"), "unexpected err: {err}");
}

#[test]
fn recall_start_end_should_accept_lowercase_rfc3339() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u1/p1").unwrap();
    let mut state = NamespaceState::open(paths).unwrap();

    state
        .append_memory(RememberArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec!["k".to_string()],
            slice: "hit".to_string(),
            diary: "diary".to_string(),
            occurred_at: Some("2025-05-01".to_string()),
            importance: None,
            source: None,
        })
        .unwrap();

    let recalled = state
        .recall(RecallArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec![],
            start: Some("2025-04-30t00:00:00z".to_string()),
            end: Some("2025-05-01t23:59:59z".to_string()),
            query: None,
            limit: 20,
            include_diary: false,
        })
        .unwrap();

    assert_eq!(recalled.items.len(), 1);
    assert_eq!(recalled.items[0].slice, "hit");
}

#[test]
fn remember_importance_out_of_range_should_error() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let paths = StorePaths::new(root, "u1/p1").unwrap();
    let mut state = NamespaceState::open(paths).unwrap();

    let err = state
        .append_memory(RememberArgs {
            namespace: "u1/p1".to_string(),
            keywords: vec!["k".to_string()],
            slice: "slice".to_string(),
            diary: "diary".to_string(),
            occurred_at: None,
            importance: Some(6),
            source: None,
        })
        .err()
        .expect("should error");

    assert!(err.contains("importance"), "unexpected err: {err}");
}
