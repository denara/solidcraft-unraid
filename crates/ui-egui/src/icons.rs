//! Stable UI rendering boundary for the collected artwork. Compiled only at L5.
#[path = "../../../assets/icons/ui/source/icons.rs"]
mod source;
pub use source::paint;
#[path = "../../../assets/icons/ui/source/glyphs.rs"]
pub(crate) mod glyphs;
#[path = "../../../assets/icons/ui/source/markers.rs"]
pub(crate) mod markers;
#[path = "../../../assets/icons/ui/source/navigation.rs"]
pub(crate) mod navigation;
#[path = "../../../assets/icons/ui/source/pointer.rs"]
pub(crate) mod pointer;
