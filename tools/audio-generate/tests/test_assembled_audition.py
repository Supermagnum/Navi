from navi_voicegen.assembled_audition import ASSEMBLED_INTER_CLIP_GAP_MS, NB_ASSEMBLED_SENTENCES


def test_gap_ms_documented():
    assert ASSEMBLED_INTER_CLIP_GAP_MS == 120
    assert len(NB_ASSEMBLED_SENTENCES) >= 3