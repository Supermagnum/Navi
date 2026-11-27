from navi_voicegen.languages import SPECS, folder_name_for, language_folder_table, resolve_language_codes


def test_folder_mapping_table_aligns_with_specs():
    for code, folder, tier, cb in language_folder_table():
        spec = SPECS[code]
        assert spec.folder == folder
        assert spec.tier == tier
        assert spec.chatterbox_id == cb


def test_folder_name_for_nb():
    assert folder_name_for("nb") == "norwegian"
    assert resolve_language_codes("norwegian") == ["nb"]