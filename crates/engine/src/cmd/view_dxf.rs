//! A drawing of the visible bodies from a supplied viewport camera. No screen pixels, grid,
//! selection highlights or tessellation diagonals are exported.

#[path = "view_dxf_projection.rs"]
mod projection;

use serde_json::{Value, json};
use solidcraft_io::{DxfEntity, DxfLayer, DxfLinetype, DxfShape};
use solidcraft_render::Camera;

use super::CommandSpec;
use crate::params::{bad, str_};
use crate::{Result, Session};

const CMD: &str = "file.export_view_dxf";
pub static COMMANDS: &[CommandSpec] = &[CommandSpec::new(CMD, "Export Current View as DXF", export)
    .icon("export")
    .noundo()
    .params("camera: Camera (current view); hidden?: omit|dashed (default omit); tolerance?: 0.0001..1 mm (curve chord tolerance, default 0.01); path?: .dxf file (else returns dxf text). Visible placed bodies only; 2D LINE/LWPOLYLINE entities on VISIBLE/HIDDEN layers, mm; perspective projected onto the camera target plane; no viewport crop or overlays.")];

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let cam: Camera =
        serde_json::from_value(p.get("camera").cloned().ok_or_else(|| bad(CMD, "camera is required"))?).map_err(|_| bad(CMD, "invalid camera"))?;
    if !cam.is_valid()
        || !(0.0..std::f64::consts::PI).contains(&cam.fov)
        || cam.pitch.abs() > std::f64::consts::FRAC_PI_2
        || cam.yaw.abs() > 1e6
        || cam.roll.abs() > 1e6
    {
        return Err(bad(CMD, "invalid camera"));
    }
    let dashed = match p.get("hidden") {
        None => false,
        Some(Value::String(v)) if v == "omit" => false,
        Some(Value::String(v)) if v == "dashed" => true,
        _ => return Err(bad(CMD, "hidden must be omit or dashed")),
    };
    let tolerance = match p.get("tolerance") {
        None => 0.01,
        Some(v) => v.as_f64().filter(|t| (0.0001..=1.0).contains(t)).ok_or_else(|| bad(CMD, "tolerance must be 0.0001..1 mm"))?,
    };
    let path = match p.get("path") {
        None => None,
        Some(_) => Some(str_(p, "path").filter(|v| !v.trim().is_empty() && v.len() < 4096).ok_or_else(|| bad(CMD, "invalid path"))?),
    };
    if let Some(path) = path {
        solidcraft_io::vfs::check(path).map_err(|e| crate::EngineError::Other(e.to_string()))?;
    }
    let world = s.world_state();
    let mut meshes = Vec::new();
    let mut count = 0usize;
    for body in world.bodies.iter().filter(|b| s.body_visible(&b.name)) {
        let mesh = body.body.reference_display_mesh_angular(tolerance, solidcraft_kernel::DISPLAY_ANGLE)?;
        count = count.saturating_add(mesh.triangles.len()).saturating_add(mesh.edges.iter().map(Vec::len).sum::<usize>());
        if count > 1_000_000 {
            return Err(bad(CMD, "view too complex to export; hide bodies or increase tolerance"));
        }
        if mesh.triangles.is_empty() {
            return Err(bad(CMD, format!("{} has no exportable surface", body.name)));
        }
        meshes.push(mesh);
    }
    if meshes.is_empty() {
        return Err(bad(CMD, "no visible bodies to export"));
    }
    let lines = projection::drawing(&meshes, &cam).map_err(|e| bad(CMD, e))?;
    let mut visible = 0;
    let mut hidden = 0;
    let mut entities = Vec::new();
    for line in lines {
        if line.hidden && !dashed {
            continue;
        }
        let layer = if line.hidden {
            hidden += 1;
            "HIDDEN"
        } else {
            visible += 1;
            "VISIBLE"
        };
        entities.push(DxfEntity { layer: layer.into(), shape: DxfShape::Line(line.a, line.b) });
    }
    let entities = projection::connect(entities);
    let mut layers = vec![DxfLayer::new("VISIBLE", 7)];
    if dashed {
        layers.push(DxfLayer { linetype: DxfLinetype::Hidden, ..DxfLayer::new("HIDDEN", 8) });
    }
    let text = solidcraft_io::write_dxf_drawing(&layers, &entities);
    let mut result = json!({"visible":visible,"hidden":hidden,"units":"mm","tolerance":tolerance,"projection":if cam.fov > 0.0 {"perspective"} else {"orthographic"}});
    if let Some(path) = path {
        solidcraft_io::vfs::write(path, text.as_bytes()).map_err(|e| crate::EngineError::Other(e.to_string()))?;
        result["path"] = json!(path);
    } else {
        result["dxf"] = json!(text);
    }
    Ok(result)
}

#[cfg(test)]
#[path = "view_dxf_tests.rs"]
mod tests;
