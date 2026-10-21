//! Panel elements for dense cards; see `docs/ui-kit.md` for tokens and usage.
use super::{actions::icon_button_at, metrics::*, text::paint_text};
use crate::{scenario, theme::Tokens};
use egui::{Ui, vec2};

/// Feature title/icon/close band. Tokens: `form_header_height, form_icon_size, form_close_size, form_inset, font_label, text, dialog_rule, space_8/10; icon_button_at tokens`.
pub(crate) fn header(ui: &mut Ui, icon: &str, title: &str, on_close: impl FnOnce()) {
    header_impl(ui, icon, title, None, on_close);
}

/// Optional fold action for managers which historically hide their inputs without losing edits.
/// Tokens: header tokens and space_4; retains the dialog:collapse scenario handle.
pub(crate) fn collapsible_header(ui: &mut Ui, icon: &str, title: &str, collapsed: &mut bool, on_close: impl FnOnce()) {
    header_impl(ui, icon, title, Some(collapsed), on_close);
}

fn header_impl(ui: &mut Ui, icon: &str, title: &str, collapsed: Option<&mut bool>, on_close: impl FnOnce()) {
    let t = Tokens::get();
    let (rect, _) = ui.allocate_exact_size(vec2(dimension(ui.available_width()), header_height()), egui::Sense::hover());
    // Shared card chrome: 12 pt left inset, 19 pt icon, 8 pt gap and 26 pt close target.
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + t.space_12 + t.form_icon_size / 2.0, rect.center().y),
        vec2(t.form_icon_size, t.form_icon_size),
    );
    crate::icons::paint_icon(ui.painter(), icon_rect, icon, 1.0);
    let close_rect = egui::Rect::from_center_size(
        egui::pos2((rect.right() - t.space_8 - t.form_close_size / 2.0).max(rect.left()), rect.center().y),
        vec2(t.form_close_size, t.form_close_size),
    );
    let close = icon_button_at(ui, close_rect, "close", "Close");
    scenario::publish_handle("dialog:close", close.rect.center());
    if close.clicked() {
        on_close();
    }
    let right = if let Some(collapsed) = collapsed {
        let fold = egui::Rect::from_min_size(egui::pos2(close_rect.left() - t.form_close_size - t.space_4, close_rect.top()), close_rect.size());
        let response = super::disclosure_button_at(ui, fold, !*collapsed, crate::i18n::tr_label(if *collapsed { "Expand" } else { "Collapse" }));
        scenario::publish_handle("dialog:collapse", response.rect.center());
        if response.clicked() {
            *collapsed = !*collapsed;
        }
        fold.left()
    } else {
        close_rect.left()
    };
    let left = (icon_rect.right() + t.space_8).min(right);
    let title_rect = egui::Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(right.max(left), rect.bottom()));
    paint_text(ui, title_rect, title, crate::fonts::semibold(t.font_heading), t.form_ink, false);
    let title_response = ui.interact(title_rect, ui.make_persistent_id("title_text"), egui::Sense::hover());
    title_response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), title));
    scenario::publish_rect("dialog:header", rect);
    ui.painter().hline(rect.x_range(), rect.bottom(), egui::Stroke::new(1.0, t.dialog_rule));
}

/// A bounded docked or floating card. Uses form_card_width and the caller's tokenised frame.
/// Window IDs and content scopes are retained so moving a card does not reset field focus.
pub(crate) struct Panel<'a> {
    title: &'a str,
    id: egui::Id,
    anchor: egui::Pos2,
    viewport: egui::Rect,
    side: crate::frame::Side,
    frame: egui::Frame,
    width: f32,
    ok: &'a str,
    cancel: &'a str,
    reset_scroll: bool,
}

pub(crate) struct FeatureOptions<'a> {
    pub icon: &'a str,
    pub title: &'a str,
    pub drag_key: &'a str,
    pub moved: egui::Vec2,
    pub docked: egui::Pos2,
    pub apply_label: Option<&'a str>,
}

/// State read after rendering the fields, just as in the original feature card.
pub(crate) struct ComputeStatus {
    pub text: Option<String>,
    pub busy: bool,
    pub applying: bool,
}

pub(crate) enum Part {
    Header,
    Body,
    Footer,
}

impl<'a> Panel<'a> {
    /// Shared floating card surface for panels, windows and modal confirmations.
    pub(super) fn card_frame() -> egui::Frame {
        let t = Tokens::get();
        crate::frame::shell_card(t.dialog_body).stroke(egui::Stroke::new(1.0, t.dialog_border)).corner_radius(t.radius_xl).inner_margin(0.0)
    }

    /// Feature surface; Tokens: dialog_body and shell_card/dock_frame's border/shadow tokens.
    pub(crate) fn feature_frame(ctx: &egui::Context, layout: crate::shell::LayoutMode, side: crate::frame::Side) -> egui::Frame {
        let t = Tokens::get();
        if layout == crate::shell::LayoutMode::Floating {
            Self::card_frame()
        } else {
            crate::frame::dock_frame(ctx, side).fill(t.dialog_body).inner_margin(0.0)
        }
    }

    /// Palette surface; Tokens: panel, sketch_palette_alpha, radius_xl and shell/dock tokens.
    pub(crate) fn palette_frame(ctx: &egui::Context, layout: crate::shell::LayoutMode) -> egui::Frame {
        let t = Tokens::get();
        if layout == crate::shell::LayoutMode::Floating {
            crate::frame::shell_card(t.panel.gamma_multiply(t.sketch_palette_alpha)).corner_radius(t.radius_xl).inner_margin(0.0)
        } else {
            crate::frame::dock_frame(ctx, crate::frame::Side::Right).fill(t.panel.gamma_multiply(t.sketch_palette_alpha)).inner_margin(0.0)
        }
    }

    pub(crate) fn new(title: &'a str, id: egui::Id, anchor: egui::Pos2, viewport: egui::Rect, side: crate::frame::Side, frame: egui::Frame) -> Self {
        Self { title, id, anchor, viewport, side, frame, width: Tokens::get().form_card_width, ok: "OK", cancel: "Cancel", reset_scroll: false }
    }

    /// A newly opened manager starts at its first section. Later frames retain user scrolling.
    pub(crate) fn reset_scroll(mut self, reset: bool) -> Self {
        self.reset_scroll = reset;
        self
    }

    pub(crate) fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// The footer's primary and closing actions when they are not OK and Cancel. An empty `ok`
    /// leaves the primary action out (a card that only closes, e.g. Measure).
    pub(crate) fn actions(mut self, ok: &'a str, cancel: &'a str) -> Self {
        (self.ok, self.cancel) = (ok, cancel);
        self
    }

    /// Table surface width derives from the same card-width token and remains viewport bounded.
    pub(crate) fn wide(self) -> Self {
        self.width(Tokens::get().form_card_width * 3.0)
    }

    /// The painted anchor also drives dragging, including the shell's edge clearance.
    fn bounded_anchor(&self) -> egui::Pos2 {
        let viewport = self.viewport;
        let width = self.width.min(viewport.width().max(0.0));
        let x = match self.side {
            crate::frame::Side::Right => {
                self.anchor.x.clamp(viewport.left() + width, (viewport.right() - Tokens::get().space_8).max(viewport.left() + width))
            }
            crate::frame::Side::Left => self.anchor.x.clamp(viewport.left(), viewport.right() - width),
        };
        let chrome =
            Tokens::get().form_header_height + Tokens::get().form_footer_height + self.frame.total_margin().sum().y + crate::shell::SHELL_GAP;
        egui::pos2(x, self.anchor.y.clamp(viewport.top(), (viewport.bottom() - chrome).max(viewport.top())))
    }

    pub(crate) fn window(&self) -> egui::Window<'a> {
        let gap = crate::shell::SHELL_GAP;
        let frame = self.frame;
        let viewport = self.viewport;
        let anchor = self.bounded_anchor();
        let side = self.side;
        let margin = frame.total_margin();
        let width = self.width.min(viewport.width().max(0.0));
        let inner_width = (width - margin.sum().x).max(0.0);
        let height = (viewport.bottom() - anchor.y - gap).max(0.0);
        // Keep drawing safe when even the header/footer cannot fit. egui clips the contents to
        // the remaining viewport; the normal small-height case still keeps both actions visible.
        egui::Window::new(self.title)
            .enabled(!crate::issue_report::cad_input_blocked())
            .id(self.id)
            .title_bar(false)
            .pivot(match side {
                crate::frame::Side::Right => egui::Align2::RIGHT_TOP,
                crate::frame::Side::Left => egui::Align2::LEFT_TOP,
            })
            .fixed_pos(anchor)
            .constrain(false)
            .collapsible(false)
            .resizable(false)
            .min_width(inner_width)
            .max_width(inner_width)
            .default_width(inner_width)
            .min_height(0.0)
            .max_height((height - margin.sum().y).max(0.0))
            .scroll(false)
            .frame(frame)
            .default_height(0.0)
            .fade_in(false)
            .fade_out(false)
            .order(egui::Order::Middle)
    }

    /// Feature header, scrolling padded body, pinned actions, drag target and compute status.
    /// Tokens: form_header_height, form_footer_height, space_6/8/10/14/16, dialog_body.
    pub(crate) fn feature(
        &self,
        ui: &mut Ui,
        heading: FeatureOptions<'_>,
        disabled: bool,
        valid: bool,
        body: impl FnOnce(&mut Ui) -> ComputeStatus,
    ) -> (bool, super::Action) {
        self.feature_impl(ui, heading, disabled, valid, None, body)
    }

    /// Same card with a persistent fold state owned by its caller.
    pub(crate) fn feature_collapsible(
        &self,
        ui: &mut Ui,
        heading: FeatureOptions<'_>,
        disabled: bool,
        valid: bool,
        collapsed: &mut bool,
        body: impl FnOnce(&mut Ui) -> ComputeStatus,
    ) -> (bool, super::Action) {
        self.feature_impl(ui, heading, disabled, valid, Some(collapsed), body)
    }

    fn feature_impl(
        &self,
        ui: &mut Ui,
        heading: FeatureOptions<'_>,
        disabled: bool,
        valid: bool,
        mut collapsed: Option<&mut bool>,
        body: impl FnOnce(&mut Ui) -> ComputeStatus,
    ) -> (bool, super::Action) {
        super::scope(ui, |ui| {
            let t = Tokens::get();
            let min = ui.cursor().min;
            let band = egui::Rect::from_min_size(min, vec2(ui.available_width(), super::header_height()));
            crate::frame::drag_dock(ui, band, heading.drag_key, heading.moved, self.bounded_anchor() - heading.docked);
            let mut close = false;
            if let Some(collapsed) = collapsed.as_deref_mut() {
                collapsible_header(ui, heading.icon, heading.title, collapsed, || close = true);
            } else {
                header(ui, heading.icon, heading.title, || close = true);
            }
            let bottom = self.viewport.bottom() - crate::shell::SHELL_GAP - self.frame.inner_margin.bottomf() - self.frame.stroke.width;
            let footer_height = t.form_footer_height;
            let fields_height = (bottom - ui.cursor().top() - footer_height).max(0.0);
            let fields_rect = egui::Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), fields_height));
            let state = if collapsed.as_deref().copied().unwrap_or(false) {
                scenario::publish_count("dialog:body:content_height", 0.0);
                ComputeStatus { text: None, busy: disabled, applying: disabled }
            } else {
                ui.scope_builder(egui::UiBuilder::new().max_rect(fields_rect), |ui| {
                    if self.reset_scroll {
                        // Clear persisted offset, animation and momentum once per opening.
                        egui::scroll_area::State::default().store(ui.ctx(), ui.make_persistent_id(egui::IdSalt::new("feature_fields")));
                    }
                    let scroll =
                        egui::ScrollArea::vertical().id_salt("feature_fields").min_scrolled_height(0.0).max_height(fields_height).show(ui, |ui| {
                            if disabled {
                                ui.disable();
                            }
                            egui::Frame::NONE
                                .inner_margin(egui::Margin {
                                    left: t.space_12 as i8,
                                    right: t.space_12 as i8,
                                    top: t.space_12 as i8,
                                    bottom: t.space_12 as i8,
                                })
                                .show(ui, body)
                                .inner
                        });
                    scenario::publish_count("dialog:body:content_height", f64::from(scroll.content_size.y));
                    scenario::publish_count("dialog:body:scroll_y", f64::from(scroll.state.offset.y));
                    scenario::publish_rect("dialog:body:viewport", scroll.inner_rect);
                    scroll.inner
                })
                .inner
            };
            let action = super::footer(
                ui,
                self.ok,
                self.cancel,
                valid && !state.applying,
                super::Footer { apply: heading.apply_label, status: state.text.as_deref(), ..Default::default() },
            );
            if state.busy {
                let rect = egui::Rect::from_min_size(
                    egui::pos2(ui.min_rect().left(), ui.min_rect().bottom() - super::row_height()),
                    vec2(t.space_16, super::row_height()),
                );
                egui::Spinner::new().size(t.space_14).paint_at(ui, rect);
                let status = state.text.as_deref().unwrap_or_else(|| crate::i18n::tr_label("Updating the preview"));
                ui.interact(rect, ui.id().with("compute_status"), egui::Sense::hover()).on_hover_text(status);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
            (close, action)
        })
    }

    /// Palette composition retains its compact padding and whole-card scrolling on short views.
    /// Tokens: space_24 and form_footer_height; header/body/footer own their historical padding.
    pub(crate) fn palette(&self, ui: &mut Ui, bottom: f32, top: f32, mut add: impl FnMut(&mut Ui, Part)) {
        let t = Tokens::get();
        ui.set_max_height((bottom - ui.cursor().top()).max(0.0));
        super::scope(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let height = (bottom - top - self.frame.total_margin().sum().y).max(0.0);
            let short = height < t.space_24 * 6.0;
            let mut contents = |ui: &mut Ui| {
                add(ui, Part::Header);
                let body_height = (bottom - ui.cursor().top() - t.form_footer_height).max(0.0);
                let mut body = |ui: &mut Ui| {
                    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(t.space_10 as i8, t.space_4 as i8)).show(ui, |ui| add(ui, Part::Body));
                };
                if short {
                    body(ui);
                } else {
                    let body_rect = egui::Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), body_height));
                    let scroll = ui
                        .scope_builder(egui::UiBuilder::new().max_rect(body_rect), |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("sc_palette_options")
                                .max_height(body_height)
                                .min_scrolled_height(0.0)
                                .auto_shrink([false, true])
                                .show(ui, body)
                        })
                        .inner;
                    scenario::publish_rect("sketch_palette:options", scroll.inner_rect);
                }
                add(ui, Part::Footer);
            };
            if short {
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("sc_palette_content")
                    .max_height(height)
                    .min_scrolled_height(0.0)
                    .auto_shrink([false, true])
                    .show(ui, contents);
                scenario::publish_rect("sketch_palette:options", scroll.inner_rect);
            } else {
                contents(ui);
            }
        });
    }
}

/// System window built on Panel's shared floating card: 40 pt header, bounded content,
/// optional page list, and 48 pt actions with a left slot. Existing IDs and editor policies
/// survive the migration. All geometry and colours derive from Tokens.
pub(crate) struct Window<'a> {
    id: egui::Id,
    icon: &'a str,
    title: &'a str,
    anchor: Option<(egui::Align2, egui::Vec2)>,
    placed: Option<(egui::Align2, egui::Pos2)>,
    ok: &'a str,
    cancel: &'a str,
    apply: Option<&'a str>,
    enter_submits: bool,
    escape_closes: bool,
    width: f32,
    body_padding: bool,
    left_slot: bool,
}

impl<'a> Window<'a> {
    /// A card with OK and Cancel, centred and movable.
    pub(crate) fn new(id: egui::Id, icon: &'a str, title: &'a str) -> Self {
        Self {
            id,
            icon,
            title,
            anchor: None,
            placed: None,
            ok: "OK",
            cancel: "Cancel",
            apply: None,
            enter_submits: true,
            escape_closes: true,
            width: Tokens::get().form_card_width * 1.5,
            body_padding: true,
            left_slot: false,
        }
    }

    /// Pin the card at `align` of the screen, `offset` away (it then no longer moves).
    pub(crate) fn anchor(mut self, align: egui::Align2, offset: egui::Vec2) -> Self {
        self.anchor = Some((align, offset));
        self
    }

    /// Start with `pivot` of the card at `pos`; the card can then be moved.
    pub(crate) fn placed(mut self, pivot: egui::Align2, pos: egui::Pos2) -> Self {
        self.placed = Some((pivot, pos));
        self
    }

    /// The primary and closing actions, and an optional third one between them. An empty `ok`
    /// leaves the primary action out.
    pub(crate) fn actions(mut self, ok: &'a str, cancel: &'a str, apply: Option<&'a str>) -> Self {
        (self.ok, self.cancel, self.apply) = (ok, cancel, apply);
        self
    }

    /// A card of editable fields: Enter belongs to the fields and never submits (confirmations
    /// let Enter give the primary action when nothing has focus).
    pub(crate) fn fields(mut self) -> Self {
        self.enter_submits = false;
        self
    }

    /// Esc does not close the card (the header's close and the closing action still do): for a
    /// card holding a long text the user is editing, where Esc must not discard the work.
    pub(crate) fn keep_open_on_escape(mut self) -> Self {
        self.escape_closes = false;
        self
    }

    /// `cards` times the card width instead of one and a half, for wide footers.
    pub(crate) fn cards(mut self, cards: f32) -> Self {
        if cards.is_finite() && cards > 0.0 {
            self.width = Tokens::get().form_card_width * cards.min(16.0);
        }
        self
    }

    /// Draw shared header, content and optional footer, retaining the caller's IDs.
    pub(crate) fn show(self, ctx: &egui::Context, valid: bool, body: impl FnOnce(&mut Ui)) -> (bool, super::Action) {
        self.surface(ctx, false, |ui| self.content(ui, valid, body, |_| {}))
    }

    /// A pinned left action slot, independent of the right-aligned footer buttons.
    pub(crate) fn show_with_left(
        mut self,
        ctx: &egui::Context,
        valid: bool,
        body: impl FnOnce(&mut Ui),
        left: impl FnOnce(&mut Ui),
    ) -> (bool, super::Action) {
        self.left_slot = true;
        self.surface(ctx, false, |ui| self.content(ui, valid, body, left))
    }

    pub(super) fn content(&self, ui: &mut Ui, valid: bool, body: impl FnOnce(&mut Ui), left: impl FnOnce(&mut Ui)) -> (bool, super::Action) {
        super::scope(ui, |ui| {
            let t = Tokens::get();
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut close = false;
            header(ui, self.icon, self.title, || close = true);
            egui::Frame::NONE.inner_margin(if self.body_padding { t.space_12 } else { 0.0 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = t.form_gap_y;
                if self.body_padding {
                    let height =
                        (dimension(ui.ctx().content_rect().height()) - t.form_header_height - t.form_footer_height - t.space_24 * 2.0).max(0.0);
                    egui::ScrollArea::vertical()
                        .id_salt("window_body")
                        .max_height(height)
                        .min_scrolled_height(0.0)
                        .auto_shrink([false, true])
                        .show(ui, body);
                } else {
                    body(ui);
                }
            });
            if !self.enter_submits {
                ui.ctx().input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            }
            if !self.escape_closes {
                ui.ctx().input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
            let action = if self.ok.is_empty() && self.cancel.is_empty() && self.apply.is_none() && !self.left_slot {
                super::Action::None
            } else {
                super::actions::footer_with_left(ui, self.ok, self.cancel, valid, super::Footer { apply: self.apply, ..Default::default() }, left)
            };
            (close, action)
        })
    }

    /// Declarative page navigation: the kit owns geometry, selection and page scrolling.
    pub(crate) fn show_pages<S>(
        mut self,
        ctx: &egui::Context,
        selected: &mut usize,
        pages: &[WindowPage<S>],
        state: &mut S,
        left: impl FnOnce(&mut Ui),
    ) -> (bool, super::Action) {
        self.body_padding = false;
        self.show_with_left(ctx, true, |ui| page_list(ui, selected, pages, state), left)
    }

    /// The same bounded card frame, either movable or an input-blocking modal.
    pub(super) fn surface(&self, ctx: &egui::Context, modal: bool, add: impl FnOnce(&mut Ui) -> (bool, super::Action)) -> (bool, super::Action) {
        let frame = Panel::card_frame();
        let inner_width = (self.width.min(dimension(ctx.content_rect().width())) - frame.total_margin().sum().x).max(0.0);
        if modal {
            let shown = egui::Modal::new(self.id)
                .area(egui::Modal::default_area(self.id).fade_in(false))
                .backdrop_color(Tokens::get().tab_on_fill_dark.gamma_multiply(0.35))
                .frame(frame)
                .show(ctx, |ui| {
                    ui.set_width(inner_width);
                    add(ui)
                });
            scenario::publish_rect("dialog:window", shown.response.rect);
            return shown.inner;
        }
        let window = egui::Window::new(self.title)
            .enabled(!crate::issue_report::cad_input_blocked())
            .id(self.id)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .min_width(inner_width)
            .max_width(inner_width)
            .default_width(inner_width)
            .scroll(false)
            .frame(frame)
            .fade_in(false)
            .fade_out(false);
        let window = match (self.anchor, self.placed) {
            (Some((align, offset)), _) => window.anchor(align, offset),
            (None, Some((pivot, pos))) => window.pivot(pivot).default_pos(pos),
            (None, None) => window.pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.content_rect().center() - vec2(0.0, row_height() * 2.5)),
        };
        let shown = window.show(ctx, add);
        if let Some(shown) = &shown {
            scenario::publish_rect("dialog:window", shown.response.rect);
        }
        shown.and_then(|r| r.inner).unwrap_or((false, super::Action::None))
    }
}

/// A page's existing icon, translated label key, stable handle and draw callback.
pub(crate) struct WindowPage<S> {
    pub icon: &'static str,
    pub label: &'static str,
    pub handle: &'static str,
    pub scroll: bool,
    pub draw: fn(&mut Ui, &mut S),
}

pub(super) fn page_list<S>(ui: &mut Ui, selected: &mut usize, pages: &[WindowPage<S>], state: &mut S) {
    let t = Tokens::get();
    let height = (dimension(ui.ctx().content_rect().height()) - t.form_header_height - t.form_footer_height - t.space_24 * 3.0)
        .clamp(0.0, t.control_height * 16.0);
    let (body, _) = ui.allocate_exact_size(vec2(dimension(ui.available_width()), height), egui::Sense::hover());
    let width = (t.form_label_width * 2.0 + t.space_24 * 2.0).min(body.width());
    let nav = egui::Rect::from_min_size(body.min, vec2(width, body.height()));
    ui.painter().rect_filled(nav, 0.0, t.dialog_footer);
    ui.painter().vline(nav.right(), nav.y_range(), egui::Stroke::new(1.0, t.dialog_rule));
    scenario::publish_rect("dialog:pages", nav);
    *selected = (*selected).min(pages.len().saturating_sub(1));
    ui.scope_builder(egui::UiBuilder::new().max_rect(nav.shrink2(vec2(t.space_8, t.space_10))), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(nav));
        ui.spacing_mut().item_spacing.y = t.space_2;
        egui::ScrollArea::vertical().id_salt("window_pages").show(ui, |ui| {
            for (index, page) in pages.iter().enumerate() {
                let label = crate::i18n::tr(page.label);
                let (rect, response) =
                    ui.allocate_exact_size(vec2(dimension(ui.available_width()), t.control_height + t.space_4), egui::Sense::click());
                if response.clicked() {
                    *selected = index;
                }
                let on = *selected == index;
                if on || response.hovered() || response.has_focus() {
                    ui.painter().rect_filled(rect, t.control_radius, if on { t.selected_bg } else { t.hover });
                }
                let ink = if on { t.selected_ink } else { t.form_ink };
                let icon =
                    egui::Rect::from_center_size(egui::pos2(rect.left() + t.space_10 + t.space_8, rect.center().y), vec2(t.space_16, t.space_16));
                crate::icons::paint_ink(ui.painter(), icon, page.icon, if on { ink } else { t.form_dim });
                let text_rect = egui::Rect::from_min_max(egui::pos2(icon.right() + t.space_8, rect.top()), rect.right_bottom());
                paint_text(
                    ui,
                    text_rect,
                    label,
                    if on { crate::fonts::semibold(t.font_label) } else { egui::FontId::proportional(t.font_label) },
                    ink,
                    false,
                );
                response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, ui.is_enabled(), on, label));
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                scenario::publish_target(page.handle, rect);
                response.on_hover_text(label);
            }
        });
    });
    let content = egui::Rect::from_min_max(egui::pos2(nav.right(), body.top()), body.max);
    let content = content.shrink2(vec2(t.space_16.min(content.width() / 2.0), t.space_16.min(content.height() / 2.0)));
    ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(content));
        ui.spacing_mut().item_spacing.y = t.form_gap_y;
        if let Some(page) = pages.get(*selected) {
            if page.scroll {
                let scroll = egui::ScrollArea::vertical()
                    .id_salt(("sc_prefs_page", *selected))
                    .auto_shrink([false, false])
                    .show(ui, |ui| (page.draw)(ui, state));
                scenario::publish_rect("dialog:page_body", scroll.inner_rect);
            } else {
                (page.draw)(ui, state);
                scenario::publish_rect("dialog:page_body", content);
            }
        }
    });
}

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
