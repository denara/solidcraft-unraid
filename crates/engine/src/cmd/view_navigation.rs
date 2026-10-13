//! Stateless camera commands shared by the navigation cube and programmatic callers.
use serde_json::{Value, json};
use solidcraft_render::{Camera, StandardView};

use super::CommandSpec;
use crate::params::{bad, vec3};
use crate::{Result, Session};

pub static COMMANDS: &[CommandSpec] = &[CommandSpec::new("view.navigate", "Navigate View", navigate)
    .icon("orbit")
    .noundo()
    .params("camera: Camera; direction?: [x,y,z] in the camera's up-axis frame; or action: left|right|up|down|clockwise|counterclockwise|home|fit → camera, fit")];

fn navigate(_s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "view.navigate";
    let mut cam: Camera =
        serde_json::from_value(p.get("camera").cloned().ok_or_else(|| bad(cmd, "camera is required"))?).map_err(|_| bad(cmd, "invalid camera"))?;
    if !cam.is_valid()
        || !cam.fov.is_finite()
        || !(0.0..std::f64::consts::PI).contains(&cam.fov)
        || cam.yaw.abs() > 1e6
        || cam.roll.abs() > 1e6
        || cam.pitch.abs() > std::f64::consts::FRAC_PI_2
    {
        return Err(bad(cmd, "invalid camera"));
    }
    let mut fit = false;
    if let Some(value) = p.get("direction") {
        let dir = vec3(value)
            .filter(|v| v.is_finite() && v.len() <= 2.0)
            .and_then(|v| v.normalized())
            .ok_or_else(|| bad(cmd, "direction must be a finite nonzero vector of length at most 2"))?;
        let heading = cam.yaw;
        cam = cam.looking_from(cam.from_frame(dir));
        if dir.x.abs() < 1e-9 && dir.y.abs() < 1e-9 {
            cam.yaw = square_heading(heading);
        }
    } else {
        use std::f64::consts::FRAC_PI_2;
        let (r, u, _) = cam.basis();
        match p.get("action").and_then(Value::as_str) {
            Some("left") => cam = cam.looking_from(-r),
            Some("right") => cam = cam.looking_from(r),
            Some("up") => cam = cam.looking_from(u),
            Some("down") => cam = cam.looking_from(-u),
            Some("clockwise") => cam.roll = (cam.roll + FRAC_PI_2).rem_euclid(std::f64::consts::TAU),
            Some("counterclockwise") => cam.roll = (cam.roll - FRAC_PI_2).rem_euclid(std::f64::consts::TAU),
            Some("home") => {
                cam.set_view(StandardView::Iso);
                fit = true;
            }
            Some("fit") => fit = true,
            _ => return Err(bad(cmd, "unknown navigation action")),
        }
    }
    Ok(json!({"camera": cam, "fit": fit}))
}

/// At a halfway tie prefer the heading nearer the front plan view.
pub(crate) fn square_heading(yaw: f64) -> f64 {
    if !yaw.is_finite() {
        return 0.0;
    }
    let q = std::f64::consts::FRAC_PI_2;
    let lo = (yaw / q).floor() * q;
    let hi = lo + q;
    if ((yaw - lo) - (hi - yaw)).abs() < 1e-6 {
        let off_front = |a: f64| ((a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI).abs();
        if off_front(lo) <= off_front(hi) { lo } else { hi }
    } else if yaw - lo < hi - yaw {
        lo
    } else {
        hi
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solidcraft_geom::Vec3;
    use solidcraft_render::UpAxis;

    #[test]
    fn every_cube_direction_preserves_framing_without_editing_the_design() {
        for up in [UpAxis::Z, UpAxis::Y] {
            let mut s = Session::default();
            let before = s.doc.to_json();
            let cam = Camera { up, target: Vec3::new(10.0, 20.0, 30.0), distance: 123.0, ..Default::default() };
            for x in [-1.0, 0.0, 1.0] {
                for y in [-1.0, 0.0, 1.0] {
                    for z in [-1.0, 0.0, 1.0] {
                        let Some(dir) = Vec3::new(x, y, z).normalized() else { continue };
                        let result = s.execute("view.navigate", &json!({"camera":cam,"direction":[x,y,z]})).unwrap();
                        let to: Camera = serde_json::from_value(result["camera"].clone()).unwrap();
                        assert!(to.back().dist(cam.from_frame(dir)) < 2e-6);
                        assert_eq!((to.target, to.distance, to.fov, to.up), (cam.target, cam.distance, cam.fov, cam.up));
                    }
                }
            }
            assert_eq!(s.doc.to_json(), before);
            assert!(s.undo.is_empty());
        }
    }

    #[test]
    fn arrows_roll_and_home_are_programmatic_and_reject_hostile_input() {
        let mut s = Session::default();
        let cam = Camera::default();
        let (r, u, _) = cam.basis();
        for (action, dir) in [("left", -r), ("right", r), ("up", u), ("down", -u)] {
            let result = navigate(&mut s, &json!({"camera":cam,"action":action})).unwrap();
            let to: Camera = serde_json::from_value(result["camera"].clone()).unwrap();
            assert!(to.back().dist(dir) < 2e-6);
        }
        let result = navigate(&mut s, &json!({"camera":cam,"action":"clockwise"})).unwrap();
        let to: Camera = serde_json::from_value(result["camera"].clone()).unwrap();
        assert_eq!(to.back(), cam.back());
        assert!((to.basis().0 - u).len() < 1e-9);
        for action in ["home", "fit"] {
            assert_eq!(navigate(&mut s, &json!({"camera":cam,"action":action})).unwrap()["fit"], true);
        }
        for params in [
            json!({}),
            json!({"camera":cam,"direction":[0,0,0]}),
            json!({"camera":cam,"direction":[1e308,0,0]}),
            json!({"camera":cam,"action":"unknown"}),
        ] {
            assert!(s.execute("view.navigate", &params).is_err());
        }
    }
}
