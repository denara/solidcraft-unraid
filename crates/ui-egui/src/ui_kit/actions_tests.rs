use crate::ui_kit::test_support::*;
use crate::ui_kit::*;

use crate::scenario;
use egui::Ui;

#[test]
fn footer_clicks_and_keyboard_respect_validation_and_editor_focus() {
    for enabled in [false, true] {
        let ctx = crate::fonts::test_context();
        let mut action = Action::None;
        let mut draw = |ui: &mut Ui| {
            action = footer(ui, "OK", "Cancel", enabled, Footer::default());
        };
        frame(&ctx, vec![], &mut draw);
        let pos = scenario::handle_at("dialog:ok").unwrap();
        frame(&ctx, pointer(pos, true), &mut draw);
        frame(&ctx, pointer(pos, false), &mut draw);
        assert_eq!(action, if enabled { Action::Ok } else { Action::None });
        // Button clicks may own focus; keyboard submission is for an unfocused form.
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        frame(&ctx, key(egui::Key::Enter), |ui| action = footer(ui, "OK", "Cancel", enabled, Footer::default()));
        assert_eq!(action, if enabled { Action::Ok } else { Action::None });
        frame(&ctx, key(egui::Key::Escape), |ui| action = footer(ui, "OK", "Cancel", enabled, Footer::default()));
        assert_eq!(action, Action::Cancel);
        frame(&ctx, vec![], |ui| {
            footer(ui, "OK", "Cancel", enabled, Footer::default());
        });
        let cancel = scenario::handle_at("dialog:cancel").unwrap();
        frame(&ctx, pointer(cancel, true), |ui| action = footer(ui, "OK", "Cancel", enabled, Footer::default()));
        frame(&ctx, pointer(cancel, false), |ui| action = footer(ui, "OK", "Cancel", enabled, Footer::default()));
        assert_eq!(action, Action::Cancel);
    }
    let ctx = crate::fonts::test_context();
    let mut action = Action::None;
    let mut text = "radius".to_string();
    frame(&ctx, vec![], |ui| {
        let response = ui.text_edit_singleline(&mut text);
        response.request_focus();
        footer(ui, "OK", "Cancel", true, Footer::default());
    });
    frame(&ctx, key(egui::Key::Enter), |ui| {
        ui.text_edit_singleline(&mut text).request_focus();
        action = footer(ui, "OK", "Cancel", true, Footer::default());
    });
    // A text editor handles Enter itself; the footer cannot apply merely because it was pressed.
    assert_eq!(action, Action::None);
}

#[test]
fn enabled_buttons_show_the_pointing_hand_and_disabled_ones_do_not() {
    for (enabled, expected) in [(true, egui::CursorIcon::PointingHand), (false, egui::CursorIcon::Default)] {
        let ctx = crate::fonts::test_context();
        let mut draw = |ui: &mut Ui| {
            footer(ui, "OK", "Cancel", enabled, Footer { apply: Some("Apply"), ..Default::default() });
        };
        frame(&ctx, vec![], &mut draw);
        let ok = scenario::handle_at("dialog:ok").unwrap();
        // Settled over two frames: the hover is known from the first frame's layout.
        frame_cursor(&ctx, ok, &mut draw);
        assert_eq!(frame_cursor(&ctx, ok, &mut draw), expected, "OK, enabled {enabled}");
        // Apply is a button too, and Cancel is always enabled.
        let apply = scenario::handle_at("dialog:apply").unwrap();
        frame_cursor(&ctx, apply, &mut draw);
        assert_eq!(frame_cursor(&ctx, apply, &mut draw), expected, "Apply, enabled {enabled}");
        let cancel = scenario::handle_at("dialog:cancel").unwrap();
        frame_cursor(&ctx, cancel, &mut draw);
        assert_eq!(frame_cursor(&ctx, cancel, &mut draw), egui::CursorIcon::PointingHand, "Cancel is always enabled");
    }
}
