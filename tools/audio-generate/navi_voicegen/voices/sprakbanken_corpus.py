"""Språkbanken NST Norwegian Dictation (sbr-14 / 22 kHz) corpus fetch + safe extract."""

from __future__ import annotations

import hashlib
import re
import shutil
import subprocess
import tarfile
from dataclasses import dataclass
from pathlib import Path

from navi_voicegen.paths import tool_root

SBR14_ID = "sbr-14"
SPRAKBANKEN_ROOT = tool_root() / "cache" / "sprakbanken"
SBR14_ROOT = SPRAKBANKEN_ROOT / SBR14_ID
SBR14_ARCHIVES = SBR14_ROOT / "archives"
SBR14_EXTRACTED = SBR14_ROOT / "extracted"
SBR14_SHA256SUMS = SBR14_ROOT / "SHA256SUMS"

CATALOGUE_URL = "https://www.nb.no/sprakbanken/ressurskatalog/en/oai-nb-no-sbr-14/"

UNSAFE_TAR_MEMBER = re.compile(r"(^/)|(^|/)\.\.(/|$)|\\")


@dataclass(frozen=True)
class CorpusArchive:
    name: str
    url: str
    content_length: int


SBR14_ARCHIVES_SPEC: tuple[CorpusArchive, ...] = (
    CorpusArchive(
        "no.22khz.skript-doc.tar.gz",
        "https://www.nb.no/sbfil/talegjenkjenning/22kHz_2020/no.22khz.skript-doc.tar.gz",
        16_068_146,
    ),
    CorpusArchive(
        "202006_changelog.txt",
        "https://www.nb.no/sbfil/talegjenkjenning/22kHz_2020/202006_changelog.txt",
        735,
    ),
    CorpusArchive(
        "no.22khz.tar.gz",
        "https://www.nb.no/sbfil/talegjenkjenning/22kHz_2020/no.22khz.tar.gz",
        6_440_630_415,
    ),
)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def head_content_length(url: str, *, timeout_s: int = 120) -> int | None:
    proc = subprocess.run(
        ["curl", "-fsSIL", "--max-time", str(timeout_s), url],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        return None
    for line in proc.stdout.splitlines():
        if line.lower().startswith("content-length:"):
            try:
                return int(line.split(":", 1)[1].strip())
            except ValueError:
                return None
    return None


def read_sha256sums(path: Path) -> dict[str, str]:
    if not path.is_file():
        return {}
    out: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(None, 1)
        if len(parts) != 2:
            continue
        digest, name = parts
        if len(digest) == 64:
            out[name] = digest.lower()
    return out


def write_sha256sums(path: Path, mapping: dict[str, str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"{mapping[k].lower()}  {k}\n" for k in sorted(mapping)]
    path.write_text("".join(lines), encoding="utf-8")


def _record_sha256(name: str, digest: str) -> None:
    sums = read_sha256sums(SBR14_SHA256SUMS)
    sums[name] = digest.lower()
    write_sha256sums(SBR14_SHA256SUMS, sums)


def is_safe_tar_member(name: str) -> bool:
    n = name.replace("\\", "/")
    if n.startswith("/") or n.startswith("../") or "/../" in n:
        return False
    return UNSAFE_TAR_MEMBER.search(n) is None


def assert_tar_safe(archive: Path) -> list[str]:
    names: list[str] = []
    with tarfile.open(archive, "r:*") as tf:
        for m in tf.getmembers():
            if not is_safe_tar_member(m.name):
                raise ValueError(f"unsafe tar member: {m.name!r}")
            names.append(m.name)
    return names


def safe_tar_extract(archive: Path, dest_dir: Path) -> None:
    """Extract archive under dest_dir; reject absolute paths and `..` members."""
    dest_dir = dest_dir.resolve()
    dest_dir.mkdir(parents=True, exist_ok=True)
    assert_tar_safe(archive)
    with tarfile.open(archive, "r:*") as tf:
        for m in tf.getmembers():
            if not m.isfile() and not m.isdir():
                continue
            target = (dest_dir / m.name).resolve()
            if not str(target).startswith(str(dest_dir)):
                raise ValueError(f"tar member escapes destination: {m.name!r}")
            if m.isdir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            src = tf.extractfile(m)
            if src is None:
                continue
            with src, target.open("wb") as out:
                shutil.copyfileobj(src, out)


def download_resumable(
    url: str,
    dest: Path,
    *,
    expected_size: int | None = None,
    timeout_s: int = 0,
) -> None:
    """Resumable download with curl -C -; timeout_s=0 means no limit."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    partial = dest.with_suffix(dest.suffix + ".partial")
    if dest.is_file() and expected_size is not None and dest.stat().st_size == expected_size:
        return
    if partial.is_file() and dest.is_file() and partial.stat().st_size <= dest.stat().st_size:
        partial.unlink(missing_ok=True)

    remote_len = head_content_length(url)
    if expected_size is not None and remote_len is not None and remote_len != expected_size:
        raise SystemExit(
            f"Content-Length mismatch for {url}: expected {expected_size}, HEAD reported {remote_len}"
        )

    outfile = partial if partial.exists() or not dest.is_file() else dest
    cmd = [
        "curl",
        "-fSL",
        "-C",
        "-",
        "--retry",
        "8",
        "--retry-delay",
        "5",
        "--retry-all-errors",
        "-o",
        str(outfile),
        url,
    ]
    if timeout_s > 0:
        cmd[1:1] = ["--max-time", str(timeout_s)]
    subprocess.run(cmd, check=True)

    if outfile == partial:
        partial.rename(dest)

    size = dest.stat().st_size
    if expected_size is not None and size != expected_size:
        raise SystemExit(f"Downloaded {dest.name}: size {size} != expected {expected_size}")
    if remote_len is not None and size != remote_len:
        raise SystemExit(f"Downloaded {dest.name}: size {size} != Content-Length {remote_len}")

    _record_sha256(dest.name, sha256_file(dest))


def corpus_file_ready(arch: CorpusArchive) -> bool:
    dest = SBR14_ARCHIVES / arch.name
    if not dest.is_file():
        return False
    if dest.stat().st_size != arch.content_length:
        return False
    pinned = read_sha256sums(SBR14_SHA256SUMS).get(arch.name)
    if not pinned:
        return False
    return sha256_file(dest) == pinned


def fetch_sprakbanken_sbr14(*, extract: bool = True, audio_archive_only: bool = False) -> None:
    """Download sbr-14 archives (idempotent when size + SHA match). Optionally extract."""
    SBR14_ARCHIVES.mkdir(parents=True, exist_ok=True)
    specs = SBR14_ARCHIVES_SPEC
    if audio_archive_only:
        specs = tuple(a for a in specs if a.name.endswith(".tar.gz") and "skript" not in a.name)
    for arch in specs:
        dest = SBR14_ARCHIVES / arch.name
        if corpus_file_ready(arch):
            print(f"skip corpus archive (verified): {dest}", flush=True)
            continue
        print(f"fetch corpus archive: {arch.name} → {dest}", flush=True)
        download_resumable(arch.url, dest, expected_size=arch.content_length, timeout_s=0)

    if not extract:
        return

    doc_arc = SBR14_ARCHIVES / "no.22khz.skript-doc.tar.gz"
    if doc_arc.is_file():
        doc_marker = SBR14_EXTRACTED / ".extracted_skript_doc"
        if not doc_marker.is_file():
            print(f"extract {doc_arc.name} …", flush=True)
            safe_tar_extract(doc_arc, SBR14_EXTRACTED / "skript-doc")
            doc_marker.write_text("ok\n", encoding="utf-8")

    audio_arc = SBR14_ARCHIVES / "no.22khz.tar.gz"
    if audio_arc.is_file():
        audio_marker = SBR14_EXTRACTED / ".extracted_audio"
        if not audio_marker.is_file():
            print(f"extract {audio_arc.name} (large) …", flush=True)
            safe_tar_extract(audio_arc, SBR14_EXTRACTED / "audio")
            audio_marker.write_text("ok\n", encoding="utf-8")


def resolve_sbr14_member_path(member: str) -> Path | None:
    """Find a wav (or other file) under sbr-14 extracted tree matching member path."""
    rel = member.lstrip("/")
    direct = SBR14_ROOT / rel
    if direct.is_file() and direct.suffix.lower() == ".wav":
        return direct
    if direct.is_file():
        return direct
    for root_name in ("audio", "skript-doc", ""):
        base = SBR14_EXTRACTED / root_name if root_name else SBR14_EXTRACTED
        hit = base / rel
        if hit.is_file():
            return hit
    basename = Path(rel).name
    for hit in SBR14_EXTRACTED.rglob(basename):
        if hit.is_file():
            return hit
    return None