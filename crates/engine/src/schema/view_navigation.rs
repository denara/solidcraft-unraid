//! Render shell requests. The UI applies validated echoes without design edits.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderDock {
    pub panel: RenderPanel,
    pub side: DockSide,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RenderPanel {
    Inspector,
    Browser,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DockSide {
    Left,
    Right,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderFocus {
    /// Hide the Render docks while retaining their layout and the current camera.
    pub on: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderBrowser {
    /// Open the floating Browser card in a narrow Render window.
    pub open: bool,
}
