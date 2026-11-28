import pytest

from navi_voicegen.qc.config import assert_generate_allowed, load_qc_config, thresholds_for
from navi_voicegen.phrases import PhraseFile, PhraseEntry, write_phrase_file, load_phrase_file
from navi_voicegen.qc.machine_check import _maneuver_check


def test_thresholds_language_override():
    cfg = load_qc_config()
    nb = thresholds_for("nb", cfg)
    assert nb.get("cer_warn") == 0.18


def test_generate_allowlist_refuses_ar_zh():
    with pytest.raises(SystemExit, match="refused"):
        assert_generate_allowed("ar", action="generation")
    with pytest.raises(SystemExit, match="refused"):
        assert_generate_allowed("zh", action="audition")


def test_machine_checked_never_counts_as_reviewed(tmp_path, monkeypatch):
    phrases = tmp_path / "phrases"
    phrases.mkdir()
    monkeypatch.setattr("navi_voicegen.phrases.phrases_dir", lambda: phrases)
    monkeypatch.setattr("navi_voicegen.review_gate.assert_review_gate", lambda *_a, **_k: None)
    pf = PhraseFile(
        lang_code="xx",
        language_folder="x",
        reviewed_file=False,
        template_source=None,
        template_commit=None,
        entries=[PhraseEntry("left", "gauche", False)],
        review_status="machine_checked",
    )
    path = phrases / "xx.yaml"
    write_phrase_file(path, pf)
    loaded = load_phrase_file("xx")
    assert loaded.review_status == "machine_checked"
    assert loaded.reviewed_file is False
    assert loaded.is_natively_reviewed is False


def test_maneuver_left_right_swap_fails():
    status, reason = _maneuver_check("left", "turn right", "Turn left")
    assert status == "FAIL"
    assert "swapped" in reason or "missing" in reason
