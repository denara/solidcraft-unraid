import subprocess
import sys
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).parents[1]
sys.path.insert(0, str(SCRIPTS))
import run  # noqa: E402


def cube_step(path: Path, x: float) -> Path:
    run.execute([
        {"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10, "body_name": "Cube"}},
        {"command": "solid.move", "params": {"bodies": ["Cube"], "translate": [x, 0, 0]}},
        {"command": "file.export", "params": {"path": str(path), "bodies": ["Cube"]}},
    ])
    return path


@pytest.fixture
def cubes(tmp_path):
    return cube_step(tmp_path / "a.step", 0), cube_step(tmp_path / "b.step", 1)


def compare(*args):
    return subprocess.run([sys.executable, SCRIPTS / "compare_step.py", *args], capture_output=True, text=True)


def test_identical_solids_have_no_difference(cubes):
    a, _ = cubes
    result = compare(a, a)
    assert result.returncode == 0, result.stderr
    assert "xor 0 mm3" in result.stdout


def test_shifted_solid_shows_where_it_differs(cubes):
    a, b = cubes
    result = compare(b, a)
    assert result.returncode == 1, result.stderr
    assert "xor 200 mm3 in 2 pieces" in result.stdout
    assert "only in actual: 100 mm3 at (10, 0, 0)..(11, 10, 10)" in result.stdout
    assert "only in expected: 100 mm3 at (0, 0, 0)..(1, 10, 10)" in result.stdout


def test_tolerance(cubes):
    a, b = cubes
    result = compare(b, a, "--tolerance", "500")
    assert result.returncode == 0, result.stderr
    assert "xor 200 mm3 in 2 pieces" in result.stdout


def two_boxes_step(path: Path, shift: float) -> Path:
    run.execute([
        {"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10, "body_name": "A"}},
        {"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10, "body_name": "B"}},
        {"command": "solid.move", "params": {"bodies": ["B"], "translate": [shift, 0, 0]}},
        {"command": "file.export", "params": {"path": str(path), "bodies": ["A", "B"]}},
    ])
    return path


def test_overlapping_solids_in_one_file_count_once(tmp_path):
    result = compare(two_boxes_step(tmp_path / "a.step", 5), two_boxes_step(tmp_path / "b.step", 10))
    assert "volume 1500 vs 2000 mm3" in result.stdout, result.stdout + result.stderr
    assert "xor 500 mm3" in result.stdout
