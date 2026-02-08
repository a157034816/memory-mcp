from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class MemoryPaths:
    repo_root: Path
    memory_dir: Path
    tools_dir: Path
    manifest_path: Path
    release_exe_path: Path


def _find_repo_root(start: Path) -> Path:
    for p in [start, *start.parents]:
        if (p / ".git").exists():
            return p
    raise FileNotFoundError(f"未找到仓库根目录（缺少 .git）：start={start}")


def get_paths() -> MemoryPaths:
    tools_dir = Path(__file__).resolve().parent
    memory_dir = tools_dir.parent

    repo_root = _find_repo_root(memory_dir)

    manifest_path = memory_dir / "Cargo.toml"
    if not manifest_path.exists():
        raise FileNotFoundError(f"未找到 Cargo.toml：{manifest_path}")

    release_exe_path = memory_dir / "target" / "release" / "memory.exe"

    return MemoryPaths(
        repo_root=repo_root,
        memory_dir=memory_dir,
        tools_dir=tools_dir,
        manifest_path=manifest_path,
        release_exe_path=release_exe_path,
    )
