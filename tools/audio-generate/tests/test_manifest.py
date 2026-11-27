from pathlib import Path

from navi_voicegen.manifest_out import ClipRecord, write_language_manifest


def test_manifest_aggregate_bytes(tmp_path: Path):
    clips = [
        ClipRecord("left", "male", "male/left.opus", 8000, "aa" * 32, True, "Turn left"),
        ClipRecord("right", "male", "male/right.opus", 9000, "bb" * 32, True, "Turn right"),
    ]
    man = write_language_manifest(
        tmp_path,
        language_folder="english",
        lang_code="en",
        pack_style="fragment",
        clips=clips,
        authoring={"tool": "navi-voicegen"},
        template_source="hand-written",
        template_commit=None,
    )
    text = man.read_text(encoding="utf-8")
    assert '"aggregate_bytes": 17000' in text
    assert '"clip_count": 2' in text