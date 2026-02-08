from __future__ import annotations

import shlex
import sys
from pathlib import Path
from typing import Optional

from _oneshot import action_cli_recall, action_cli_remember, select_store_dir
from _paths import get_paths
from _run import run, split_by_double_dash


def _print_help() -> None:
    print(
        "\n".join(
            [
                "Memory Rust 快捷命令：",
                "",
                "交互式（推荐）：",
                "  memory_tools.bat（含一键 remember/recall）",
                "",
                "命令行模式（可选，CI/脚本用；必须显式提供 --cli）：",
                "  memory_tools.py --cli test [-- <test args>]",
                "  memory_tools.py --cli build-release [<cargo args...>]",
                "  memory_tools.py --cli build-static-windows [<cargo args...>]",
                "  memory_tools.py --cli run-release [--store-dir <dir>] [--backtrace]",
                "  memory_tools.py --cli remember [--store-dir <dir>] [--backtrace] [--build] <memory.exe remember args...>",
                "  memory_tools.py --cli recall [--store-dir <dir>] [--backtrace] [--build] <memory.exe recall args...>",
                "  memory_tools.py --cli clean",
                "",
                "示例：",
                "  memory_tools.py --cli test -- --nocapture",
                "  memory_tools.py --cli run-release --store-dir .memory_store --backtrace",
                "  memory_tools.py --cli recall --store-dir .memory_store --namespace u1/p1 --keyword 项目 --text",
            ]
        )
    )


def _split_args_line(text: str) -> list[str]:
    text = text.strip()
    if not text:
        return []

    try:
        return shlex.split(text, posix=False)
    except ValueError:
        return text.split()


def _prompt_optional_args(prompt: str) -> list[str]:
    try:
        line = input(prompt).strip()
    except EOFError:
        return []
    return _split_args_line(line)


def _action_test() -> int:
    paths = get_paths()
    extra = _prompt_optional_args(
        "可选：cargo test 额外参数（回车跳过，例如：-- --nocapture）："
    )
    cargo_args, test_args = split_by_double_dash(extra)
    cmdline = [
        "cargo",
        "test",
        "--manifest-path",
        str(paths.manifest_path),
        *cargo_args,
    ]
    if test_args:
        cmdline += ["--", *test_args]
    return run(cmdline, cwd=paths.repo_root)


def _action_build_release(static_windows: bool = False) -> int:
    paths = get_paths()
    label = "（Windows 静态 CRT）" if static_windows else ""
    extra = _prompt_optional_args(
        f"可选：cargo build --release{label} 额外参数（回车跳过）："
    )

    env = {"RUSTFLAGS": "-C target-feature=+crt-static"} if static_windows else None
    cmdline = [
        "cargo",
        "build",
        "--release",
        "--manifest-path",
        str(paths.manifest_path),
        *extra,
    ]
    code = run(cmdline, cwd=paths.repo_root, env_overrides=env)
    if code == 0:
        if static_windows:
            print("说明：该方式会静态链接 VC CRT，但仍会依赖 Windows 系统 DLL。")
        print(f"产物：{paths.release_exe_path}")
    return code


def _action_clean() -> int:
    paths = get_paths()
    return run(
        ["cargo", "clean", "--manifest-path", str(paths.manifest_path)],
        cwd=paths.repo_root,
    )


def _action_run_release_exe(
    store_dir: Optional[Path] = None, enable_backtrace: bool = False
) -> int:
    paths = get_paths()

    if not paths.release_exe_path.exists():
        print(f"未找到 Release 产物：{paths.release_exe_path}")
        print("请先选择“构建 Release”。")
        return 2

    default_store_dir = paths.memory_dir / ".memory_store"
    store_dir = select_store_dir(store_dir, default_store_dir)

    env: dict[str, str] = {"MEMORY_STORE_DIR": str(store_dir)}
    if enable_backtrace:
        env["RUST_BACKTRACE"] = "1"

    print("已启动 memory.exe（stdio MCP server）。")
    print("注意：该进程会等待 MCP JSON-RPC 输入；按 Ctrl+C 结束进程。")
    print(f"MEMORY_STORE_DIR={store_dir}")

    try:
        return run([str(paths.release_exe_path)], cwd=paths.memory_dir, env_overrides=env)
    except KeyboardInterrupt:
        print("\n已退出。")
        return 130


def _interactive_menu() -> int:
    paths = get_paths()
    while True:
        print("")
        print("Memory Rust 工具菜单：")
        print(f"  项目目录：{paths.memory_dir}")
        print(f"  Release 产物：{paths.release_exe_path}")
        print("")
        print("  1) 运行测试（cargo test）")
        print("  2) 构建 Release（cargo build --release）")
        print("  3) 构建 Release（Windows 静态 CRT）")
        print("  4) 运行 Release 产物（memory.exe）")
        print("  5) 清理构建产物（cargo clean）")
        print("  6) 一键 remember（memory.exe remember）")
        print("  7) 一键 recall（memory.exe recall）")
        print("  0) 退出")
        print("")

        try:
            choice = input("请选择：").strip()
        except EOFError:
            return 0

        if choice in ("0", "q", "quit", "exit"):
            return 0
        if choice == "1":
            _action_test()
            continue
        if choice == "2":
            _action_build_release(static_windows=False)
            continue
        if choice == "3":
            _action_build_release(static_windows=True)
            continue
        if choice == "4":
            try:
                backtrace_text = input("是否启用 RUST_BACKTRACE=1？[Y/n]：").strip().lower()
            except EOFError:
                backtrace_text = ""
            enable_backtrace = backtrace_text in ("", "y", "yes")
            _action_run_release_exe(store_dir=None, enable_backtrace=enable_backtrace)
            continue
        if choice == "5":
            _action_clean()
            continue
        if choice == "6":
            action_cli_remember()
            continue
        if choice == "7":
            action_cli_recall()
            continue

        print(f"无效选项：{choice}")


def _parse_cli_passthrough_args(
    raw_args: list[str],
) -> tuple[Optional[Path], bool, bool, list[str]]:
    store_dir: Optional[Path] = None
    enable_backtrace = False
    build_if_missing = False
    forwarded: list[str] = []

    i = 0
    while i < len(raw_args):
        a = raw_args[i]
        if a == "--store-dir" and i + 1 < len(raw_args):
            store_dir = Path(raw_args[i + 1])
            i += 2
            continue
        if a in ("--backtrace", "--bt"):
            enable_backtrace = True
            i += 1
            continue
        if a in ("--build", "--build-if-missing"):
            build_if_missing = True
            i += 1
            continue
        if a == "--":
            forwarded.extend(raw_args[i + 1 :])
            break
        forwarded.append(a)
        i += 1

    return store_dir, enable_backtrace, build_if_missing, forwarded


def _ensure_release_exe_noninteractive(build_if_missing: bool) -> bool:
    paths = get_paths()
    if paths.release_exe_path.exists():
        return True

    print(f"未找到 Release 产物：{paths.release_exe_path}")
    if not build_if_missing:
        print("请先执行：memory_tools.py --cli build-release")
        print("或添加参数：--build（缺失时自动构建 Release）")
        return False

    cmdline = [
        "cargo",
        "build",
        "--release",
        "--manifest-path",
        str(paths.manifest_path),
    ]
    return run(cmdline, cwd=paths.repo_root) == 0


def _action_cli_passthrough(tool: str, raw_args: list[str]) -> int:
    paths = get_paths()
    store_dir, enable_backtrace, build_if_missing, forwarded = _parse_cli_passthrough_args(
        raw_args
    )

    if not _ensure_release_exe_noninteractive(build_if_missing):
        return 2

    default_store_dir = paths.memory_dir / ".memory_store"
    store_dir = store_dir.expanduser() if store_dir else default_store_dir
    if not store_dir.is_absolute():
        store_dir = paths.memory_dir / store_dir
    store_dir.mkdir(parents=True, exist_ok=True)

    env: dict[str, str] = {"MEMORY_STORE_DIR": str(store_dir)}
    if enable_backtrace:
        env["RUST_BACKTRACE"] = "1"

    cmdline: list[str] = [str(paths.release_exe_path), "--cli", tool, *forwarded]
    return run(cmdline, cwd=paths.memory_dir, env_overrides=env)


def main(argv: list[str]) -> int:
    if not argv:
        return _interactive_menu()

    if "--cli" not in argv:
        if argv[0] in ("-h", "--help", "help"):
            _print_help()
            return 0

        print("提示：未提供 --cli，将忽略命令行参数并进入交互菜单。")
        return _interactive_menu()

    cli_argv = [a for a in argv if a != "--cli"]
    if not cli_argv:
        _print_help()
        return 0

    cmd = cli_argv[0]
    rest = cli_argv[1:]

    if cmd in ("-h", "--help", "help"):
        _print_help()
        return 0

    if cmd == "test":
        paths = get_paths()
        cargo_args, test_args = split_by_double_dash(rest)
        cmdline = [
            "cargo",
            "test",
            "--manifest-path",
            str(paths.manifest_path),
            *cargo_args,
        ]
        if test_args:
            cmdline += ["--", *test_args]
        return run(cmdline, cwd=paths.repo_root)

    if cmd == "build-release":
        paths = get_paths()
        cmdline = [
            "cargo",
            "build",
            "--release",
            "--manifest-path",
            str(paths.manifest_path),
            *rest,
        ]
        code = run(cmdline, cwd=paths.repo_root)
        if code == 0:
            print(f"产物：{paths.release_exe_path}")
        return code

    if cmd == "build-static-windows":
        paths = get_paths()
        env = {"RUSTFLAGS": "-C target-feature=+crt-static"}
        cmdline = [
            "cargo",
            "build",
            "--release",
            "--manifest-path",
            str(paths.manifest_path),
            *rest,
        ]
        code = run(cmdline, cwd=paths.repo_root, env_overrides=env)
        if code == 0:
            print("说明：该方式会静态链接 VC CRT，但仍会依赖 Windows 系统 DLL。")
            print(f"产物：{paths.release_exe_path}")
        return code

    if cmd == "clean":
        paths = get_paths()
        cmdline = [
            "cargo",
            "clean",
            "--manifest-path",
            str(paths.manifest_path),
            *rest,
        ]
        return run(cmdline, cwd=paths.repo_root)

    if cmd == "run-release":
        store_dir: Optional[Path] = None
        enable_backtrace = False

        i = 0
        while i < len(rest):
            a = rest[i]
            if a == "--store-dir" and i + 1 < len(rest):
                store_dir = Path(rest[i + 1])
                i += 2
                continue
            if a in ("--backtrace", "--bt"):
                enable_backtrace = True
                i += 1
                continue
            if a in ("-h", "--help"):
                _print_help()
                return 0
            print(f"未知参数：{a}")
            return 2

        return _action_run_release_exe(
            store_dir=store_dir, enable_backtrace=enable_backtrace
        )

    if cmd == "remember":
        return _action_cli_passthrough("remember", rest)

    if cmd == "recall":
        return _action_cli_passthrough("recall", rest)

    print(f"未知命令：{cmd}")
    _print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
