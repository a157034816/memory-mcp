from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any, Iterable

from _paths import get_paths
from _run import ensure_no_bom

_DEP_SECTIONS = (
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
)


def _read_text(path: Path) -> str:
    return ensure_no_bom(path.read_text(encoding="utf-8", errors="replace"))


def _write_text(path: Path, text: str) -> None:
    with path.open("w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def _normalize_version(text: str) -> str:
    text = text.strip()
    if text.startswith("v") and len(text) >= 2 and text[1].isdigit():
        return text[1:]
    return text


def _update_lines_preserve_trailing_newline(original: str, lines: Iterable[str]) -> str:
    out = "\n".join(lines)
    if original.endswith("\n") and not out.endswith("\n"):
        out += "\n"
    return out


def _read_cargo_package_version(cargo_toml_text: str) -> str:
    in_package = False
    for line in cargo_toml_text.splitlines():
        sec = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if sec:
            in_package = sec.group(1).strip() == "package"
            continue
        if not in_package:
            continue

        m = re.match(r'^\s*version\s*=\s*"([^"]+)"\s*$', line)
        if m:
            return m.group(1)

    raise ValueError("未在 Cargo.toml 的 [package] 段找到 version 字段。")


def _set_cargo_toml_version(cargo_toml_text: str, new_version: str) -> str:
    in_package = False
    changed = False
    out_lines: list[str] = []

    for line in cargo_toml_text.splitlines():
        sec = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if sec:
            in_package = sec.group(1).strip() == "package"
            out_lines.append(line)
            continue

        if in_package:
            m = re.match(r'^(\s*version\s*=\s*")([^"]+)("\s*)$', line)
            if m:
                out_lines.append(f"{m.group(1)}{new_version}{m.group(3)}")
                changed = True
                continue

        out_lines.append(line)

    if not changed:
        raise ValueError("未能更新 Cargo.toml：未找到 [package] 下的 version 行。")

    return _update_lines_preserve_trailing_newline(cargo_toml_text, out_lines)


def _set_cargo_lock_local_package_version(
    cargo_lock_text: str, *, package_name: str, new_version: str
) -> tuple[str, bool]:
    lines = cargo_lock_text.splitlines()
    out_lines = list(lines)

    starts = [i for i, line in enumerate(lines) if line.strip() == "[[package]]"]
    if not starts:
        return cargo_lock_text, False
    starts.append(len(lines))

    changed = False

    for start, end in zip(starts, starts[1:]):
        block_lines = lines[start:end]

        name: str | None = None
        has_source = False
        version_line_idx: int | None = None
        version_prefix = ""
        version_value: str | None = None
        version_suffix = ""

        for offset, line in enumerate(block_lines):
            m_name = re.match(r'^\s*name\s*=\s*"([^"]+)"\s*$', line)
            if m_name:
                name = m_name.group(1)

            if re.match(r'^\s*source\s*=\s*".*"\s*$', line):
                has_source = True

            m_ver = re.match(r'^(\s*version\s*=\s*")([^"]+)("\s*)$', line)
            if m_ver:
                version_line_idx = start + offset
                version_prefix = m_ver.group(1)
                version_value = m_ver.group(2)
                version_suffix = m_ver.group(3)

        # 仅修改本地包（Cargo.lock 中通常没有 source 字段），避免误改同名的 crates.io 依赖。
        if (
            name == package_name
            and not has_source
            and version_line_idx is not None
            and version_value is not None
            and version_value != new_version
        ):
            out_lines[version_line_idx] = f"{version_prefix}{new_version}{version_suffix}"
            changed = True

    return _update_lines_preserve_trailing_newline(cargo_lock_text, out_lines), changed


def _rewrite_internal_spec(spec: Any, *, old_version: str, new_version: str) -> Any:
    if not isinstance(spec, str):
        return spec

    if spec == old_version:
        return new_version
    if spec.startswith("^" + old_version):
        return "^" + new_version + spec[len("^" + old_version) :]
    if spec.startswith("~" + old_version):
        return "~" + new_version + spec[len("~" + old_version) :]
    return spec


def _load_json(path: Path) -> Any:
    return json.loads(_read_text(path))


def _dump_json(obj: Any) -> str:
    return json.dumps(obj, ensure_ascii=False, indent=2) + "\n"


def _iter_workspace_package_json(packages_dir: Path) -> list[Path]:
    if not packages_dir.exists():
        return []
    return sorted(packages_dir.glob("*/package.json"))


def _collect_workspace_names(package_json_paths: list[Path]) -> set[str]:
    names: set[str] = set()
    for p in package_json_paths:
        data = _load_json(p)
        if isinstance(data, dict) and isinstance(data.get("name"), str) and data["name"]:
            names.add(data["name"])
    return names


def _set_package_json_versions(
    package_json_paths: list[Path],
    *,
    workspace_names: set[str],
    old_version: str,
    new_version: str,
) -> dict[Path, str]:
    changed: dict[Path, str] = {}

    for p in package_json_paths:
        data = _load_json(p)
        if not isinstance(data, dict):
            continue

        file_changed = False

        if isinstance(data.get("version"), str):
            if data["version"] != new_version:
                data["version"] = new_version
                file_changed = True

        for section in _DEP_SECTIONS:
            deps = data.get(section)
            if not isinstance(deps, dict):
                continue
            for name in workspace_names:
                if name not in deps:
                    continue
                new_spec = _rewrite_internal_spec(
                    deps[name], old_version=old_version, new_version=new_version
                )
                if new_spec != deps[name]:
                    deps[name] = new_spec
                    file_changed = True

        if file_changed:
            changed[p] = _dump_json(data)

    return changed


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="version_tools.py",
        description="批量同步 Rust（Cargo.toml/Cargo.lock）与 npm workspaces（packages/*/package.json）版本号。",
    )
    parser.add_argument(
        "version",
        nargs="?",
        help="目标版本号（例如 0.1.7 或 v0.1.7）。",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="只打印将要修改的文件，不写入。",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = _parse_args(argv)

    if not args.version:
        try:
            raw = input("请输入新版本号（例如 0.1.7 或 v0.1.7）：").strip()
        except EOFError:
            return 2
        if not raw:
            print("未输入版本号，已退出。")
            return 2
        args.version = raw

    new_version = _normalize_version(args.version)

    paths = get_paths()
    cargo_toml_path = paths.manifest_path
    cargo_lock_path = paths.memory_dir / "Cargo.lock"
    packages_dir = paths.memory_dir / "packages"

    cargo_toml_text = _read_text(cargo_toml_path)
    old_version = _read_cargo_package_version(cargo_toml_text)

    cargo_toml_new = _set_cargo_toml_version(cargo_toml_text, new_version)

    cargo_lock_changed = False
    cargo_lock_new = ""
    if cargo_lock_path.exists():
        cargo_lock_text = _read_text(cargo_lock_path)
        cargo_lock_new, cargo_lock_changed = _set_cargo_lock_local_package_version(
            cargo_lock_text, package_name="memory", new_version=new_version
        )

    package_json_paths = _iter_workspace_package_json(packages_dir)
    workspace_names = _collect_workspace_names(package_json_paths)
    package_json_updates = _set_package_json_versions(
        package_json_paths,
        workspace_names=workspace_names,
        old_version=old_version,
        new_version=new_version,
    )

    changed_files: list[Path] = []
    if cargo_toml_new != cargo_toml_text:
        changed_files.append(cargo_toml_path)
    if cargo_lock_changed:
        changed_files.append(cargo_lock_path)
    changed_files.extend(sorted(package_json_updates.keys()))

    print(f"版本变更：{old_version} -> {new_version}")
    if not changed_files:
        print("未检测到需要修改的文件。")
        return 0

    for p in changed_files:
        print(f"- {p}")

    if args.dry_run:
        print("dry-run：未写入文件。")
        return 0

    if cargo_toml_new != cargo_toml_text:
        _write_text(cargo_toml_path, cargo_toml_new)
    if cargo_lock_changed:
        _write_text(cargo_lock_path, cargo_lock_new)
    for p, content in package_json_updates.items():
        _write_text(p, content)

    print("已写入完成。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
