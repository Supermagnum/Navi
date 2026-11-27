from navi_voicegen.pack_plan import format_mode_report, plan_for_language


def test_mode_dry_run_deterministic():
    a = plan_for_language("nb", "fragment")
    b = plan_for_language("nb", "fragment")
    assert a == b
    assert a.clips_per_gender == 50
    assert a.estimated_bytes_per_gender == 50 * 8_000


def test_whole_mode_more_clips_than_fragment():
    frag = plan_for_language("nb", "fragment")
    whole = plan_for_language("nb", "whole_phrase")
    assert whole.clips_per_gender > frag.clips_per_gender


def test_format_mode_report_contains_modes():
    text = format_mode_report("nb")
    assert "fragment" in text
    assert "hybrid" in text
    assert "whole_phrase" in text