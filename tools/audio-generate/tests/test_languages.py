import pytest

from navi_voicegen.languages import (
    PackStyle,
    resolve_language_codes,
    resolve_pack_style,
    SPECS,
)


def test_language_list_tiers():
    assert resolve_language_codes("tier1") == list(
        ("en", "fr", "de", "ru", "ar", "zh", "ja", "uk", "hi")
    )
    assert len(resolve_language_codes("all")) == len(SPECS)


def test_language_aliases():
    assert resolve_language_codes("english") == ["en"]
    assert resolve_language_codes("norwegian") == ["nb"]


def test_mode_auto_section6_bias():
    assert resolve_pack_style(SPECS["en"], "auto") == PackStyle.FRAGMENT
    assert resolve_pack_style(SPECS["nb"], "auto") == PackStyle.WHOLE_PHRASE
    assert resolve_pack_style(SPECS["zh"], "auto") == PackStyle.HYBRID


def test_unknown_lang_raises():
    with pytest.raises(ValueError):
        resolve_language_codes("klingon")