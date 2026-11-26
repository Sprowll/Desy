//! Effects stack (Blender-like modifiers): editor operations, the Effects
//! window (beside the layers) and its add menu.

use crate::color_field::FieldLook;
use crate::editor::Editor;
use crate::icons::{Icon, icon};
use crate::panels::chip;
use crate::theme;
use crate::view::EditorView;
use desy_core::effects::CATALOG;
use desy_core::stroke::LineJoin;
use desy_core::{Document, Effect, EffectKind, Id};
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Selectable;
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::mem::discriminant;

// ------------------------------------------------------------------ parameters

/// A numeric parameter as shown in the panel (display units).
struct Param {
    label: &'static str,
    min: f32,
    max: f32,
    step: f32,
    unit: &'static str,
}

const fn p(label: &'static str, min: f32, max: f32, step: f32, unit: &'static str) -> Param {
    Param { label, min, max, step, unit }
}

fn kind_id(k: &EffectKind) -> u8 {
    match k {
        EffectKind::PuckerBloat { .. } => 0,
        EffectKind::Twist { .. } => 1,
        EffectKind::Roughen { .. } => 2,
        EffectKind::ZigZag { .. } => 3,
        EffectKind::Transform { .. } => 4,
        EffectKind::Offset { .. } => 5,
        EffectKind::DropShadow { .. } => 6,
        EffectKind::InnerShadow { .. } => 7,
        EffectKind::OuterGlow { .. } => 8,
        EffectKind::InnerGlow { .. } => 9,
        EffectKind::Blur { .. } => 10,
        EffectKind::Noise { .. } => 11,
    }
}

const PUCKER: &[Param] = &[p("Intensité", -100.0, 100.0, 1.0, "%")];
const TWIST: &[Param] = &[p("Angle", -720.0, 720.0, 1.0, "°")];
const ROUGHEN: &[Param] = &[p("Taille", 0.0, 60.0, 0.5, "px"), p("Détail", 1.0, 60.0, 1.0, "/100 px")];
const ZIGZAG: &[Param] = &[p("Taille", 0.0, 60.0, 0.5, "px"), p("Crêtes", 1.0, 30.0, 1.0, "")];
const TRANSFORM: &[Param] = &[
    p("Échelle H", 10.0, 300.0, 1.0, "%"),
    p("Échelle V", 10.0, 300.0, 1.0, "%"),
    p("Rotation", -180.0, 180.0, 1.0, "°"),
    p("Dépl. H", -400.0, 400.0, 1.0, "px"),
    p("Dépl. V", -400.0, 400.0, 1.0, "px"),
    p("Copies", 0.0, 36.0, 1.0, ""),
];
const OFFSET: &[Param] = &[p("Distance", -60.0, 120.0, 0.5, "px")];
const DROP_SHADOW: &[Param] = &[
    p("Décal. X", -100.0, 100.0, 1.0, "px"),
    p("Décal. Y", -100.0, 100.0, 1.0, "px"),
    p("Flou", 0.0, 100.0, 1.0, "px"),
    p("Étendue", -40.0, 60.0, 1.0, "px"),
];
const INNER_SHADOW: &[Param] =
    &[p("Décal. X", -60.0, 60.0, 1.0, "px"), p("Décal. Y", -60.0, 60.0, 1.0, "px"), p("Flou", 0.0, 60.0, 1.0, "px")];
const OUTER_GLOW: &[Param] = &[p("Flou", 0.0, 100.0, 1.0, "px"), p("Étendue", 0.0, 60.0, 1.0, "px")];
const INNER_GLOW: &[Param] = &[p("Flou", 0.0, 60.0, 1.0, "px")];
const BLUR: &[Param] = &[p("Rayon", 0.0, 100.0, 0.5, "px")];
const NOISE: &[Param] = &[p("Intensité", 0.0, 100.0, 1.0, "%"), p("Grain", 0.5, 10.0, 0.1, "px")];

fn params(k: &EffectKind) -> &'static [Param] {
    match k {
        EffectKind::PuckerBloat { .. } => PUCKER,
        EffectKind::Twist { .. } => TWIST,
        EffectKind::Roughen { .. } => ROUGHEN,
        EffectKind::ZigZag { .. } => ZIGZAG,
        EffectKind::Transform { .. } => TRANSFORM,
        EffectKind::Offset { .. } => OFFSET,
        EffectKind::DropShadow { .. } => DROP_SHADOW,
        EffectKind::InnerShadow { .. } => INNER_SHADOW,
        EffectKind::OuterGlow { .. } => OUTER_GLOW,
        EffectKind::InnerGlow { .. } => INNER_GLOW,
        EffectKind::Blur { .. } => BLUR,
        EffectKind::Noise { .. } => NOISE,
    }
}

fn get(k: &EffectKind, i: usize) -> f64 {
    match (*k, i) {
        (EffectKind::PuckerBloat { amount }, _) => amount * 100.0,
        (EffectKind::Twist { angle }, _) => angle,
        (EffectKind::Roughen { size, .. }, 0) | (EffectKind::ZigZag { size, .. }, 0) => size,
        (EffectKind::Roughen { detail, .. }, _) => detail,
        (EffectKind::ZigZag { ridges, .. }, _) => ridges as f64,
        (EffectKind::Transform { scale_x, .. }, 0) => scale_x * 100.0,
        (EffectKind::Transform { scale_y, .. }, 1) => scale_y * 100.0,
        (EffectKind::Transform { rotate, .. }, 2) => rotate,
        (EffectKind::Transform { move_x, .. }, 3) => move_x,
        (EffectKind::Transform { move_y, .. }, 4) => move_y,
        (EffectKind::Transform { copies, .. }, _) => copies as f64,
        (EffectKind::Offset { distance, .. }, _) => distance,
        (EffectKind::DropShadow { x, .. }, 0) | (EffectKind::InnerShadow { x, .. }, 0) => x,
        (EffectKind::DropShadow { y, .. }, 1) | (EffectKind::InnerShadow { y, .. }, 1) => y,
        (EffectKind::DropShadow { blur, .. }, 2) | (EffectKind::InnerShadow { blur, .. }, _) => blur,
        (EffectKind::DropShadow { spread, .. }, _) => spread,
        (EffectKind::OuterGlow { blur, .. }, 0) | (EffectKind::InnerGlow { blur, .. }, _) => blur,
        (EffectKind::OuterGlow { spread, .. }, _) => spread,
        (EffectKind::Blur { radius }, _) => radius,
        (EffectKind::Noise { amount, .. }, 0) => amount * 100.0,
        (EffectKind::Noise { size, .. }, _) => size,
    }
}

fn set(k: &mut EffectKind, i: usize, v: f64) {
    match (k, i) {
        (EffectKind::PuckerBloat { amount }, _) => *amount = v / 100.0,
        (EffectKind::Twist { angle }, _) => *angle = v,
        (EffectKind::Roughen { size, .. }, 0) | (EffectKind::ZigZag { size, .. }, 0) => *size = v.max(0.0),
        (EffectKind::Roughen { detail, .. }, _) => *detail = v.max(0.5),
        (EffectKind::ZigZag { ridges, .. }, _) => *ridges = v.round().max(1.0) as u32,
        (EffectKind::Transform { scale_x, .. }, 0) => *scale_x = v / 100.0,
        (EffectKind::Transform { scale_y, .. }, 1) => *scale_y = v / 100.0,
        (EffectKind::Transform { rotate, .. }, 2) => *rotate = v,
        (EffectKind::Transform { move_x, .. }, 3) => *move_x = v,
        (EffectKind::Transform { move_y, .. }, 4) => *move_y = v,
        (EffectKind::Transform { copies, .. }, _) => *copies = v.round().max(0.0) as u32,
        (EffectKind::Offset { distance, .. }, _) => *distance = v,
        (EffectKind::DropShadow { x, .. }, 0) | (EffectKind::InnerShadow { x, .. }, 0) => *x = v,
        (EffectKind::DropShadow { y, .. }, 1) | (EffectKind::InnerShadow { y, .. }, 1) => *y = v,
        (EffectKind::DropShadow { blur, .. }, 2) | (EffectKind::InnerShadow { blur, .. }, _) => *blur = v.max(0.0),
        (EffectKind::DropShadow { spread, .. }, _) => *spread = v,
        (EffectKind::OuterGlow { blur, .. }, 0) | (EffectKind::InnerGlow { blur, .. }, _) => *blur = v.max(0.0),
        (EffectKind::OuterGlow { spread, .. }, _) => *spread = v.max(0.0),
        (EffectKind::Blur { radius }, _) => *radius = v.max(0.0),
        (EffectKind::Noise { amount, .. }, 0) => *amount = (v / 100.0).clamp(0.0, 1.0),
        (EffectKind::Noise { size, .. }, _) => *size = v.max(0.1),
    }
}

fn fmt_value(v: f64, unit: &str) -> String {
    let r = (v * 10.0).round() / 10.0;
    let n = if r.fract().abs() < 1e-9 { format!("{}", r as i64) } else { format!("{r}") };
    if unit.is_empty() {
        n
    } else if unit == "°" {
        format!("{n}°")
    } else {
        format!("{n} {unit}")
    }
}

/// Width of one category column in the add menu.
const MENU_COL_W: f32 = 176.0;
const MENU_GAP: f32 = 8.0;
/// Columns per row of the add menu ("Générer", then the catalog
/// categories, wrapping).
const MENU_COLS: usize = 3;
/// Width of the add menu's content.
pub const MENU_W: f32 = MENU_COLS as f32 * MENU_COL_W + (MENU_COLS as f32 - 1.0) * MENU_GAP;
/// Effects window, beside the layers.
const EFFECTS_W: f32 = 300.0;
const EFFECTS_MAX_H: f32 = 520.0;

fn menu_column(category: &'static str) -> Div {
    v_flex().w(px(MENU_COL_W)).gap(px(2.0)).child(
        div()
            .px_2()
            .pb_1()
            .mb_1()
            .border_b_1()
            .border_color(theme::border())
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme::text_dim())
            .child(category),
    )
}

/// One entry of the add menu: icon, name, explanation on hover. A disabled
/// entry (symmetry already there) stays visible, dimmed.
fn menu_entry(
    id: impl Into<ElementId>,
    ic: Icon,
    name: &'static str,
    tip: &'static str,
    disabled: bool,
) -> Stateful<Div> {
    h_flex()
        .id(id.into())
        .h(px(32.0))
        .px_2()
        .gap_2()
        .items_center()
        .rounded(px(8.0))
        .text_sm()
        .when(disabled, |d| d.opacity(0.4))
        .when(!disabled, |d| d.hover(|d| d.bg(theme::hover())).active(|d| d.bg(theme::pressed())))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .child(icon(ic, 18.0, theme::accent()))
        .child(div().flex_1().min_w_0().truncate().child(name))
}

/// Icon of an effect (add menu and card headers).
pub fn effect_icon(k: &EffectKind) -> Icon {
    match k {
        EffectKind::PuckerBloat { .. } => Icon::FxPucker,
        EffectKind::Twist { .. } => Icon::FxTwist,
        EffectKind::Roughen { .. } => Icon::FxRoughen,
        EffectKind::ZigZag { .. } => Icon::FxZigZag,
        EffectKind::Transform { .. } => Icon::FxTransform,
        EffectKind::Offset { .. } => Icon::FxOffset,
        EffectKind::DropShadow { .. } => Icon::FxShadow,
        EffectKind::InnerShadow { .. } => Icon::FxInnerShadow,
        EffectKind::OuterGlow { .. } => Icon::FxGlow,
        EffectKind::InnerGlow { .. } => Icon::FxInnerGlow,
        EffectKind::Blur { .. } => Icon::FxBlur,
        EffectKind::Noise { .. } => Icon::FxNoise,
    }
}

/// "Ajouter" button opening the effects menu.
#[derive(IntoElement)]
struct AddTrigger {
    open: bool,
}

impl Selectable for AddTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.open = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.open
    }
}

impl RenderOnce for AddTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let open = self.open;
        h_flex()
            .id("fx-add")
            .h(px(28.0))
            .px(px(10.0))
            .gap_1()
            .items_center()
            .rounded(px(theme::RADIUS_CHIP))
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .when(open, |d| d.bg(theme::active()).text_color(theme::on_active()))
            .when(!open, |d| d.bg(theme::panel_alt()).hover(|d| d.bg(theme::panel_high())))
            .child(icon(Icon::Plus, 14.0, if open { theme::on_active() } else { theme::text_dim() }))
            .child("Ajouter")
    }
}

/// Compact circular icon button for the effect card headers.
fn mini_button(
    id: impl Into<ElementId>,
    ic: Icon,
    tip: &'static str,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .size(px(26.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(!disabled, |d| d.hover(|d| d.bg(theme::hover())).on_click(on_click))
        .when(disabled, |d| d.opacity(0.38))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .child(icon(ic, 16.0, theme::text_dim()))
}

// ------------------------------------------------------------------ editor operations

impl Editor {
    /// Effects of the first selected object.
    pub fn selected_effects(&self) -> Vec<Effect> {
        self.selected.first().and_then(|id| self.doc.get(*id)).map(|o| o.effects.clone()).unwrap_or_default()
    }

    /// Add an effect at the end of the stack of every selected object (images excluded).
    pub fn add_effect(&mut self, kind: EffectKind) {
        let base = desy_core::document::random_seed();
        self.update_selected(|o, _| {
            if o.is_image() {
                return;
            }
            let mut k = kind;
            if let EffectKind::Roughen { seed, .. } = &mut k {
                *seed = base.wrapping_add(o.id as u32);
            }
            o.effects.push(Effect::new(k));
        });
    }

    /// Run `f` on effect `idx` of each selected object whose effect at that
    /// place is of the same kind as the first selected object's.
    fn each_effect(doc: &mut Document, ids: &[Id], idx: usize, f: &dyn Fn(&mut Effect)) {
        let Some(kind) = ids.first().and_then(|id| doc.get(*id)).and_then(|o| o.effects.get(idx)).map(|e| e.kind)
        else {
            return;
        };
        for id in ids {
            if let Some(e) = doc.get_mut(*id).and_then(|o| o.effects.get_mut(idx))
                && discriminant(&e.kind) == discriminant(&kind)
            {
                f(e);
            }
        }
    }

    /// Change an effect without an undo step (slider drag).
    pub fn preview_effect(&mut self, idx: usize, f: impl Fn(&mut Effect)) {
        let ids = self.selected.clone();
        Self::each_effect(&mut self.doc, &ids, idx, &f);
        self.touch();
    }

    /// Change an effect as one undo step.
    pub fn edit_effect(&mut self, idx: usize, f: impl Fn(&mut Effect)) {
        let ids = self.selected.clone();
        self.edit(|ed| Self::each_effect(&mut ed.doc, &ids, idx, &f));
    }

    pub fn toggle_effect(&mut self, idx: usize) {
        self.edit_effect(idx, |e| e.enabled = !e.enabled);
    }

    pub fn remove_effect(&mut self, idx: usize) {
        self.update_selected(|o, _| {
            if idx < o.effects.len() {
                o.effects.remove(idx);
            }
        });
    }

    /// Move an effect up (earlier, `-1`) or down (later, `+1`) in the stack.
    pub fn move_effect(&mut self, idx: usize, delta: isize) {
        self.update_selected(|o, _| {
            let to = idx as isize + delta;
            if idx < o.effects.len() && to >= 0 && (to as usize) < o.effects.len() {
                o.effects.swap(idx, to as usize);
            }
        });
    }

    /// Bake the stack up to `idx` into plain paths (like Blender's Apply).
    pub fn apply_effect(&mut self, idx: usize) {
        self.update_selected(|o, fonts| o.apply_effects(idx, fonts));
    }

    /// New random variation for the roughen effects at `idx`.
    pub fn reseed_effect(&mut self, idx: usize) {
        let seed = desy_core::document::random_seed();
        self.edit_effect(idx, |e| {
            if let EffectKind::Roughen { seed: s, .. } | EffectKind::Noise { seed: s, .. } = &mut e.kind {
                *s = seed;
            }
        });
    }
}

// ------------------------------------------------------------------ panel state

/// Key of a parameter slider: effect index, effect kind, parameter index.
type SliderKey = (usize, u8, usize);

#[derive(Default)]
pub struct EffectsUi {
    pub menu_open: bool,
    collapsed: HashSet<usize>,
    sliders: HashMap<SliderKey, (Entity<SliderState>, Subscription)>,
    /// Color pickers of the effects that have a color: (index, kind).
    colors: HashMap<(usize, u8), (Entity<ColorPickerState>, Subscription)>,
    /// Document before the slider drag started (one undo step per drag).
    drag: Option<Document>,
}

impl EditorView {
    /// Push the selection's effect values into the sliders.
    pub fn sync_effect_sliders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.props.effects.drag.is_some() {
            return;
        }
        let effects = self.ed.selected_effects();
        for (&(idx, kid), (state, _)) in &self.props.effects.colors {
            let Some(c) = effects.get(idx).filter(|e| kind_id(&e.kind) == kid).and_then(|e| e.kind.color()) else {
                continue;
            };
            if state.read(cx).value().map(crate::theme::from_hsla) != Some(c) {
                state.update(cx, |s, cx| s.set_value(crate::theme::to_hsla(c), window, cx));
            }
        }
        for (&(idx, kid, pi), (state, _)) in &self.props.effects.sliders {
            let Some(e) = effects.get(idx).filter(|e| kind_id(&e.kind) == kid) else { continue };
            let v = get(&e.kind, pi) as f32;
            if (state.read(cx).value().start() - v).abs() > 1e-4 {
                state.update(cx, |s, cx| s.set_value(v, window, cx));
            }
        }
    }

    fn effect_slider(
        &mut self,
        key: SliderKey,
        kind: &EffectKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SliderState> {
        if let Some((s, _)) = self.props.effects.sliders.get(&key) {
            return s.clone();
        }
        let (idx, _, pi) = key;
        let pm = &params(kind)[pi];
        let value = get(kind, pi) as f32;
        let state = cx.new(|_| {
            SliderState::new().min(pm.min).max(pm.max).step(pm.step).default_value(value.clamp(pm.min, pm.max))
        });
        let sub = cx.subscribe_in(&state, window, move |v: &mut EditorView, _s, ev: &SliderEvent, _window, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = ev;
            if v.props.effects.drag.is_none() {
                v.props.effects.drag = Some(v.ed.doc.clone());
            }
            let x = value.start() as f64;
            v.ed.preview_effect(idx, |e| set(&mut e.kind, pi, x));
            if matches!(ev, SliderEvent::Release(_))
                && let Some(origin) = v.props.effects.drag.take()
            {
                v.ed.commit(origin);
            }
            v.after_edit(cx);
        });
        self.props.effects.sliders.insert(key, (state.clone(), sub));
        state
    }

    /// Color picker of the effect at `idx` (created on first use).
    fn effect_color(
        &mut self,
        idx: usize,
        kind: &EffectKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ColorPickerState> {
        let key = (idx, kind_id(kind));
        if let Some((s, _)) = self.props.effects.colors.get(&key) {
            return s.clone();
        }
        let value = kind.color().map(crate::theme::to_hsla);
        let state = cx.new(|cx| {
            let mut s = ColorPickerState::new(window, cx);
            if let Some(v) = value {
                s.set_value(v, window, cx);
            }
            s
        });
        let sub = cx.subscribe_in(&state, window, move |v: &mut EditorView, _, ev: &ColorPickerEvent, _, cx| {
            let ColorPickerEvent::Change(Some(c)) = ev else { return };
            let color = crate::theme::from_hsla(*c);
            let current = v.ed.selected_effects().get(idx).and_then(|e| e.kind.color());
            if current != Some(color) {
                v.ed.edit_effect(idx, |e| e.kind.set_color(color));
                v.after_edit(cx);
            }
        });
        self.props.effects.colors.insert(key, (state.clone(), sub));
        state
    }

    // ------------------------------------------------------------------ rendering

    /// The Effects window, beside the layers (bottom left): header with the
    /// add menu, then the stack: the symmetry first (applied before the
    /// effects), then one card per effect. Hidden without a selection that
    /// can take effects.
    pub fn effects_window(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ed = &self.ed;
        let any_shape = ed.selected.iter().filter_map(|id| ed.doc.get(*id)).any(|o| !o.is_image());
        if !any_shape {
            return None;
        }
        let (left, _, max_h) = self.effects_frame(window);
        let effects = self.ed.selected_effects();
        // Forget sliders of effects that no longer exist at that place.
        let live: HashSet<(usize, u8)> = effects.iter().enumerate().map(|(i, e)| (i, kind_id(&e.kind))).collect();
        self.props.effects.sliders.retain(|(i, k, _), _| live.contains(&(*i, *k)));
        self.props.effects.colors.retain(|key, _| live.contains(key));
        self.props.effects.collapsed.retain(|i| *i < effects.len());

        let open = self.props.effects.menu_open;
        let view = cx.entity().downgrade();
        // The window sits at the bottom: the menu opens upwards.
        let add = Popover::new("fx-add-menu")
            .anchor(Anchor::BottomLeft)
            .open(open)
            .on_open_change(move |now, _, cx| {
                let _ = view.update(cx, |v, cx| {
                    v.props.effects.menu_open = *now;
                    cx.notify();
                });
            })
            .trigger(AddTrigger { open })
            .when(open, |p| p.child(self.effects_menu_grid(cx)));
        let header = h_flex()
            .h(px(44.0))
            .flex_none()
            .pl_4()
            .pr_2()
            .items_center()
            .justify_between()
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme::accent()).child("Effets"))
            .child(add);

        let mut stack = v_flex().pb_2();
        stack = stack.children(self.mirror_card(cx));
        let n = effects.len();
        for (idx, e) in effects.iter().enumerate() {
            stack = stack.child(self.effect_card(idx, n, e, window, cx));
        }
        Some(
            v_flex()
                .occlude()
                .absolute()
                .left(px(left))
                .bottom(px(crate::view::M))
                .w(px(EFFECTS_W))
                .max_h(px(max_h))
                .overflow_hidden()
                .rounded(px(theme::RADIUS_PANEL))
                .bg(theme::panel())
                .border_1()
                .border_color(theme::border())
                .shadow_lg()
                .child(header)
                .child(div().id("fx-scroll").flex_shrink(1.0).min_h_0().overflow_y_scroll().child(stack))
                .into_any_element(),
        )
    }

    /// Effects window (left, width, max height), right of the layers.
    pub fn effects_frame(&self, window: &Window) -> (f32, f32, f32) {
        let (ll, _, lw, _) = self.layers_frame(window);
        let window_h = f32::from(window.viewport_size().height);
        let max_h = (window_h - crate::view::PANEL_TOP - crate::view::M).min(EFFECTS_MAX_H);
        (ll + lw + 8.0, EFFECTS_W, max_h)
    }

    /// The layer (or shape) symmetry as the first card of the stack.
    fn mirror_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ed = &self.ed;
        let layer_axis = ed.doc.layers.get(ed.mirror_layer()).is_some_and(|l| l.mirror.is_some());
        let own_axis = ed.selected.iter().filter_map(|id| ed.doc.get(*id)).any(|o| o.mirror.is_some());
        if !layer_axis && !own_axis {
            return None;
        }
        Some(
            v_flex()
                .mx_3()
                .my_1()
                .pb_2()
                .rounded(px(14.0))
                .border_1()
                .border_color(theme::border())
                .bg(theme::panel_alt())
                .child(
                    h_flex()
                        .h(px(34.0))
                        .px_2()
                        .gap_1()
                        .items_center()
                        .child(icon(Icon::Mirror, 16.0, theme::accent()))
                        .child(div().flex_1().text_sm().font_weight(FontWeight::MEDIUM).child(if layer_axis {
                            "Symétrie du calque"
                        } else {
                            "Symétrie de la forme"
                        }))
                        .child(crate::panels::info("fx-mirror-info", self.mirror_tip())),
                )
                .child(self.mirror_options(cx))
                .into_any_element(),
        )
    }

    /// Add menu in the manner of Blender's "Add Modifier": one column per
    /// category, an icon and a name per effect, its explanation on hover. A
    /// click adds the effect to the selection and closes the menu (Effects
    /// window or quick bar). "Générer" holds the symmetries.
    pub(crate) fn effects_menu_grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let ed = &self.ed;
        let layer_axis = ed.doc.layers.get(ed.mirror_layer()).is_some_and(|l| l.mirror.is_some());
        let own_axis = ed.selected.iter().filter_map(|id| ed.doc.get(*id)).any(|o| o.mirror.is_some());
        let generate = menu_column("Générer")
            .child(
                menu_entry(
                    "fx-menu-mirror-layer",
                    Icon::Mirror,
                    "Symétrie du calque",
                    "Reflète tout le calque de la sélection, y compris ce que tu dessines ensuite (comme le modificateur Miroir de Blender). Axe vertical au centre ; outil Symétrie (X) pour tracer le tien.",
                    layer_axis,
                )
                .when(!layer_axis, |d| {
                    d.on_click(cx.listener(|v, _, _, cx| {
                        v.props.effects.menu_open = false;
                        v.quick.menu = None;
                        v.ed.mirror_layer_center(true);
                        v.after_edit(cx);
                    }))
                }),
            )
            .child(
                menu_entry(
                    "fx-menu-mirror-shape",
                    Icon::FlipH,
                    "Symétrie de la forme",
                    "Seulement les formes sélectionnées, autour d'un axe vertical au centre du plan de travail.",
                    layer_axis || own_axis,
                )
                .when(!layer_axis && !own_axis, |d| {
                    d.on_click(cx.listener(|v, _, _, cx| {
                        v.props.effects.menu_open = false;
                        v.quick.menu = None;
                        v.ed.mirror_artboard_center(true);
                        v.after_edit(cx);
                    }))
                }),
            );
        let mut cols = h_flex().w(px(MENU_W)).flex_wrap().items_start().gap(px(MENU_GAP)).child(generate);
        for (ci, (category, kinds)) in CATALOG.iter().enumerate() {
            let mut col = menu_column(category);
            for (ki, kind) in kinds.iter().enumerate() {
                let kind = *kind;
                col = col.child(
                    menu_entry(("fx-menu", ci * 100 + ki), effect_icon(&kind), kind.name(), kind.tip(), false)
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.props.effects.menu_open = false;
                            v.quick.menu = None;
                            v.ed.add_effect(kind);
                            v.after_edit(cx);
                        })),
                );
            }
            cols = cols.child(col);
        }
        cols.into_any_element()
    }

    fn effect_card(
        &mut self,
        idx: usize,
        n: usize,
        e: &Effect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let collapsed = self.props.effects.collapsed.contains(&idx);
        let enabled = e.enabled;
        let header = h_flex()
            .gap_1()
            .items_center()
            .child(mini_button(
                ("fx-eye", idx),
                if enabled { Icon::Eye } else { Icon::EyeOff },
                if enabled { "Masquer l'effet" } else { "Afficher l'effet" },
                false,
                cx.listener(move |v, _, _, cx| {
                    v.ed.toggle_effect(idx);
                    v.after_edit(cx);
                }),
            ))
            .child(
                h_flex()
                    .id(("fx-name", idx))
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .items_center()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .when(!enabled, |d| d.text_color(theme::text_faint()))
                    .child(icon(
                        effect_icon(&e.kind),
                        16.0,
                        if enabled { theme::accent() } else { theme::text_faint() },
                    ))
                    .child(div().flex_1().min_w_0().truncate().child(e.kind.name()))
                    .child(icon(if collapsed { Icon::ChevronDown } else { Icon::ChevronUp }, 14.0, theme::text_faint()))
                    .tooltip({
                        let tip = e.kind.tip();
                        move |window, cx| Tooltip::new(tip).build(window, cx)
                    })
                    .on_click(cx.listener(move |v, _, _, cx| {
                        let c = &mut v.props.effects.collapsed;
                        if !c.remove(&idx) {
                            c.insert(idx);
                        }
                        cx.notify();
                    })),
            )
            .when(n > 1, |h| {
                h.child(mini_button(
                    ("fx-up", idx),
                    Icon::Up,
                    "Monter (appliqué plus tôt)",
                    idx == 0,
                    cx.listener(move |v, _, _, cx| {
                        v.ed.move_effect(idx, -1);
                        v.after_edit(cx);
                    }),
                ))
                .child(mini_button(
                    ("fx-down", idx),
                    Icon::Down,
                    "Descendre (appliqué plus tard)",
                    idx + 1 >= n,
                    cx.listener(move |v, _, _, cx| {
                        v.ed.move_effect(idx, 1);
                        v.after_edit(cx);
                    }),
                ))
            })
            // Render effects stay live: nothing to bake into the path.
            .when(!e.kind.is_render(), |h| {
                h.child(mini_button(
                    ("fx-apply", idx),
                    Icon::Check,
                    "Appliquer : fige cet effet (et ceux au-dessus) en tracé",
                    !enabled,
                    cx.listener(move |v, _, _, cx| {
                        v.ed.apply_effect(idx);
                        v.after_edit(cx);
                    }),
                ))
            })
            .child(mini_button(
                ("fx-del", idx),
                Icon::Trash,
                "Supprimer l'effet",
                false,
                cx.listener(move |v, _, _, cx| {
                    v.ed.remove_effect(idx);
                    v.after_edit(cx);
                }),
            ));

        let mut body = v_flex().gap_1();
        if !collapsed {
            let kid = kind_id(&e.kind);
            for (pi, pm) in params(&e.kind).iter().enumerate() {
                let slider = self.effect_slider((idx, kid, pi), &e.kind, window, cx);
                body = body.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().w(px(62.0)).flex_none().text_xs().text_color(theme::text_dim()).child(pm.label))
                        .child(div().flex_1().px_1().child(Slider::new(&slider)))
                        .child(
                            div()
                                .w(px(58.0))
                                .flex_none()
                                .text_right()
                                .text_xs()
                                .text_color(theme::text_faint())
                                .child(fmt_value(get(&e.kind, pi), pm.unit)),
                        ),
                );
            }
            if let Some(c) = e.kind.color() {
                let state = self.effect_color(idx, &e.kind, window, cx);
                body = body.child(
                    h_flex()
                        .pt_1()
                        .gap_2()
                        .items_center()
                        .child(div().w(px(62.0)).flex_none().text_xs().text_color(theme::text_dim()).child("Couleur"))
                        .child(self.color_field(
                            &state,
                            FieldLook::Disc { none: false },
                            "Couleur et opacité de l'effet",
                            true,
                            window,
                            cx,
                        ))
                        .child(div().text_xs().text_color(theme::text_faint()).child(format!(
                            "{} · {:.0} %",
                            c.to_hex(),
                            c.a * 100.0
                        ))),
                );
            }
            body = body.child(self.effect_options(idx, &e.kind, cx));
        }

        v_flex()
            .mx_3()
            .my_1()
            .p_1()
            .pb_2()
            .gap_1()
            .rounded(px(14.0))
            .border_1()
            .border_color(theme::border())
            .bg(theme::panel_alt())
            .when(!enabled, |d| d.opacity(0.7))
            .child(header)
            .child(div().px_2().child(body))
    }

    /// Non-numeric options of an effect (chips).
    fn effect_options(&self, idx: usize, kind: &EffectKind, cx: &mut Context<Self>) -> AnyElement {
        let row = || h_flex().pt_1().gap_2().flex_wrap().items_center();
        match *kind {
            EffectKind::Roughen { smooth, .. } | EffectKind::ZigZag { smooth, .. } => {
                let mut r = row();
                for (i, (on, name)) in [(true, "Arrondi"), (false, "Anguleux")].into_iter().enumerate() {
                    r = r.child(chip(
                        ("fx-smooth", idx * 10 + i),
                        name,
                        smooth == on,
                        cx.listener(move |v, _, _, cx| {
                            v.ed.edit_effect(idx, |e| match &mut e.kind {
                                EffectKind::Roughen { smooth, .. } | EffectKind::ZigZag { smooth, .. } => *smooth = on,
                                _ => {}
                            });
                            v.after_edit(cx);
                        }),
                    ));
                }
                if matches!(kind, EffectKind::Roughen { .. }) {
                    r = r.child(chip(
                        ("fx-seed", idx),
                        "Nouvelle variante",
                        false,
                        cx.listener(move |v, _, _, cx| {
                            v.ed.reseed_effect(idx);
                            v.after_edit(cx);
                        }),
                    ));
                }
                r.into_any_element()
            }
            EffectKind::Offset { join, .. } => {
                let mut r = row();
                let joins = [(LineJoin::Round, "Rond"), (LineJoin::Miter, "Angle"), (LineJoin::Bevel, "Biseau")];
                for (i, (j, name)) in joins.into_iter().enumerate() {
                    r = r.child(chip(
                        ("fx-join", idx * 10 + i),
                        name,
                        join == j,
                        cx.listener(move |v, _, _, cx| {
                            v.ed.edit_effect(idx, |e| {
                                if let EffectKind::Offset { join, .. } = &mut e.kind {
                                    *join = j;
                                }
                            });
                            v.after_edit(cx);
                        }),
                    ));
                }
                r.into_any_element()
            }
            EffectKind::Transform { origin, .. } => {
                let mut grid = v_flex().gap(px(2.0));
                for (yi, y) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                    let mut line = h_flex().gap(px(2.0));
                    for (xi, x) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                        let on = (origin[0] - x).abs() < 1e-9 && (origin[1] - y).abs() < 1e-9;
                        line = line.child(
                            div()
                                .id(("fx-origin", idx * 10 + yi * 3 + xi))
                                .size(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.0))
                                .hover(|d| d.bg(theme::hover()))
                                .child(div().size(px(if on { 10.0 } else { 6.0 })).rounded_full().bg(if on {
                                    theme::accent()
                                } else {
                                    theme::text_faint()
                                }))
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.ed.edit_effect(idx, |e| {
                                        if let EffectKind::Transform { origin, .. } = &mut e.kind {
                                            *origin = [x, y];
                                        }
                                    });
                                    v.after_edit(cx);
                                })),
                        );
                    }
                    grid = grid.child(line);
                }
                row()
                    .child(div().w(px(62.0)).flex_none().text_xs().text_color(theme::text_dim()).child("Origine"))
                    .child(
                        div()
                            .id(("fx-origin-grid", idx))
                            .p(px(2.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(theme::border())
                            .tooltip(|window, cx| {
                                Tooltip::new("Point autour duquel la forme tourne et change d'échelle. En bas au milieu + copies = rosace.")
                                    .build(window, cx)
                            })
                            .child(grid),
                    )
                    .into_any_element()
            }
            EffectKind::Noise { .. } => row()
                .child(chip(
                    ("fx-seed", idx),
                    "Nouvelle variante",
                    false,
                    cx.listener(move |v, _, _, cx| {
                        v.ed.reseed_effect(idx);
                        v.after_edit(cx);
                    }),
                ))
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CATALOG, Document, Editor, EffectKind, Id, LineJoin, get, params, set};
    use desy_core::kurbo::{Affine, Shape as _};
    use desy_core::{Color, FontLibrary, Shape};

    fn editor_with_rect() -> (Editor, Id) {
        let mut doc = Document::new("t", 500.0, 500.0);
        let id = doc.insert(
            0,
            desy_core::document::new_object(
                Shape::Rect { w: 100.0, h: 100.0, radius: 0.0, corners: None },
                Affine::translate((100.0, 100.0)),
                Some(Color::BLACK),
                None,
            ),
        );
        let mut e = Editor::new(doc, None, FontLibrary::empty());
        e.selected = vec![id];
        (e, id)
    }

    #[test]
    fn params_roundtrip_in_display_units() {
        for (_, kinds) in CATALOG {
            for k in *kinds {
                for (i, pm) in params(k).iter().enumerate() {
                    let mut k2 = *k;
                    let v = ((pm.min + pm.max) / 2.0).round() as f64;
                    set(&mut k2, i, v);
                    assert!((get(&k2, i) - v).abs() < 1e-9, "{} {}", k.name(), pm.label);
                }
            }
        }
    }

    #[test]
    fn stack_edits_are_undoable_and_apply_bakes() {
        let (mut e, id) = editor_with_rect();
        e.add_effect(EffectKind::Offset { distance: 10.0, join: LineJoin::Miter });
        e.add_effect(EffectKind::Twist { angle: 45.0 });
        assert_eq!(e.selected_effects().len(), 2);
        e.move_effect(1, -1);
        assert!(matches!(e.selected_effects()[0].kind, EffectKind::Twist { .. }));
        e.toggle_effect(0);
        assert!(!e.selected_effects()[0].enabled);

        // A slider drag is one undo step.
        let origin = e.doc.clone();
        e.preview_effect(1, |fx| set(&mut fx.kind, 0, 20.0));
        e.preview_effect(1, |fx| set(&mut fx.kind, 0, 30.0));
        e.commit(origin);
        assert_eq!(get(&e.selected_effects()[1].kind, 0), 30.0);
        e.undo();
        assert_eq!(get(&e.selected_effects()[1].kind, 0), 10.0);

        e.apply_effect(1);
        let o = e.doc.get(id).unwrap();
        assert!(matches!(o.shape, Shape::Path(_)));
        assert_eq!(o.effects.len(), 1, "the hidden twist stays");
        assert!((o.world_path(&e.fonts).area().abs() - 120.0 * 120.0).abs() < 1.0);
        e.remove_effect(0);
        assert!(e.selected_effects().is_empty());
    }
}
