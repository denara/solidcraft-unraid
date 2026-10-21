use crate::ui_kit::metrics::dimension;
use crate::ui_kit::test_support::*;
use crate::ui_kit::*;

use crate::scenario;

#[test]
fn non_finite_and_tiny_row_widths_are_sanitized() {
    assert_eq!(dimension(f32::NAN), 0.0);
    assert_eq!(dimension(f32::INFINITY), 0.0);
    assert_eq!(dimension(-10.0), 0.0);
    let ctx = crate::fonts::test_context();
    for width in [0.0, 1.0, 12.0] {
        frame(&ctx, vec![], |ui| {
            ui.set_width(width);
            row(ui, "Tiny", |ui| {
                let mut value = String::new();
                field(ui, &mut value);
            });
        });
        assert!(scenario::rect_of("dialog:row:Tiny").unwrap().is_finite());
    }
}
