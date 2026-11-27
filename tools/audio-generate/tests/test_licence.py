import pytest

from navi_voicegen.voices.licence import licence_allowed, validate_voice_blob


def test_licence_gate_rejects_missing():
    with pytest.raises(SystemExit):
        validate_voice_blob({"source_url": "https://example.com/a.mp3"}, candidate_id="x")


def test_licence_gate_rejects_npsc():
    with pytest.raises(SystemExit):
        validate_voice_blob(
            {
                "licence": "cc0",
                "source_url": "https://example.com/a.mp3",
                "dataset": "NPSC parliamentary",
            },
            candidate_id="npsc_bad",
        )


def test_licence_gate_accepts_pd():
    assert licence_allowed({"licence": "public-domain"})


def test_german_librivox_rejected_for_nb():
    with pytest.raises(SystemExit):
        validate_voice_blob(
            {
                "licence": "public-domain",
                "source_url": "https://archive.org/download/norwegischevolksmahrchenii_2509_librivox/x.mp3",
                "archive_language": "deu",
                "intended_lang_code": "nb",
            },
            candidate_id="folktales_bad",
        )