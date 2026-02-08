from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path
from typing import Iterable


def _cmd_to_string(cmd: list[str]) -> str:
    try:
        return subprocess.list2cmdline(cmd)
    except Exception:
        return " ".join(cmd)


def run(
    cmd: list[str],
    cwd: Path,
    env_overrides: dict[str, str] | None = None,
    passthrough: bool = True,
) -> int:
    env = os.environ.copy()
    if env_overrides:
        env.update({k: v for k, v in env_overrides.items() if v is not None})

    print(f"[RUN] {_cmd_to_string(cmd)}", flush=True)

    try:
        if passthrough:
            result = subprocess.run(cmd, cwd=str(cwd), env=env)
        else:
            result = subprocess.run(
                cmd,
                cwd=str(cwd),
                env=env,
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
            )
            sys.stdout.write(result.stdout)
            sys.stderr.write(result.stderr)
    except FileNotFoundError:
        print(f"[FAIL] 未找到命令：{cmd[0]}", file=sys.stderr)
        return 127

    if result.returncode == 0:
        print("[OK]", flush=True)
    else:
        print(f"[FAIL] exit_code={result.returncode}", flush=True)

    return result.returncode


def split_by_double_dash(args: list[str]) -> tuple[list[str], list[str]]:
    if "--" not in args:
        return args, []

    idx = args.index("--")
    return args[:idx], args[idx + 1 :]


def ensure_no_bom(text: str) -> str:
    return text.lstrip("\ufeff")


def join_lines(lines: Iterable[str]) -> str:
    return "\n".join(lines)
