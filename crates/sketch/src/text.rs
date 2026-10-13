//! Sketch text: glyph outlines of a font as lines, conics and cubic splines, so text closes
//! into profiles that can be extruded. TrueType quadratic Béziers are conics with rho = 0.5;
//! OpenType (CFF) cubic Béziers are cubic control splines, both exact. A glyph's counters (the
//! holes of A, B, 8…) are loops inside its outline; profile detection makes them holes.
//!
//! The default font is embedded (Liberation Sans, SIL OFL 1.1; see `fonts/OFL.txt` and
//! ATTRIBUTION.md); [`crate::fonts`] finds others by file path or installed family name.

use solidcraft_geom::Vec2;

use crate::fonts::font_data;
use crate::link::{LinkGeom, MAX_LINK_ENTITIES};
use crate::model::{Result, SketchError};

/// Most characters in one text.
pub const MAX_TEXT_CHARS: usize = 1000;

struct Outline {
    out: Vec<LinkGeom>,
    start: Vec2,
    cur: Vec2,
    map: Box<dyn Fn(f32, f32) -> Vec2>,
}

impl Outline {
    fn line(&mut self, to: Vec2) {
        if self.cur.dist(to) > 1e-9 {
            self.out.push(LinkGeom::Line(self.cur, to));
        }
        self.cur = to;
    }
    fn quad(&mut self, apex: Vec2, to: Vec2) {
        let (a, b) = (self.cur, to);
        let chord = b - a;
        let off = chord.cross(apex - a).abs() / chord.len().max(1e-12);
        if off < 1e-6 || a.dist(b) < 1e-9 {
            self.line(to);
            return;
        }
        self.out.push(LinkGeom::Conic { a, apex, b, rho: 0.5 });
        self.cur = to;
    }
    fn cubic(&mut self, c1: Vec2, c2: Vec2, to: Vec2) {
        let (a, b) = (self.cur, to);
        let chord = b - a;
        let l = chord.len().max(1e-12);
        let off = (chord.cross(c1 - a).abs() / l).max(chord.cross(c2 - a).abs() / l);
        // Straight (controls on the chord, between its ends): a line.
        let between = |c: Vec2| (c - a).dot(chord) >= -1e-9 && (c - b).dot(chord) <= 1e-9;
        if a.dist(b) < 1e-9 || (off < 1e-6 && between(c1) && between(c2)) {
            self.line(to);
            return;
        }
        self.out.push(LinkGeom::Spline { pts: vec![a, c1, c2, b], control: true, degree: 3, periodic: false });
        self.cur = to;
    }
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = (self.map)(x, y);
        self.start = p;
        self.cur = p;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = (self.map)(x, y);
        self.line(p);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, p) = ((self.map)(x1, y1), (self.map)(x, y));
        self.quad(c, p);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, p) = ((self.map)(x1, y1), (self.map)(x2, y2), (self.map)(x, y));
        self.cubic(c1, c2, p);
    }
    fn close(&mut self) {
        let s = self.start;
        self.line(s);
    }
}

/// Outline geometry of `text` in the built-in font (see [`text_geometry_in`]).
pub fn text_geometry(text: &str, at: Vec2, height: f64, angle: f64) -> Result<Vec<LinkGeom>> {
    text_geometry_in(None, text, at, height, angle)
}

/// Outline geometry of `text` in `font` (None: the built-in font; else a font file path or an
/// installed family name, see [`crate::fonts::font_data`]) with its baseline starting at `at`,
/// capital height `height` (mm), rotated by `angle` (radians) about `at`. Lines break at `\n`.
pub fn text_geometry_in(font: Option<&str>, text: &str, at: Vec2, height: f64, angle: f64) -> Result<Vec<LinkGeom>> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(SketchError::TooLarge);
    }
    if !(height.is_finite() && height > 1e-6 && height < 1e6 && angle.is_finite() && at.is_finite()) {
        return Err(SketchError::Invalid("text height must be positive".into()));
    }
    let data = font_data(font)?;
    let face = ttf_parser::Face::parse(data.bytes(), data.index).map_err(|e| SketchError::Invalid(format!("font: {e}")))?;
    let cap = cap_height(&face);
    let k = height / cap;
    let line_gap = f64::from(face.ascender()) - f64::from(face.descender()) + f64::from(face.line_gap());
    let line_gap = if line_gap > 0.0 { line_gap * k } else { height * 1.5 };
    let (s, c) = angle.sin_cos();
    let mut out = Vec::new();
    for (row, line) in text.split('\n').enumerate() {
        let mut pen = 0.0f64;
        let base_y = -(row as f64) * line_gap;
        for ch in line.chars() {
            let Some(gid) = face.glyph_index(ch).or_else(|| face.glyph_index('?')) else { continue };
            let x0 = pen;
            let map = Box::new(move |x: f32, y: f32| {
                let (lx, ly) = (x0 + f64::from(x) * k, base_y + f64::from(y) * k);
                at + Vec2::new(lx * c - ly * s, lx * s + ly * c)
            });
            let mut ob = Outline { out: Vec::new(), start: at, cur: at, map };
            let _ = face.outline_glyph(gid, &mut ob);
            out.extend(ob.out);
            // A font can give a glyph any number of points: stop where the link would be refused anyway.
            if out.len() > MAX_LINK_ENTITIES {
                return Err(SketchError::TooLarge);
            }
            pen += f64::from(face.glyph_hor_advance(gid).unwrap_or(0)) * k;
        }
    }
    Ok(out)
}

/// The font's capital height in font units: from its OS/2 table, else the top of its `H`, else
/// 70 % of the em.
fn cap_height(face: &ttf_parser::Face) -> f64 {
    face.capital_height()
        .map(f64::from)
        .filter(|c| *c > 0.0)
        .or_else(|| face.glyph_index('H').and_then(|g| face.glyph_bounding_box(g)).map(|b| f64::from(b.y_max)).filter(|c| *c > 0.0))
        .unwrap_or(f64::from(face.units_per_em().max(1)) * 0.7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_have_closed_outlines_at_the_cap_height() {
        let g = text_geometry("HO", Vec2::new(5.0, 0.0), 10.0, 0.0).unwrap();
        assert!(!g.is_empty());
        let ys: Vec<f64> = g
            .iter()
            .flat_map(|x| match *x {
                LinkGeom::Line(a, b) => vec![a.y, b.y],
                LinkGeom::Conic { a, b, .. } => vec![a.y, b.y],
                _ => vec![],
            })
            .collect();
        let top = ys.iter().cloned().fold(f64::MIN, f64::max);
        assert!((top - 10.0).abs() < 0.3, "{top}");
        assert!(g.iter().any(|x| matches!(x, LinkGeom::Conic { .. })), "O has curves");
        assert!(text_geometry("x", Vec2::ZERO, -1.0, 0.0).is_err());
        assert!(text_geometry(&"x".repeat(2000), Vec2::ZERO, 1.0, 0.0).is_err());
        // Empty text makes nothing (not an error here; the command rejects it).
        assert!(text_geometry("", Vec2::ZERO, 1.0, 0.0).unwrap().is_empty());
    }

    #[test]
    fn a_font_file_gives_the_same_outlines_and_unknown_fonts_are_errors() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts").join("LiberationSans-Regular.ttf");
        let by_file = text_geometry_in(Some(&path.to_string_lossy()), "A7_L", Vec2::ZERO, 8.0, 0.0).unwrap();
        assert_eq!(by_file, text_geometry("A7_L", Vec2::ZERO, 8.0, 0.0).unwrap());
        let e = text_geometry_in(Some("No Such Family 12345"), "A", Vec2::ZERO, 8.0, 0.0).unwrap_err().to_string();
        assert!(e.contains("unknown font"), "{e}");
    }

    #[test]
    fn cubic_outlines_stay_exact_splines() {
        let mut ob = Outline { out: Vec::new(), start: Vec2::ZERO, cur: Vec2::ZERO, map: Box::new(|x, y| Vec2::new(f64::from(x), f64::from(y))) };
        ttf_parser::OutlineBuilder::move_to(&mut ob, 0.0, 0.0);
        ttf_parser::OutlineBuilder::curve_to(&mut ob, 0.0, 10.0, 10.0, 10.0, 10.0, 0.0);
        // A cubic with its controls on the chord is a line.
        ttf_parser::OutlineBuilder::curve_to(&mut ob, 7.0, 0.0, 3.0, 0.0, 0.0, 0.0);
        assert_eq!(ob.out.len(), 2);
        assert!(matches!(&ob.out[0], LinkGeom::Spline { pts, control: true, degree: 3, .. } if pts.len() == 4));
        assert!(matches!(ob.out[1], LinkGeom::Line(..)));
    }
}
