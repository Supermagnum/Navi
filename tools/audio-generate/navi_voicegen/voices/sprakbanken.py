"""Språkbanken / NST local archive voice references."""

from __future__ import annotations

import shutil
import tarfile
import zipfile
from pathlib import Path

from navi_voicegen.paths import tool_root
from navi_voicegen.voices.process import sha256_file
from navi_voicegen.voices.sprakbanken_corpus import (
    CATALOGUE_URL as SBR14_CATALOGUE_URL,
    resolve_sbr14_member_path,
)

SPRAKBANKEN_ROOT = tool_root() / "cache" / "sprakbanken"


def sprakbanken_instructions(catalogue_id: str, *, member: str) -> str:
    return (
        f"Place Språkbanken NST audio for {catalogue_id} under:\n"
        f"  {SPRAKBANKEN_ROOT / catalogue_id / member}\n"
        "or drop the official archive as:\n"
        f"  {SPRAKBANKEN_ROOT / 'archives' / catalogue_id}.zip\n"
        f"  {SPRAKBANKEN_ROOT / 'archives' / catalogue_id}.tar.gz\n"
        "Catalogue page (verify CC-0 before use): "
        + (
            SBR14_CATALOGUE_URL
            if catalogue_id == "sbr-14"
            else f"https://www.nb.no/sprakbanken/en/resource-catalogue/oai-nb-no-{catalogue_id}/"
        )
        + "\n"
        "Do not use NPSC / Stortinget parliamentary corpora."
    )


def _maybe_extract_member(catalogue_id: str, member: str, dest_wav: Path) -> bool:
    archives = [
        SPRAKBANKEN_ROOT / "archives" / f"{catalogue_id}.zip",
        SPRAKBANKEN_ROOT / "archives" / f"{catalogue_id}.tar.gz",
        SPRAKBANKEN_ROOT / "archives" / f"{catalogue_id}.tgz",
    ]
    dest_wav.parent.mkdir(parents=True, exist_ok=True)
    for arc in archives:
        if not arc.is_file():
            continue
        if arc.suffix == ".zip":
            with zipfile.ZipFile(arc) as zf:
                hit = next((n for n in zf.namelist() if n.endswith(member) or n.endswith(member.lstrip("/"))), None)
                if not hit:
                    continue
                with zf.open(hit) as src, dest_wav.open("wb") as out:
                    shutil.copyfileobj(src, out)
                return True
        if arc.name.endswith((".tar.gz", ".tgz")):
            with tarfile.open(arc, "r:*") as tf:
                hit = next(
                    (m for m in tf.getmembers() if m.isfile() and m.name.endswith(member)),
                    None,
                )
                if not hit:
                    continue
                src = tf.extractfile(hit)
                if src is None:
                    continue
                with src, dest_wav.open("wb") as out:
                    shutil.copyfileobj(src, out)
                return True
    return False


def resolve_sprakbanken_download(catalogue_id: str, member: str, download_dest: Path) -> None:
    if catalogue_id == "sbr-14":
        hit = resolve_sbr14_member_path(member)
        if hit is not None and hit.is_file():
            download_dest.parent.mkdir(parents=True, exist_ok=True)
            if download_dest.resolve() != hit.resolve():
                shutil.copy2(hit, download_dest)
            return
    cached = SPRAKBANKEN_ROOT / catalogue_id / member
    if cached.is_file():
        download_dest.parent.mkdir(parents=True, exist_ok=True)
        if download_dest.resolve() != cached.resolve():
            shutil.copy2(cached, download_dest)
        return
    if _maybe_extract_member(catalogue_id, member, download_dest):
        return
    raise SystemExit(
        "Språkbanken voice reference not found in local cache.\n\n"
        + sprakbanken_instructions(catalogue_id, member=member)
    )


def sprakbanken_member_sha256(catalogue_id: str, member: str) -> str:
    cached = SPRAKBANKEN_ROOT / catalogue_id / member
    if cached.is_file():
        return sha256_file(cached)
    tmp = SPRAKBANKEN_ROOT / catalogue_id / ".partial" / Path(member).name
    resolve_sprakbanken_download(catalogue_id, member, tmp)
    return sha256_file(tmp)