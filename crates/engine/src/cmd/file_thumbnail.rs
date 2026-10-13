//! A portable picture from warm display meshes only; never evaluate on save.
use crate::Session;
use solidcraft_geom::Aabb3;
use solidcraft_render::{Camera, Rgb, Scene, SceneMesh, StandardView};

pub(super) fn saved(s: &Session) -> Option<Vec<u8>> {
    if s.active_sketch.is_some() || !s.pending_moves.is_empty() || s.model.results.iter().any(|r| r.error.is_some()) {
        return None;
    }
    let world = s.cached_world_state()?;
    if world.bodies.is_empty() || world.bodies.len() > 1024 {
        return None;
    }
    let mut scene = Scene::default();
    let mut bounds = Aabb3::EMPTY;
    let mut triangles = 0usize;
    let mut points = 0usize;
    for body in world.bodies.iter().filter(|b| s.body_visible(&b.name)) {
        let mesh = body.cached_mesh()?;
        triangles = triangles.saturating_add(mesh.triangles.len());
        points = points.saturating_add(mesh.positions.len());
        if triangles > 100_000 || points > 100_000 {
            return None;
        }
        bounds = bounds.union(&mesh.bounds());
        let channel = |x: f32| (x.clamp(0., 1.) * 255.).round() as u8;
        let color = s.doc.body_color(body).map_or(crate::view::colors::BODY, |[r, g, b]| Rgb(channel(r), channel(g), channel(b)));
        let face_colors =
            s.doc.face_colors(body).into_iter().map(|(face, look)| (face as u32, Rgb(look.color[0], look.color[1], look.color[2]))).collect();
        scene.meshes.push(SceneMesh { mesh, color, face_colors });
    }
    if bounds.is_empty() {
        return None;
    }
    let mut camera = Camera::default();
    camera.set_view(StandardView::Iso);
    camera.fit(&bounds);
    camera.distance *= 0.7;
    scene.radius = (bounds.diagonal() * 0.5).max(camera.half_height()).max(10.);
    scene.background = Some((crate::view::colors::BG_TOP, crate::view::colors::BG_BOTTOM));
    let [width, height] = solidcraft_io::THUMBNAIL_SIZE;
    if !within_raster_budget(&scene, &camera, width, height) {
        return None;
    }
    let image = solidcraft_render::render(&scene, &camera, width, height);
    solidcraft_render::png(width, height, image.rgba)
}

// Bound overlapping-triangle work too, not only mesh size. The renderer uses
// 2x supersampling; conservatively count each projected triangle's pixel box.
fn within_raster_budget(scene: &Scene, camera: &Camera, width: usize, height: usize) -> bool {
    let (w, h) = ((width * 2) as f64, (height * 2) as f64);
    let matrix = camera.view_proj(w / h, scene.radius.max(1.));
    let mut pixels = 0usize;
    let mut colors = 0usize;
    for entry in &scene.meshes {
        colors = colors.saturating_add(entry.mesh.triangles.len().saturating_mul(entry.face_colors.len()));
        if colors > 1_000_000 {
            return false;
        }
        let mut projected = Vec::with_capacity(entry.mesh.positions.len());
        for point in &entry.mesh.positions {
            let q = matrix.apply(*point);
            if q.iter().any(|x| !x.is_finite()) || q[3] <= 1e-9 {
                return false;
            }
            let pixel = [(q[0] / q[3] + 1.) * 0.5 * w, (1. - q[1] / q[3]) * 0.5 * h];
            if pixel.iter().any(|x| !x.is_finite()) {
                return false;
            }
            projected.push(pixel);
        }
        for triangle in &entry.mesh.triangles {
            let [Some(a), Some(b), Some(c)] = triangle.map(|i| projected.get(i as usize).copied()) else {
                return false;
            };
            let dx = (a[0].max(b[0]).max(c[0]).ceil().clamp(0., w) - a[0].min(b[0]).min(c[0]).floor().clamp(0., w) + 2.) as usize;
            let dy = (a[1].max(b[1]).max(c[1]).ceil().clamp(0., h) - a[1].min(b[1]).min(c[1]).floor().clamp(0., h) + 2.) as usize;
            pixels = pixels.saturating_add(dx.saturating_mul(dy));
            if pixels > 4_000_000 {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_thumbnail_refuses_excessive_overdraw() {
        let mut session = Session::default();
        session.execute("solid.box", &serde_json::json!({"length":10,"width":20,"height":30})).unwrap();
        let mesh = session.world_state().bodies[0].mesh();
        let bounds = mesh.bounds();
        let mut camera = Camera::default();
        camera.set_view(StandardView::Iso);
        camera.fit(&bounds);
        let mut scene = Scene { radius: bounds.diagonal(), ..Scene::default() };
        scene.meshes.push(SceneMesh { mesh: mesh.clone(), color: Rgb(100, 100, 100), face_colors: vec![] });
        assert!(within_raster_budget(&scene, &camera, 240, 150));
        let mut repeated = (*mesh).clone();
        repeated.triangles = vec![repeated.triangles[0]; 100_000];
        scene.meshes[0].mesh = std::sync::Arc::new(repeated);
        assert!(!within_raster_budget(&scene, &camera, 240, 150));
    }

    #[test]
    fn portable_thumbnail_preserves_saved_visibility() {
        let mut session = Session::default();
        session.execute("solid.box", &serde_json::json!({"length":10,"width":20,"height":30,"body_name":"Visible"})).unwrap();
        session.execute("solid.box", &serde_json::json!({"length":100,"width":100,"height":100,"body_name":"Hidden"})).unwrap();
        session.execute("browser.visibility", &serde_json::json!({"items":["Hidden"],"visible":false})).unwrap();
        let world = session.world_state();
        for body in &world.bodies {
            body.mesh();
        }
        let expected = saved(&session).unwrap();
        let bytes = session.design_container(solidcraft_io::DesignCompression::Fast, false, true).unwrap();
        let doc = solidcraft_io::read_design(&bytes).unwrap();
        assert_eq!(doc.visibility.hidden_bodies, vec!["Hidden"]);
        assert_eq!(doc, *session.design());
        assert_eq!(solidcraft_io::read_design_thumbnail(&bytes), Some(expected));
        let reopened = Session::new(doc);
        assert_eq!(reopened.visibility, session.visibility);
        let disabled = session.design_container(solidcraft_io::DesignCompression::Fast, false, false).unwrap();
        assert_eq!(solidcraft_io::read_design(&disabled).unwrap().visibility, session.visibility);
        assert!(solidcraft_io::read_design_thumbnail(&disabled).is_none());
    }

    #[test]
    fn portable_thumbnail_never_forces_cold_evaluation_or_meshing() {
        let mut session = Session::default();
        session.execute("solid.box", &serde_json::json!({"length":10,"width":20,"height":30})).unwrap();
        let world = session.world_state();
        assert!(world.bodies.iter().all(|b| b.cached_mesh().is_none()));
        assert!(saved(&session).is_none());
        assert!(world.bodies.iter().all(|b| b.cached_mesh().is_none()));
        for body in &world.bodies {
            body.mesh();
        }
        let png = saved(&session).unwrap();
        let bytes = session.design_container(solidcraft_io::DesignCompression::Fast, false, true).unwrap();
        assert_eq!(solidcraft_io::read_design_thumbnail(&bytes), Some(png));
        assert_eq!(solidcraft_io::read_design(&bytes).unwrap(), *session.doc);
        let disabled = session.design_container(solidcraft_io::DesignCompression::Fast, false, false).unwrap();
        assert!(solidcraft_io::read_design_thumbnail(&disabled).is_none());
    }
}
