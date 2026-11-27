from navi_voicegen.voices.config import language_voice_config, load_voices_yaml


def test_voices_yaml_en_nb_have_hashes():
    raw = load_voices_yaml()
    for lang in ("en", "nb"):
        for gender in ("male", "female"):
            cands = raw["languages"][lang][gender]["candidates"]
            assert len(cands) >= 1
            for cid, blob in cands.items():
                assert len(blob["download_sha256"]) == 64, f"{lang}/{gender}/{cid}"
                assert blob["source_url"].startswith("https://")


def test_language_voice_config_selected():
    cfg = language_voice_config("en")
    assert cfg.male is not None
    assert cfg.male.selected == "walden_gord"
    assert "walden_gord" in cfg.male.candidates