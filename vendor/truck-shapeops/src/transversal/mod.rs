mod divide_face;
mod faces_classification;
mod history;
mod integrate;
mod intersection_curve;
// SolidCraft: expose the constructor for workspace regression coverage of the vendored fix.
#[doc(hidden)]
pub use intersection_curve::IntersectionCurveWithParameters;
mod loops_store;
mod polyline_construction;
pub use history::{EdgeOrigin, FaceIndex, FaceOrigin, History};
pub use integrate::{and, and_with_history, or, or_with_history, ShapeOpsCurve, ShapeOpsSurface};
