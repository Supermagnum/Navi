import pytest

from navi_voicegen.phrases import load_phrase_file
from navi_voicegen.review_gate import assert_review_gate


def test_reviewed_without_record_raises(tmp_path, monkeypatch):
    monkeypatch.setattr(
        "navi_voicegen.review_gate.RECORD_DIR",
        tmp_path,
    )
    with pytest.raises(SystemExit, match="reviewed:true"):
        assert_review_gate("xx", reviewed_file=True)


def test_en_hand_authored_record_passes():
    load_phrase_file("en")


def test_nb_unreviewed_loads():
    pf = load_phrase_file("nb")
    assert pf.reviewed_file is False