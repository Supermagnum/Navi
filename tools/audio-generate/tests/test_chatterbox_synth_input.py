from navi_voicegen.engines.chatterbox_engine import _synth_candidates, _synth_input


def test_short_tokens_prefer_bare_text_not_now():
    assert _synth_input("Forty") == "Forty"
    assert _synth_input("To") == "To"
    assert _synth_input("Førti") == "Førti"
    assert _synth_input("Zero") == "Zero"


def test_short_token_fallbacks_put_now_last():
    cands = _synth_candidates("Two")
    assert cands[0] == "Two"
    assert cands[1:4] == ["Two.", "Two…", "Two,"]
    assert cands[-2] == "Two now"
    assert cands[-1] == "Two Two"
    assert cands.index("Two now") > cands.index("Two.")


def test_english_aliases_still_apply():
    assert _synth_input("One") == "Wun"
    cands = _synth_candidates("One")
    assert cands[0] == "Wun"
    assert "One" in cands
    assert cands[-2] == "Wun now"


def test_multi_word_phrases_unchanged():
    phrase = "Turn left"
    assert _synth_candidates(phrase) == [phrase]
