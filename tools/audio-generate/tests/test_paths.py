from pathlib import Path

from navi_voicegen.paths import clip_path, default_out_dir


def test_clip_path_layout():
    root = Path("/tmp/out")
    p = clip_path(root, "english", "male", "left", "opus")
    assert p == Path("/tmp/out/english/male/left.opus")


def test_unreviewed_subfolder():
    root = Path("/tmp/out")
    p = clip_path(root, "norwegian", "female", "left", "mp3", unreviewed=True)
    assert p == Path("/tmp/out/norwegian/female/unreviewed/left.mp3")


def test_default_out_under_repo_build():
    out = default_out_dir()
    assert out.name == "sounds"
    assert out.parent.name == "build"