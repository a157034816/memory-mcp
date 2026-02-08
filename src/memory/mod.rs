mod index;
mod model;
mod store;
mod time;

use crate::memory::store::{NamespaceState, StorePaths};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub use crate::memory::model::{RecallArgs, RememberArgs};

/// 解析并返回存储根目录。
pub fn resolve_root_dir() -> PathBuf {
    if let Ok(value) = std::env::var("MEMORY_STORE_DIR") {
        let p = value.trim();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }

    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "ERP_NewFrame", "Memory") {
        return proj_dirs.data_local_dir().to_path_buf();
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Memory 引擎：按 namespace 管理 JSONL + 索引，并提供 remember/recall 操作。
pub struct MemoryEngine {
    root_dir: PathBuf,
    namespaces: HashMap<String, NamespaceState>,
}

impl MemoryEngine {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            namespaces: HashMap::new(),
        }
    }

    pub fn now(&self) -> Result<Value, String> {
        let (utc_rfc3339, utc_ts) = time::now_rfc3339_and_ts();
        let (local_rfc3339, local_offset_seconds) = time::now_local_rfc3339_and_offset_seconds();
        let local_offset_minutes = local_offset_seconds / 60;
        let local_offset_text = {
            let sign = if local_offset_seconds >= 0 { '+' } else { '-' };
            let abs = local_offset_seconds.abs();
            let hours = abs / 3600;
            let minutes = (abs % 3600) / 60;
            format!("{sign}{hours:02}:{minutes:02}")
        };

        Ok(json!({
            "content": [
                {
                    "type": "text",
                    "text": format!("当前时间：{}（本地，UTC{}）｜{}（UTC）", local_rfc3339, local_offset_text, utc_rfc3339)
                }
            ],
            "data": {
                "utc_rfc3339": utc_rfc3339,
                "utc_ts": utc_ts,
                "local_rfc3339": local_rfc3339,
                "local_offset_seconds": local_offset_seconds,
                "local_offset_minutes": local_offset_minutes
            }
        }))
    }

    pub fn remember(&mut self, args: RememberArgs) -> Result<Value, String> {
        let state = self.get_or_open_namespace(&args.namespace)?;
        let namespace = state.namespace().to_string();
        let recorded = state.append_memory(args)?;

        Ok(json!({
            "content": [
                { "type": "text", "text": format!("已记录记忆：{}（namespace={}）", recorded.id, namespace) }
            ],
            "data": {
                "id": recorded.id,
                "namespace": namespace,
                "recorded_at": recorded.recorded_at,
                "occurred_at": recorded.occurred_at,
                "keywords": recorded.keywords
            }
        }))
    }

    pub fn recall(&mut self, args: RecallArgs) -> Result<Value, String> {
        let state = self.get_or_open_namespace(&args.namespace)?;
        let namespace = state.namespace().to_string();
        let result = state.recall(args)?;

        Ok(json!({
            "content": [
                { "type": "text", "text": result.render_text_summary() }
            ],
            "data": {
                "namespace": namespace,
                "total": result.total,
                "items": result.items
            }
        }))
    }

    pub fn keywords_list(&mut self, namespace: String) -> Result<Value, String> {
        let input = namespace.trim();
        let state = self.get_or_open_namespace(input)?;
        let ns = state.namespace().to_string();
        let keywords = state.list_keywords()?;
        let total = keywords.len();

        let text = if total == 0 {
            format!("namespace={}：暂无关键字。", ns)
        } else {
            format!("namespace={}：共 {} 个关键字。", ns, total)
        };

        Ok(json!({
            "content": [
                { "type": "text", "text": text }
            ],
            "data": {
                "namespace": ns,
                "total": total,
                "keywords": keywords
            }
        }))
    }

    pub fn keywords_list_global(&self) -> Result<Value, String> {
        let stats = collect_global_keyword_stats(&self.root_dir);
        let total = stats.keywords.len();

        let text = if total == 0 {
            "全局：暂无关键字。".to_string()
        } else {
            format!("全局：共 {} 个关键字，覆盖 {} 个 namespace。", total, stats.scanned_namespaces)
        };

        Ok(json!({
            "content": [
                { "type": "text", "text": text }
            ],
            "data": {
                "total": total,
                "scanned_namespaces": stats.scanned_namespaces,
                "keywords": stats.keywords
            }
        }))
    }

    fn get_or_open_namespace(&mut self, namespace: &str) -> Result<&mut NamespaceState, String> {
        let raw = namespace.trim();
        if raw.is_empty() {
            return Err("namespace 不能为空".to_string());
        }

        let paths = StorePaths::new(&self.root_dir, raw)?;
        let key = paths.namespace.clone();

        if !self.namespaces.contains_key(&key) {
            let state = NamespaceState::open(paths)?;
            self.namespaces.insert(key.clone(), state);
        }

        Ok(self
            .namespaces
            .get_mut(&key)
            .expect("namespace exists"))
    }
}

#[derive(Debug, Clone)]
struct GlobalKeywordStats {
    scanned_namespaces: usize,
    keywords: Vec<Value>,
}

fn collect_global_keyword_stats(root_dir: &Path) -> GlobalKeywordStats {
    if !root_dir.exists() {
        return GlobalKeywordStats {
            scanned_namespaces: 0,
            keywords: Vec::new(),
        };
    }

    let mut namespaces_scanned = 0usize;
    let mut keyword_namespaces: HashMap<String, usize> = HashMap::new();
    let mut keyword_items: HashMap<String, usize> = HashMap::new();

    let mut stack: Vec<PathBuf> = vec![root_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(v) => v,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }

            if path.file_name().and_then(|x| x.to_str()) != Some("index.json") {
                continue;
            }

            let text = match fs::read_to_string(&path) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let index: index::IndexData = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if index.version != index::INDEX_VERSION {
                continue;
            }

            namespaces_scanned += 1;
            for (kw, postings) in index.keyword_postings {
                let kw = kw.trim().to_lowercase();
                if kw.is_empty() || store::is_time_like_keyword(&kw) {
                    continue;
                }
                *keyword_namespaces.entry(kw.clone()).or_insert(0) += 1;
                *keyword_items.entry(kw).or_insert(0) += postings.len();
            }
        }
    }

    let mut out: Vec<(String, usize, usize)> = Vec::new();
    for (kw, ns_count) in keyword_namespaces {
        let items = keyword_items.get(&kw).copied().unwrap_or(0);
        out.push((kw, ns_count, items));
    }

    out.sort_by(|a, b| {
        a.0.chars()
            .count()
            .cmp(&b.0.chars().count())
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.0.cmp(&b.0))
    });

    let keywords: Vec<Value> = out
        .into_iter()
        .map(|(keyword, namespaces, items)| json!({ "keyword": keyword, "namespaces": namespaces, "items": items }))
        .collect();

    GlobalKeywordStats {
        scanned_namespaces: namespaces_scanned,
        keywords,
    }
}
