import tarfile
from pathlib import Path

import pytest

from navi_voicegen.voices.sprakbanken_corpus import (
    SBR14_ARCHIVES,
    assert_tar_safe,
    corpus_file_ready,
    fetch_sprakbanken_sbr14,
    is_safe_tar_member,
    read_sha256sums,
    safe_tar_extract,
)
from navi_voicegen.voices.config import load_voices_yaml


def test_is_safe_tar_member_rejects_traversal():
    assert not is_safe_tar_member("/etc/passwd")
    assert not is_safe_tar_member("../escape.wav")
    assert not is_safe_tar_member("audio/../../tmp/x.wav")
    assert is_safe_tar_member("channel_1/0001.wav")


def test_assert_tar_safe_rejects_dotdot(tmp_path: Path):
    bad = tmp_path / "bad.tar"
    with tarfile.open(bad, "w") as tf:
        info = tarfile.TarInfo(name="../evil.txt")
        info.size = 0
        tf.addfile(info)
    with pytest.raises(ValueError, match="unsafe tar member"):
        assert_tar_safe(bad)


def test_safe_tar_extract_writes_only_under_dest(tmp_path: Path):
    arc = tmp_path / "ok.tar"
    with tarfile.open(arc, "w") as tf:
        data = b"hi"
        for name in ("a/1.txt", "b/c/2.txt"):
            info = tarfile.TarInfo(name=name)
            info.size = len(data)
            tf.addfile(info, fileobj=__import__("io").BytesIO(data))
    dest = tmp_path / "out"
    safe_tar_extract(arc, dest)
    assert (dest / "a" / "1.txt").read_bytes() == b"hi"
    assert (dest / "b" / "c" / "2.txt").is_file()


def test_corpus_fetch_skips_when_size_and_sha_match(monkeypatch, tmp_path: Path):
    arch_dir = tmp_path / "archives"
    arch_dir.mkdir()
    changelog = arch_dir / "202006_changelog.txt"
    changelog.write_text("x", encoding="utf-8")

    import navi_voicegen.voices.sprakbanken_corpus as sc

    monkeypatch.setattr(sc, "SBR14_ARCHIVES", arch_dir)
    monkeypatch.setattr(sc, "SBR14_ROOT", tmp_path)
    monkeypatch.setattr(sc, "SBR14_SHA256SUMS", tmp_path / "SHA256SUMS")
    monkeypatch.setattr(sc, "SBR14_EXTRACTED", tmp_path / "extracted")
    monkeypatch.setattr(
        sc,
        "SBR14_ARCHIVES_SPEC",
        (
            sc.CorpusArchive(
                "202006_changelog.txt",
                "https://example.invalid/changelog",
                1,
            ),
        ),
    )

    digest = sc.sha256_file(changelog)
    sc.write_sha256sums(tmp_path / "SHA256SUMS", {"202006_changelog.txt": digest})

    calls: list[str] = []

    def fake_download(url: str, dest: Path, **kwargs):
        calls.append(url)

    monkeypatch.setattr(sc, "download_resumable", fake_download)
    fetch_sprakbanken_sbr14(extract=False)
    assert calls == []
    assert corpus_file_ready(sc.SBR14_ARCHIVES_SPEC[0])


def test_voices_yaml_sbr14_has_licence_url_and_sha():
    raw = load_voices_yaml()
    male = raw["languages"]["nb"]["male"]
    assert male["selected"].startswith("sbr14_")
    for _cid, blob in male["candidates"].items():
        if blob.get("catalogue_id") != "sbr-14":
            continue
        assert blob.get("licence_url", "").startswith("https://creativecommons.org/")
        assert blob.get("catalogue_url", "").startswith("https://www.nb.no/")
        sha = str(blob["download_sha256"])
        assert len(sha) == 64 and sha != "0" * 64, "pin download_sha256 for sbr-14 clips"


def test_nb_male_default_is_sprakbanken_sbr14():
    raw = load_voices_yaml()
    male = raw["languages"]["nb"]["male"]
    assert "sbr15" not in male.get("selected", "")
    assert "sbr-15" not in str(male.get("candidates") or {})
    sbr = male["candidates"][male["selected"]]
    assert sbr["fetch_kind"] == "sprakbanken"
    assert sbr["catalogue_id"] == "sbr-14"