from pathlib import Path

from navi_voicegen.state import load_state, phrase_inputs_hash, save_state, should_skip


def test_phrase_hash_changes_with_seed():
    entries = [("left", "Turn left", True)]
    a = phrase_inputs_hash(
        lang_code="en", gender="male", engine_id="chatterbox", seed=1, entries=entries, suffix=None
    )
    b = phrase_inputs_hash(
        lang_code="en", gender="male", engine_id="chatterbox", seed=2, entries=entries, suffix=None
    )
    assert a != b


def test_force_skips_skip_logic(tmp_path: Path):
    clip = tmp_path / "left.opus"
    clip.write_bytes(b"x")
    state = {}
    h = "deadbeef"
    assert should_skip(state, "male/left", h, clip, force=False) is False
    state["clips"] = {"male/left": {"hash": h}}
    assert should_skip(state, "male/left", h, clip, force=False) is True
    assert should_skip(state, "male/left", h, clip, force=True) is False


def test_state_roundtrip(tmp_path: Path):
    path = tmp_path / "state.json"
    save_state(path, {"clips": {"k": {"hash": "1"}}})
    assert load_state(path)["clips"]["k"]["hash"] == "1"