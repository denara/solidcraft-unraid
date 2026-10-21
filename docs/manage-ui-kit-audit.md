# UI kit coverage boundaries

Feature dialogs, manager cards, settings/help/customisation windows, confirmations
and matching inline controls now compose `ui_kit`. The raw-control allowance table
is empty. The cleanup removes unused frame chrome, row-publication state, appearance
wrappers and the plastic editor fallback. Shared controls and their options are
listed in [the kit reference](ui-kit.md).

These specialised surfaces remain in their owners:

| Surface / source | Why it remains outside the kit |
| --- | --- |
| Start page: `home.rs` | Painted start tiles, thumbnails and links rather than form cards |
| Assistant transcript/composer containers, gallery and docking: `agent_chat/panel.rs` | Conversation/media layout, rich selectable content, drag/docking behavior; matching controls use the kit |
| Native file/program pickers: `file_picker.rs`, host services | Host-native sheets have no egui control surface; load/progress cards use the kit |
| Command-result ranking/list/pins: `shortcut_box.rs` | Search layout and ranking; popup shell and matching controls use the kit |
| Ribbon and browser/timeline rows: `toolbar.rs`, `browser.rs`, `timeline_left.rs` | Specialised navigation, canvas painting, drag/drop and inline rename placement; context/overflow menus, their rows and floating rename now use the kit |
| Motion transport, key strips and chart: `dialogs_motion.rs`, `dialogs_motion_plot.rs` | Playback/chart interaction; matching form controls use the kit |
| Generated part thumbnails: `dialogs_parts.rs` | Cached images rather than controls |
| Joint-browser glyphs and flat-pattern canvas: `dialogs_assembly.rs`, `dialogs_sheet.rs` | Model navigation and geometry; dialog/export actions use the kit |
| Slicer program popup entries: `dialogs_save_mesh.rs` | Painted installed-program icons/entries and native program picking; dropdown shell uses the kit |
| Dimension/placement geometry: `dim_view.rs`, `sketch_dims.rs`, `canvas.rs` | Canvas positioning, completion ownership and geometry painting; matching dimension/placement editors use the kit |
| Select Other and viewport display controls: `select_other.rs`, `viewport.rs` | Painter-based entity icons, hit testing and hover highlighting |
| Document tabs and render canvas: `documents.rs`, `render_workspace.rs` | Tab-strip/canvas painting and popup positioning; overflow choices/settings controls use the kit |
| Native colour chooser inside `color_field` | Kit owns the field; egui owns its HSV/RGB popup internals |

Immediate commands such as `parameters.graph`, `parameters.move`, `config.rules`,
`config.preview` and `library.*` keep their existing JSON execution behavior. Their
related controls live in Parameters, Configurations and Appearance; the kit creates
no extra command dialogs. Command IDs, model operations and scenario handles are
unchanged by cleanup.

Context menus are covered by `Menu`/`MenuItem`, including browser and history targets,
workspace popup chrome, Appearance submenus, section headings, separators, checks,
disabled/danger rows and ribbon pin rows. The optional marking menu uses the kit's
rounded-sector renderer; its command choices and hold/flick behavior remain owned
by `context_menu.rs`. Floating rename uses kit fields and actions; Properties already
uses the shared Window. No menu-frame or raw-control allowances were added.
The light/dark gallery and CPU evidence cover representative row states and radial
geometry; desktop captures of the live menus remain part of reviewer acceptance.
