from pathlib import Path

import pytest
import yaml

from navi_voicegen.phrases import load_phrase_file, phrases_dir


def test_nb_zero_is_quoted_null_string():
    path = phrases_dir() / "nb.yaml"
    raw = yaml.safe_load(path.read_text(encoding="utf-8"))
    assert raw["phrases"]["zero"]["text"] == "null"


def test_load_rejects_yaml_null_text(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    phrases = tmp_path / "phrases"
    phrases.mkdir()
    (phrases / "xx.yaml").write_text(
        "lang_code: xx\nlanguage_folder: x\nreviewed: false\nphrases:\n  zero:\n    text: null\n",
        encoding="utf-8",
    )
    monkeypatch.setattr("navi_voicegen.phrases.phrases_dir", lambda: phrases)
    monkeypatch.setattr(
        "navi_voicegen.review_gate.assert_review_gate",
        lambda *_a, **_k: None,
    )
    with pytest.raises(ValueError, match="null text"):
        load_phrase_file("xx")
