//! How finely the viewport tessellates curved faces and edges (Preferences ▸ Graphics ▸ Display
//! quality). A viewing preference, not design data: it only picks the tolerances of the display
//! plan ([`crate::ModelBody::display_plan`]) the viewport draws. The reference mesh the model
//! reads (picks, holes, measuring) and exports keep their own tolerances.

use std::sync::atomic::{AtomicU8, Ordering};

/// A display quality level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DisplayQuality {
    Coarse,
    #[default]
    Normal,
    Fine,
    Ultra,
}

impl DisplayQuality {
    pub const ALL: [DisplayQuality; 4] = [DisplayQuality::Coarse, DisplayQuality::Normal, DisplayQuality::Fine, DisplayQuality::Ultra];

    /// The id kept in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            DisplayQuality::Coarse => "coarse",
            DisplayQuality::Normal => "normal",
            DisplayQuality::Fine => "fine",
            DisplayQuality::Ultra => "ultra",
        }
    }

    pub fn parse(id: &str) -> Option<DisplayQuality> {
        Self::ALL.into_iter().find(|q| q.id() == id)
    }

    /// Position in [`DisplayQuality::ALL`] (indexes the per-level display caches).
    pub fn index(self) -> usize {
        self as usize
    }

    /// Chord tolerance as a fraction of the body's size: the largest gap allowed between a
    /// facet and its surface.
    pub fn chord_fraction(self) -> f64 {
        match self {
            DisplayQuality::Coarse => 3e-3,
            DisplayQuality::Normal => 1e-3,
            DisplayQuality::Fine => 3e-4,
            DisplayQuality::Ultra => 1e-4,
        }
    }

    /// Smallest chord tolerance (mm), so a tiny body does not mesh down to nothing.
    pub fn min_chord(self) -> f64 {
        match self {
            DisplayQuality::Coarse => 3e-3,
            DisplayQuality::Normal => 1e-3,
            DisplayQuality::Fine => 3e-4,
            DisplayQuality::Ultra => 1e-4,
        }
    }

    /// Most a facet of a curved face or edge may turn (radians), so a small radius on a large
    /// body still looks round.
    pub fn angle(self) -> f64 {
        let deg: f64 = match self {
            DisplayQuality::Coarse => 30.0,
            DisplayQuality::Normal => 20.0,
            DisplayQuality::Fine => 12.0,
            DisplayQuality::Ultra => 7.0,
        };
        deg.to_radians()
    }

    /// The chord tolerance (mm) for a body of this size.
    pub fn chord(self, size: f64) -> f64 {
        let size = if size.is_finite() && size > 0.0 { size } else { 0.0 };
        (size * self.chord_fraction()).max(self.min_chord())
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(1);

/// The level the viewport draws at (process-wide; set from the preferences).
pub fn current() -> DisplayQuality {
    DisplayQuality::ALL.get(CURRENT.load(Ordering::Relaxed) as usize).copied().unwrap_or_default()
}

/// Set the level the viewport draws at. Bodies keep a display plan per level, so switching back
/// is instant and nothing is re-evaluated.
pub fn set_current(q: DisplayQuality) {
    CURRENT.store(q.index() as u8, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_is_the_application_tolerance_and_levels_get_finer() {
        let n = DisplayQuality::Normal;
        assert_eq!(n.chord(200.0), 0.2);
        assert_eq!(n.chord(0.5), 1e-3);
        assert!((n.angle() - solidcraft_kernel::DISPLAY_ANGLE).abs() < 1e-12);
        for w in DisplayQuality::ALL.windows(2) {
            assert!(w[1].chord(100.0) < w[0].chord(100.0) && w[1].angle() < w[0].angle());
        }
        for q in DisplayQuality::ALL {
            assert_eq!(DisplayQuality::parse(q.id()), Some(q));
            assert_eq!(DisplayQuality::ALL[q.index()], q);
        }
    }

    /// Each level keeps its own plan: finer levels draw a round body with more triangles and
    /// closer to the surface, and the reference mesh the model reads never changes.
    #[test]
    fn each_level_has_its_own_plan_and_the_reference_mesh_stays() {
        use solidcraft_geom::Vec3;
        let body = solidcraft_kernel::cylinder(Vec3::ZERO, Vec3::Z, 20.0, 50.0).unwrap();
        let mb = crate::ModelBody::new("Body1".into(), body, 1);
        let reference = mb.mesh();
        let mut last = 0;
        for q in DisplayQuality::ALL {
            let mesh = mb.plan_mesh_at(q).unwrap();
            let r = |v: Vec3| (v.x * v.x + v.y * v.y).sqrt();
            // The worst chord's sag on the side wall, from its triangle centres.
            let sag = mesh
                .triangles
                .iter()
                .map(|t| t.map(|i| mesh.positions[i as usize]))
                .filter(|t| t.iter().all(|p| (r(*p) - 20.0).abs() < 1e-9))
                .map(|[a, b, c]| 20.0 - r((a + b + c) * (1.0 / 3.0)))
                .fold(0.0, f64::max);
            assert!(sag <= q.chord(mb.body.size()) * 1.01, "{q:?}: sag {sag}");
            assert!(mesh.triangles.len() > last, "{q:?}: {} triangles", mesh.triangles.len());
            last = mesh.triangles.len();
            assert!(std::sync::Arc::ptr_eq(&mesh, &mb.plan_mesh_at(q).unwrap()), "made once per level");
        }
        assert!(std::sync::Arc::ptr_eq(&reference, &mb.mesh()));
        assert_eq!(mb.display_plan_at(DisplayQuality::Normal).unwrap().vertex_count(), {
            let plan = mb.body.display_plan_angular((mb.body.size() * 1e-3).max(1e-3), solidcraft_kernel::DISPLAY_ANGLE).unwrap();
            plan.vertex_count()
        });
    }
}
