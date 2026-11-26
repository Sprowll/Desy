//! Quick edit bar floating above the selection, in the spirit of Canva's
//! contextual toolbar: fill and stroke colors, stroke weight, corner radius,
//! transparency, effects, position (order and alignment) and flips. Each
//! control opens a small popover right under it; the side panel keeps the
//! detailed settings.
//!
//! The bar has its own color pickers and sliders: a GPUI slider remembers
//! where it was painted, so the panel's ones cannot be shown twice.

use crate::color_field::FieldLook;
use crate::editor::{Align, Drag, Editor, HANDLE_RADIUS, Tool};
use crate::icons::{Icon, icon};
use crate::panels::{PickerKind, chip};
use crate::theme;
use crate::view::{EditorView, floating};
use desy_core::{Document, Object, Shape, Stroke};
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Selectable;
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// Popover menus of the bar (the color popovers keep their own state).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickMenu {
    StrokeWeight,
    Radius,
    Opacity,
    Effects,
    Position,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuickSlider {
    StrokeWeight,
    Radius,
    Opacity,
}

/// Largest stroke weight on the slider (the panel field accepts more).
const MAX_STROKE: f32 = 40.0;
const BAR_H: f32 = 48.0;
const BAR_PAD: f32 = 6.0;
const ITEM_GAP: f32 = 2.0;
const BUTTON: f32 = 36.0;
const DIVIDER: f32 = 1.0 + 2.0 * 6.0;
/// Space between the bar and the selection (or its rotation handle).
const GAP: f64 = 10.0;

pub struct QuickBar {
    pub menu: Option<QuickMenu>,
    pub fill: Entity<ColorPickerState>,
    pub stroke: Entity<ColorPickerState>,
    stroke_weight: Entity<SliderState>,
    radius: Entity<SliderState>,
    opacity: Entity<SliderState>,
    /// Document before the current slider drag: one undo step per drag.
    drag: Option<Document>,
    _subs: Vec<Subscription>,
}

impl QuickBar {
    pub fn new(window: &mut Window, cx: &mut Context<EditorView>) -> Self {
        let mut subs = Vec::new();
        let mut picker = |kind: PickerKind| {
            let state = cx.new(|cx| ColorPickerState::new(window, cx));
            subs.push(cx.subscribe_in(&state, window, move |v: &mut EditorView, _, ev: &ColorPickerEvent, _, cx| {
                if let ColorPickerEvent::Change(Some(c)) = ev {
                    v.apply_color(kind, theme::from_hsla(*c), cx);
                }
            }));
            state
        };
        let fill = picker(PickerKind::Fill);
        let stroke = picker(PickerKind::Stroke);

        let mut slider = |which: QuickSlider, max: f32, step: f32| {
            let state = cx.new(|_| SliderState::new().min(0.0).max(max).step(step));
            subs.push(cx.subscribe_in(&state, window, move |v: &mut EditorView, _, ev: &SliderEvent, _, cx| {
                let (SliderEvent::Change(value) | SliderEvent::Release(value)) = ev;
                if v.quick.drag.is_none() {
                    v.quick.drag = Some(v.ed.doc.clone());
                }
                v.ed.preview_quick(which, value.start() as f64);
                if matches!(ev, SliderEvent::Release(_))
                    && let Some(origin) = v.quick.drag.take()
                {
                    v.ed.commit(origin);
                }
                v.after_edit(cx);
            }));
            state
        };
        let stroke_weight = slider(QuickSlider::StrokeWeight, MAX_STROKE, 0.5);
        let radius = slider(QuickSlider::Radius, 100.0, 1.0);
        let opacity = slider(QuickSlider::Opacity, 100.0, 1.0);

        Self { menu: None, fill, stroke, stroke_weight, radius, opacity, drag: None, _subs: subs }
    }
}

impl Editor {
    /// Live value while a quick bar slider is dragged: no undo step, the
    /// caller commits the document it started from.
    pub(crate) fn preview_quick(&mut self, which: QuickSlider, value: f64) {
        match which {
            QuickSlider::Radius => {
                if let Some((_, _, max_r)) = self.rect_corners() {
                    self.preview_rect_radius((value / 100.0 * max_r).round());
                }
            }
            QuickSlider::StrokeWeight => {
                let line = self.line_style.stroke.unwrap_or_default();
                self.preview_selected(|o| {
                    let s = o.stroke.get_or_insert(line);
                    s.width = value.max(0.0);
                });
            }
            QuickSlider::Opacity => {
                let a = (value / 100.0).clamp(0.0, 1.0) as f32;
                self.preview_selected(|o| o.opacity = a);
            }
        }
    }

    fn preview_selected(&mut self, f: impl Fn(&mut Object)) {
        for id in self.selected.clone() {
            if let Some(o) = self.doc.get_mut(id) {
                f(o);
            }
        }
        self.touch();
    }

    /// What the quick bar offers for the current selection.
    fn quick_caps(&self) -> QuickCaps {
        let objs: Vec<&Object> = self.selected.iter().filter_map(|id| self.doc.get(*id)).collect();
        let paint = objs.iter().any(|o| !matches!(o.shape, Shape::Image { .. }));
        QuickCaps { paint, radius: self.rect_corners().is_some() }
    }
}

struct QuickCaps {
    /// Fill, stroke and effects (anything but images).
    paint: bool,
    /// A rectangle is selected.
    radius: bool,
}

impl EditorView {
    /// Mirror the selection into the bar's pickers and sliders.
    pub fn sync_quick_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(o) = self.ed.selected.first().and_then(|id| self.ed.doc.get(*id)).cloned() else { return };
        let fill = self.fill_picker_color();
        for (state, color) in [(&self.quick.fill, fill), (&self.quick.stroke, o.stroke.map(|s| s.color))] {
            if let Some(c) = color
                && state.read(cx).value().map(theme::from_hsla) != Some(c)
            {
                state.update(cx, |s, cx| s.set_value(theme::to_hsla(c), window, cx));
            }
        }
        if self.quick.drag.is_some() {
            return;
        }
        let weight = o.stroke.map(|s| s.width as f32).unwrap_or(0.0).min(MAX_STROKE);
        let radius =
            self.ed.rect_corners().map(|(c, _, max_r)| if max_r > 0.0 { c[0].min(max_r) / max_r * 100.0 } else { 0.0 });
        let opacity = o.opacity * 100.0;
        self.quick.stroke_weight.update(cx, |s, cx| s.set_value(weight, window, cx));
        if let Some(r) = radius {
            self.quick.radius.update(cx, |s, cx| s.set_value(r as f32, window, cx));
        }
        self.quick.opacity.update(cx, |s, cx| s.set_value(opacity, window, cx));
    }

    /// The bar, centered above the selection (above its rotation handle),
    /// or below it when there is no room, kept clear of the panels.
    pub fn quick_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ed = &self.ed;
        if ed.tool != Tool::Select || !matches!(ed.drag, Drag::None) || ed.space_down {
            return None;
        }
        let b = ed.selection_bounds()?;
        let first = ed.selected.first().and_then(|id| ed.doc.get(*id))?;
        let caps = ed.quick_caps();

        // (element, width): the width is needed to place the bar.
        let mut items: Vec<(AnyElement, f32)> = Vec::new();
        if caps.paint {
            let fill_none = first.fill.is_none();
            let fill_extra = self.quick.fill.read(cx).is_open().then(|| self.fill_header(window, cx));
            items.push((
                self.color_field_with(
                    &self.quick.fill,
                    match first.fill_gradient() {
                        Some(g) => crate::panels::gradient_look(g),
                        None => FieldLook::Disc { none: fill_none },
                    },
                    "Remplissage : couleur unie ou dégradé",
                    true,
                    fill_extra,
                    window,
                    cx,
                )
                .into_any_element(),
                BUTTON,
            ));
            let stroke_none = first.stroke.is_none();
            let stroke_extra = self.none_row("quick-stroke-none", stroke_none, cx);
            items.push((
                self.color_field_with(
                    &self.quick.stroke,
                    FieldLook::Ring { none: stroke_none },
                    "Couleur du contour",
                    true,
                    Some(stroke_extra),
                    window,
                    cx,
                )
                .into_any_element(),
                BUTTON,
            ));
            items.push((self.menu_button(QuickMenu::StrokeWeight, cx), BUTTON));
        }
        if caps.radius {
            items.push((self.menu_button(QuickMenu::Radius, cx), BUTTON));
        }
        items.push((self.menu_button(QuickMenu::Opacity, cx), BUTTON));
        items.push((divider(), DIVIDER));
        if caps.paint {
            items.push((self.menu_button(QuickMenu::Effects, cx), QuickMenu::Effects.width()));
        }
        items.push((self.menu_button(QuickMenu::Position, cx), QuickMenu::Position.width()));
        items.push((divider(), DIVIDER));
        for (id, ic, tip, horizontal) in [
            ("quick-flip-h", Icon::FlipH, "Miroir horizontal ↔ (⇧H)", true),
            ("quick-flip-v", Icon::FlipV, "Miroir vertical ↕ (⇧V)", false),
        ] {
            items.push((
                bar_icon_button(id, ic, tip, false)
                    .on_click(cx.listener(move |v, _, window, cx| {
                        v.ed.flip(horizontal);
                        window.focus(&v.canvas_focus, cx);
                        v.after_edit(cx)
                    }))
                    .into_any_element(),
                BUTTON,
            ));
        }

        let w =
            items.iter().map(|(_, w)| *w).sum::<f32>() + ITEM_GAP * (items.len() as f32 - 1.0) + 2.0 * BAR_PAD + 2.0;
        let (w, h) = (w as f64, BAR_H as f64);
        let o = ed.canvas_origin.to_vec2();
        let tl = ed.to_screen(b.origin()) + o;
        let br = ed.to_screen(kurbo::Point::new(b.x1, b.y1)) + o;
        let (l, t, r, bottom) = ed.view_insets;
        let min_x = o.x + l + 4.0;
        let max_x = o.x + ed.canvas_size.x - r - w - 4.0;
        let min_y = o.y + t + 4.0;
        let max_y = o.y + ed.canvas_size.y - bottom - h - 4.0;
        if max_x < min_x || max_y < min_y {
            return None;
        }
        let mut x = ((tl.x + br.x) / 2.0 - w / 2.0).clamp(min_x, max_x);
        // The rotation handle sits above the middle of the box.
        let top = ed.rotate_handle().map(|p| p.y + o.y - HANDLE_RADIUS).unwrap_or(tl.y).min(tl.y);
        let above = top - GAP - h;
        let below = br.y + GAP;
        let y = if above >= min_y {
            above
        } else if below <= max_y {
            below
        } else {
            // The selection fills the view: stick to its top.
            min_y
        };

        // Slide right of the layers card (bottom left) rather than under it.
        let (ll, lt, lw, _) = self.layers_frame(window);
        let (layers_right, layers_top) = ((ll + lw) as f64 + 8.0, lt as f64 - 4.0);
        if y + h > layers_top && x < layers_right && layers_right <= max_x {
            x = layers_right;
        }

        Some(
            floating()
                .id("quick-bar")
                .absolute()
                .left(px(x as f32))
                .top(px(y as f32))
                .h(px(BAR_H))
                .px(px(BAR_PAD))
                .gap(px(ITEM_GAP))
                .rounded_full()
                .bg(theme::panel_high())
                .children(items.into_iter().map(|(e, _)| e))
                .into_any_element(),
        )
    }

    /// "None" chip at the top of the quick stroke popover; toggles the stroke.
    fn none_row(&self, id: &'static str, none: bool, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .px_1()
            .pb_2()
            .gap_2()
            .items_center()
            .child(chip(
                id,
                "Aucun",
                none,
                cx.listener(move |v, _, _, cx| {
                    let line = v.ed.line_style.stroke.unwrap_or_default();
                    v.ed.set_stroke(|s| *s = if none { Some(line) } else { None });
                    v.after_edit(cx);
                }),
            ))
            .into_any_element()
    }

    fn menu_button(&self, menu: QuickMenu, cx: &mut Context<Self>) -> AnyElement {
        let open = self.quick.menu == Some(menu);
        let view = cx.entity().downgrade();
        Popover::new(("quick-menu", menu as usize))
            .open(open)
            .on_open_change(move |now, _, cx| {
                let _ = view.update(cx, |v, cx| {
                    if *now {
                        v.quick.menu = Some(menu);
                    } else if v.quick.menu == Some(menu) {
                        v.quick.menu = None;
                    }
                    cx.notify();
                });
            })
            .trigger(MenuTrigger { menu, open })
            .when(open, |p| p.w(px(menu.popover_w())).child(self.menu_body(menu, cx)))
            .into_any_element()
    }

    fn menu_body(&self, menu: QuickMenu, cx: &mut Context<Self>) -> AnyElement {
        let first = self.ed.selected.first().and_then(|id| self.ed.doc.get(*id));
        match menu {
            QuickMenu::StrokeWeight => {
                let stroke = first.and_then(|o| o.stroke);
                let value = stroke.map(|s| format!("{} px", fmt(s.width))).unwrap_or_else(|| "Aucun".into());
                let mut presets = h_flex().gap_1().flex_wrap();
                for (i, w) in [1.0, 2.0, 4.0, 8.0, 16.0].into_iter().enumerate() {
                    let on = stroke.is_some_and(|s| (s.width - w).abs() < 1e-6);
                    presets = presets.child(preset(("quick-stroke-w", i), format!("{w}"), on).on_click(cx.listener(
                        move |v, _, _, cx| {
                            let line = v.ed.line_style.stroke.unwrap_or_default();
                            v.ed.set_stroke(|s| match s {
                                Some(s) => s.width = w,
                                None => *s = Some(Stroke { width: w, ..line }),
                            });
                            v.after_edit(cx);
                        },
                    )));
                }
                popover_body("Épaisseur du contour", value)
                    .child(Slider::new(&self.quick.stroke_weight))
                    .child(presets)
                    .into_any_element()
            }
            QuickMenu::Radius => {
                let (corners, independent, _) = self.ed.rect_corners().unwrap_or(([0.0; 4], false, 0.0));
                let same = corners.iter().all(|r| (r - corners[0]).abs() < 1e-9);
                let value = if same { format!("{} px", fmt(corners[0])) } else { "Mixte".into() };
                popover_body("Arrondi des coins", value)
                    .child(Slider::new(&self.quick.radius))
                    .when(independent, |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(theme::text_faint())
                                .child("Coins indépendants : le curseur les remet tous à la même valeur."),
                        )
                    })
                    .into_any_element()
            }
            QuickMenu::Opacity => {
                let pct = first.map(|o| o.opacity * 100.0).unwrap_or(100.0);
                let mut presets = h_flex().gap_1();
                for (i, p) in [25u8, 50, 75, 100].into_iter().enumerate() {
                    let on = (pct.round() as u8) == p;
                    presets = presets.child(preset(("quick-opacity", i), format!("{p} %"), on).on_click(cx.listener(
                        move |v, _, _, cx| {
                            v.ed.update_selected(|o, _| o.opacity = p as f32 / 100.0);
                            v.after_edit(cx);
                        },
                    )));
                }
                popover_body("Transparence", format!("{pct:.0} %"))
                    .child(Slider::new(&self.quick.opacity))
                    .child(presets)
                    .into_any_element()
            }
            QuickMenu::Effects => self.effects_menu_grid(cx),
            QuickMenu::Position => self.position_body(cx),
        }
    }

    fn position_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let order = [
            (Icon::ToFront, "Premier plan", "⇧⌘]", i64::MAX),
            (Icon::Up, "Avancer", "⌘]", 1),
            (Icon::Down, "Reculer", "⌘[", -1),
            (Icon::ToBack, "Arrière-plan", "⇧⌘[", i64::MIN),
        ];
        let mut list = v_flex().gap(px(2.0));
        for (i, (ic, name, keys, delta)) in order.into_iter().enumerate() {
            list = list.child(
                h_flex()
                    .id(("quick-order", i))
                    .h(px(36.0))
                    .px_2()
                    .gap_3()
                    .items_center()
                    .rounded(px(10.0))
                    .hover(|d| d.bg(theme::hover()))
                    .active(|d| d.bg(theme::pressed()))
                    .child(icon(ic, 18.0, theme::text_dim()))
                    .child(div().flex_1().text_sm().child(name))
                    .child(div().text_xs().text_color(theme::text_faint()).child(keys))
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.ed.reorder(delta);
                        v.after_edit(cx);
                    })),
            );
        }
        let single = self.ed.selected.len() < 2;
        let aligns = [
            (Align::Left, Icon::AlignLeft, "Aligner à gauche"),
            (Align::HCenter, Icon::AlignHCenter, "Centrer horizontalement"),
            (Align::Right, Icon::AlignRight, "Aligner à droite"),
            (Align::Top, Icon::AlignTop, "Aligner en haut"),
            (Align::VCenter, Icon::AlignVCenter, "Centrer verticalement"),
            (Align::Bottom, Icon::AlignBottom, "Aligner en bas"),
        ];
        let mut align_row = h_flex().justify_between();
        for (i, (a, ic, tip)) in aligns.into_iter().enumerate() {
            align_row = align_row.child(
                bar_icon_button(("quick-align", i), ic, tip, false)
                    .on_click(cx.listener(move |v, _, _, cx| v.apply_align(a, cx))),
            );
        }
        v_flex()
            .gap_2()
            .child(section_title("Ordre"))
            .child(list)
            .child(div().h(px(1.0)).bg(theme::border()))
            .child(section_title(if single { "Aligner sur le plan de travail" } else { "Aligner la sélection" }))
            .child(align_row)
            .into_any_element()
    }
}

impl QuickMenu {
    fn tip(self) -> &'static str {
        match self {
            QuickMenu::StrokeWeight => "Épaisseur du contour",
            QuickMenu::Radius => "Arrondi des coins",
            QuickMenu::Opacity => "Transparence",
            QuickMenu::Effects => "Ajouter un effet",
            QuickMenu::Position => "Ordre et alignement",
        }
    }

    fn label(self) -> Option<&'static str> {
        match self {
            QuickMenu::Effects => Some("Effets"),
            QuickMenu::Position => Some("Position"),
            _ => None,
        }
    }

    fn icon(self) -> Icon {
        match self {
            QuickMenu::StrokeWeight => Icon::StrokeWeight,
            QuickMenu::Radius => Icon::Radius,
            QuickMenu::Opacity => Icon::Opacity,
            QuickMenu::Effects => Icon::Plus,
            QuickMenu::Position => Icon::Layers,
        }
    }

    /// Trigger width in the bar.
    fn width(self) -> f32 {
        match self {
            QuickMenu::Effects => 74.0,
            QuickMenu::Position => 86.0,
            _ => BUTTON,
        }
    }

    fn popover_w(self) -> f32 {
        match self {
            QuickMenu::Effects => crate::effects_panel::MENU_W + 26.0,
            QuickMenu::Position => 272.0,
            _ => 264.0,
        }
    }
}

/// Bar button opening a popover: an icon, or a label like "Effets".
#[derive(IntoElement)]
struct MenuTrigger {
    menu: QuickMenu,
    open: bool,
}

impl Selectable for MenuTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.open = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.open
    }
}

impl RenderOnce for MenuTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let tip = self.menu.tip();
        let fg = if self.open { theme::on_active() } else { theme::text() };
        div()
            .id(("quick-trigger", self.menu as usize))
            .h(px(BUTTON))
            .w(px(self.menu.width()))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded_full()
            .when(self.open, |d| d.bg(theme::active()))
            .when(!self.open, |d| {
                d.hover(|d| d.bg(theme::hover())).tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
            })
            .map(|d| match self.menu.label() {
                Some(label) => d.text_sm().font_weight(FontWeight::MEDIUM).text_color(fg).child(label),
                None => d.child(icon(self.menu.icon(), 20.0, fg)),
            })
    }
}

fn bar_icon_button(id: impl Into<ElementId>, ic: Icon, tip: &'static str, on: bool) -> Stateful<Div> {
    div()
        .id(id.into())
        .size(px(BUTTON))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(on, |d| d.bg(theme::active()))
        .hover(|d| d.bg(theme::hover()))
        .active(|d| d.bg(theme::pressed()))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .child(icon(ic, 20.0, theme::text()))
}

fn divider() -> AnyElement {
    div().w(px(1.0)).h(px(24.0)).mx(px(6.0)).flex_none().bg(theme::border()).into_any_element()
}

/// Popover content: title, current value on the right, then the controls.
fn popover_body(title: &'static str, value: String) -> Div {
    v_flex().p_1().gap_3().child(
        h_flex()
            .justify_between()
            .items_center()
            .child(section_title(title))
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(value)),
    )
}

fn section_title(title: &'static str) -> impl IntoElement {
    div().text_xs().font_weight(FontWeight::MEDIUM).text_color(theme::text_dim()).child(title)
}

/// Compact preset pill (stroke weights, opacity steps).
fn preset(id: impl Into<ElementId>, label: String, on: bool) -> Stateful<Div> {
    div()
        .id(id.into())
        .h(px(28.0))
        .min_w(px(40.0))
        .px_2()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(theme::RADIUS_CHIP))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .border_1()
        .when(on, |d| d.bg(theme::active()).border_color(theme::active()).text_color(theme::on_active()))
        .when(!on, |d| d.border_color(theme::border()).text_color(theme::text_dim()).hover(|d| d.bg(theme::hover())))
        .child(label)
}

fn fmt(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract().abs() < 1e-9 { format!("{}", r as i64) } else { format!("{r}") }
}

#[cfg(test)]
mod tests {
    use super::QuickSlider;
    use crate::editor::Editor;
    use desy_core::kurbo::Affine;
    use desy_core::{Color, Document, FontLibrary, Shape};

    /// A 100 × 50 rectangle, selected.
    fn editor_with_rect() -> Editor {
        let mut doc = Document::new("t", 400.0, 400.0);
        let id = doc.insert(
            0,
            desy_core::document::new_object(
                Shape::Rect { w: 100.0, h: 50.0, radius: 0.0, corners: None },
                Affine::translate((10.0, 10.0)),
                Some(Color::BLACK),
                None,
            ),
        );
        let mut ed = Editor::new(doc, None, FontLibrary::empty());
        ed.selected = vec![id];
        ed
    }

    #[test]
    fn caps_follow_the_selection() {
        let ed = editor_with_rect();
        let caps = ed.quick_caps();
        assert!(caps.paint && caps.radius);
    }

    #[test]
    fn slider_previews_then_one_undo_step() {
        let mut ed = editor_with_rect();
        let id = ed.selected[0];
        let origin = ed.doc.clone();
        ed.preview_quick(QuickSlider::Opacity, 40.0);
        ed.preview_quick(QuickSlider::StrokeWeight, 6.0);
        ed.preview_quick(QuickSlider::Radius, 100.0);
        let o = ed.doc.get(id).unwrap();
        assert!((o.opacity - 0.4).abs() < 1e-6);
        assert_eq!(o.stroke.map(|s| s.width), Some(6.0));
        // 100 % of the largest radius: half the short side.
        assert!(matches!(o.shape, Shape::Rect { radius, .. } if (radius - 25.0).abs() < 1e-6));
        ed.commit(origin.clone());
        ed.undo();
        assert_eq!(ed.doc, origin);
    }

    #[test]
    fn rotation_field_reads_back_and_undoes() {
        let mut ed = editor_with_rect();
        assert_eq!(ed.selection_rotation(), Some(0.0));
        ed.set_rotation(30.0);
        assert!((ed.selection_rotation().unwrap() - 30.0).abs() < 1e-6);
        ed.rotate_selection(90.0);
        assert!((ed.selection_rotation().unwrap() - 120.0).abs() < 1e-6);
        ed.undo();
        ed.undo();
        assert_eq!(ed.selection_rotation(), Some(0.0));
    }

    #[test]
    fn a_plain_color_replaces_the_gradient() {
        let mut ed = editor_with_rect();
        let id = ed.selected[0];
        ed.set_gradient(Some(desy_core::Gradient::from_color(Color::BLACK)));
        assert!(ed.selection_gradient().is_some());
        ed.edit_gradient(|g| g.angle = 30.0);
        assert_eq!(ed.selection_gradient().map(|g| g.angle), Some(30.0));
        ed.set_fill(Some(Color::WHITE));
        assert!(ed.doc.get(id).unwrap().gradient.is_none());
        ed.undo();
        assert_eq!(ed.selection_gradient().map(|g| g.angle), Some(30.0));
    }
}
