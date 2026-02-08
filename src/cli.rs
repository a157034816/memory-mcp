use crate::memory::{MemoryEngine, RecallArgs, RememberArgs};
use clap::{Args, CommandFactory, Parser, Subcommand};
use serde_json::Value;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "memory",
    version,
    about = "Memory MCP 记忆服务器（stdio）/ CLI 一键调用工具",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 记录一条长期记忆（关键字 + 内容切片 + AI 日记）
    Remember(RememberCommand),

    /// 按关键字/时间范围检索记忆
    Recall(RecallCommand),

    /// 获取当前时间（本地 + UTC）
    Now(NowCommand),

    /// 关键字管理（列出）
    Keywords(KeywordsCommand),
}

#[derive(Args, Debug)]
pub struct RememberCommand {
    #[arg(long)]
    pub namespace: String,

    /// 关键字（可重复；至少 1 个）
    #[arg(long = "keyword", short = 'k', required = true, num_args = 1..)]
    pub keywords: Vec<String>,

    #[arg(long, required_unless_present = "slice_file", conflicts_with = "slice_file")]
    pub slice: Option<String>,

    #[arg(
        long = "slice-file",
        value_name = "PATH",
        required_unless_present = "slice",
        conflicts_with = "slice"
    )]
    pub slice_file: Option<PathBuf>,

    #[arg(long, required_unless_present = "diary_file", conflicts_with = "diary_file")]
    pub diary: Option<String>,

    #[arg(
        long = "diary-file",
        value_name = "PATH",
        required_unless_present = "diary",
        conflicts_with = "diary"
    )]
    pub diary_file: Option<PathBuf>,

    #[arg(long = "occurred-at")]
    pub occurred_at: Option<String>,

    #[arg(long)]
    pub importance: Option<u8>,

    #[arg(long)]
    pub source: Option<String>,

    /// 输出 JSON（Pretty）
    #[arg(long)]
    pub pretty: bool,

    /// 输出文本摘要（如果同时提供 --pretty，则以 --text 为准）
    #[arg(long)]
    pub text: bool,
}

#[derive(Args, Debug)]
pub struct RecallCommand {
    #[arg(long)]
    pub namespace: String,

    /// 关键字（可重复；不提供则按时间倒序召回）
    #[arg(long = "keyword", short = 'k')]
    pub keywords: Vec<String>,

    #[arg(long)]
    pub start: Option<String>,

    #[arg(long)]
    pub end: Option<String>,

    #[arg(long)]
    pub query: Option<String>,

    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    #[arg(long = "include-diary")]
    pub include_diary: bool,

    /// 输出 JSON（Pretty）
    #[arg(long)]
    pub pretty: bool,

    /// 输出文本摘要（如果同时提供 --pretty，则以 --text 为准）
    #[arg(long)]
    pub text: bool,
}

#[derive(Args, Debug)]
pub struct NowCommand {
    /// 输出 JSON（Pretty）
    #[arg(long)]
    pub pretty: bool,

    /// 输出文本摘要（如果同时提供 --pretty，则以 --text 为准）
    #[arg(long)]
    pub text: bool,
}

#[derive(Args, Debug)]
pub struct KeywordsCommand {
    #[command(subcommand)]
    pub command: KeywordsSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum KeywordsSubcommand {
    /// 列出指定 namespace 下已存在的关键字
    List(KeywordsListCommand),

    /// 列出全局已存在的关键字（跨 namespace 汇总）
    ListGlobal(KeywordsListGlobalCommand),
}

#[derive(Args, Debug)]
pub struct KeywordsListCommand {
    #[arg(long)]
    pub namespace: String,

    /// 输出 JSON（Pretty）
    #[arg(long)]
    pub pretty: bool,

    /// 输出文本摘要（如果同时提供 --pretty，则以 --text 为准）
    #[arg(long)]
    pub text: bool,
}

#[derive(Args, Debug)]
pub struct KeywordsListGlobalCommand {
    /// 输出 JSON（Pretty）
    #[arg(long)]
    pub pretty: bool,

    /// 输出文本摘要（如果同时提供 --pretty，则以 --text 为准）
    #[arg(long)]
    pub text: bool,
}

impl RememberCommand {
    fn into_args(self) -> Result<RememberArgs, String> {
        if let Some(n) = self.importance {
            if !(1..=5).contains(&n) {
                return Err("importance 必须在 1~5".to_string());
            }
        }

        let slice = resolve_inline_or_file("slice", self.slice, self.slice_file)?;
        let diary = resolve_inline_or_file("diary", self.diary, self.diary_file)?;

        Ok(RememberArgs {
            namespace: self.namespace,
            keywords: self.keywords,
            slice,
            diary,
            occurred_at: self.occurred_at,
            importance: self.importance,
            source: self.source,
        })
    }
}

impl RecallCommand {
    fn into_args(self) -> RecallArgs {
        let mut limit = self.limit;
        if limit == 0 {
            limit = 20;
        }
        if limit > 100 {
            limit = 100;
        }

        RecallArgs {
            namespace: self.namespace,
            keywords: self.keywords,
            start: self.start,
            end: self.end,
            query: self.query,
            limit,
            include_diary: self.include_diary,
        }
    }
}

pub fn run_one_shot(root_dir: PathBuf, argv: Vec<String>) -> i32 {
    let cli = match Cli::try_parse_from(&argv) {
        Ok(v) => v,
        Err(e) => {
            let code = e.exit_code();
            let _ = e.print();
            return code;
        }
    };

    let Some(cmd) = cli.command else {
        let mut c = Cli::command();
        let _ = c.print_help();
        let _ = io::stdout().write_all(b"\n");
        return 2;
    };

    match cmd {
        Command::Remember(cmd) => run_remember(root_dir, cmd),
        Command::Recall(cmd) => run_recall(root_dir, cmd),
        Command::Now(cmd) => run_now(root_dir, cmd),
        Command::Keywords(cmd) => run_keywords(root_dir, cmd),
    }
}

fn run_remember(root_dir: PathBuf, cmd: RememberCommand) -> i32 {
    let prefer_text = cmd.text;
    let pretty = cmd.pretty && !prefer_text;

    let args = match cmd.into_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let mut engine = MemoryEngine::new(root_dir);
    let result = match engine.remember(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    match format_tool_result(&result, prefer_text, pretty) {
        Ok(text) => {
            print!("{text}\n");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn run_recall(root_dir: PathBuf, cmd: RecallCommand) -> i32 {
    let prefer_text = cmd.text;
    let pretty = cmd.pretty && !prefer_text;

    let args = cmd.into_args();

    let mut engine = MemoryEngine::new(root_dir);
    let result = match engine.recall(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    match format_tool_result(&result, prefer_text, pretty) {
        Ok(text) => {
            print!("{text}\n");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn run_now(root_dir: PathBuf, cmd: NowCommand) -> i32 {
    let prefer_text = cmd.text;
    let pretty = cmd.pretty && !prefer_text;

    let engine = MemoryEngine::new(root_dir);
    let result = match engine.now() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    match format_tool_result(&result, prefer_text, pretty) {
        Ok(text) => {
            print!("{text}\n");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn run_keywords(root_dir: PathBuf, cmd: KeywordsCommand) -> i32 {
    match cmd.command {
        KeywordsSubcommand::List(cmd) => run_keywords_list(root_dir, cmd),
        KeywordsSubcommand::ListGlobal(cmd) => run_keywords_list_global(root_dir, cmd),
    }
}

fn run_keywords_list(root_dir: PathBuf, cmd: KeywordsListCommand) -> i32 {
    let prefer_text = cmd.text;
    let pretty = cmd.pretty && !prefer_text;

    let mut engine = MemoryEngine::new(root_dir);
    let result = match engine.keywords_list(cmd.namespace) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    match format_tool_result(&result, prefer_text, pretty) {
        Ok(text) => {
            print!("{text}\n");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn run_keywords_list_global(root_dir: PathBuf, cmd: KeywordsListGlobalCommand) -> i32 {
    let prefer_text = cmd.text;
    let pretty = cmd.pretty && !prefer_text;

    let engine = MemoryEngine::new(root_dir);
    let result = match engine.keywords_list_global() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    match format_tool_result(&result, prefer_text, pretty) {
        Ok(text) => {
            print!("{text}\n");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn format_tool_result(result: &Value, prefer_text: bool, pretty: bool) -> Result<String, String> {
    if prefer_text {
        if let Some(text) = extract_primary_text(result) {
            return Ok(text);
        }
    }

    format_json(result, pretty)
}

fn format_json(v: &Value, pretty: bool) -> Result<String, String> {
    if pretty {
        serde_json::to_string_pretty(v).map_err(|e| format!("输出 JSON 失败：{e}"))
    } else {
        Ok(v.to_string())
    }
}

fn extract_primary_text(result: &Value) -> Option<String> {
    let content = result.get("content")?.as_array()?;
    for item in content {
        if let Some(text) = item.get("text").and_then(|x| x.as_str()) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn resolve_inline_or_file(
    name: &str,
    inline: Option<String>,
    file: Option<PathBuf>,
) -> Result<String, String> {
    if let Some(v) = inline {
        return Ok(v);
    }

    let Some(path) = file else {
        return Err(format!("{name} 不能为空"));
    };

    read_utf8_file_strip_bom(&path)
        .map_err(|e| format!("读取 {name} 失败：{e}"))
}

fn read_utf8_file_strip_bom(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = strip_utf8_bom(&bytes);

    String::from_utf8(bytes.to_vec()).map_err(|e| format!("{}: {e}", path.display()))
}

fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
    if bytes.starts_with(BOM) {
        &bytes[BOM.len()..]
    } else {
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cli_parse_remember_missing_diary_should_error() {
        let args = [
            "memory",
            "remember",
            "--namespace",
            "u1/p1",
            "--keyword",
            "项目",
            "--slice",
            "slice",
        ];
        assert!(Cli::try_parse_from(args).is_err());
    }

    #[test]
    fn cli_parse_now_should_work() {
        let args = ["memory", "now"];
        assert!(Cli::try_parse_from(args).is_ok());
    }

    #[test]
    fn cli_parse_keywords_list_should_work() {
        let args = ["memory", "keywords", "list", "--namespace", "u1/p1"];
        assert!(Cli::try_parse_from(args).is_ok());
    }

    #[test]
    fn cli_parse_keywords_list_global_should_work() {
        let args = ["memory", "keywords", "list-global"];
        assert!(Cli::try_parse_from(args).is_ok());
    }

    #[test]
    fn read_utf8_file_strip_bom_should_work() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let p = dir.path().join("a.txt");

        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"hello");

        fs::write(&p, bytes).expect("write file");

        let text = read_utf8_file_strip_bom(&p).expect("read");
        assert_eq!(text, "hello");
    }

    #[test]
    fn remember_command_file_inputs_should_load_text() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let slice_path = dir.path().join("slice.txt");
        let diary_path = dir.path().join("diary.txt");

        fs::write(&slice_path, "slice").expect("write slice");
        fs::write(&diary_path, "diary").expect("write diary");

        let cmd = RememberCommand {
            namespace: "u1/p1".to_string(),
            keywords: vec!["项目".to_string()],
            slice: None,
            slice_file: Some(slice_path),
            diary: None,
            diary_file: Some(diary_path),
            occurred_at: Some("2025-01-02".to_string()),
            importance: Some(3),
            source: Some("test".to_string()),
            pretty: false,
            text: false,
        };

        let args = cmd.into_args().expect("into args");
        assert_eq!(args.slice, "slice");
        assert_eq!(args.diary, "diary");
        assert_eq!(args.importance, Some(3));
    }

    #[test]
    fn extract_primary_text_should_find_summary() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        let mut engine = MemoryEngine::new(dir.path().to_path_buf());

        let _ = engine
            .remember(RememberArgs {
                namespace: "u1/p1".to_string(),
                keywords: vec!["项目".to_string()],
                slice: "我们做过 A 项目".to_string(),
                diary: "diary".to_string(),
                occurred_at: None,
                importance: None,
                source: None,
            })
            .expect("remember");

        let out = engine
            .recall(RecallArgs {
                namespace: "u1/p1".to_string(),
                keywords: vec!["项目".to_string()],
                start: None,
                end: None,
                query: None,
                limit: 20,
                include_diary: false,
            })
            .expect("recall");

        let text = extract_primary_text(&out).expect("text");
        assert!(text.contains("命中"));
    }
}
