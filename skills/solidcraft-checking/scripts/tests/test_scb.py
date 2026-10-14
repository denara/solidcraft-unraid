import json
import sys
from math import pi, sqrt
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parents[1]))
import run  # noqa: E402
import scb  # noqa: E402


def built(script: scb.Script, sketch: str) -> dict:
    """Inspection after extruding the sketch 1 mm."""
    script.cmd("solid.extrude", sketch=sketch, distance="1 mm", body_name="Body")
    inspection = run.execute(script.commands)
    assert [(s["name"], s["dof"], s["status"]) for s in inspection["sketches"]] == [(sketch, 0, "solved")]
    return inspection["bodies"][0]


def box(body: dict) -> tuple:
    return (*body["bbox"]["min"], *body["bbox"]["max"])


HALF_HEIGHT = 20 / sqrt(3)  # centre to vertex of a hexagon 20 mm across flats


def test_hexagon_on_the_x_axis():
    script = scb.Script()
    script.hexagon("Hex", across_flats="20 mm", at=(-30, 0), x="30 mm")
    assert box(built(script, "Hex")) == pytest.approx((-40, -HALF_HEIGHT, 0, -20, HALF_HEIGHT, 1), abs=1e-6)


def test_hexagon_off_both_axes_on_an_offset_plane():
    script = scb.Script()
    script.hexagon("Hex", across_flats="20 mm", at=(-30, 15), x="30 mm", y="15 mm", offset="5 mm")
    assert box(built(script, "Hex")) == pytest.approx(
        (-40, 15 - HALF_HEIGHT, 5, -20, 15 + HALF_HEIGHT, 6), abs=1e-6
    )


def test_hexagon_at_the_origin():
    script = scb.Script()
    script.hexagon("Hex", across_flats="20 mm", at=(0, 0))
    assert box(built(script, "Hex")) == pytest.approx((-10, -HALF_HEIGHT, 0, 10, HALF_HEIGHT, 1), abs=1e-6)


def test_hexagon_stays_regular_when_its_parameter_changes():
    script = scb.Script()
    script.parameter("across", "20 mm")
    script.hexagon("Hex", across_flats="across", at=(-30, 0), x="30 mm")
    script.cmd("parameters.change", name="across", expression="34 mm")
    half_height = 34 / sqrt(3)
    assert box(built(script, "Hex")) == pytest.approx((-47, -half_height, 0, -13, half_height, 1), abs=1e-6)


def test_hexagon_needs_a_position_unless_at_the_origin():
    with pytest.raises(ValueError):
        scb.Script().hexagon("Hex", across_flats="20 mm", at=(-30, 0))


def test_circle():
    script = scb.Script()
    script.circle("Round", diameter="10 mm", at=(20, -10), x="20 mm", y="10 mm")
    assert box(built(script, "Round")) == pytest.approx((15, -15, 0, 25, -5, 1), abs=1e-3)


def test_rounded_rectangle_symmetric_about_the_y_axis():
    script = scb.Script()
    script.rounded_rectangle("Plate", width="100 mm", height="50 mm", bottom="10 mm", corner_radius="5 mm",
                             at=(-50, -10, 50, 40))
    body = built(script, "Plate")
    assert box(body) == pytest.approx((-50, -10, 0, 50, 40, 1), abs=1e-6)
    assert body["volume_mm3"] == pytest.approx(100 * 50 - (4 - pi) * 25, rel=1e-4)


def test_parameter():
    script = scb.Script()
    script.parameter("size", "2 * 10 mm", comment="across the flats")
    params = run.execute(script.commands)["params"]
    assert [(p["name"], p["value"], p["comment"]) for p in params] == [("size", 20, "across the flats")]


def test_save(tmp_path):
    script = scb.Script()
    script.cmd("solid.box", length=10, width=10, height=10)
    script.save(tmp_path / "build.json")
    assert json.loads((tmp_path / "build.json").read_text()) == {
        "commands": [{"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10}}]
    }


def test_rejects_a_repeated_name():
    # SolidCraft resolves a name to its first match, so a repeat silently points at the wrong feature
    script = scb.Script()
    script.circle("Hole sketch", diameter="5 mm", at=(10, 10), x="10 mm", y="10 mm")
    with pytest.raises(ValueError, match="Hole sketch"):
        script.circle("Hole sketch", diameter="5 mm", at=(20, 10), x="20 mm", y="10 mm")
    script.cmd("solid.extrude", sketch="Hole sketch", distance="1 mm", name="Cut")
    with pytest.raises(ValueError, match="Cut"):
        script.cmd("solid.extrude", sketch="Hole sketch", distance="2 mm", name="Cut")
    with pytest.raises(ValueError, match="Hole sketch"):
        script.cmd("solid.cylinder", diameter="5 mm", height="1 mm", name="Hole sketch")


def test_parameter_names_are_not_feature_names():
    script = scb.Script()
    script.parameter("size", "10 mm")
    script.cmd("parameters.change", name="size", expression="20 mm")
    script.cmd("solid.box", length="size", width="size", height="size", name="size")


@pytest.mark.parametrize("at, x, y", [((0, 0), "10 mm", None), ((10, 0), None, "5 mm")])
def test_place_rejects_a_dimension_that_starts_at_zero(at, x, y):
    with pytest.raises(ValueError, match="starts at 0"):
        scb.Script().circle("Round", diameter="5 mm", at=at, x=x, y=y)


def test_rejects_a_repeated_body_name():
    # SolidCraft renames the second body silently, so targets would hit only the first
    script = scb.Script()
    script.cmd("solid.box", length=10, width=10, height=10, body_name="M")
    with pytest.raises(ValueError, match="M"):
        script.cmd("solid.cylinder", diameter="5 mm", height="5 mm", body_name="M")


def test_cmd_passes_a_nested_command_parameter():
    script = scb.Script()
    script.cmd("sketch.create", plane="XY", name="Plate sketch")
    script.cmd("sketch.draw.dimensioned", command="sketch.rectangle.center", params={"center": [0, 0], "corner": [5, 3]},
               dimensions={"width": "10 mm", "height": "6 mm"})
    script.cmd("sketch.finish")
    script.cmd("solid.extrude", sketch="Plate sketch", distance="1 mm", body_name="Plate")
    assert run.execute(script.commands)["bodies"][0]["volume_mm3"] == pytest.approx(60)


def test_rounded_rectangle_rejects_a_bottom_that_starts_at_zero():
    with pytest.raises(ValueError, match="starts at 0"):
        scb.Script().rounded_rectangle("Plate sketch", width="100 mm", height="50 mm", bottom="10 mm",
                                       corner_radius="5 mm", at=(-50, 0, 50, 50))


@pytest.mark.parametrize("command", ["surface.patch", "sheet.flange", "mesh.convert", "file.insert_step"])
def test_names_from_other_feature_commands_count_too(command):
    script = scb.Script()
    script.cmd("solid.box", length=10, width=10, height=10, name="Part")
    with pytest.raises(ValueError, match="Part"):
        script.cmd(command, name="Part")


@pytest.mark.parametrize("command", ["sheet.manage_rules", "plastic.manage_rules"])
def test_rule_names_are_not_feature_names(command):
    script = scb.Script()
    script.cmd("solid.box", length=10, width=10, height=10, name="Part")
    script.cmd(command, name="Part")
    script.cmd(command, name="Part")
