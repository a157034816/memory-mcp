use crate::memory::index::IndexData;
use crate::memory::model::{MemoryItem, RecallArgs, RecallItemOut, RecallResult, RememberArgs};
use crate::memory::time::{self, DateBoundKind};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StorePaths {
    pub namespace: String,
    pub namespace_dir: PathBuf,
    pub memories_path: PathBuf,
    pub index_path: PathBuf,
}

impl StorePaths {
    pub fn new(root_dir: &Path, namespace: &str) -> Result<Self, String> {
        let namespace = namespace.trim().to_string();
        if namespace.is_empty() {
            return Err("namespace 不能为空".to_string());
        }

        let namespace_dir = resolve_namespace_dir(root_dir, &namespace);
        let memories_path = namespace_dir.join("memories.jsonl");
        let index_path = namespace_dir.join("index.json");

        Ok(Self {
            namespace,
            namespace_dir,
            memories_path,
            index_path,
        })
    }
}

pub struct NamespaceState {
    paths: StorePaths,
    index: IndexData,
}

pub struct RememberRecorded {
    pub id: String,
    pub recorded_at: String,
    pub occurred_at: Option<String>,
    pub keywords: Vec<String>,
}

impl NamespaceState {
    pub fn open(paths: StorePaths) -> Result<Self, String> {
        fs::create_dir_all(&paths.namespace_dir)
            .map_err(|e| format!("create namespace dir failed: {e}"))?;

        if !paths.memories_path.exists() {
            File::create(&paths.memories_path)
                .map_err(|e| format!("create memories.jsonl failed: {e}"))?;
        }

        let index = load_or_create_index(&paths)?;
        Ok(Self { paths, index })
    }

    pub fn list_keywords(&mut self) -> Result<Vec<String>, String> {
        self.sync_index().map_err(|e| e.to_string())?;

        let mut keywords: Vec<String> = self.index.keyword_postings.keys().cloned().collect();
        keywords.sort_by(|a, b| {
            a.chars()
                .count()
                .cmp(&b.chars().count())
                .then_with(|| a.cmp(b))
        });
        Ok(keywords)
    }

    pub fn append_memory(&mut self, args: RememberArgs) -> Result<RememberRecorded, String> {
        self.sync_index().map_err(|e| e.to_string())?;

        let namespace = self.paths.namespace.clone();
        let (recorded_at, recorded_at_ts) = time::now_rfc3339_and_ts();

        let (occurred_at, occurred_at_ts) = match args.occurred_at.as_deref() {
            Some(text) => {
                let (ts, canonical) = time::parse_time_to_ts_and_canonical(text, DateBoundKind::Start)?;
                (Some(canonical), Some(ts))
            }
            None => (None, None),
        };

        let keywords = normalize_keywords(args.keywords);
        if keywords.is_empty() {
            return Err("keywords 不能为空".to_string());
        }

        let id = Uuid::new_v4().to_string();
        let item = MemoryItem {
            id: id.clone(),
            namespace: namespace.clone(),
            recorded_at: recorded_at.clone(),
            occurred_at: occurred_at.clone(),
            keywords: keywords.clone(),
            slice: args.slice,
            diary: args.diary,
            importance: args.importance,
            source: args.source,
        };

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.paths.memories_path)
            .map_err(|e| format!("open memories.jsonl failed: {e}"))?;

        let offset = file
            .metadata()
            .map_err(|e| format!("stat memories.jsonl failed: {e}"))?
            .len();

        let mut line = serde_json::to_vec(&item)
            .map_err(|e| format!("serialize memory item failed: {e}"))?;
        line.push(b'\n');
        let length = line.len() as u32;

        file.write_all(&line)
            .and_then(|_| file.flush())
            .map_err(|e| format!("append memories.jsonl failed: {e}"))?;

        self.index.add_memory_item(
            &item,
            offset,
            length,
            recorded_at_ts,
            occurred_at_ts,
            keywords.clone(),
        );
        self.index.indexed_up_to_offset = offset + length as u64;

        save_index(&self.paths, &self.index)?;

        Ok(RememberRecorded {
            id,
            recorded_at,
            occurred_at,
            keywords,
        })
    }

    pub fn recall(&mut self, args: RecallArgs) -> Result<RecallResult, String> {
        self.sync_index().map_err(|e| e.to_string())?;
        self.index.ensure_time_sorted();

        let keywords = normalize_keywords(args.keywords);
        let keyword_set: Option<HashSet<String>> = if keywords.is_empty() {
            None
        } else {
            Some(keywords.iter().cloned().collect())
        };
        let query = args
            .query
            .as_ref()
            .map(|x| x.trim().to_lowercase())
            .filter(|x| !x.is_empty());

        let start_ts = match args.start.as_deref() {
            Some(s) => Some(time::parse_time_to_ts_and_canonical(s, DateBoundKind::Start)?.0),
            None => None,
        };
        let end_ts = match args.end.as_deref() {
            Some(s) => Some(time::parse_time_to_ts_and_canonical(s, DateBoundKind::End)?.0),
            None => None,
        };

        let mut results: Vec<RecallItemOut> = Vec::new();

        if keywords.is_empty() {
            // 无关键字：按时间索引倒序扫描（近 → 远）
            let candidates = self.iter_time_candidates(start_ts, end_ts);
            for idx in candidates {
                if results.len() >= args.limit {
                    break;
                }
                if let Some(item) =
                    self.try_load_item_for_recall(idx, None, &query, args.include_diary)?
                {
                    results.push(item);
                }
            }
        } else {
            // 有关键字：倒排索引求并集，并按命中数/重要度/时间排序
            let mut counts: HashMap<u32, u32> = HashMap::new();
            for kw in &keywords {
                if let Some(list) = self.index.keyword_postings.get(kw) {
                    for &idx in list {
                        *counts.entry(idx).or_insert(0) += 1;
                    }
                }
            }

            let mut scored: Vec<(u32, u32, i64, u8)> = Vec::new();
            for (idx, hit) in counts {
                let item = &self.index.items[idx as usize];
                let ts = item.time_key_ts();
                if !in_time_range(ts, start_ts, end_ts) {
                    continue;
                }
                let imp = item.importance.unwrap_or(0);
                scored.push((idx, hit, ts, imp));
            }

            scored.sort_by(|a, b| {
                // hit desc, importance desc, time desc
                b.1.cmp(&a.1)
                    .then_with(|| b.3.cmp(&a.3))
                    .then_with(|| b.2.cmp(&a.2))
            });

            for (idx, _hit, _ts, _imp) in scored {
                if results.len() >= args.limit {
                    break;
                }
                if let Some(item) = self.try_load_item_for_recall(
                    idx,
                    keyword_set.as_ref(),
                    &query,
                    args.include_diary,
                )? {
                    results.push(item);
                }
            }
        }

        let total = results.len();
        Ok(RecallResult { total, items: results })
    }

    fn iter_time_candidates(&self, start_ts: Option<i64>, end_ts: Option<i64>) -> Vec<u32> {
        if start_ts.is_none() && end_ts.is_none() {
            return self.index.time_sorted.iter().rev().copied().collect();
        }

        // time_sorted asc；这里做线性过滤（候选在 index 中，且仅在“无关键字”分支触发）。
        // 以后如需更快可升级为二分范围裁剪。
        self.index
            .time_sorted
            .iter()
            .copied()
            .filter(|&idx| {
                self.index
                    .items
                    .get(idx as usize)
                    .map(|x| in_time_range(x.time_key_ts(), start_ts, end_ts))
                    .unwrap_or(false)
            })
            .rev()
            .collect()
    }

    fn try_load_item_for_recall(
        &self,
        idx: u32,
        keyword_set: Option<&HashSet<String>>,
        query: &Option<String>,
        include_diary: bool,
    ) -> Result<Option<RecallItemOut>, String> {
        let item = load_item_by_index(&self.paths.memories_path, &self.index, idx)?;

        if let Some(q) = query {
            let q = q.as_str();
            let hay = format!(
                "{}\n{}\n{}",
                item.slice.to_lowercase(),
                item.diary.to_lowercase(),
                item.source.clone().unwrap_or_default().to_lowercase()
            );
            if !hay.contains(q) {
                return Ok(None);
            }
        }

        let matched_keywords = keyword_set.map(|set| {
            let mut out: Vec<String> = item
                .keywords
                .iter()
                .filter(|kw| set.contains(*kw))
                .cloned()
                .collect();
            out.sort_by(|a, b| {
                a.chars()
                    .count()
                    .cmp(&b.chars().count())
                    .then_with(|| a.cmp(b))
            });
            out
        });

        Ok(Some(RecallItemOut {
            id: item.id,
            recorded_at: item.recorded_at,
            occurred_at: item.occurred_at,
            keywords: item.keywords,
            matched_keywords,
            slice: item.slice,
            diary: include_diary.then_some(item.diary),
            importance: item.importance,
            source: item.source,
        }))
    }

    fn sync_index(&mut self) -> io::Result<()> {
        let file_len = fs::metadata(&self.paths.memories_path)?.len();

        // 文件回退：重建索引
        if file_len < self.index.indexed_up_to_offset {
            self.index = IndexData::new(&self.paths.namespace);
        }

        if file_len == self.index.indexed_up_to_offset {
            return Ok(());
        }

        incremental_index(&self.paths.memories_path, &mut self.index)?;
        save_index(&self.paths, &self.index)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(())
    }
}

fn normalize_keywords(keywords: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for kw in keywords {
        let norm = kw.trim().to_lowercase();
        if norm.is_empty() {
            continue;
        }
        if seen.insert(norm.clone()) {
            out.push(norm);
        }
    }

    out
}

fn in_time_range(ts: i64, start: Option<i64>, end: Option<i64>) -> bool {
    if let Some(s) = start {
        if ts < s {
            return false;
        }
    }
    if let Some(e) = end {
        if ts > e {
            return false;
        }
    }
    true
}

fn resolve_namespace_dir(root_dir: &Path, namespace: &str) -> PathBuf {
    let ns = namespace.replace('\\', "/");
    let parts = ns.split('/').filter_map(|p| {
        let p = p.trim();
        if p.is_empty() || p == "." || p == ".." {
            return None;
        }
        Some(sanitize_path_component(p))
    });

    let mut dir = root_dir.to_path_buf();
    let mut any = false;
    for p in parts {
        any = true;
        dir.push(p);
    }

    if !any {
        dir.push("default");
    }

    dir
}

fn sanitize_path_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let illegal = matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|');
        if illegal {
            out.push('_');
        } else {
            out.push(ch);
        }
    }

    let trimmed = out.trim_matches([' ', '.']).to_string();
    if trimmed.is_empty() {
        "_".to_string()
    } else {
        trimmed
    }
}

fn load_or_create_index(paths: &StorePaths) -> Result<IndexData, String> {
    if !paths.index_path.exists() {
        let index = IndexData::new(&paths.namespace);
        save_index(paths, &index)?;
        return Ok(index);
    }

    let text = fs::read_to_string(&paths.index_path)
        .map_err(|e| format!("read index.json failed: {e}"))?;
    let mut index: IndexData =
        serde_json::from_str(&text).map_err(|e| format!("parse index.json failed: {e}"))?;

    if index.version != 1 {
        index = IndexData::new(&paths.namespace);
        save_index(paths, &index)?;
        return Ok(index);
    }

    if index.namespace.trim() != paths.namespace {
        index.namespace = paths.namespace.clone();
    }

    Ok(index)
}

fn save_index(paths: &StorePaths, index: &IndexData) -> Result<(), String> {
    let json = serde_json::to_string_pretty(index)
        .map_err(|e| format!("serialize index.json failed: {e}"))?;

    let tmp = paths.index_path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("write index tmp failed: {e}"))?;

    // Windows rename 不允许覆盖；做 best-effort 替换。
    if let Err(e) = fs::rename(&tmp, &paths.index_path) {
        let _ = fs::remove_file(&paths.index_path);
        fs::rename(&tmp, &paths.index_path)
            .map_err(|_| format!("replace index.json failed: {e}"))?;
    }

    Ok(())
}

fn incremental_index(memories_path: &Path, index: &mut IndexData) -> io::Result<()> {
    let mut file = File::open(memories_path)?;
    let start = index.indexed_up_to_offset;
    file.seek(SeekFrom::Start(start))?;

    let mut reader = BufReader::new(file);
    let mut offset = start;
    let mut buf: Vec<u8> = Vec::new();

    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }

        let length = n as u32;
        let line = buf
            .strip_suffix(b"\r\n")
            .or_else(|| buf.strip_suffix(b"\n"))
            .unwrap_or(&buf);

        if let Ok(item) = serde_json::from_slice::<MemoryItem>(line) {
            let recorded_ts = time::parse_time_to_ts_and_canonical(&item.recorded_at, DateBoundKind::Start)
                .map(|x| x.0)
                .unwrap_or(0);
            let occurred_ts = item
                .occurred_at
                .as_deref()
                .and_then(|s| time::parse_time_to_ts_and_canonical(s, DateBoundKind::Start).ok())
                .map(|x| x.0);

            let keywords = normalize_keywords(item.keywords.clone());
            index.add_memory_item(&item, offset, length, recorded_ts, occurred_ts, keywords);
        }

        offset += length as u64;
    }

    index.indexed_up_to_offset = offset;
    Ok(())
}

fn load_item_by_index(memories_path: &Path, index: &IndexData, idx: u32) -> Result<MemoryItem, String> {
    let Some(entry) = index.items.get(idx as usize) else {
        return Err("索引越界".to_string());
    };

    let mut file = File::open(memories_path).map_err(|e| format!("open memories.jsonl failed: {e}"))?;
    file.seek(SeekFrom::Start(entry.offset))
        .map_err(|e| format!("seek memories.jsonl failed: {e}"))?;

    let mut buf = vec![0u8; entry.length as usize];
    file.read_exact(&mut buf)
        .map_err(|e| format!("read memories.jsonl failed: {e}"))?;

    let line = buf
        .strip_suffix(b"\r\n")
        .or_else(|| buf.strip_suffix(b"\n"))
        .unwrap_or(&buf);

    serde_json::from_slice::<MemoryItem>(line).map_err(|e| format!("parse memory item failed: {e}"))
}

#[cfg(test)]
mod tests;
