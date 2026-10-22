//! Tokenised dense card library. Import elements through `crate::ui_kit`.
//! `Panel` owns card composition; rows own aligned label/control columns and retain egui focus.
//! Geometry, typography and colours come from `theme::Tokens`. See `docs/ui-kit.md`.
mod actions;
mod assistant;
mod assistant_entry;
mod checks;
mod choice;
mod color;
mod confirm;
pub mod container;
mod disclosure;
mod fields;
mod history;
mod layout;
mod menu;
mod metrics;
pub(crate) mod panel;
mod radial;
mod reports;
mod search;
mod status;
mod table;
mod text;

pub(crate) use actions::*;
pub use actions::{Button, button};
pub(crate) use assistant::*;
pub(crate) use assistant_entry::*;
pub(crate) use checks::*;
pub(crate) use choice::*;
pub(crate) use color::*;
pub(crate) use confirm::{Confirm, Tone};
pub use container::{Container, DockEdges, Edge, Surface};
pub(crate) use disclosure::*;
pub use fields::cell_editor;
pub(crate) use fields::*;
pub(crate) use history::*;
pub(crate) use layout::*;
pub(crate) use menu::{Menu, MenuItem, rename_menu, workspace_menu};
pub(crate) use metrics::{ContentWidth, header_height, row_height, table_row_height};
pub use panel::Window;
pub(crate) use panel::{ComputeStatus, FeatureOptions, Panel, Part, WindowPage, panel_header_hover};
#[cfg(test)]
pub(crate) use panel::{collapsible_header, header};
pub(crate) use radial::{RADIAL_RADIUS, radial_menu};
pub(crate) use reports::*;
pub(crate) use search::{CommandSearch, SearchBadge, SearchFieldGeometry};
pub(crate) use status::*;
pub(crate) use table::*;

#[cfg(test)]
mod gallery;
#[cfg(test)]
mod slider_tests;
#[cfg(test)]
#[path = "test_support_tests.rs"]
mod test_support;

pub(crate) use layout::label as row_label;
