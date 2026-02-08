use crate::memory::{MemoryEngine, RecallArgs, RememberArgs};
use serde_json::{json, Value};

pub fn handle_stdin_line(engine: &mut MemoryEngine, line: &str) -> Result<Option<String>, String> {
    let text = line.trim();
    if text.is_empty() {
        return Ok(None);
    }

    let message: Value = serde_json::from_str(text).map_err(|e| format!("invalid json: {e}"))?;
    let response = handle_message(engine, &message)?;
    Ok(response.map(|v| v.to_string()))
}

fn handle_message(engine: &mut MemoryEngine, message: &Value) -> Result<Option<Value>, String> {
    let id = message.get("id").and_then(|x| x.as_i64());
    let method = message
        .get("method")
        .and_then(|x| x.as_str())
        .unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => handle_initialize(id, &params),
        "initialized" => Ok(None),
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(engine, id, &params),
        _ => Ok(id.map(|id| {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("method not found: {method}") }
            })
        })),
    }
}

fn handle_initialize(id: Option<i64>, params: &Value) -> Result<Option<Value>, String> {
    let requested = params
        .get("protocolVersion")
        .and_then(|x| x.as_str())
        .unwrap_or("2025-06-18");

    let supported = match requested {
        "2025-06-18" | "2024-11-05" => requested,
        _ => "2025-06-18",
    };

    Ok(id.map(|id| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": supported,
                "serverInfo": { "name": "Memory", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": {}
            }
        })
    }))
}

fn handle_tools_list(id: Option<i64>) -> Result<Option<Value>, String> {
    Ok(id.map(|id| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "now",
                        "description": "获取当前时间（本地 + UTC），用于需要准确日期时间的回答/计算。",
                        "inputSchema": now_schema()
                    },
                    {
                        "name": "keywords_list",
                        "description": "列出指定 namespace 下已存在的关键字（已归一化为小写，用于复用短关键字）。",
                        "inputSchema": keywords_list_schema()
                    },
                    {
                        "name": "keywords_list_global",
                        "description": "列出全局已存在的关键字（跨 namespace 汇总；关键字已归一化为小写）。",
                        "inputSchema": keywords_list_global_schema()
                    },
                    {
                        "name": "remember",
                        "description": "记录一条长期记忆（关键字会归一化为小写；时间类关键字会被忽略 + 内容切片 + AI 日记），用于后续检索。",
                        "inputSchema": remember_schema()
                    },
                    {
                        "name": "recall",
                        "description": "按关键字/时间范围检索记忆，并返回最相关的若干条。",
                        "inputSchema": recall_schema()
                    }
                ]
            }
        })
    }))
}

fn handle_tools_call(engine: &mut MemoryEngine, id: Option<i64>, params: &Value) -> Result<Option<Value>, String> {
    let Some(id) = id else {
        return Ok(None);
    };

    let tool_name = params.get("name").and_then(|x| x.as_str()).unwrap_or_default();
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    let result = match tool_name {
        "now" => engine.now()?,
        "keywords_list" => {
            let namespace = get_required_string(&args, "namespace")?;
            engine.keywords_list(namespace)?
        }
        "keywords_list_global" => engine.keywords_list_global()?,
        "remember" => {
            let parsed = RememberArgs::from_json(&args)?;
            engine.remember(parsed)?
        }
        "recall" => {
            let parsed = RecallArgs::from_json(&args)?;
            engine.recall(parsed)?
        }
        _ => {
            return Ok(Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("unknown tool: {tool_name}") }
            })));
        }
    };

    Ok(Some(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })))
}

fn now_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {}
    })
}

fn keywords_list_global_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {}
    })
}

fn keywords_list_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["namespace"],
        "properties": {
            "namespace": {
                "type": "string",
                "minLength": 1,
                "description": "命名空间：必须为 {userId}/{projectId}（严格两段；会做分隔符归一化与路径净化）。"
            }
        }
    })
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

fn remember_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["namespace", "keywords", "slice", "diary"],
        "properties": {
            "namespace": {
                "type": "string",
                "description": "命名空间：必须为 {userId}/{projectId}（严格两段），用于隔离不同用户/项目的记忆；会做分隔符归一化与路径净化。"
            },
            "keywords": {
                "type": "array",
                "minItems": 1,
                "items": { "type": "string" },
                "description": "关键字列表（至少 1 个，建议 2~8 个；会做 trim+lowercase 并去重；时间类关键字会被忽略）。"
            },
            "slice": {
                "type": "string",
                "description": "重要内容切片（短文本，可展示/可检索）。"
            },
            "diary": {
                "type": "string",
                "description": "AI 日记（第一人称长文本，默认 recall 不返回）。"
            },
            "occurred_at": {
                "type": "string",
                "description": "事件发生时间（RFC3339 或 YYYY-MM-DD）。"
            },
            "importance": {
                "type": "integer",
                "minimum": 1,
                "maximum": 5,
                "description": "重要度 1~5。"
            },
            "source": {
                "type": "string",
                "description": "来源信息（可选，例如会话/模块/页面）。"
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn tools_list_should_include_keywords_tools() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        let out = handle_stdin_line(
            &mut engine,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#,
        )
        .expect("handle")
        .expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");

        let tools = v["result"]["tools"].as_array().expect("tools array");
        let names: HashSet<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|x| x.as_str()))
            .collect();
        for name in [
            "now",
            "keywords_list",
            "keywords_list_global",
            "remember",
            "recall",
        ] {
            assert!(names.contains(name), "missing tool: {name}");
        }
    }

    #[test]
    fn tools_call_now_should_return_time_fields() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        let out = handle_stdin_line(
            &mut engine,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"now","arguments":{}}}"#,
        )
        .expect("handle")
        .expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");

        let data = &v["result"]["data"];
        assert!(data.get("utc_rfc3339").and_then(|x| x.as_str()).is_some());
        assert!(data.get("utc_ts").and_then(|x| x.as_i64()).is_some());
        assert!(data
            .get("local_rfc3339")
            .and_then(|x| x.as_str())
            .is_some());
        assert!(data
            .get("local_offset_seconds")
            .and_then(|x| x.as_i64())
            .is_some());
        assert!(data
            .get("local_offset_minutes")
            .and_then(|x| x.as_i64())
            .is_some());
    }

    #[test]
    fn tools_call_keywords_list_should_work() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        let remember = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "remember",
                "arguments": {
                    "namespace": "u1/p1",
                    "keywords": ["ERP", "项目"],
                    "slice": "slice",
                    "diary": "diary"
                }
            }
        })
        .to_string();
        let _ = handle_stdin_line(&mut engine, &remember)
            .expect("handle")
            .expect("response");

        let list = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "keywords_list",
                "arguments": { "namespace": "u1/p1" }
            }
        })
        .to_string();
        let out = handle_stdin_line(&mut engine, &list)
            .expect("handle")
            .expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        let keywords = v["result"]["data"]["keywords"].as_array().expect("keywords");
        assert_eq!(keywords[0].as_str().unwrap(), "项目");
        assert_eq!(keywords[1].as_str().unwrap(), "erp");
    }

    #[test]
    fn tools_call_keywords_list_should_work_with_noncanonical_namespace() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        let remember = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "remember",
                "arguments": {
                    "namespace": "u1\\p1//",
                    "keywords": ["ERP", "项目"],
                    "slice": "slice",
                    "diary": "diary"
                }
            }
        })
        .to_string();
        let _ = handle_stdin_line(&mut engine, &remember)
            .expect("handle")
            .expect("response");

        let list = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "keywords_list",
                "arguments": { "namespace": "u1/p1" }
            }
        })
        .to_string();
        let out = handle_stdin_line(&mut engine, &list)
            .expect("handle")
            .expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(v["result"]["data"]["namespace"].as_str().unwrap(), "u1/p1");

        let keywords = v["result"]["data"]["keywords"].as_array().expect("keywords");
        assert_eq!(keywords[0].as_str().unwrap(), "项目");
        assert_eq!(keywords[1].as_str().unwrap(), "erp");
    }

    #[test]
    fn tools_call_keywords_list_global_should_include_keywords() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        let remember = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "remember",
                "arguments": {
                    "namespace": "u1/p1",
                    "keywords": ["ERP", "项目"],
                    "slice": "slice",
                    "diary": "diary"
                }
            }
        })
        .to_string();
        let _ = handle_stdin_line(&mut engine, &remember)
            .expect("handle")
            .expect("response");

        let list_global = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "keywords_list_global", "arguments": {} }
        })
        .to_string();
        let out = handle_stdin_line(&mut engine, &list_global)
            .expect("handle")
            .expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");

        assert_eq!(
            v["result"]["data"]["scanned_namespaces"].as_u64().unwrap(),
            1
        );

        let kws = v["result"]["data"]["keywords"].as_array().expect("keywords");
        assert!(kws.iter().any(|x| x.get("keyword").and_then(|v| v.as_str()) == Some("项目")));
        assert!(kws.iter().any(|x| x.get("keyword").and_then(|v| v.as_str()) == Some("erp")));
    }

    #[test]
    fn tools_call_recall_should_include_matched_keywords_when_keywords_provided() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        let remember = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "remember",
                "arguments": {
                    "namespace": "u1/p1",
                    "keywords": ["ERP", "项目"],
                    "slice": "slice",
                    "diary": "diary"
                }
            }
        })
        .to_string();
        let _ = handle_stdin_line(&mut engine, &remember)
            .expect("handle")
            .expect("response");

        let recall = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "recall",
                "arguments": {
                    "namespace": "u1/p1",
                    "keywords": ["ERP", "项目"],
                    "limit": 10
                }
            }
        })
        .to_string();
        let out = handle_stdin_line(&mut engine, &recall)
            .expect("handle")
            .expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        let items = v["result"]["data"]["items"].as_array().expect("items");
        assert_eq!(items.len(), 1);
        let mk = items[0]
            .get("matched_keywords")
            .and_then(|x| x.as_array())
            .expect("matched_keywords");
        assert_eq!(mk[0].as_str().unwrap(), "项目");
        assert_eq!(mk[1].as_str().unwrap(), "erp");
    }

    #[test]
    fn tools_call_recall_without_keywords_should_not_return_matched_keywords() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        let remember = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "remember",
                "arguments": {
                    "namespace": "u1/p1",
                    "keywords": ["ERP", "项目"],
                    "slice": "slice",
                    "diary": "diary"
                }
            }
        })
        .to_string();
        let _ = handle_stdin_line(&mut engine, &remember)
            .expect("handle")
            .expect("response");

        let recall = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "recall",
                "arguments": { "namespace": "u1/p1", "limit": 10 }
            }
        })
        .to_string();
        let out = handle_stdin_line(&mut engine, &recall)
            .expect("handle")
            .expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        let items = v["result"]["data"]["items"].as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert!(items[0].get("matched_keywords").is_none());
    }

    #[test]
    fn tools_call_remember_importance_out_of_range_should_error() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        let remember = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "remember",
                "arguments": {
                    "namespace": "u1/p1",
                    "keywords": ["项目"],
                    "slice": "slice",
                    "diary": "diary",
                    "importance": 6
                }
            }
        })
        .to_string();

        let err = handle_stdin_line(&mut engine, &remember)
            .err()
            .expect("should error");
        assert!(err.contains("importance"), "unexpected err: {err}");
    }

    #[test]
    fn tools_call_recall_should_support_query_time_expr() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        for (id, slice, occurred_at) in [
            (1, "older", "2025-04-01"),
            (2, "newer", "2025-05-01"),
        ] {
            let remember = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {
                    "name": "remember",
                    "arguments": {
                        "namespace": "u1/p1",
                        "keywords": ["k"],
                        "slice": slice,
                        "diary": "diary",
                        "occurred_at": occurred_at
                    }
                }
            })
            .to_string();
            let _ = handle_stdin_line(&mut engine, &remember)
                .expect("handle")
                .expect("response");
        }

        let recall = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "recall",
                "arguments": {
                    "namespace": "u1/p1",
                    "query": "time>=2025-05-01",
                    "limit": 10
                }
            }
        })
        .to_string();
        let out = handle_stdin_line(&mut engine, &recall)
            .expect("handle")
            .expect("response");
        let v: Value = serde_json::from_str(&out).expect("json");
        let items = v["result"]["data"]["items"].as_array().expect("items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["slice"].as_str().unwrap(), "newer");
    }
}

fn recall_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["namespace"],
        "properties": {
            "namespace": {
                "type": "string",
                "description": "命名空间：必须为 {userId}/{projectId}（严格两段；会做分隔符归一化与路径净化）。"
            },
            "keywords": {
                "type": "array",
                "items": { "type": "string" },
                "description": "关键字列表（可选）。"
            },
            "start": {
                "type": "string",
                "description": "起始时间（RFC3339 或 YYYY-MM-DD）。"
            },
            "end": {
                "type": "string",
                "description": "结束时间（RFC3339 或 YYYY-MM-DD）。"
            },
            "query": {
                "type": "string",
                "description": "自由文本查询（可选，包含匹配 slice/diary/source；支持 time>=... / time<=... / time=a..b 时间表达式）。"
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "default": 20
            },
            "include_diary": {
                "type": "boolean",
                "default": false,
                "description": "是否返回 diary 字段（默认 false）。"
            }
        }
    })
}
