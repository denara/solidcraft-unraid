//! The command registry. Ids are SolidCraft's own dotted ids (`solid.extrude`,
//! `sketch.rectangle.two_point`); the toolbar placement (`tab`, `panel`) follows the familiar
//! Design workspace layout.

/// The params-doc note of every command that adds sketch geometry (see `sketch::created`).
macro_rules! created_doc {
    () => {
        ". Returns created: {points, curves, constraints, dimensions}: the ids of what it added (stable and never reused: usable right away in sketch.dimension, sketch.constraint.*, sketch.delete…)"
    };
}

mod browser;
mod clipboard;
mod component;
mod component_clone;
mod component_clone_assembly;
mod construct;
mod delete;
mod diagnostics;
mod edit;
mod face;
mod features;
mod features_more;
pub use features::auto_operation;
mod component_link;
mod config;
mod file;
mod inspect;
pub(crate) mod joints;
mod materials;
mod measure_sel;
mod mesh;
pub(crate) mod motion;
mod parts;
mod section;
pub(crate) use measure_sel::measure_items;
#[cfg(test)]
mod kernel_heal_tests;
mod parameters;
mod plastic;
mod render;
mod sheet;
mod sketch;
pub(crate) use sketch::author_path_for_rebind;
pub(crate) use sketch::created as sketch_created;
mod sketch_3d;
mod sketch_canvas;
mod sketch_constraints;
mod sketch_create;
mod sketch_freeform;
mod sketch_import;
mod sketch_inspect;
mod sketch_modify;
mod sketch_project;
mod surface;
mod surface_analysis;
mod view_navigation;

use serde::Serialize;
use serde_json::Value;

use crate::{Result, Session};

pub type Run = fn(&mut Session, &Value) -> Result<Value>;
/// A command's form for one frame of an interactive gesture: its result and the sketch it
/// changed, for [`Session::execute`] to show without evaluating the rest of the model (`None`:
/// evaluate as usual).
pub type LiveRun = fn(&mut Session, &Value) -> Result<(Value, Option<crate::LiveEdit>)>;
pub type Enabled = fn(&Session) -> std::result::Result<(), String>;

pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Toolbar tab (`SOLID`, `SKETCH`…); empty = not on the toolbar.
    pub tab: &'static str,
    /// Toolbar panel (`CREATE`, `MODIFY`…).
    pub panel: &'static str,
    /// Icon name for the UI (drawn in code).
    pub icon: &'static str,
    pub shortcut: Option<&'static str>,
    /// One-line parameter documentation.
    pub params: &'static str,
    pub enabled: Enabled,
    pub run: Run,
    pub undoable: bool,
    /// What runs instead of `run` when the command is given `live: true` (a frame of an
    /// interactive gesture, see [`Session::execute`]).
    pub live: Option<LiveRun>,
    /// Reads the evaluated model (the default). A command that reads only the document and the
    /// session's history runs on a model left stale by a live gesture without evaluating it first.
    pub reads_model: bool,
}

impl CommandSpec {
    pub const fn new(id: &'static str, label: &'static str, run: Run) -> Self {
        CommandSpec {
            id,
            label,
            tab: "",
            panel: "",
            icon: "",
            shortcut: None,
            params: "",
            enabled: always,
            run,
            undoable: true,
            live: None,
            reads_model: true,
        }
    }
    /// Takes `live: true`, run by `run`.
    pub const fn live(mut self, run: LiveRun) -> Self {
        self.live = Some(run);
        self
    }
    /// Reads only the document (and history), never the evaluated model.
    pub const fn doc_only(mut self) -> Self {
        self.reads_model = false;
        self
    }
    pub const fn at(mut self, tab: &'static str, panel: &'static str) -> Self {
        self.tab = tab;
        self.panel = panel;
        self
    }
    pub const fn icon(mut self, i: &'static str) -> Self {
        self.icon = i;
        self
    }
    pub const fn key(mut self, k: &'static str) -> Self {
        self.shortcut = Some(k);
        self
    }
    pub const fn params(mut self, p: &'static str) -> Self {
        self.params = p;
        self
    }
    pub const fn enabled(mut self, e: Enabled) -> Self {
        self.enabled = e;
        self
    }
    pub const fn noundo(mut self) -> Self {
        self.undoable = false;
        self
    }
    /// Run inside another command: when it is enabled, its `run` (the caller's command owns the
    /// undo step and the evaluation).
    pub(crate) fn run_nested(&self, s: &mut Session, p: &Value) -> Result<Value> {
        (self.enabled)(s).map_err(|why| crate::EngineError::Disabled(self.id.to_string(), why))?;
        (self.run)(s, p)
    }
    pub fn info(&self, s: &Session) -> CommandInfo {
        let e = (self.enabled)(s);
        CommandInfo {
            id: self.id,
            label: self.label,
            tab: self.tab,
            panel: self.panel,
            icon: self.icon,
            shortcut: self.shortcut,
            params: self.params,
            enabled: e.is_ok(),
            disabled_reason: e.err(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CommandInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub tab: &'static str,
    pub panel: &'static str,
    pub icon: &'static str,
    pub shortcut: Option<&'static str>,
    pub params: &'static str,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
}

pub fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

pub fn in_sketch(s: &Session) -> std::result::Result<(), String> {
    if s.active_sketch.is_some() { Ok(()) } else { Err("no sketch is being edited (Create Sketch or Edit Sketch first)".into()) }
}

/// The mirror of [`in_sketch`], for a command that acts on bodies and has nothing to offer the
/// geometry under the cursor while a sketch is open.
pub fn not_in_sketch(s: &Session) -> std::result::Result<(), String> {
    if s.active_sketch.is_none() { Ok(()) } else { Err("a sketch is being edited (this works on bodies: Finish Sketch first)".into()) }
}

/// Every command, in toolbar order.
pub fn command_specs() -> Vec<&'static CommandSpec> {
    let mut v: Vec<&'static CommandSpec> = Vec::new();
    v.extend(sketch::COMMANDS.iter());
    v.extend(sketch_create::COMMANDS.iter());
    v.extend(sketch_constraints::COMMANDS.iter());
    v.extend(sketch_import::COMMANDS.iter());
    v.extend(sketch_inspect::COMMANDS.iter());
    v.extend(sketch_canvas::COMMANDS.iter());
    v.extend(sketch_3d::COMMANDS.iter());
    v.extend(surface_analysis::COMMANDS.iter());
    v.extend(surface::COMMANDS.iter());
    v.extend(sketch_modify::COMMANDS.iter());
    v.extend(sketch_project::COMMANDS.iter());
    v.extend(features::COMMANDS.iter());
    v.extend(features_more::COMMANDS.iter());
    v.extend(mesh::COMMANDS.iter());
    v.extend(construct::COMMANDS.iter());
    v.extend(sheet::COMMANDS.iter());
    v.extend(plastic::COMMANDS.iter());
    v.extend(component::COMMANDS.iter());
    v.extend(component_link::COMMANDS.iter());
    v.extend(joints::COMMANDS.iter());
    v.extend(motion::COMMANDS.iter());
    v.extend(config::COMMANDS.iter());
    v.extend(parts::COMMANDS.iter());
    v.extend(browser::COMMANDS.iter());
    v.extend(delete::COMMANDS.iter());
    v.extend(edit::COMMANDS.iter());
    v.extend(clipboard::COMMANDS.iter());
    v.extend(parameters::COMMANDS.iter());
    v.extend(file::COMMANDS.iter());
    v.extend(inspect::COMMANDS.iter());
    v.extend(materials::COMMANDS.iter());
    v.extend(section::COMMANDS.iter());
    v.extend(view_navigation::COMMANDS.iter());
    v.extend(diagnostics::COMMANDS.iter());
    v.extend(render::COMMANDS.iter());
    v
}

pub fn find_command(id: &str) -> Option<&'static CommandSpec> {
    let all = command_specs();
    all.iter().find(|c| c.id == id).or_else(|| all.iter().find(|c| c.id.eq_ignore_ascii_case(id))).copied()
}
