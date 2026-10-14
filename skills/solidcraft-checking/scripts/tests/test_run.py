import json
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPTS = Path(__file__).parents[1]
sys.path.insert(0, str(SCRIPTS))
import run  # noqa: E402

BOX = [{"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10, "body_name": "Cube"}}]


def report(tmp_path, commands, *args):
    script = tmp_path / "script.json"
    script.write_text(json.dumps({"commands": commands}))
    return subprocess.run(
        [sys.executable, SCRIPTS / "run.py", script, *args], capture_output=True, text=True
    )


def test_reports_bodies(tmp_path):
    result = report(tmp_path, BOX)
    assert result.returncode == 0, result.stderr
    assert "BODY Cube: volume 1000 mm3, area 600 mm2, bbox (0, 0, 0)..(10, 10, 10), com (5, 5, 5)" in result.stdout


def test_flags_underconstrained_sketch(tmp_path):
    result = report(tmp_path, [
        {"command": "sketch.create", "params": {"plane": "XY", "name": "Loose"}},
        {"command": "sketch.circle.center", "params": {"center": [5, 5], "radius": 5}},
        {"command": "sketch.finish", "params": {}},
    ])
    assert result.returncode == 0, result.stderr
    assert 'SKETCH "Loose": dof 3, solved' in result.stdout


def test_reports_step_warnings(tmp_path):
    result = report(tmp_path, [
        {"command": "sketch.create", "params": {"plane": "XY", "offset": "12 mm", "name": "Plate"}},
        {"command": "sketch.rectangle.center", "params": {"center": [0, 0], "corner": [40, 40]}},
        {"command": "sketch.finish", "params": {}},
        {"command": "solid.extrude", "params": {"sketch": "Plate", "distance": "12 mm", "direction": "negative",
                                                "body_name": "P"}},
        {"command": "sketch.create", "params": {"plane": "XY", "name": "Hole"}},
        {"command": "sketch.circle.center", "params": {"center": [0, 0], "diameter": 20}},
        {"command": "sketch.finish", "params": {}},
        {"command": "solid.extrude", "params": {"sketch": "Hole", "distance": "12 mm", "taper": "-20 deg",
                                                "operation": "cut", "targets": ["P"], "name": "Flush cut"}},
    ])
    assert result.returncode == 0, result.stderr
    assert 'WARN "Flush cut": Boolean retry' in result.stdout


def test_fails_on_a_bad_step(tmp_path):
    result = report(tmp_path, [{"command": "solid.box", "params": {"width": 10}}])
    assert result.returncode == 1
    assert result.stdout.startswith("FAIL") and "step 1 (solid.box)" in result.stdout


def test_applies_a_script_to_a_saved_design(tmp_path):
    design = tmp_path / "cube.json"
    built = report(tmp_path, [
        {"command": "parameters.add", "params": {"name": "size", "expression": "10 mm"}},
        {"command": "solid.box", "params": {"length": "size", "width": "size", "height": "size", "body_name": "Cube"}},
    ], "--save", design)
    assert built.returncode == 0, built.stderr

    changed = report(tmp_path, [
        {"command": "parameters.change", "params": {"name": "size", "expression": "20 mm"}},
    ], "--in", design)
    assert changed.returncode == 0, changed.stderr
    assert "BODY Cube: volume 8000 mm3" in changed.stdout


def test_execute_returns_the_final_inspection():
    inspection = run.execute(BOX)
    assert [b["name"] for b in inspection["bodies"]] == ["Cube"]


def test_execute_raises_on_failure():
    with pytest.raises(run.SolidCraftError, match="step 1"):
        run.execute([{"command": "solid.box", "params": {"width": 10}}])


@pytest.fixture
def flush_design(tmp_path):
    """A cut 10 mm deep into a 12 mm plate; at depth 13 mm its tapered end lies on the plate top."""
    design = tmp_path / "plate.json"
    built = report(tmp_path, [
        {"command": "parameters.add", "params": {"name": "depth", "expression": "10 mm"}},
        {"command": "sketch.create", "params": {"plane": "XY", "name": "Plate sketch"}},
        {"command": "sketch.rectangle.center", "params": {"center": [0, 0], "corner": [40, 40]}},
        {"command": "sketch.finish", "params": {}},
        {"command": "solid.extrude", "params": {"sketch": "Plate sketch", "distance": "12 mm", "body_name": "P"}},
        {"command": "sketch.create", "params": {"plane": "XY", "offset": "-1 mm", "name": "Cut sketch"}},
        {"command": "sketch.circle.center", "params": {"center": [5, 5], "diameter": 20}},
        {"command": "sketch.finish", "params": {}},
        {"command": "solid.extrude", "params": {"sketch": "Cut sketch", "distance": "depth", "taper": "10 deg",
                                                "operation": "cut", "targets": ["P"], "name": "Cut"}},
    ], "--save", design)
    assert built.returncode == 0 and "WARN" not in built.stdout, built.stdout + built.stderr
    return design


def depth(value: str) -> dict:
    return {"command": "parameters.change", "params": {"name": "depth", "expression": value}}


def test_reports_warnings_after_a_parameter_change(tmp_path, flush_design):
    flush = report(tmp_path, [depth("13 mm")], "--in", flush_design)
    assert 'WARN "Cut": Boolean retry' in flush.stdout


def test_reports_every_inspection_in_the_script(tmp_path, flush_design):
    walk = report(tmp_path, [depth("13 mm"), {"command": "document.inspect", "params": {"measure": True}},
                             depth("10 mm")], "--in", flush_design)
    lines = walk.stdout.splitlines()
    assert any(line.startswith('step 2 WARN "Cut": Boolean retry') for line in lines), walk.stdout
    assert any(line.startswith("step 2 BODY P: volume") for line in lines)
    assert not any(line.startswith("WARN") for line in lines)


def test_reports_bodies_inspected_without_measurements(tmp_path):
    result = report(tmp_path, [*BOX, {"command": "document.inspect", "params": {}}])
    assert "step 2 BODY Cube\n" in result.stdout
    assert "BODY Cube: volume 1000 mm3" in result.stdout


def test_says_so_when_there_is_nothing_to_report(tmp_path):
    result = report(tmp_path, [{"command": "parameters.add", "params": {"name": "size", "expression": "10 mm"}}])
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "no bodies"
