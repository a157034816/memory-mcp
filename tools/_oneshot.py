from __future__ import annotations

from pathlib import Path
from typing import Optional

from _paths import get_paths
from _run import run


def _ensure_dir(p: Path) -> None:
    p.mkdir(parents=True, exist_ok=True)


def select_store_dir(store_dir: Optional[Path], default_store_dir: Path) -> Path:
    if store_dir is None:
        try:
            text = input(f"MEMORY_STORE_DIR（回车使用默认：{default_store_dir}）：").strip()
        except EOFError:
            text = ""
        store_dir = Path(text) if text else default_store_dir

    store_dir = store_dir.expanduser()
    _ensure_dir(store_dir)
    return store_dir


def _prompt_yes_no(prompt: str, default_yes: bool = True) -> bool:
    suffix = "[Y/n]" if default_yes else "[y/N]"
    try:
        text = input(f"{prompt}{suffix}：").strip().lower()
    except EOFError:
        return default_yes

    if not text:
        return default_yes

    return text in ("y", "yes", "1", "true", "t", "是")


def _parse_keywords(text: str) -> list[str]:
    text = text.replace("，", ",")

    def _is_cjk_char(ch: str) -> bool:
        code = ord(ch)
        return 0x4E00 <= code <= 0x9FFF

    def _max_len(token: str) -> int:
        # 关键字尽量短：CJK 建议更短，英文/数字允许稍长一些。
        return 4 if any(_is_cjk_char(ch) for ch in token) else 12

    parts: list[str] = []
    for seg in text.split(","):
        seg = seg.strip()
        if not seg:
            continue
        parts.extend([p for p in seg.split() if p])

    out: list[str] = []
    seen: set[str] = set()
    for raw in parts:
        token = raw.strip().lower()
        if not token:
            continue

        max_len = _max_len(token)
        if len(token) > max_len:
            token = token[:max_len]

        if token and token not in seen:
            seen.add(token)
            out.append(token)

    return out


def _prompt_keywords(required: bool) -> list[str]:
    while True:
        try:
            text = input("关键字（逗号分隔；至少 1 个；建议短词）：").strip()
        except EOFError:
            text = ""
        keywords = _parse_keywords(text)
        if keywords or not required:
            return keywords
        print("关键字不能为空。")


def _prompt_inline_or_file(label: str, inline_flag: str, file_flag: str) -> list[str]:
    while True:
        print(f"{label} 输入方式：")
        print("  1) 直接输入（单行）")
        print("  2) 文件路径（推荐，支持多行/长文本）")

        try:
            choice = input("请选择 [2]：").strip()
        except EOFError:
            choice = "2"

        if choice in ("", "2"):
            try:
                path_text = input(f"{label} 文件路径：").strip()
            except EOFError:
                path_text = ""

            path_text = path_text.strip().strip('"')
            if not path_text:
                print("文件路径不能为空。")
                continue

            p = Path(path_text).expanduser()
            if not p.exists():
                print(f"文件不存在：{p}")
                continue

            return [file_flag, str(p)]

        if choice == "1":
            try:
                text = input(f"{label}（单行；长文本请用文件）：").strip()
            except EOFError:
                text = ""

            if not text:
                print(f"{label} 不能为空。")
                continue

            return [inline_flag, text]

        print(f"无效选项：{choice}")


def _prompt_output_mode_args(default_text: bool = True) -> list[str]:
    print("输出模式：")
    print("  1) JSON")
    print("  2) Pretty JSON")
    print("  3) Text")

    default_choice = "3" if default_text else "1"
    try:
        choice = input(f"请选择 [{default_choice}]：").strip()
    except EOFError:
        choice = default_choice

    if not choice:
        choice = default_choice

    if choice == "1":
        return []
    if choice == "2":
        return ["--pretty"]
    return ["--text"]


def _ensure_release_exe(paths) -> bool:
    if paths.release_exe_path.exists():
        return True

    print(f"未找到 Release 产物：{paths.release_exe_path}")
    if not _prompt_yes_no("是否先构建 Release？", default_yes=True):
        return False

    cmdline = [
        "cargo",
        "build",
        "--release",
        "--manifest-path",
        str(paths.manifest_path),
    ]
    return run(cmdline, cwd=paths.repo_root) == 0


def action_cli_remember() -> int:
    paths = get_paths()
    if not _ensure_release_exe(paths):
        return 2

    default_store_dir = paths.memory_dir / ".memory_store"
    store_dir = select_store_dir(None, default_store_dir)

    env: dict[str, str] = {"MEMORY_STORE_DIR": str(store_dir)}
    if _prompt_yes_no("是否启用 RUST_BACKTRACE=1？", default_yes=False):
        env["RUST_BACKTRACE"] = "1"

    while True:
        try:
            namespace = input("namespace（必填，例如 u1/p1）：").strip()
        except EOFError:
            namespace = ""
        if namespace:
            break
        print("namespace 不能为空。")

    keywords = _prompt_keywords(required=True)

    slice_args = _prompt_inline_or_file("slice", "--slice", "--slice-file")
    diary_args = _prompt_inline_or_file("diary", "--diary", "--diary-file")

    try:
        occurred_at = input("occurred_at（可选，RFC3339 或 YYYY-MM-DD，回车跳过）：").strip()
    except EOFError:
        occurred_at = ""

    importance: str = ""
    while True:
        try:
            importance = input("importance（可选 1~5，回车跳过）：").strip()
        except EOFError:
            importance = ""

        if not importance:
            break

        try:
            n = int(importance)
        except ValueError:
            print("importance 必须是整数（1~5）。")
            continue

        if 1 <= n <= 5:
            break

        print("importance 必须在 1~5。")

    try:
        source = input("source（可选，回车跳过）：").strip()
    except EOFError:
        source = ""

    out_args = _prompt_output_mode_args(default_text=True)

    cmdline: list[str] = [str(paths.release_exe_path), "--cli", "remember", "--namespace", namespace]
    for kw in keywords:
        cmdline += ["--keyword", kw]

    cmdline += slice_args
    cmdline += diary_args

    if occurred_at:
        cmdline += ["--occurred-at", occurred_at]
    if importance:
        cmdline += ["--importance", importance]
    if source:
        cmdline += ["--source", source]

    cmdline += out_args

    return run(cmdline, cwd=paths.memory_dir, env_overrides=env)


def action_cli_recall() -> int:
    paths = get_paths()
    if not _ensure_release_exe(paths):
        return 2

    default_store_dir = paths.memory_dir / ".memory_store"
    store_dir = select_store_dir(None, default_store_dir)

    env: dict[str, str] = {"MEMORY_STORE_DIR": str(store_dir)}
    if _prompt_yes_no("是否启用 RUST_BACKTRACE=1？", default_yes=False):
        env["RUST_BACKTRACE"] = "1"

    while True:
        try:
            namespace = input("namespace（必填，例如 u1/p1）：").strip()
        except EOFError:
            namespace = ""
        if namespace:
            break
        print("namespace 不能为空。")

    keywords = _prompt_keywords(required=False)

    try:
        start = input("start（可选，RFC3339 或 YYYY-MM-DD，回车跳过）：").strip()
    except EOFError:
        start = ""

    try:
        end = input("end（可选，RFC3339 或 YYYY-MM-DD，回车跳过）：").strip()
    except EOFError:
        end = ""

    try:
        query = input("query（可选，回车跳过）：").strip()
    except EOFError:
        query = ""

    limit: str = ""
    while True:
        try:
            limit = input("limit（可选 1~100，回车跳过使用默认 20）：").strip()
        except EOFError:
            limit = ""

        if not limit:
            break

        try:
            n = int(limit)
        except ValueError:
            print("limit 必须是整数（1~100）。")
            continue

        if 1 <= n <= 100:
            break

        print("limit 必须在 1~100。")

    include_diary = _prompt_yes_no("include_diary（是否返回 diary）？", default_yes=False)
    out_args = _prompt_output_mode_args(default_text=True)

    cmdline: list[str] = [str(paths.release_exe_path), "--cli", "recall", "--namespace", namespace]
    for kw in keywords:
        cmdline += ["--keyword", kw]

    if start:
        cmdline += ["--start", start]
    if end:
        cmdline += ["--end", end]
    if query:
        cmdline += ["--query", query]
    if limit:
        cmdline += ["--limit", limit]
    if include_diary:
        cmdline += ["--include-diary"]

    cmdline += out_args

    return run(cmdline, cwd=paths.memory_dir, env_overrides=env)
