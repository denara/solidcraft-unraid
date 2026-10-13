# Print a build result, or write it as the case's expected.json (+ reference.stl).
# Run by run.sh under FreeCAD's interpreter (plain Python; no FreeCAD API used here).
import json
import os
import shutil
import sys

case = os.environ["CASE"]
res = json.load(open(os.environ["OUT"]))
name = res["case"]
write = os.environ.get("WRITE") == "1"
stl = os.environ.get("STL") or ""

def emit():
    if "error" in res:
        print("%s: ERROR %s" % (name, res["error"]))
        if os.environ.get("ORACLE_VERBOSE"):
            print(res.get("trace", ""))
    else:
        c = res["center_of_mass"]
        print("%s: bodies %d  V %.6f  A %.6f  com [%.4f, %.4f, %.4f]  bbox %s..%s%s%s" % (
            name, res["body_count"], res["volume_mm3"], res["area_mm2"], c[0], c[1], c[2],
            [round(x, 4) for x in res["bbox"]["min"]], [round(x, 4) for x in res["bbox"]["max"]],
            "" if res.get("valid", True) else "  (INVALID SHAPE)",
            ("  notes: " + "; ".join(res["notes"])) if res.get("notes") else ""))
        if write:
            path = os.path.join(case, "expected.json")
            if os.path.exists(path):
                old = json.load(open(path))
                if str(old.get("source", "")).startswith("analytic"):
                    print("%s: expected.json is analytic; not overwritten" % name)
                    return 0
            if not res.get("valid", True):
                print("%s: FreeCAD shape is invalid; not written" % name)
                return 1
            method = res["method"]
            if res.get("method_extra"):
                method += "; " + res["method_extra"]
            exp = {
                "source": "freecad %s" % res["version"],
                "method": method,
                "body_count": res["body_count"],
                "volume_mm3": res["volume_mm3"],
                "area_mm2": res["area_mm2"],
                "center_of_mass": res["center_of_mass"],
                "bbox": res["bbox"],
            }
            for k in ("compare", "tolerance", "note"):
                if k in res:
                    exp[k] = res[k]
            if res.get("stl_skipped"):
                exp["note"] = (exp.get("note", "") + " " if exp.get("note") else "") + "No reference.stl: " + res["stl_skipped"] + "."
            with open(path, "w") as fh:
                json.dump(exp, fh, indent=2)
                fh.write("\n")
            ref = os.path.join(case, "reference.stl")
            if stl and os.path.exists(stl):
                shutil.copyfile(stl, ref)
                print("%s: wrote expected.json, reference.stl (%d bytes, deflection %s mm)" % (name, os.path.getsize(ref), res.get("stl_deflection_mm")))
            else:
                print("%s: wrote expected.json (no STL)" % name)
    return 0


emit()
sys.stdout.flush()
