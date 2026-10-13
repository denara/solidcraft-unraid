use super::*;
use crate::bspline::tests::{Lcg, random_surface};
use crate::bspline::{BezierPatch, NurbsSurface, SplineError};

fn uniform(n: usize) -> Vec<f64> {
    (0..n).map(|k| k as f64).collect()
}

fn col(i: usize, j0: usize, j1: usize) -> Segment {
    Segment::Column { i, j0, j1 }
}
fn row(j: usize, i0: usize, i1: usize) -> Segment {
    Segment::Row { j, i0, i1 }
}

/// 7 × 7 lines, row 3 only from column 0 to column 3: one T-junction at (3, 3) whose
/// extension runs along s.
fn t_mesh() -> TMesh {
    let mut segs: Vec<Segment> = (0..7).map(|i| col(i, 0, 6)).collect();
    segs.extend([0, 1, 2, 4, 5, 6].map(|j| row(j, 0, 6)));
    segs.push(row(3, 0, 3));
    TMesh::from_segments(uniform(7), uniform(7), &segs).unwrap()
}

fn random_points(seed: u64, n: usize) -> (Vec<Vec3>, Vec<f64>) {
    let mut rng = Lcg(seed);
    let pts = (0..n).map(|_| Vec3::new(rng.unit(), rng.unit(), rng.unit()) * 10.0).collect();
    let w = (0..n).map(|_| 0.5 + rng.unit()).collect();
    (pts, w)
}

fn t_spline(seed: u64) -> TSpline {
    let mesh = t_mesh();
    let (pts, w) = random_points(seed, mesh.vertices().len());
    TSpline::new(mesh, [3, 3], &pts, &w).unwrap()
}

/// A clamped bicubic NURBS with uneven interior knots, as a T-spline.
fn tensor_spline(seed: u64) -> (NurbsSurface, TSpline) {
    let ku = vec![0.0, 0.0, 0.0, 0.0, 0.2, 0.45, 0.5, 0.8, 1.0, 1.0, 1.0, 1.0];
    let kv = vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 1.0];
    let n = random_surface(seed, ku, kv, [3, 3]);
    let ts = TSpline::from_nurbs(&n).unwrap();
    (n, ts)
}

fn grid_params(dom: [[f64; 2]; 2], k: usize) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for i in 0..=k {
        for j in 0..=k {
            let s = dom[0][0] + (dom[0][1] - dom[0][0]) * i as f64 / k as f64;
            let t = dom[1][0] + (dom[1][1] - dom[1][0]) * j as f64 / k as f64;
            out.push((s, t));
        }
    }
    out
}

fn max_deviation(a: &TSpline, b: &TSpline) -> f64 {
    assert_eq!(a.domain(), b.domain());
    grid_params(a.domain(), 60).into_iter().map(|(s, t)| a.eval(s, t).unwrap().dist(b.eval(s, t).unwrap())).fold(0.0, f64::max)
}

#[test]
fn tmesh_validation_refuses_malformed_meshes() {
    let frame = |extra: &[Segment]| -> Vec<Segment> {
        let mut s = vec![col(0, 0, 4), col(4, 0, 4), row(0, 0, 4), row(4, 0, 4)];
        s.extend_from_slice(extra);
        s
    };
    let full: Vec<Segment> = (1..4).flat_map(|k| [col(k, 0, 4), row(k, 0, 4)]).collect();
    assert!(TMesh::from_segments(uniform(5), uniform(5), &frame(&full)).is_ok());
    // A dangling edge, an L-shaped corner, an open boundary, an empty line.
    let mut dangling = full.clone();
    dangling.push(col(2, 0, 4));
    dangling.retain(|s| *s != row(2, 0, 4));
    dangling.push(row(2, 0, 1));
    dangling.push(row(2, 3, 4));
    assert!(TMesh::from_segments(uniform(5), uniform(5), &frame(&dangling)).is_ok(), "T-junctions on both sides are fine");
    let dangling_edge = frame(&[col(1, 0, 4), col(3, 0, 4), row(1, 0, 4), row(2, 0, 4), row(3, 0, 1), col(2, 0, 3)]);
    assert!(TMesh::from_segments(uniform(5), uniform(5), &dangling_edge).is_err(), "edge ending inside a face");
    let mut l_shape = frame(&[col(1, 0, 4), col(2, 0, 4), col(3, 0, 4), row(1, 0, 4), row(3, 0, 4)]);
    l_shape.push(row(2, 0, 2));
    l_shape.retain(|s| *s != col(2, 0, 4));
    l_shape.extend([col(2, 0, 2), col(2, 3, 4)]);
    assert!(TMesh::from_segments(uniform(5), uniform(5), &l_shape).is_err(), "L-shaped face corner");
    assert!(TMesh::from_segments(uniform(5), uniform(5), &full).is_err(), "no outer rectangle");
    let mut empty = frame(&full);
    empty.retain(|s| *s != col(2, 0, 4));
    assert!(TMesh::from_segments(uniform(5), uniform(5), &empty).is_err(), "a column without edges");
    assert!(TMesh::from_segments(vec![0.0, 1.0, f64::NAN, 3.0, 4.0], uniform(5), &frame(&full)).is_err());
    assert!(TMesh::from_segments(vec![0.0, 2.0, 1.0, 3.0, 4.0], uniform(5), &frame(&full)).is_err());
    assert!(TMesh::from_segments(uniform(5), uniform(5), &[col(9, 0, 4)]).is_err());
    assert!(TMesh::from_segments(uniform(5), uniform(5), &[col(1, 3, 3)]).is_err());
    assert!(matches!(TMesh::tensor(uniform(MAX_INDEX_LINES + 1), uniform(2)), Err(SplineError::TooLarge(_))));
    assert!(matches!(TMesh::tensor(uniform(2048), uniform(2048)), Err(SplineError::TooLarge(_))));
    assert!(TMesh::tensor(uniform(1), uniform(3)).is_err());
}

#[test]
fn local_knot_vectors_are_read_off_rays() {
    let ts = t_spline(1);
    let m = ts.mesh();
    assert!(m.vertex_at(4, 3).is_none(), "(4, 3) lies on a column edge only");
    assert!(m.vertex_at(3, 3).is_some(), "the T-junction is a vertex");
    let knots = |i, j| {
        let k = m.vertex_at(i, j).unwrap();
        let (s, t) = ts.local_knots(k).unwrap();
        (s.to_vec(), t.to_vec())
    };
    assert_eq!(knots(2, 3), (vec![0.0, 1.0, 2.0, 3.0, 4.0], vec![1.0, 2.0, 3.0, 4.0, 5.0]));
    assert_eq!(knots(3, 4).1, vec![2.0, 3.0, 4.0, 5.0, 6.0]);
    // The ray down column 4 passes (4, 3), which carries no row edge.
    assert_eq!(knots(4, 4).1, vec![1.0, 2.0, 4.0, 5.0, 6.0]);
    assert_eq!(knots(4, 2).1, vec![0.0, 1.0, 2.0, 4.0, 5.0]);
    // Rays leaving the mesh repeat the boundary line.
    assert_eq!(knots(0, 0), (vec![0.0, 0.0, 0.0, 1.0, 2.0], vec![0.0, 0.0, 0.0, 1.0, 2.0]));
    assert_eq!(knots(6, 5).1, vec![2.0, 4.0, 5.0, 6.0, 6.0]);
}

#[test]
fn regular_tspline_is_the_tensor_nurbs_surface() {
    for (seed, degree, ku, kv) in [
        (2, [3, 3], vec![0.0, 0.0, 0.0, 0.0, 0.2, 0.45, 0.5, 0.8, 1.0, 1.0, 1.0, 1.0], vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 1.0]),
        (3, [3, 1], vec![0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0], vec![0.0, 0.0, 0.4, 1.0, 1.0]),
        (4, [5, 3], vec![0.0; 6].into_iter().chain([0.3, 0.7]).chain([1.0; 6]).collect(), vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0]),
    ] {
        let n = random_surface(seed, ku, kv, degree);
        let ts = TSpline::from_nurbs(&n).unwrap();
        assert!(ts.mesh().is_regular() && ts.is_analysis_suitable());
        assert_eq!(ts.domain(), n.domain());
        let worst = grid_params(n.domain(), 40).into_iter().map(|(s, t)| ts.eval(s, t).unwrap().dist(n.eval(s, t).unwrap())).fold(0.0, f64::max);
        assert!(worst < 1e-12, "degree {degree:?}: T-spline differs from NURBS by {worst}");
        assert_eq!(ts.to_nurbs().unwrap(), n, "NURBS → T-spline → NURBS is exact");
    }
    // An unclamped NURBS is clamped on the way in (same surface).
    let n = random_surface(5, uniform(10), uniform(9), [3, 3]);
    let ts = TSpline::from_nurbs(&n).unwrap();
    let worst = grid_params(n.domain(), 30).into_iter().map(|(s, t)| ts.eval(s, t).unwrap().dist(n.eval(s, t).unwrap())).fold(0.0, f64::max);
    assert!(worst < 1e-12, "unclamped: {worst}");
    assert!(TSpline::from_nurbs(&random_surface(6, uniform(9), uniform(9), [2, 3])).is_err(), "even degree");
}

#[test]
fn blending_functions_sum_to_one_with_t_junctions() {
    let ts = t_spline(1);
    assert!(ts.is_analysis_suitable());
    let n = ts.mesh().vertices().len();
    let mut worst = 0.0f64;
    for (s, t) in grid_params(ts.domain(), 64) {
        let sum: f64 = (0..n).map(|k| ts.blending(k, s, t).unwrap()).sum();
        worst = worst.max((sum - 1.0).abs());
    }
    assert!(worst < 1e-14, "partition of unity off by {worst}");
}

#[test]
fn derivatives_match_finite_differences() {
    let ts = t_spline(9);
    let h = 1e-6;
    let mut worst = 0.0f64;
    for (s, t) in grid_params([[1.05, 4.95], [1.05, 4.95]], 17) {
        let [_, ds, dt] = ts.derivs(s, t).unwrap();
        let fs = (ts.eval(s + h, t).unwrap() - ts.eval(s - h, t).unwrap()) / (2.0 * h);
        let ft = (ts.eval(s, t + h).unwrap() - ts.eval(s, t - h).unwrap()) / (2.0 * h);
        worst = worst.max((ds - fs).len()).max((dt - ft).len());
    }
    assert!(worst < 1e-6, "derivative error {worst}");
    assert!(ts.normal(3.0, 3.0).unwrap().is_some());
}

#[test]
fn analysis_suitability_follows_t_junction_extensions() {
    assert!(tensor_spline(1).1.is_analysis_suitable());
    let ext = t_mesh().extensions([3, 3]);
    assert_eq!(ext, vec![Extension { dir: Dir::S, at: 3, range: [2, 5], from: (3, 3) }]);
    assert!(t_mesh().is_analysis_suitable([3, 3]));
    // Row 4 stops at column 4 (extension along s over columns 3..7 on row 4: column 5 has no
    // edge there) and column 5 stops at row 3 (extension along t over rows 2..6 on column 5):
    // they cross at (5, 4).
    let mut segs: Vec<Segment> = [0, 1, 2, 3, 4, 6, 7, 8].iter().map(|&i| col(i, 0, 8)).collect();
    segs.push(col(5, 0, 3));
    segs.extend([0, 1, 2, 3, 5, 6, 7, 8].map(|j| row(j, 0, 8)));
    segs.push(row(4, 0, 4));
    let m = TMesh::from_segments(uniform(9), uniform(9), &segs).unwrap();
    let ext = m.extensions([3, 3]);
    assert!(ext.contains(&Extension { dir: Dir::S, at: 4, range: [3, 7], from: (4, 4) }));
    assert!(ext.contains(&Extension { dir: Dir::T, at: 5, range: [2, 6], from: (5, 3) }));
    assert!(!m.is_analysis_suitable([3, 3]));
    // Refining it does not repair it (only suitable T-splines are kept suitable), but stays exact.
    let (pts, w) = random_points(4, m.vertices().len());
    let ts = TSpline::new(m, [3, 3], &pts, &w).unwrap();
    let r = ts.refine(&[Cut::S { s: 2.5, t0: 2.0, t1: 3.0 }]).unwrap();
    assert!(max_deviation(&ts, &r) < 1e-12);
}

#[test]
fn a_cut_across_the_domain_is_boehm_knot_insertion() {
    let (n, ts) = tensor_spline(4);
    let r = ts.refine(&[Cut::S { s: 0.33, t0: 0.0, t1: 1.0 }, Cut::T { t: 0.75, s0: 0.0, s1: 1.0 }]).unwrap();
    assert!(r.mesh().is_regular());
    let want = n.insert_knot(Dir::S, 0.33).unwrap().insert_knot(Dir::T, 0.75).unwrap();
    let got = r.to_nurbs().unwrap();
    assert_eq!(got.knots(), want.knots());
    let worst = got.homogeneous().iter().zip(want.homogeneous()).map(|(a, b)| (*a - *b).xyz().len() + (a.w - b.w).abs()).fold(0.0, f64::max);
    assert!(worst < 1e-12, "control points differ by {worst}");
}

#[test]
fn local_refinement_keeps_the_surface() {
    let (_, ts) = tensor_spline(7);
    let before = ts.mesh().vertices().len();
    let r = ts.refine(&[Cut::S { s: 0.33, t0: 0.3, t1: 0.6 }]).unwrap();
    let dev = max_deviation(&ts, &r);
    eprintln!("local cut: {} → {} vertices, AS {}, max deviation {dev:e}", before, r.mesh().vertices().len(), r.is_analysis_suitable());
    assert!(dev < 1e-12, "deviation {dev}");
    assert!(r.mesh().vertices().len() <= before + 4, "a one-face cut stays local");
    assert!(r.is_analysis_suitable());
    // Refining again near the new T-junctions, and across them.
    let r2 = r.refine(&[Cut::T { t: 0.45, s0: 0.2, s1: 0.45 }, Cut::S { s: 0.39, t0: 0.3, t1: 0.45 }]).unwrap();
    let dev2 = max_deviation(&ts, &r2);
    eprintln!("second refinement: {} vertices, max deviation {dev2:e}", r2.mesh().vertices().len());
    assert!(dev2 < 1e-12, "deviation {dev2}");
    assert!(r2.is_analysis_suitable(), "refinement keeps analysis-suitability");
    let n2 = r2.mesh().vertices().len();
    let pu = grid_params(r2.domain(), 50)
        .into_iter()
        .map(|(s, t)| ((0..n2).map(|k| r2.blending(k, s, t).unwrap()).sum::<f64>() - 1.0).abs())
        .fold(0.0, f64::max);
    assert!(pu < 1e-14, "partition of unity after refinement off by {pu}");
    // A rational T-spline with T-junctions.
    let t = t_spline(3);
    let r3 = t.refine(&[Cut::S { s: 2.5, t0: 2.0, t1: 4.0 }, Cut::T { t: 3.5, s0: 3.0, s1: 5.0 }]).unwrap();
    let dev3 = max_deviation(&t, &r3);
    eprintln!("rational T-spline: {} → {} vertices, max deviation {dev3:e}", t.mesh().vertices().len(), r3.mesh().vertices().len());
    assert!(dev3 < 1e-12, "deviation {dev3}");
}

#[test]
fn random_local_refinements_keep_the_surface() {
    let mut rng = Lcg(42);
    let mut worst = 0.0f64;
    let mut vertices = 0;
    for round in 0..40 {
        let (_, ts) = tensor_spline(round);
        let mut cur = ts.clone();
        for _ in 0..1 + (rng.unit() * 3.0) as usize {
            let v = 0.05 + 0.9 * (rng.unit() * 40.0).round() / 40.0;
            let a = (rng.unit() * 8.0).round() / 10.0;
            let b = (a + 0.1 + (rng.unit() * 5.0).round() / 10.0).min(1.0);
            let cut = if rng.unit() < 0.5 { Cut::S { s: v, t0: a, t1: b } } else { Cut::T { t: v, s0: a, s1: b } };
            cur = cur.refine(&[cut]).unwrap_or_else(|e| panic!("round {round}: {cut:?}: {e}"));
        }
        assert!(cur.is_analysis_suitable(), "round {round}: refinement keeps analysis-suitability");
        vertices += cur.mesh().vertices().len() - ts.mesh().vertices().len();
        worst = worst.max(max_deviation(&ts, &cur));
    }
    eprintln!("random refinements: max deviation {worst:e}, {vertices} vertices added over 40 rounds");
    assert!(worst < 1e-12, "deviation {worst}");
}

fn check_patches(ts: &TSpline, patches: &[BezierPatch]) -> f64 {
    let [ds, dt] = ts.domain();
    let area: f64 = patches.iter().map(|p| (p.s[1] - p.s[0]) * (p.t[1] - p.t[0])).sum();
    assert!((area - (ds[1] - ds[0]) * (dt[1] - dt[0])).abs() < 1e-12, "the patches tile the domain");
    let mut worst = 0.0f64;
    for p in patches {
        for (s, t) in grid_params([p.s, p.t], 6) {
            worst = worst.max(p.eval(s, t).unwrap().dist(ts.eval(s, t).unwrap()));
        }
    }
    worst
}

#[test]
fn bezier_extraction_reproduces_the_surface() {
    let ts = t_spline(5);
    let patches = ts.bezier_elements().unwrap();
    let dev = check_patches(&ts, &patches);
    eprintln!("Bézier extraction, T-junction mesh: {} patches, max deviation {dev:e}", patches.len());
    assert!(dev < 1e-12);
    let (n, tensor) = tensor_spline(8);
    let patches = tensor.bezier_elements().unwrap();
    assert_eq!(patches.len(), 5 * 3, "one patch per pair of knot spans");
    assert!(check_patches(&tensor, &patches) < 1e-12);
    let _ = n;
    let refined = tensor.refine(&[Cut::S { s: 0.33, t0: 0.3, t1: 0.6 }, Cut::T { t: 0.45, s0: 0.2, s1: 0.45 }]).unwrap();
    let patches = refined.bezier_elements().unwrap();
    let dev = check_patches(&refined, &patches);
    eprintln!("Bézier extraction, refined: {} patches, max deviation {dev:e}", patches.len());
    assert!(dev < 1e-12);
}

#[test]
fn hostile_tspline_input_is_refused() {
    let mesh = t_mesh();
    let n = mesh.vertices().len();
    let (pts, w) = random_points(1, n);
    assert!(TSpline::new(mesh.clone(), [3, 3], &pts, &w).is_ok());
    let mut bad = pts.clone();
    bad[3] = Vec3::new(f64::NAN, 0.0, 0.0);
    assert!(TSpline::new(mesh.clone(), [3, 3], &bad, &w).is_err());
    let mut bw = w.clone();
    bw[0] = 0.0;
    assert!(TSpline::new(mesh.clone(), [3, 3], &pts, &bw).is_err());
    bw[0] = f64::INFINITY;
    assert!(TSpline::new(mesh.clone(), [3, 3], &pts, &bw).is_err());
    assert!(TSpline::new(mesh.clone(), [3, 3], &pts[1..], &w[1..]).is_err());
    assert!(matches!(TSpline::new(mesh.clone(), [2, 3], &pts, &w), Err(SplineError::Unsupported(_))));
    assert!(TSpline::new(mesh.clone(), [0, 3], &pts, &w).is_err());
    assert!(TSpline::new(mesh.clone(), [9, 3], &pts, &w).is_err());
    // Degree 5 needs the three outermost lines each way complete; row 3 is not, so a mesh
    // whose row 2 stops short is refused.
    let mut segs: Vec<Segment> = (0..7).map(|i| col(i, 0, 6)).collect();
    segs.extend([0, 1, 3, 4, 5, 6].map(|j| row(j, 0, 6)));
    segs.push(row(2, 0, 3));
    let short_frame = TMesh::from_segments(uniform(7), uniform(7), &segs).unwrap();
    let (p6, w6) = random_points(3, short_frame.vertices().len());
    assert!(TSpline::new(short_frame.clone(), [3, 3], &p6, &w6).is_ok());
    assert!(TSpline::new(short_frame, [5, 5], &p6, &w6).is_err());
    // A row of knots repeated so often that a blending function is empty.
    let m = TMesh::tensor(vec![0.0, 0.0, 0.0, 1.0, 2.0], uniform(5)).unwrap();
    let (p, w5) = random_points(2, 25);
    assert!(TSpline::new(m, [3, 3], &p, &w5).is_err());
    // An empty domain.
    let m = TMesh::tensor(vec![0.0, 1.0, 1.0, 2.0], uniform(4)).unwrap();
    assert!(TSpline::new(m, [3, 3], &p[..16], &w5[..16]).is_err());

    let ts = TSpline::new(mesh, [3, 3], &pts, &w).unwrap();
    for (s, t) in [(f64::NAN, 2.0), (2.0, f64::INFINITY), (0.5, 2.0), (2.0, 5.5)] {
        assert!(ts.eval(s, t).is_err() && ts.derivs(s, t).is_err() && ts.blending(0, s, t).is_err());
    }
    assert!(ts.blending(n + 5, 2.0, 2.0).is_err());
    for cut in [
        Cut::S { s: f64::NAN, t0: 1.0, t1: 2.0 },
        Cut::S { s: 2.5, t0: 2.0, t1: 1.0 },
        Cut::S { s: 2.5, t0: 2.0, t1: f64::INFINITY },
        Cut::S { s: 0.5, t0: 1.0, t1: 2.0 },
        Cut::T { t: 5.0, s0: 1.0, s1: 2.0 },
        Cut::T { t: 9.0, s0: 1.0, s1: 2.0 },
        Cut::S { s: 2.5, t0: 8.0, t1: 9.0 },
    ] {
        assert!(ts.refine(&[cut]).is_err(), "{cut:?}");
    }
    assert!(matches!(ts.refine(&vec![Cut::S { s: 2.5, t0: 1.0, t1: 2.0 }; 5000]), Err(SplineError::TooLarge(_))));
    // Refining the same line twice is a no-op the second time.
    let a = ts.refine(&[Cut::S { s: 2.5, t0: 1.0, t1: 5.0 }]).unwrap();
    let b = a.refine(&[Cut::S { s: 2.5, t0: 1.0, t1: 5.0 }]).unwrap();
    assert_eq!(a, b);
}
