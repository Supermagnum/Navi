from navi_voicegen.voices.config import language_voice_config, load_voices_yaml


def test_voices_yaml_en_nb_have_hashes():
    raw = load_voices_yaml()
    for lang in ("en", "nb"):
        for gender in ("male", "female"):
            cands = raw["languages"][lang][gender]["candidates"]
            assert len(cands) >= 1
            for cid, blob in cands.items():
                assert len(str(blob["download_sha256"])) == 64, f"{lang}/{gender}/{cid}"
                url = str(blob.get("source_url") or blob.get("catalogue_url") or "")
                assert url.startswith("https://"), f"{lang}/{gender}/{cid}"


def test_language_voice_config_selected():
    cfg = language_voice_config("en")
    assert cfg.male is not None
    assert cfg.male.selected == "walden_gord"
    assert "walden_gord" in cfg.male.candidates


def test_nb_male_default_is_sprakbanken_not_german_librivox():
    raw = load_voices_yaml()
    male = raw["languages"]["nb"]["male"]
    assert male["selected"] == "sbr15_eastern_male"
    assert "folktales_02" not in (male.get("candidates") or {})
    sbr = male["candidates"]["sbr15_eastern_male"]
    assert sbr["fetch_kind"] == "sprakbanken"
    assert sbr["catalogue_id"] == "sbr-15"
    assert sbr["licence"] == "cc0"