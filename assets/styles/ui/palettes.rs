//! Original light and dark palette values. Compiled by the UI theme boundary.
use crate::theme::Tokens;
use egui::Color32;
impl Tokens {
    pub const fn light() -> Tokens {
        Tokens {
            app_bar: Color32::from_rgb(52, 56, 64),
            toolbar: Color32::from_rgb(246, 247, 249),
            panel: Color32::from_rgb(250, 251, 252),
            panel_header: Color32::from_rgb(236, 238, 242),
            border: Color32::from_rgb(208, 212, 220),
            text: Color32::from_rgb(38, 42, 50),
            text_dim: Color32::from_rgb(110, 116, 128),
            accent: Color32::from_rgb(38, 120, 218),
            accent_soft: Color32::from_rgb(214, 230, 250),
            sketch_accent: Color32::from_rgb(46, 150, 90),
            error: Color32::from_rgb(214, 54, 54),
            warning: Color32::from_rgb(222, 150, 30),
            icon: Color32::from_rgb(70, 78, 92),
            icon_fill: Color32::from_rgb(178, 196, 222),
            timeline: Color32::from_rgb(232, 235, 240),
            hover: Color32::from_rgb(226, 234, 246),
            sel_face: Color32::from_rgb(76, 127, 203),
            sel_edge: Color32::from_rgb(0, 127, 255),
            sel_edge_rim: Color32::from_rgb(127, 191, 255),
            hover_edge: Color32::from_rgb(0, 0, 0),
            hover_edge_halo: Color32::from_rgb(203, 203, 203),
            hover_face_lift: 40,
            sel_profile: Color32::from_rgb(26, 84, 167),
            sel_profile_edge: Color32::from_rgb(47, 109, 198),
            hover_profile: Color32::from_rgba_unmultiplied_const(75, 129, 253, 48),
            hover_profile_edge: Color32::from_rgb(75, 129, 253),
            sel_vertex: Color32::from_rgb(70, 110, 170),
            origin_plane: Color32::from_rgba_unmultiplied_const(249, 184, 134, 94),
            origin_plane_edge: Color32::from_rgba_unmultiplied_const(214, 140, 84, 200),
            origin_plane_hover: Color32::from_rgba_unmultiplied_const(120, 130, 230, 150),
            origin_point: Color32::from_rgb(214, 215, 218),
            axis_x: Color32::from_rgb(255, 0, 0),
            axis_y: Color32::from_rgb(0, 255, 0),
            axis_z: Color32::from_rgb(2, 2, 248),
            construction_plane: Color32::from_rgba_unmultiplied_const(249, 184, 134, 70),
            construction_geom: Color32::from_rgb(232, 140, 48),
            body: Color32::from_rgb(176, 186, 198),
            body_edge: Color32::from_rgb(40, 44, 52),
            manipulator: Color32::from_rgb(40, 110, 220),
            preview_add: Color32::from_rgb(112, 160, 226),
            preview_sketch: Color32::from_rgb(32, 190, 96),
            preview_cut: Color32::from_rgba_unmultiplied_const(232, 84, 44, 110),
            cube_face: Color32::from_rgb(236, 239, 244),
            cube_border: Color32::from_rgb(85, 94, 110),
            cube_text: Color32::from_rgb(38, 42, 50),
            cube_hover: Color32::from_rgb(190, 220, 252),
            viewport_top: Color32::from_rgb(240, 243, 247),
            viewport_bottom: Color32::from_rgb(203, 210, 221),
            grid: Color32::from_rgb(196, 202, 212),
            grid_major: Color32::from_rgb(170, 178, 190),
            body_selected: Color32::from_rgb(110, 170, 235),
            sketch: Color32::from_rgb(30, 90, 200),
            sketch_fixed: Color32::from_rgb(20, 20, 20),
            sketch_point: Color32::BLACK,
            sketch_locked: Color32::from_rgb(0, 140, 60),
            dimension: Color32::from_rgb(36, 40, 48),
            dimension_driven: Color32::from_rgb(120, 126, 138),
            sketch_point_free: Color32::from_rgb(30, 90, 200),
            section_cap: Color32::from_rgb(214, 128, 72),
            dialog_bg: Color32::from_rgb(250, 251, 252),
            dialog_border: Color32::from_rgb(208, 212, 220),
            chip: Color32::from_rgb(38, 120, 218),
            heading_bg: Color32::from_rgb(218, 223, 231),
            subheading_bg: Color32::from_rgb(234, 237, 242),
            profile_fill: Color32::from_rgba_unmultiplied_const(255, 196, 120, 70),
            glyph_bg: Color32::WHITE,
            glyph_edge: Color32::from_rgb(190, 110, 100),
            glyph_text: Color32::from_rgb(150, 40, 35),
            overlay: Color32::from_rgba_unmultiplied_const(255, 255, 255, 235),
            rubber_band: Color32::from_rgb(30, 120, 230),
            app_bar_text: Color32::from_rgb(230, 234, 242),
            timeline_item: Color32::WHITE,
            timeline_item_error: Color32::from_rgb(250, 220, 220),
            timeline_marker: Color32::from_rgb(70, 76, 88),
            field: Color32::WHITE,
            box_window: Color32::from_rgb(38, 120, 218),
            box_crossing: Color32::from_rgb(40, 160, 80),
        }
    }
    pub const fn dark() -> Tokens {
        let mut t = Tokens::light();
        t.app_bar = Color32::from_rgb(28, 31, 38);
        t.toolbar = Color32::from_rgb(37, 41, 49);
        t.panel = Color32::from_rgb(39, 43, 52);
        t.panel_header = Color32::from_rgb(47, 52, 62);
        t.border = Color32::from_rgb(63, 69, 82);
        t.text = Color32::from_rgb(222, 226, 232);
        t.text_dim = Color32::from_rgb(148, 155, 168);
        t.accent = Color32::from_rgb(58, 136, 238);
        t.accent_soft = Color32::from_rgb(42, 70, 110);
        t.sketch_accent = Color32::from_rgb(110, 200, 140);
        t.error = Color32::from_rgb(240, 100, 96);
        t.warning = Color32::from_rgb(236, 170, 70);
        t.icon = Color32::from_rgb(208, 214, 226);
        t.icon_fill = Color32::from_rgb(86, 112, 150);
        t.timeline = Color32::from_rgb(33, 37, 45);
        t.hover = Color32::from_rgb(54, 61, 74);
        t.hover_edge_halo = Color32::from_rgb(203, 203, 203);
        t.origin_point = Color32::from_rgb(214, 215, 218);
        t.viewport_top = Color32::from_rgb(46, 52, 64);
        t.viewport_bottom = Color32::from_rgb(46, 52, 64);
        t.grid = Color32::from_rgb(59, 65, 76);
        t.grid_major = Color32::from_rgb(73, 79, 90);
        t.body = Color32::from_rgb(152, 156, 162);
        t.body_edge = Color32::from_rgb(16, 16, 18);
        t.sketch = Color32::from_rgb(92, 150, 255);
        t.sketch_fixed = Color32::from_rgb(232, 234, 238);
        t.sketch_point = Color32::from_rgb(232, 234, 238);
        t.sketch_locked = Color32::from_rgb(96, 204, 116);
        t.dimension = Color32::from_rgb(224, 228, 234);
        t.dimension_driven = Color32::from_rgb(142, 150, 164);
        t.sketch_point_free = Color32::from_rgb(92, 150, 255);
        t.profile_fill = Color32::from_rgba_unmultiplied_const(120, 150, 200, 40);
        t.dialog_bg = Color32::from_rgb(59, 68, 83);
        t.dialog_border = Color32::from_rgb(72, 83, 99);
        t.chip = Color32::from_rgb(59, 101, 168);
        t.heading_bg = Color32::from_rgb(38, 44, 55);
        t.subheading_bg = Color32::from_rgb(48, 55, 68);
        t.glyph_bg = Color32::from_rgb(44, 50, 60);
        t.glyph_edge = Color32::from_rgb(200, 130, 120);
        t.glyph_text = Color32::from_rgb(240, 165, 155);
        t.overlay = Color32::from_rgba_unmultiplied_const(34, 38, 46, 240);
        t.rubber_band = Color32::from_rgb(92, 160, 255);
        t.timeline_item = Color32::from_rgb(52, 58, 70);
        t.timeline_item_error = Color32::from_rgb(110, 50, 54);
        t.timeline_marker = Color32::from_rgb(170, 178, 192);
        t.field = Color32::from_rgb(30, 34, 41);
        t.box_window = Color32::from_rgb(70, 150, 255);
        t.box_crossing = Color32::from_rgb(80, 200, 120);
        t
    }
}
