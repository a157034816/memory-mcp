use crate::memory::model::MemoryItem;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexItem {
    pub id: String,
    pub offset: u64,
    pub length: u32,
    pub recorded_at_ts: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at_ts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance: Option<u8>,
    pub keywords: Vec<String>,
}

impl IndexItem {
    pub fn time_key_ts(&self) -> i64 {
        self.occurred_at_ts.unwrap_or(self.recorded_at_ts)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexData {
    pub version: u32,
    pub namespace: String,
    pub memories_file: String,
    pub indexed_up_to_offset: u64,

    pub items: Vec<IndexItem>,

    pub keyword_postings: HashMap<String, Vec<u32>>,
    pub time_sorted: Vec<u32>,
    pub time_sorted_dirty: bool,
}

impl IndexData {
    pub fn new(namespace: &str) -> Self {
        Self {
            version: 1,
            namespace: namespace.to_string(),
            memories_file: "memories.jsonl".to_string(),
            indexed_up_to_offset: 0,
            items: Vec::new(),
            keyword_postings: HashMap::new(),
            time_sorted: Vec::new(),
            time_sorted_dirty: false,
        }
    }

    pub fn add_memory_item(
        &mut self,
        item: &MemoryItem,
        offset: u64,
        length: u32,
        recorded_at_ts: i64,
        occurred_at_ts: Option<i64>,
        keywords: Vec<String>,
    ) {
        let idx = self.items.len() as u32;

        self.items.push(IndexItem {
            id: item.id.clone(),
            offset,
            length,
            recorded_at_ts,
            occurred_at_ts,
            importance: item.importance,
            keywords: keywords.clone(),
        });

        for kw in keywords {
            self.keyword_postings.entry(kw).or_default().push(idx);
        }

        self.time_sorted.push(idx);
        self.time_sorted_dirty = true;
    }

    pub fn ensure_time_sorted(&mut self) {
        if !self.time_sorted_dirty {
            return;
        }

        let items = &self.items;
        self.time_sorted.sort_by_key(|idx| {
            let i = *idx as usize;
            items.get(i).map(|x| x.time_key_ts()).unwrap_or(0)
        });
        self.time_sorted_dirty = false;
    }
}
