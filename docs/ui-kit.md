# SolidCraft UI kit

`crates/ui-egui/src/ui_kit/` contains the tokenised controls used by feature dialogs,
preferences, manager cards, prompts, context menus and the sketch palette. Import through
`crate::ui_kit::{...}`. The API is crate-visible; implementation modules are private.

## Composition and rules

- Colours, typography, dimensions and spacing come from `theme::Tokens`.
- Sliders only via `ui_kit::slider`; `Slider` options change behaviour, never appearance.
  Raw sliders and duplicate track/thumb painters are forbidden in the UI and apps.
- `scope(ui, add)` supplies form styling. `row(ui, label, add)` aligns labels and
  controls at `control_height` (28 pt). Wide manager rows use a 104 pt label basis
  with a 10 pt gutter, derived from the compact 70 pt basis and spacing tokens.
  Compact rows retain their 8 pt gutter. Labels widen from the appropriate basis
  to at most 1.6 times that width and never exceed half the row.
- `expr_row`, `hint_row`, `text_row` and `choice_row` compose `row` with the existing
  expression, hinted expression, plain text and dropdown controls. They preserve row order,
  labels, focus IDs and completion; callers keep Enter/change ownership and explicit dropdown IDs.
- `Panel` owns bounded windows, feature/palette composition, scrolling bodies,
  pinned actions, compute status and keyboard routing. `wide()` uses three card
  widths. `feature_frame` and `palette_frame` retain docked/floating surfaces.
  `FeatureOptions::apply_label` adds Apply; `Panel::actions` supplies custom labels.
- `Window` composes the shared Panel frame with a 40 pt icon/title/close header,
  padded content and optional 48 pt footer. `show_with_left` pins a left action beside
  the right-aligned buttons. `show_pages` accepts declarative `WindowPage` entries
  (existing icon, i18n label key, stable handle, scroll policy and draw callback).
  Its 188 pt soft page list has 32 pt icon/label rows, selected tint/ink and a divider.
  All nine existing Preferences pages retain their order, settings and handles.
  `actions`, `anchor`/`placed`, `cards`, `fields` and `keep_open_on_escape` retain
  existing window behavior. Empty action labels omit the footer.
- `Confirm` is an input-blocking modal using the same card frame and footer controls.
  It shows a 36 pt icon disc in warning, danger or info tone, a title, caller-supplied
  message and optional monospace `detail`, above a 52 pt footer. Glyphs use the construction
  ramp for warning, the shared shaded danger treatment for danger, and base blue for info;
  the disc keeps its tint. `actions(primary, cancel, left_ghost)`
  places the optional third action at the left. Danger uses `form_error` with a
  contrast-selected on-colour; other primaries use blue. Enter invokes the selected
  answer (primary by default) when valid; Esc cancels. It retains close, answer and
  message scenario handles. The old `Prompt` element is removed.
- Callers supply data and enabled state, then dispatch existing commands. Editors
  leave their established completion, Enter, Tab and commit ownership unchanged.
- Every text input uses the kit's shared `text_input` policy: symmetric horizontal
  `form_inset`, zero vertical margin and centred galley alignment. Font metrics
  therefore centre both mono and proportional text within the allocated field,
  including invalid and attached Library fields, without per-caller offsets.
  Canvas placement editors retain their native line-height plus `space_4` hit
  target so nearby sketch picks are not intercepted; alignment stays shared.
- Scenario handles and accessible labels are API. Consolidated options preserve
  the former geometry, wrapping, native metadata and publication keys.

## Elements

Module names below describe implementation locations, not caller import paths.
Every inline element/variant has one entry in the light/dark `gallery::ELEMENTS`
table. Panel/Window/Confirm compositions also have dedicated geometry and interaction tests.

| Module / element | Use and options | Main tokens |
| --- | --- | --- |
| search: `CommandSearch`, `MenuItem::search_row` | Ribbon search composition: one persistent ribbon editor expands from 28 to 40 pt and to 520 pt wide with its right edge anchored, magnifier and S/Esc keycap, 38 pt two-line menu row with 26 pt icon tile, accent bold substring, path, shortcut/kind keycap, counts, JSON detail box and footer. The ordinary 30 pt menu row cannot fit a second line; this variant keeps its enabled/selected states and scenario IDs. Ranking and dispatch stay with the caller. | `form_card_width × 2 − space_8` (520 pt), `control_height`, `space_2/3/4/6/8/10/12/14/16/24`, `radius_xl/control_radius`, `control_fill`, `dialog_body/footer/rule/border`, `form_ink/dim/primary/error/error_fill/success/success_fill`, `selected_ink`, `accent_soft`, typography |
| menu: `Menu`, `MenuItem` | Shared popup, 30 pt rows, 28 pt icon column, clipped labels, right-aligned mono shortcuts, submenu chevrons, checked/unchecked and document swatches, inert 40% disabled rows and danger ink; headings and separators; ribbon drag/pin targets retained | `dialog_body/border/rule`, `radius_xl`, `control_height/radius`, `form_ink/dim/primary/error`, `accent_soft`, typography and spacing |
| radial: `radial_menu` | Eight rounded annular sectors, parallel 4 pt gaps, 9 pt circular corner joins, tab-accent artwork and hover, centre label/key; existing hold/flick/release dispatch | `dialog_body/border`, `form_ink/dim`, `TabAccent`, icon palette, font/spacing tokens |
| panel: `Window`, `WindowPage` | Shared card frame, header, optional page list, bounded content and footer with a left action | `form_header/footer_height`, `form_card_width`, `form_label_width`, `control_height`, `dialog_footer/rule`, `selected_bg/ink`, spacing |
| confirm: `Confirm`, `Tone` | Modal warning/danger/info, title/message, mono detail and ghost/cancel/primary actions | `warning`, `form_error/fill`, `accent_soft`, `form_primary`, `on_accent`, `tab_on_fill_dark`, `font_subtitle`, spacing |
| metrics: `row_height`, `table_row_height`, `header_height` | Shared control/header heights | `control_height`, `space_8`, `form_header_height` |
| layout: `scope`, `row` | Form styling and aligned label/control columns | `form_label_width`, `form_gap_y`, `control_*`, `font_label`, `dialog_label`, `space_4/8` |
| `caption` | Dim caption aligned beneath a control | `font_small`, `text_dim`, label-column tokens |
| `section`, `section_count`, `divider` | Persisted small-caps disclosure with chevron, optional count badge and trailing hairline; full-width rule | `control_height`, `dialog_rule`, `font_caption`, tracking, spacing |
| `list_row` | Picked item with a remove target | `control_height`, `font_label`, icon/spacing tokens |
| `text`, `Text` | Hint, note, report, inline or cell text; see variants below | `font_small/label`, `text/dim/error`, `control_height`, `space_4` |
| `text_lines` | Virtualized read-only licence/report lines, horizontal overflow | `font_small`, `text`, `control_height` |
| fields: `field`, `hint_field`, `focused_field` | Expression editors, optional empty hint or retained focus; returns accepted Enter | `control_height/fill`, `form_inset`, `font_label` |
| `text_field`, `focused_text_field` | Plain name editor, optionally retaining focus | field tokens |
| `multiline_text_field` | Three-line plain-text row with stable focus ID | field/row tokens |
| `password_field` | Stable secret editor; excludes the secret from scenario/accessibility metadata | field tokens |
| `message_field` | Growing message editor; owner retains scroll and Enter routing | field tokens |
| `cell_editor`, `bounded_cell_editor`, `search_field`, `library_cell` | Stable-ID inline/search editors; bounded variant caps Unicode characters; `library_cell` attaches a Library dropdown inside the editor border | field tokens, `text/error` |
| `text_area` | Bounded multiline JSON/data editor | field tokens |
| `dropdown`, `dropdown_menu` | Indexed choices or caller-supplied menu controls | `control_height`, `text_dim`, form tokens |
| `searchable_dropdown`, `SearchOption` | Filtered choices, custom-value action and pending message | dropdown tokens |
| `selection_field`, `Selection` | Selection chip with explicit `active` and `filled` state | `selected_bg/ink`, `accent`, `control_*`, `form_inset`, `space_8/24` |
| `value_pair` | Paired captions/expressions; optional independent field names | row/field tokens, `space_8` |
| `fraction_field`, `scalar_field` | Locale-aware bounded dimensionless values; scalar fields fill their control column | form control/font tokens |
| `number_field` | Integer/decimal field with caller range, drag speed and suffix | form control/font tokens |
| `length_field`, `angle_field` | Length in design units, angle in degrees | form control/font tokens |
| `slider` + `Slider` options | One token-painted track/thumb over native slider/value interaction; optional step, caller formatting, whole-step manipulation and hidden value editor | `control_height`, `space_24/8/4/2`, form control/font tokens |
| `placement_field` | Stable focus ID; drawing tool owns Enter/Tab validation and completion | `control_fill`, `font_body`, `text`, `control_height`, spacing |
| `color_field`, `color_popover` | Opaque document RGB swatch and kit popover; global U8/F segments, copy, gamma-byte or linear RGB fields, painter-drawn HSV square/hue and current strip; outside click dismisses | `control_*`, `form_*`, `border`, `on_accent`, `space_*` |
| choice: `segmented`, `Segments` | Equal text choices; `states` supplies mixed selection, `compact` clips instead of falling back to a dropdown, `primary` uses accent/on-accent for the selected segment | `control_*`, `selected_bg/ink`, `font_label`, `space_4/8` |
| `segmented_icons`, `IconSegment` | Equal icon choices and mixed states | segment tokens, icon palette |
| `checkbox`, `checkbox_indeterminate`, `checkbox_row`, `checkbox_grid`, `options` | One box/tick painter for inline, mixed, full-row, grid and menu choices | `space_14/3/2`, `radius_sm`, `control_fill/border`, `form_primary/ink`, `hover`, `on_accent` |
| `text_emphasis` | Bold/italic with translated accessible names | form control/font tokens |
| `tool_choices`, `ToolChoice`, `toggle_icon` | Wrapped command modes or independent icon toggles | sketch mode/icon tokens, `accent` |
| `list_picker` | Selectable row with caller identity and native checked state | `control_height`, form selection tokens |
| `popup_list`, `PopupOption` | Focus-neutral suggestions; owner retains keyboard routing | `space_2`, `form_label_width`, `control_height`, form tokens |
| `color_swatch`, `swatch_grid`, `color_thumbnail` | 30 pt draggable samples, 6 pt grid step gap, selected outer ring and checker pattern under transparent colours; caller payloads retained; local-library thumbnails use 16 pt | `control_height/radius`, `border`, `form_primary`, `control_fill`, `space_2/6/8` |
| actions: `button`, `Button` | Secondary action: `Normal`, `Small`, or `Enabled(bool)` | native sizing, form font/control tokens |
| `primary_button` | Accent action outside a footer | `accent`, `on_accent`, form tokens |
| `icon_button_at` | Icon action inside a supplied rectangle | `form_icon_inset`, `text/dim`, icon palette |
| `button_pair`, `PairButton` | Equal secondary actions, optional icons and selected state | `form_button_height`, control/selection/icon tokens |
| `footer`, `Footer`, `Action` | OK/Cancel with optional Apply, compute status or selected action | `form_footer/button_height`, `dialog_footer/rule`, `control_*`, `accent`, spacing |
| `keyboard` | Esc cancels, unowned Enter submits when valid | none |
| `action_menu`, `command_menu_button` | Nested dense action menu or ribbon overflow action retaining its icon, shortcut and enabled state | form control/font tokens, `space_16`, icon palette |
| table: `table`, `table_custom`, `Column`, `Table::headers`, `Table::row`, `TableRow::cell`, `Table::add_prompt` | Left-aligned headers/cells, horizontal overflow, selected/hover rows and empty state. `table_custom` supplies header actions through `headers`. `Column::flex` shares remaining width by weight, freezing columns at their control minimums before redistributing the remaining space; `Column::icon_only` hides the heading while retaining its automation identity. `add_prompt` is the final slim dashed line and requests a regular draft row from its owner; `Column::new(label, width)` uses label-column multiples, `min_content_width(points)` reserves measured control content; `ContentWidth::name` measures strong configuration names | label-column tokens, `control_height`, `dialog_rule/footer`, `selected_bg`, spacing |
| `removable_header`, `plus_header`, `bin_button`, `active_pill` | Hover-only column removal, anchored Add Column menu with insertion brackets, icon-only immediate deletion and active-row status. Bins preserve `dialog:button:Actions:Delete` and `:×`; header removal preserves `dialog:button:LABEL:×`. | `control_*`, `form_*`, `selected_bg`, spacing |
| `compact_button`, `summary_row` | Borderless stars and chevrons with enabled/selected states; named two-line rule summary beside an enable checkbox and compact actions. | `form_close_size`, `control_*`, `form_*`, typography and spacing |
| `label_actions` | Clipped row name beside compact actions | `control_height`, form font/spacing tokens |
| status: `status_header`, `StatusHeader` | Supplied icon/name/status and constraint meter | `font_label`, `text/dim`, `accent`, sketch/header tokens |
| `progress` | Clamped determinate progress | `space_3/6`, `dialog_rule`, `accent` |

`text(ui, text, options)` supports the existing text behaviors and the new error band:

| Option | Layout and scenario publication |
| --- | --- |
| `Text::Hint(key)` | Wrapping 12 pt hint; rect and handle `dialog:hint:KEY` |
| `Text::Note(key)`, `Text::MonoNote(key)` | Wrapping small dim text (optional monospace) with vertical padding; rect `dialog:note:KEY` |
| `Text::Error(key)` | Wrapping full-width error band with tinted background and padding; retains `dialog:report:KEY` |
| `Text::Report(key, error)` | Native wrapping label, optional error colour; rect/handle `dialog:report:KEY` |
| `Text::Heading(key)` | Semibold body heading; retains `dialog:report:KEY` |
| `Text::NameCell(strong)` | Medium/semibold clipped clickable name; retains `dialog:cell:TEXT` |
| `Text::MonoCell`, `Text::Value(key, error)` | Monospace unit/value cells; clipped to one row with full tooltips. Value/error cells retain `dialog:report:KEY`. |
| `Text::Inline` | Native truncated label; rect/handle `dialog:inline:TEXT` |
| `Text::Cell` | Fixed-height clipped clickable cell with tooltip; rect/handle `dialog:cell:TEXT` |

Examples:

```rust
segmented(ui, &mut mode, &["One", "Two"], Segments::default());
selection_field(ui, "Select", Selection { active: true, filled: false });
value_pair(ui, "Position", [&mut x, &mut y], ["X", "Y"], Some(["Distance X", "Distance Y"]));
button(ui, "Add", Button::Enabled(valid));
footer(ui, "OK", "Cancel", valid, Footer { apply: Some("Apply"), ..Default::default() });
text(ui, report, Text::Report("validation", invalid));
```

Icon surfaces that explicitly choose a palette ramp use `icons::paint_accented`.
`None` selects base blue, while unknown names retain the icon's semantic accent.
Only active roles change: ink/line outlines, passive faces, guides and knockouts keep
normal artwork colours. Existing icon painters retain their semantic-accent behaviour.
Danger menu items and danger confirmation glyphs both use `icons::paint_danger`.
It additionally maps passive-1/2/3 faces to constraint active-1/2/3, retaining the
shading steps, ink outlines, handle and transparent slat details. The cache distinguishes
this whole-glyph treatment from ordinary accent selection and monochrome ink masks.
Radial icons follow the active tab (Sketch, Surface, Mesh, Form; other tabs use base blue)
in both normal and hovered sectors; the hovered background retains its tab tint.

## Extending and checking the kit

Add one implementation, document its tokens/options here, re-export it through
`mod.rs`, and add one table entry in `gallery::ELEMENTS`. Keep implementation-only
helpers private. Add interaction tests when the element introduces an action.

The gallery checks geometry, label alignment, handles and effective paint bounds in
both themes. Field/footer/panel tests retain focus, completion and action coverage.
`ui_kit_guard_tests` scans kit callers recursively, rejects raw control constructors
and methods (including aliases), and rejects removed legacy helpers. The raw-control
allowance table is empty and guarded against additions. Standard colour/font guards,
Clippy, wasm, i18n, assets, icons and layers still apply.

See [remaining coverage boundaries](manage-ui-kit-audit.md) for specialised surfaces
that remain outside the kit and why.

## Popup presentation and draft rows

Panel and Window use 28 pt controls, a 40 pt title band (14 pt semibold title,
19 pt command icon and 26 pt close/fold targets), and a 48 pt footer with
28 pt actions. Table rows are `control_height + space_8` (36 pt), independent
of control height. Bodies use 12 pt insets. Width limits and scroll boundaries
remain owned by Panel/Window/Confirm. Titles retain the supplied case.

The `form_ink`, `form_dim`, `form_primary`, `form_error` and `form_error_fill`
colour keys match the approved light cards. Dark values follow existing text,
accent and error palette roles; every role is exported by theme files and measured
by the colour-vision checks. Invalid fields include both error text and a tinted
surface. Long variable-height lists can use `Window::placed` with a top-left pivot to
keep their painted controls and scenario targets aligned as they grow.
Completion lists use the approved 230 pt minimum width (derived from card and
spacing tokens), show kind labels on the right, selected details beneath
the list, and the existing keyboard hint.

Parameters and Configuration Table use this one table grid, including Favorites
and each model group. The grid has `space_8` horizontal insets within the row
highlight, shared by headers and cells, plus `space_8` column gaps. Flexible widths
are measured inside the scroll viewport and rounded by cumulative column edges;
fitting content does not acquire a horizontal scrollbar from independent rounding
or inherited scroll margins. Explicit minimum widths still allow genuine overflow.
The viewport and measured content width publish `dialog:table:TABLE:viewport` and
`dialog:table:TABLE:content_width` for geometry checks. Hover uses the clipped row
rectangle, including interactive children, while preserving child click ownership.
Their add prompts publish `dialog:table:TABLE:add`, use `control_height - space_4`
height, a dashed `control_radius` outline and a pointing-hand cursor.
Clicking creates a regular unsaved row, focuses its name and exposes the existing
field controls and bin. Enter or valid focus loss commits through the existing
commands; abandoning a nameless row drops it. Configuration Enter with an empty
name retains ConfigurationN naming and copies the active row. Parameter validity
is published as `param:new:valid`; invalid expressions cannot commit on focus loss.
The natural scrolling body height is published as `dialog:body:content_height`,
so geometry tests distinguish content size from the viewport cap.
After a successful parameter add, a fresh empty draft retains Name focus for
continuous keyboard entry; leaving that empty row drops it.
Parameter draft fields retain `param:new:name/unit/expression/comment`.
Configuration drafts publish `config:new:name`; existing `config:cell:ROW:COLUMN`,
activation, rule and preview handles remain. The header plus retains
`dialog:menu:Add Column`, including the existing menu nesting. The old separate
Add forms and column-removal band have been removed.


Crash Reports uses `report_list` (bordered bounded scroll), `report_header` (kind,
count pill and mono UTC time), `path_chip` (clipped mono path and adjacent action),
`empty_box` (dashed empty state), and `icon_label_button` (secondary icon/text,
optional danger ink). These use `dialog_rule/footer`, `control_*`, `form_*`,
`font_label/small` and spacing tokens. The report header retains the combined-title
cell alias; bins retain the previous Delete handle. Report actions remain immediate.
`list_frame` supplies the same border to virtualised library rows.
The list frame owns its border and horizontal padding; the report viewport adds
no inherited scroll margin. Report list measurements publish
`dialog:list:KEY:viewport` and `dialog:list:KEY:content_width`. Reports use vertical
scrolling; the horizontal rule beneath each report remains a divider.
Appearance uses a 340 pt card derived from card/label/spacing tokens, with the
104 pt label basis where space permits, 10 pt body gaps, and a count on In This Design.

Colour popovers retain `dialog:color:ROW` and add `color:format:U8/F`,
`color:copy`, `color:channel:R/G/B`, `color:sv`, `color:hue`, `color:current`
and the `color:popover` rectangle. Format changes update egui's global numeric
colour presentation without changing RGB. Linear fields edit linear RGB, not
normalised gamma bytes. HSV state preserves hue at zero saturation/value and
sub-byte precision during drags. Gradient colours are document colour data;
all interface chrome uses existing paired light/dark tokens.

Sliders only via `ui_kit::slider`, everywhere in `crates/ui-egui/src` and `apps`.
The element owns its form styling even when called from a viewport menu.
`Slider` options select behaviour, never appearance: range and values remain in caller
units, `step` preserves stepping, `format` preserves suffixes/display mapping,
`integer` rounds manipulation and `show_value: false` hides the editor. Disabled
sliders use `ui.add_enabled_ui`; tooltips attach to the returned response. Preferences
UI scale still applies only after the pointer is released. There are no alternative
slider helpers, raw `egui::Slider` callers, or custom track/thumb painters outside
the kit. The source guard scans both trees without an allowance table; the gallery
covers continuous, percent/stepped, integer/no-editor and disabled states in both themes.
Numeric fields use the existing kit `scalar_field`, `length_field`, `angle_field`
or `number_field` rather than constructing a raw `egui::DragValue` at a caller.

Parameters retains its bounded header drag offset while the manager is open,
including frames without a regular feature dialog. The shared Panel bound reserves
header and footer space at the bottom edge. Import and both export actions use
`icon_label_button` with the existing `import`/`export` arrow icons and unchanged
command routing and scenario identities. The gallery includes all three actions.
The Add Column target is `form_close_size - space_4` (22 pt) square: idle paint is
only `form_dim` ink; hover adds the selected accent fill and insertion guides.
HSV gradients use a rounded mesh silhouette sampled along `control_radius` arcs;
the current-colour strip uses the same rounded radius. Render tests sample all
four corner pixels and opaque interiors of all three surfaces at 1× and 2× in
both themes.

Checkboxes only via `ui_kit::checkbox`, everywhere in `crates/ui-egui/src` and
`apps`. `checkbox_row`, `checkbox_grid` and menu checkbox rows share its single
box painter and `paint_check_glyph` path. The box is `space_14` square with
`radius_sm` corners, a `control_border` outline and `control_fill` background.
Checked/mixed boxes use `form_primary` and `on_accent`; hover uses `hover` and
an accent border, focus adds a `space_2` outset ring, and disabled controls inherit
egui's disabled opacity. Geometry never depends on caller font or native icon size.
`checkbox_indeterminate` represents a mixed selection with a bar and activates to
checked. Space/Enter, accessible checked state, tooltips and scenario handles are
retained. Exclusive choices use existing kit segments/dropdowns/list pickers;
there are no radio controls. The recursive guard rejects native checkbox/radio
constructors (including aliases), raw toggle methods and duplicate check glyph
painters across both trees without allowances. Offscreen checks compare actual
box and tick geometry/pixels across form, grid, palette and menu call sites at
1×/2× in both themes.

Disclosure chevrons point **down when open** (content visible) and **right when
closed** (content hidden). Only `ui_kit::disclosure` draws them, using the original
`chevron` / `chevron_right` SVGs at `form_section_icon_size`, without animation
(as in kit section headers). Sections, nested groups, Browser trees, card headers,
window folds and the Assistant strip all follow this rule. Keep existing scenario
handles, including `dialog:collapse`, and translated Collapse/Expand tooltips.
Dropdown arrows always point down; submenu arrows point right. Playback, reorder,
window management and overflow controls are not disclosures. The source guard scans
UI, apps and compiled asset glyphs without caller allowances; render tests inspect
the actual icon textures used by sections, card folds and window folds.

## Per-tab accent tokens

`TabAccent::for_tab` supplies three independent theme colours: `color` (fill) for
pills, active-tool borders and the history marker; `ink` for small text and selected
rows; `soft` for tinted selection/tool backgrounds. These are palette entries, not
blends. Browser rows and selected ribbon labels use `ink` directly.

| Tab id | Fill token | Ink token | Soft token |
| --- | --- | --- | --- |
| `MODEL` / default | `accent` | `selected_ink` | `accent_soft` |
| `SKETCH` | `sketch_accent` | `sketch_ink` | `sketch_soft` |
| `SURFACE` | `surface_fill` | `surface_ink` | `surface_soft` |
| `MESH` | `mesh_fill` | `mesh_ink` | `mesh_soft` |
| `FORM` | `form_fill` | `form_tab_ink` | `form_soft` |
| `SHEET METAL` | `sheet_fill` | `sheet_ink` | `sheet_soft` |
| `RENDER` | `render_fill` | `render_ink` | `render_soft` |

`form_tab_ink` is separate from the existing `form_ink` used for form controls.
Sketch reuses its existing fill in both themes. Surface and Form fills match their
existing accents in light mode but need independent dark values; Mesh and Render
fills also differ from their existing `mesh_accent` and `render_accent` values.
All existing tokens, including Model's fill, ink and tint, retain their values.
The Model/default mapping preserves its existing appearance rather than adopting
the proposed Model colours from the board.

The new palette entries (fill / ink / soft) are:

| Tab | Light | Dark |
| --- | --- | --- |
| Sketch | `#057F45` / `#057F45` / `#EDF7F1` | `#128D45` / `#46BE78` / `#1F3B2B` |
| Surface | `#A45000` / `#A45000` / `#FBEBD8` | `#B26A14` / `#E0914A` / `#45331E` |
| Mesh | `#E0B200` / `#8A6D00` / `#FFF8DC` | `#C6A800` / `#D4B726` / `#41391A` |
| Form | `#7349CD` / `#7349CD` / `#EEE8FA` | `#8A6FCC` / `#B39BEE` / `#372F4F` |
| Sheet Metal | `#1E6F96` / `#1E6F96` / `#E1F0F7` | `#2A7FA8` / `#4FB3D9` / `#1F3A47` |
| Render | `#A8326E` / `#A8326E` / `#F8E6EF` | `#C8588F` / `#E074AE` / `#402336` |

Each key is supported by `solidcraft-theme/1` import/export, including
`tab_on_fill_dark` (`#000000` in both themes). `TabAccent::on_fill` uses `on_accent`
(white in the built-in palettes) when it reaches 4.5:1, otherwise that dark ink.
The fallback is black because the board's dark ink does not reach 4.5:1 on several
of the approved fills. It only styles tab pills; global primary actions still use
`on_accent`. Built-in palette tests require WCAG contrast of at least 4.5:1 for
ink on the card and panel, ink on soft, and pill text on fill in both themes.
Only Mesh separates ink from fill in the light theme.

Context menu adapters retain item IDs, enabled logic and command parameters. `Menu`
owns popup elevation (10 pt corners, 1 pt border), headings, separators, adjacent
submenus and bounded arrow navigation. Enter opens a submenu or chooses its row;
Right opens, Left returns, and Esc dismisses the menu. Appearance includes document
swatches and Remove override through `appearance.assign`; its original action still
opens the full editor. Workspace retains its descriptive cards and scenario handles
on the shared surface. Ribbon rows reuse `MenuItem::paint` while owning drag, focus
and pin interactions. History context menus and ribbon overflow use kit popup chrome.
Rename uses the shared caption, field policy and Cancel/OK actions with its stable
editor ID. No additional translation keys or colour roles are needed.

The menu gallery includes normal, hovered, open, disabled, checked/unchecked,
swatch and danger rows, a submenu and rename. Radial geometry tests measure the
actual side endpoints at both radii and sample all four rounded corners. Optional
CPU-rendered menu evidence can be produced without a window with
`SOLIDCRAFT_MENU_EVIDENCE=/tmp/menus cargo test -p solidcraft-ui-egui --lib menu_board_in_both_themes`.
Desktop interaction and visual acceptance still require a reviewer capture.

Command search uses the ribbon field itself (`sc_sbox_text`); `ribbon:search`,
`shortcut:search` and `palette:search` all identify that same field. Opening gives
it the accent focus ring, cursor and Esc keycap. The field and the attached results
sheet unfold in 120 ms with egui cubic ease-out, capped by egui's animation time
so instant settings remain instant. Esc, click away and command dispatch reverse
the animation. Only the visible sheet height participates in layout and hit testing;
result content is clipped while unfolding. The sheet's `ribbon:search_popup` /
`palette:outer` handles exclude the field. Its footer (`ribbon:search_footer`)
fills the bottom strip edge to edge, with rounded bottom corners and a 1 pt top
rule. Ranking, command mode and keyboard routing remain with the caller.

Generate CPU-rendered evidence of all five search states and the actual ribbon's
closed/opening/open/closing transition in both themes with
`SOLIDCRAFT_SEARCH_EVIDENCE=/tmp/search cargo test -p solidcraft-ui-egui --lib command_search`.
