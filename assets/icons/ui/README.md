# UI artwork

Edit the Rust drawing masters in `source/`; the UI crate compiles them through
`crates/ui-egui/src/icons.rs`. Use the 24 × 24 grid and semantic ink/fill/accent
colours. Icon IDs and their geometry live together in `source/icons.rs`;
unknown IDs retain the existing fallback. Add new artwork there without
maintaining a separate ID registry.

| Source | Artwork | Consumers |
| --- | --- | --- |
| `icons.rs` | Toolbar, browser, menu and constraint icons | `icons::paint` |
| `glyphs.rs` | Brand mark, captions, transport, badges, pin, caret, disclosure, radial centre and removal cross | toolbar, titlebar, timeline, browser, menus |
| `navigation.rs` | Signed axes, cube faces/labels and arrows | viewport/navigation cube |
| `markers.rs` | Constraint mappings/letters, snap markers and dimension arrowheads | sketch, assembly and dimensions |
| `pointer.rs` | Agent pointer and tag | agent cursor |

Widget state, layout, projection, picking and commands stay with their widgets.
Review drawing changes in the actual application in both light and dark themes.
No separate native design projects were available for this set. Preserve any
future native work and linked inputs under `source/work/`, with attribution.
Original artwork is MIT OR Apache-2.0; caption geometry retains VectorCraft
attribution in the root `ATTRIBUTION.md`.
