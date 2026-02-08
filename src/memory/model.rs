use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub namespace: String,
    pub recorded_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
    pub keywords: Vec<String>,
    pub slice: String,
    pub diary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RememberArgs {
    pub namespace: String,
    pub keywords: Vec<String>,
    pub slice: String,
    pub diary: String,
    pub occurred_at: Option<String>,
    pub importance: Option<u8>,
    pub source: Option<String>,
}

impl RememberArgs {
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let namespace = get_required_string(v, "namespace")?;
        let keywords = get_string_array(v, "keywords")?;
        let slice = get_required_string(v, "slice")?;
        let diary = get_required_string(v, "diary")?;

        let occurred_at = get_optional_string(v, "occurred_at")?;
        let importance = get_optional_u8(v, "importance")?;
        let source = get_optional_string(v, "source")?;

        Ok(Self {
            namespace,
            keywords,
            slice,
            diary,
            occurred_at,
            importance,
            source,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RecallArgs {
    pub namespace: String,
    pub keywords: Vec<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub query: Option<String>,
    pub limit: usize,
    pub include_diary: bool,
}

impl RecallArgs {
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let namespace = get_required_string(v, "namespace")?;
        let keywords = get_optional_string_array(v, "keywords")?.unwrap_or_default();
        let start = get_optional_string(v, "start")?;
        let end = get_optional_string(v, "end")?;
        let query = get_optional_string(v, "query")?;

        let mut limit = get_optional_usize(v, "limit")?.unwrap_or(20);
        if limit == 0 {
            limit = 20;
        }
        if limit > 100 {
            limit = 100;
        }

        let include_diary = v
            .get("include_diary")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);

        Ok(Self {
            namespace,
            keywords,
            start,
            end,
            query,
            limit,
            include_diary,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RecallItemOut {
    pub id: String,
    pub recorded_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
    pub keywords: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_keywords: Option<Vec<String>>,
    pub slice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecallResult {
    pub total: usize,
    pub items: Vec<RecallItemOut>,
}

impl RecallResult {
    pub fn render_text_summary(&self) -> String {
        if self.items.is_empty() {
            return "未命中记忆。".to_string();
        }

        let mut lines = Vec::with_capacity(self.items.len() + 1);
        lines.push(format!("命中 {} 条记忆：", self.items.len()));

        for (i, item) in self.items.iter().enumerate() {
            let t = item.occurred_at.as_deref().unwrap_or(&item.recorded_at);
            let kws = if item.keywords.is_empty() {
                String::new()
            } else {
                format!(" keywords={}", item.keywords.join(","))
            };
            lines.push(format!(
                "{}. [{}]{} id={} slice={}",
                i + 1,
                t,
                kws,
                item.id,
                truncate_one_line(&item.slice, 120)
            ));
        }

        lines.join("\n")
    }
}

fn truncate_one_line(text: &str, max_len: usize) -> String {
    let s = text.replace('\n', " ").replace('\r', " ").trim().to_string();
    if s.chars().count() <= max_len {
        return s;
    }
    let mut out = String::with_capacity(max_len + 1);
    for (i, ch) in s.chars().enumerate() {
        if i >= max_len {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn get_required_string(v: &Value, key: &str) -> Result<String, String> {
    let Some(s) = v.get(key).and_then(|x| x.as_str()) else {
        return Err(format!("{key} 不能为空"));
    };
    let s = s.trim().to_string();
    if s.is_empty() {
        return Err(format!("{key} 不能为空"));
    }
    Ok(s)
}

fn get_optional_string(v: &Value, key: &str) -> Result<Option<String>, String> {
    Ok(v.get(key)
        .and_then(|x| x.as_str())
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty()))
}

fn get_string_array(v: &Value, key: &str) -> Result<Vec<String>, String> {
    let Some(arr) = v.get(key).and_then(|x| x.as_array()) else {
        return Err(format!("{key} 必须是字符串数组"));
    };
    Ok(arr
        .iter()
        .filter_map(|x| x.as_str())
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect())
}

fn get_optional_string_array(v: &Value, key: &str) -> Result<Option<Vec<String>>, String> {
    let Some(value) = v.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(get_string_array(v, key)?))
}

fn get_optional_u8(v: &Value, key: &str) -> Result<Option<u8>, String> {
    let Some(value) = v.get(key) else {
        return Ok(None);
    };

    if let Some(n) = value.as_u64() {
        return Ok(Some(n.min(u8::MAX as u64) as u8));
    }

    Ok(None)
}

fn get_optional_usize(v: &Value, key: &str) -> Result<Option<usize>, String> {
    let Some(value) = v.get(key) else {
        return Ok(None);
    };

    if let Some(n) = value.as_u64() {
        return Ok(Some(n as usize));
    }

    Ok(None)
}
