//! Crate for operationg shapes. Provides boolean operations to Solid, and shape healing for importing shapes from other CAD systems.

// SolidCraft: warnings are not errors in the vendored copy (newer compilers flag more).
#![allow(dead_code)]
#![warn(clippy::all, rust_2018_idioms)]
#![warn(
    missing_docs,
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

/// SolidCraft: report where an operation gives up (set `SHAPEOPS_TRACE`).
macro_rules! tr {
    ($e:expr, $tag:expr) => {
        match $e {
            Some(v) => v,
            None => {
                if std::env::var_os("SHAPEOPS_TRACE").is_some() {
                    eprintln!("shapeops: gave up at {} ({}:{})", $tag, file!(), line!());
                }
                return None;
            }
        }
    };
}
pub(crate) use tr;

mod healing;
pub use healing::{RobustSplitClosedEdgesAndFaces, SplitClosedEdgesAndFaces};
mod transversal;
pub use transversal::{
    and, and_with_history, or, or_with_history, EdgeOrigin, FaceIndex, FaceOrigin, History, ShapeOpsCurve,
    ShapeOpsSurface,
};
// SolidCraft: exercised by kernel regressions in the workspace CI.
#[doc(hidden)]
pub use transversal::IntersectionCurveWithParameters;
mod alternative;
