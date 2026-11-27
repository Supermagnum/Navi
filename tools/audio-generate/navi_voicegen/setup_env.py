"""Create tool venv, install Python deps, clone template caches."""

from __future__ import annotations

import json
import subprocess
import sys
import venv
from pathlib import Path

from navi_voicegen.deps import ensure_system_deps, tool_venv_python
from navi_voicegen.paths import tool_root


def _shallow_sparse_clone(
    url: str,
    dest: Path,
    sparse_paths: list[str],
) -> str | None:
    if dest.is_dir() and (dest / ".git").is_dir():
        rev = subprocess.run(
            ["git", "-C", str(dest), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        )
        return rev.stdout.strip()
    dest.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "git",
            "clone",
            "--depth",
            "1",
            "--filter=blob:none",
            "--sparse",
            url,
            str(dest),
        ],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(dest), "sparse-checkout", "set", *sparse_paths],
        check=True,
    )
    rev = subprocess.run(
        ["git", "-C", str(dest), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    )
    return rev.stdout.strip()


def setup_only() -> dict:
    root = tool_root()
    ensure_system_deps(for_generate=False)

    lock_path = root / "lock" / "manifest.json"
    lock = json.loads(lock_path.read_text(encoding="utf-8"))

    venv_dir = root / ".venv"
    if not venv_dir.is_dir():
        venv.create(venv_dir, with_pip=True)

    py = tool_venv_python(root)
    subprocess.run([str(py), "-m", "pip", "install", "-U", "pip", "wheel"], check=True)
    subprocess.run([str(py), "-m", "pip", "install", "-e", "."], check=True)
    subprocess.run([str(py), "-m", "pip", "install", "-e", ".[engines]"], check=True)

    cache = root / "cache" / "phrase-templates"
    clones: dict[str, str | None] = {}
    for name, spec in lock["phrase_template_clones"].items():
        dest = cache / spec["path_in_cache"]
        commits = _shallow_sparse_clone(
            spec["url"],
            dest,
            list(spec["sparse_paths"]),
        )
        clones[name] = commits

    out = {
        "venv": str(venv_dir),
        "python": str(py),
        "chatterbox_tts": lock["engines"]["chatterbox-tts"]["pypi_version"],
        "template_clones": clones,
    }
    record = root / "lock" / "install-record.json"
    record.write_text(json.dumps(out, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(out, indent=2))
    return out


if __name__ == "__main__":
    try:
        setup_only()
    except Exception as exc:
        print(exc, file=sys.stderr)
        raise SystemExit(1)