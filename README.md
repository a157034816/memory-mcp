# Memory（MCP 记忆服务器）

Memory 是一个 MCP stdio server（Rust），用于为 AI 调用方提供“长期记忆”能力。

## 能力

- `now`：获取当前时间（本地 + UTC）。
- `keywords_list`：列出指定 namespace 下已存在的关键字（用于复用短关键字）。
- `keywords_list_global`：列出全局已存在的关键字（跨 namespace 汇总）。
- `remember`：记录记忆（关键字 + 重要内容切片 + AI 日记）。
- `recall`：按关键字与时间范围检索记忆，并返回最相关的若干条。

> 说明：Memory 只负责“存取与检索”。
> - `userId/projectId`（即 namespace 的组成）由调用方从项目上下文中获取后传入。
> - 何时记、如何提取关键字/时间范围由提示词与调用方策略决定。

## 与 MCP Client 集成（npx，推荐）

如果你的 MCP Client 支持以 `command + args` 启动 stdio server，推荐使用 `npx` 直接运行已发布的 npm 包（会按平台自动安装对应二进制）：

```json
{
  "mcpServers": {
    "Memory": {
      "command": "npx",
      "args": ["-y", "@a157034816/memory-mcp"],
      "env": {
        "MEMORY_STORE_DIR": "C:\\path\\to\\MemoryStore"
      }
    }
  }
}
```

说明：

- 需要 Node.js（含 npx），建议 18+。
- Windows 下部分客户端可能需要将 `command` 写为 `npx.cmd`（取决于其进程启动方式与 PATH/PATHEXT 处理）。
- `-y` 用于非交互环境自动确认安装（很多客户端不会给你输入确认的机会）。
- `MEMORY_STORE_DIR` 可选；不设置会使用系统用户数据目录。
- 调试/脚本：可以把参数透传给二进制，例如：`npx -y @a157034816/memory-mcp -- --cli recall --namespace "u1/p1" --keyword 项目 --text`

## 与 MCP Client 集成（直接二进制）

也可以直接指定本机的可执行文件路径：

```json
{
  "mcpServers": {
    "Memory": {
      "command": "C:\\path\\to\\memory.exe",
      "args": [],
      "env": {
        "MEMORY_STORE_DIR": "C:\\path\\to\\MemoryStore"
      }
    }
  }
}
```

## Tool 参数

### now

无入参。

返回：

- `data.utc_rfc3339`: `string`（UTC，RFC3339，秒级）
- `data.utc_ts`: `integer`（UTC，Unix 时间戳秒）
- `data.local_rfc3339`: `string`（本地时区，RFC3339，秒级）
- `data.local_offset_seconds`: `integer`（本地时区偏移秒，local - utc）
- `data.local_offset_minutes`: `integer`（本地时区偏移分钟，local - utc）

### keywords_list

必填：

- `namespace`: `string`

返回：

- `data.namespace`: `string`
- `data.total`: `integer`
- `data.keywords`: `string[]`（已归一化：trim + lowercase；排序：长度优先）

### keywords_list_global

无入参。

返回：

- `data.total`: `integer`
- `data.scanned_namespaces`: `integer`（扫描到的 namespace 数）
- `data.keywords`: `{ keyword: string, namespaces: integer, items: integer }[]`

### remember

必填：

- `namespace`: `string`（建议 `{userId}/{projectId}`，用于隔离不同用户/项目）
- `keywords`: `string[]`（至少 1 个）
- `slice`: `string`
- `diary`: `string`

可选：

- `occurred_at`: `string`（RFC3339 或 `YYYY-MM-DD`）
- `importance`: `integer`（1~5）
- `source`: `string`

### recall

必填：

- `namespace`: `string`

可选：

- `keywords`: `string[]`
- `start`: `string`（RFC3339 或 `YYYY-MM-DD`）
- `end`: `string`（RFC3339 或 `YYYY-MM-DD`）
- `query`: `string`（包含匹配 `slice/diary/source`）
- `limit`: `integer`（默认 20，最大 100）
- `include_diary`: `boolean`（默认 `false`；为避免泄露/噪声，默认不返回 diary）

输出补充：

- 当传入 `keywords` 非空时，`data.items[].matched_keywords` 会返回该条记忆命中的关键字交集（便于调用方解释命中原因）。

## 存储设计（JSONL + 索引）

- 存储根目录：
  - 优先：环境变量 `MEMORY_STORE_DIR`
  - 否则：使用 OS 用户数据目录（例如 Windows 的 LocalAppData 下）
- 每个 `namespace` 单独一个目录（会对路径非法字符做净化，防止路径穿越）。
- `memories.jsonl`：追加写（append-only），每行一条 JSON。
- `index.json`：索引文件，用于加速检索：
  - 倒排：`keyword -> itemIndex[]`
  - 时间排序：`time_sorted[]`（按 `occurred_at ?? recorded_at` 升序）
  - 定位：记录每条记忆在 `memories.jsonl` 中的 `offset/length`，召回时只读取命中行。

> 当前实现不做自动淘汰（TTL/上限）。后续可新增 `forget/compact` 等工具，在不破坏数据格式的前提下做清理/归档。

## namespace 生成建议（示例）

建议在调用方统一生成 `userId/projectId`（用于隔离不同用户/项目/工作区的记忆）：

- `userId`：当前登录用户的唯一标识（例如用户 ID，或稳定的匿名化标识）
- `projectId`：当前项目/工作区/仓库/租户的唯一标识（例如 workspace id、repo 名称、tenant id）
- `namespace`：`{userId}/{projectId}`

> 多租户场景建议将 `tenantId` 纳入 `projectId`（或作为其一部分），避免不同租户/工作区间记忆串味。

## 开发与测试

### 快捷命令（推荐）

在仓库根目录执行：

```powershell
& "./memory_tools.bat"
```

进入交互菜单后选择：测试 / Release 构建 / Windows 静态 CRT Release 构建 / 运行 Release 产物 / clean / 一键 remember / 一键 recall。

- 菜单“运行 Release 产物”会提示设置 `MEMORY_STORE_DIR`（回车使用默认：`./.memory_store`）。
- 为避免 bat 参数转发与中文兼容问题，`memory_tools.bat` 不再透传参数；如需透传参数或做 CI，请使用命令行模式或直接运行 `cargo`（见下文）。

命令行模式示例（可选）：

```powershell
python -X utf8 "./tools/memory_tools.py" --cli test -- --nocapture
python -X utf8 "./tools/memory_tools.py" --cli run-release --store-dir ".memory_store" --backtrace

# 以运行参数方式调用两个 MCP tools（会自动设置 MEMORY_STORE_DIR）
python -X utf8 "./tools/memory_tools.py" --cli remember --store-dir ".memory_store" --namespace "u1/p1" --keyword 项目 --slice "我们做过 A 项目" --diary "（省略）" --pretty
python -X utf8 "./tools/memory_tools.py" --cli recall --store-dir ".memory_store" --namespace "u1/p1" --keyword 项目 --text
```

### 直接使用 cargo

在仓库根目录执行：

```powershell
cargo test
cargo build --release
```

### CLI 一键调用（非 MCP）

> 默认（不带 `--cli`）时，`memory.exe` 会作为 MCP stdio server 工作（即使传了其它参数也会忽略）；只有带 `--cli` 才是一键调用模式。

先准备可执行文件路径（示例）：

```powershell
$exe = "./target/release/memory.exe"
```

#### now

```powershell
& $exe --cli now --text
```

#### keywords（关键字管理）

```powershell
& $exe --cli keywords list --namespace "u1/p1" --text
& $exe --cli keywords list-global --text
```

#### remember

```powershell
& $exe --cli remember --namespace "u1/p1" --keyword 项目 --slice "我们做过 A 项目" --diary "（省略）" --pretty

# 推荐：长文本用文件传入
& $exe --cli remember --namespace "u1/p1" --keyword 项目 --slice-file ".\slice.txt" --diary-file ".\diary.txt"
```

#### recall

```powershell
& $exe --cli recall --namespace "u1/p1" --keyword 项目 --limit 20
& $exe --cli recall --namespace "u1/p1" --keyword 项目 --start 2025-01-01 --end 2025-12-31 --text
```

输出说明：

- 默认输出 JSON（stdout）
- `--pretty` 输出 Pretty JSON
- `--text` 输出摘要文本（同时提供 `--pretty` 时以 `--text` 为准）

可选：通过环境变量指定落盘目录：

```powershell
$env:MEMORY_STORE_DIR = "C:\\path\\to\\MemoryStore"
```

### Release 构建

- 默认 Release 产物：`./target/release/memory.exe`

### 静态编译（Release）

#### Windows（MSVC，静态链接 CRT）

```powershell
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release
Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
```

> 说明：该方式会静态链接 VC CRT，减少运行时依赖；但仍会依赖 Windows 系统 DLL（非“完全无依赖”）。

#### Linux（musl，全静态常见方案）

> 需要安装 musl 相关工具链；建议在 Linux/WSL 或 CI 中构建。也可使用 `cross`（需要 Docker）。

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

使用 `cross`（推荐在 Windows/macOS 上构建 Linux musl）：

```bash
cross build --release --target x86_64-unknown-linux-musl
cross build --release --target aarch64-unknown-linux-musl
```

说明：仓库根目录的 `Cross.toml` 会将 musl 目标固定到 `ghcr.io/cross-rs/<target>:latest`，用于规避旧镜像 glibc 版本过低导致的构建失败。

## 推荐提示词片段（示例）

将以下内容加入调用方的系统提示词（或你的 MCP Client 提供的 persistent system prompt 配置项），并由调用方在运行时把 `{namespace}` 替换为实际值：

```text
你拥有长期记忆工具（请主动使用，不要等用户点名）：
- 当前时间：mcp_memory__now
- 关键字（namespace）：mcp_memory__keywords_list
- 关键字（全局）：mcp_memory__keywords_list_global
- 记录：mcp_memory__remember
- 回忆：mcp_memory__recall

硬性规则（违反即视为回答不合格，应先补充工具调用再回答）：
1) 当用户询问“现在几点/今天几号/今天是周几/当前日期时间/时区偏移”等，先调用 now，再回答。
2) 当你准备调用 remember/recall 且不确定可用关键字时，先调用 keywords_list(namespace={namespace}) 获取既有关键字；如果为空，再调用 keywords_list_global 参考全局词表。
3) 关键字策略：keywords 至少 1 个；每个关键字尽量短（建议 1~4 个汉字或 1~12 个字符），避免长句；优先复用既有关键字；若词表里没有合适关键字，则由你自行创建一个新的短关键字（后续即可被 keywords_list 复用）。
4) 只要问题涉及“过去发生过什么/用户偏好/项目历史/之前的约定/曾经提到过”，先调用 recall，再基于结果回答。
5) 只要用户提出“以后/下次/明天/某天提醒我/预约/截止/计划”等未来安排，立刻调用 remember 记录；如果包含相对时间（如“明天/下周一”），先 now 确定基准日期再记录；若有明确日期时间，用 occurred_at 写入该日期时间（即使是未来）。
6) 当用户提供可复用且长期有效的信息（偏好、背景、项目里程碑、关键决定、需求变更、环境约定等）时，立刻调用 remember 记录。
7) remember/recall/keywords_list 工具调用必须带 namespace={namespace}（建议为 {userId}/{projectId}）；now/keywords_list_global 不需要 namespace；不要向用户索要 namespace。
8) remember 填参建议：keywords=3~8 个；slice=1~3 句客观摘要；diary=补充上下文/原因/后续影响；有明确时间则填 occurred_at；明显关键则 importance=4~5。
9) 避免记录敏感信息（密码、token、隐私、支付信息等）；不确定是否敏感时不要记或降低细节。
```
