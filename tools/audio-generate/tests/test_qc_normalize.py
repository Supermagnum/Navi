from navi_voicegen.qc.normalize import normalize_text, tokens_for_wer, wer_cer


def test_en_number_word_roundtrip():
    assert normalize_text("30", "en") == normalize_text("thirty", "en")
    assert normalize_text("200", "en") == normalize_text("two hundred", "en")


def test_nb_number_word_roundtrip():
    assert normalize_text("30", "nb") == normalize_text("tretti", "nb")
    assert normalize_text("200", "nb") == normalize_text("to hundre", "nb")
    assert normalize_text("null", "nb") == normalize_text("0", "nb")


def test_arabic_normaliser_diacritics_and_digits():
    a = "اَلسَّلَامُ٠١٢"
    b = "السلام012"
    assert normalize_text(a, "ar") == normalize_text(b, "ar")
    # alef variants
    assert normalize_text("أحمد", "ar") == normalize_text("احمد", "ar")


def test_zh_ja_character_tokens():
    assert tokens_for_wer("左转", "zh") == list(normalize_text("左转", "zh"))
    wer, cer, _ = wer_cer("左转", "左转", "zh")
    assert wer == 0 and cer == 0
    wer, cer, _ = wer_cer("左转", "右转", "zh")
    assert cer > 0
