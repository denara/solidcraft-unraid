//! Sketch creation families and the active part's last interactive variant.

use std::collections::BTreeMap;

use solidcraft_engine::CommandSpec;

pub struct Family {
    pub label: &'static str,
    pub commands: &'static [&'static str],
}

pub const FAMILIES: &[Family] = &[
    Family { label: "Line", commands: &["sketch.line", "sketch.line.midpoint"] },
    Family {
        label: "Rectangle",
        commands: &["sketch.rectangle.two_point", "sketch.rectangle.three_point", "sketch.rectangle.center", "sketch.rectangle.rounded"],
    },
    Family {
        label: "Circle",
        commands: &[
            "sketch.circle.center",
            "sketch.circle.two_point",
            "sketch.circle.three_point",
            "sketch.circle.two_tangent",
            "sketch.circle.three_tangent",
        ],
    },
    Family { label: "Arc", commands: &["sketch.arc.three_point", "sketch.arc.center_point", "sketch.arc.tangent"] },
    Family {
        label: "Slot",
        commands: &[
            "sketch.slot.center_to_center",
            "sketch.slot.overall",
            "sketch.slot.center_point",
            "sketch.slot.arc_three_point",
            "sketch.slot.arc_center",
        ],
    },
    Family { label: "Polygon", commands: &["sketch.polygon.inscribed", "sketch.polygon.circumscribed", "sketch.polygon.edge"] },
    Family { label: "Spline", commands: &["sketch.spline.fit_point", "sketch.spline.control_point", "sketch.spline.control_point_5"] },
];

pub fn family(id: &str) -> Option<&'static Family> {
    FAMILIES.iter().find(|f| f.commands.contains(&id))
}

/// UI state only, swapped with document tabs; never dirties the model or its undo history.
#[derive(Clone, Debug, Default)]
pub struct Choices(BTreeMap<&'static str, &'static str>);

impl Choices {
    pub fn remember(&mut self, id: &str) {
        if let Some(f) = family(id)
            && let Some(id) = f.commands.iter().find(|c| **c == id)
        {
            self.0.insert(f.label, id);
        }
    }

    pub fn selected(&self, f: &Family) -> Option<&'static CommandSpec> {
        self.0.get(f.label).copied().or_else(|| f.commands.first().copied()).and_then(solidcraft_engine::find_command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_are_disjoint_registered_creation_commands() {
        let mut seen = std::collections::BTreeSet::new();
        for f in FAMILIES {
            assert!(!f.commands.is_empty());
            for id in f.commands {
                assert!(seen.insert(id), "duplicate variant {id}");
                let c = solidcraft_engine::find_command(id).unwrap();
                assert_eq!((c.tab, c.panel), ("SKETCH", "CREATE"));
            }
        }
    }

    #[test]
    fn choices_ignore_unknown_commands_and_keep_families_independent() {
        let mut choices = Choices::default();
        choices.remember("sketch.rectangle.center");
        choices.remember("sketch.circle.three_point");
        choices.remember("sketch.rectangle.unknown");
        for (id, expected) in [
            ("sketch.rectangle.two_point", "sketch.rectangle.center"),
            ("sketch.circle.center", "sketch.circle.three_point"),
            ("sketch.line", "sketch.line"),
        ] {
            assert_eq!(choices.selected(family(id).unwrap()).unwrap().id, expected);
        }
    }
}
