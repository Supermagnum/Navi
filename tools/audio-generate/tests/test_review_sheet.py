from pathlib import Path

import yaml

from navi_voicegen.phrases import phrases_dir
from navi_voicegen.review_sheet import apply_review_sheet, write_review_sheet


def test_review_sheet_round_trip(tmp_path, monkeypatch):
    phrases = phrases_dir()
    monkeypatch.setattr("navi_voicegen.review_sheet.repo_root", lambda: tmp_path)
    nb_yaml = phrases / "nb.yaml"
    nb_yaml.parent.mkdir(parents=True, exist_ok=True)
    nb_yaml.write_text(
        yaml.safe_dump(
            {
                "lang_code": "nb",
                "language_folder": "norwegian",
                "reviewed": False,
                "phrases": {
                    "left": {"text": "Sving til venstre", "reviewed": False},
                    "right": {"text": "Sving til høyre", "reviewed": False},
                },
            },
            sort_keys=False,
            allow_unicode=True,
        ),
        encoding="utf-8",
    )
    sheet = write_review_sheet("nb")
    lines = sheet.read_text(encoding="utf-8").splitlines()
    for i, line in enumerate(lines):
        if line.startswith("left\t"):
            parts = line.split("\t")
            parts[2] = "yes"
            lines[i] = "\t".join(parts)
            break
    sheet.write_text("\n".join(lines) + "\n", encoding="utf-8")
    out = apply_review_sheet(sheet)
    assert out == nb_yaml
    data = yaml.safe_load(nb_yaml.read_text(encoding="utf-8"))
    assert data["phrases"]["left"]["reviewed"] is True