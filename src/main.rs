mod cli;
mod mcp;
mod memory;

use std::io::{self, BufRead, Write};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let root_dir = memory::resolve_root_dir();

    // 仅当包含 --cli 时，才按 CLI 一键调用模式解析参数；否则始终按 MCP stdio server 运行。
    if argv.iter().skip(1).any(|x| x == "--cli") {
        let mut cli_argv: Vec<String> = Vec::with_capacity(argv.len());
        if let Some(first) = argv.first() {
            cli_argv.push(first.clone());
        }
        for a in argv.iter().skip(1) {
            if a == "--cli" {
                continue;
            }
            cli_argv.push(a.clone());
        }

        let code = cli::run_one_shot(root_dir, cli_argv);
        std::process::exit(code);
    }

    let mut engine = memory::MemoryEngine::new(root_dir);

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let Ok(text) = line else { break };

        match mcp::handle_stdin_line(&mut engine, &text) {
            Ok(Some(response_json_line)) => {
                if stdout.write_all(response_json_line.as_bytes()).is_ok()
                    && stdout.write_all(b"\n").is_ok()
                {
                    let _ = stdout.flush();
                }
            }
            Ok(None) => {}
            Err(_err) => {
                // 兜底：避免 stderr 输出污染 MCP stdout 协议通道；因此这里静默丢弃错误。
            }
        }
    }
}
