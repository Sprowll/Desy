//! Right side panel: properties of the selection and the layers list.

use crate::color_field::FieldLook;
use crate::editor::{Align, DropSpot, Tool};
use crate::icons::{Icon, icon};
use crate::scrub::{NumField, scrubbable};
use crate::theme;
use crate::view::{EditorView, M, PANEL_TOP, PANEL_W};
use desy_core::stroke::{LineCap, LineJoin};
use desy_core::text::GOOGLE_FONTS;
use desy_core::{Color, GradientKind, Shape, Stroke, TextAlign};
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::*;
use kurbo::{Affine, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    DocName,
    DocW,
    DocH,
    X,
    Y,
    W,
    H,
    /// Rotation angle (degrees).
    Rotation,
    Radius,
    RadiusTL,
    RadiusTR,
    RadiusBR,
    RadiusBL,
    Sides,
    Inner,
    StrokeW,
    Opacity,
    NodeW,
    FontSize,
    Letter,
    LineHeight,
    /// Linear gradient angle (degrees).
    GradientAngle,
}

/// What a numeric field accepts while typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberKind {
    /// Whole numbers ≥ 0 (sides, branches).
    Count,
    /// Decimals ≥ 0 (sizes, radii, percentages).
    Positive,
    /// Decimals of any sign (position, letter spacing).
    Signed,
}

impl NumberKind {
    /// Whether `text` is a number being typed: digits, one decimal
    /// separator (`.` or `,`) and, for signed fields, a leading minus.
    /// Partial input like `-` or `12,` is accepted.
    pub fn accepts(self, text: &str) -> bool {
        let body = match (self, text.strip_prefix('-')) {
            (NumberKind::Signed, Some(rest)) => rest,
            _ => text,
        };
        let mut separators = 0;
        body.chars().all(|c| {
            if c.is_ascii_digit() {
                true
            } else if matches!(c, '.' | ',') && self != NumberKind::Count {
                separators += 1;
                separators == 1
            } else {
                false
            }
        })
    }
}

impl Field {
    /// The kind of number this field holds; `None` for text fields.
    pub(crate) fn number(self) -> Option<NumberKind> {
        match self {
            Field::DocName => None,
            Field::Sides => Some(NumberKind::Count),
            Field::X | Field::Y | Field::Letter | Field::Rotation | Field::GradientAngle => Some(NumberKind::Signed),
            _ => Some(NumberKind::Positive),
        }
    }
}

const FIELDS: [Field; 22] = [
    Field::DocName,
    Field::DocW,
    Field::DocH,
    Field::X,
    Field::Y,
    Field::W,
    Field::H,
    Field::Rotation,
    Field::Radius,
    Field::RadiusTL,
    Field::RadiusTR,
    Field::RadiusBR,
    Field::RadiusBL,
    Field::Sides,
    Field::Inner,
    Field::StrokeW,
    Field::Opacity,
    Field::NodeW,
    Field::FontSize,
    Field::Letter,
    Field::LineHeight,
    Field::GradientAngle,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerKind {
    Fill,
    Stroke,
    Background,
}

pub struct Props {
    inputs: Vec<(Field, Entity<InputState>)>,
    text: Entity<TextareaState>,
    font_search: Entity<InputState>,
    layer_name: Entity<InputState>,
    pub(crate) fill: Entity<ColorPickerState>,
    stroke: Entity<ColorPickerState>,
    background: Entity<ColorPickerState>,
    /// Gradient stop the fill pickers edit: first (`false`) or last.
    pub(crate) gradient_stop: bool,
    pub font_picker_open: bool,
    pub palette_menu_open: bool,
    pub palette_picker: Entity<ColorPickerState>,
    pub palette_edit: crate::palette_panel::PaletteEdit,
    pub picker_eyedrop: Option<crate::color_field::PickerEyedrop>,
    /// Corner radius slider (0–100 % of the largest radius).
    radius_slider: Entity<SliderState>,
    /// Document before the slider drag started (one undo step per drag).
    radius_drag: Option<desy_core::Document>,
    /// Opacity slider (0–100 %) of the Appearance card.
    opacity_slider: Entity<SliderState>,
    /// Document before the opacity drag started.
    opacity_drag: Option<desy_core::Document>,
    /// Pencil smoothing slider (0–100 %).
    pencil_slider: Entity<SliderState>,
    /// Document before the smoothing drag started.
    pencil_drag: Option<desy_core::Document>,
    /// Effects stack: add menu, collapsed cards, parameter sliders.
    pub effects: crate::effects_panel::EffectsUi,
    pub scroll: ScrollHandle,
    /// The editor view, for scrubbing number fields.
    view: WeakEntity<EditorView>,
    _subs: Vec<Subscription>,
}

impl Props {
    pub fn new(window: &mut Window, cx: &mut Context<EditorView>) -> Self {
        let mut subs = Vec::new();
        let mut inputs = Vec::new();
        for f in FIELDS {
            let state = cx.new(|cx| {
                let mut s = InputState::new(window, cx);
                if let Some(kind) = f.number() {
                    s = s.validate(move |text, _| kind.accepts(text));
                }
                // Shown when the corners differ.
                if f == Field::Radius { s.placeholder("Mixte") } else { s }
            });
            subs.push(cx.subscribe_in(&state, window, move |v: &mut EditorView, s, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    let value = s.read(cx).value().to_string();
                    v.apply_field(f, &value, cx);
                    if matches!(ev, InputEvent::PressEnter { .. }) {
                        window.focus(&v.canvas_focus, cx);
                    }
                }
            }));
            inputs.push((f, state));
        }
        let text = cx.new(|cx| TextareaState::new(window, cx));
        subs.push(cx.subscribe_in(&text, window, |v: &mut EditorView, s, ev: &InputEvent, _window, cx| {
            if matches!(ev, InputEvent::Change) {
                let content = s.read(cx).value().to_string();
                v.ed.update_selected(|o, _| {
                    if let Shape::Text(spec) = &mut o.shape {
                        spec.content = content.clone();
                    }
                });
                v.after_edit(cx);
            }
        }));
        let font_search = cx.new(|cx| InputState::new(window, cx).placeholder("Rechercher ou nom Google Fonts…"));
        subs.push(cx.subscribe_in(&font_search, window, |_v: &mut EditorView, _s, ev: &InputEvent, _window, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        }));
        let layer_name = cx.new(|cx| InputState::new(window, cx));
        subs.push(cx.subscribe_in(&layer_name, window, |v: &mut EditorView, s, ev: &InputEvent, _window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur)
                && let Some(id) = v.renaming_layer.take()
            {
                let name = s.read(cx).value().trim().to_string();
                if !name.is_empty() {
                    v.ed.edit(|ed| {
                        if let Some(li) = ed.doc.layer_index(id) {
                            ed.doc.layers[li].name = name.clone();
                        }
                    });
                }
                v.after_edit(cx);
            }
        }));

        let mut picker = |kind: PickerKind| {
            let state = cx.new(|cx| ColorPickerState::new(window, cx));
            subs.push(cx.subscribe_in(
                &state,
                window,
                move |v: &mut EditorView, _s, ev: &ColorPickerEvent, _window, cx| {
                    let ColorPickerEvent::Change(Some(c)) = ev else { return };
                    let color = theme::from_hsla(*c);
                    v.apply_color(kind, color, cx);
                },
            ));
            state
        };
        let fill = picker(PickerKind::Fill);
        let stroke = picker(PickerKind::Stroke);
        let background = picker(PickerKind::Background);
        let radius_slider = cx.new(|_| SliderState::new().min(0.0).max(100.0).step(1.0));
        subs.push(cx.subscribe_in(&radius_slider, window, |v: &mut EditorView, _s, ev: &SliderEvent, _window, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = ev;
            let Some((_, _, max_r)) = v.ed.rect_corners() else { return };
            if v.props.radius_drag.is_none() {
                v.props.radius_drag = Some(v.ed.doc.clone());
            }
            v.ed.preview_rect_radius((value.start() as f64 / 100.0 * max_r).round());
            if matches!(ev, SliderEvent::Release(_))
                && let Some(origin) = v.props.radius_drag.take()
            {
                v.ed.commit(origin);
            }
            v.after_edit(cx);
        }));
        let opacity_slider = cx.new(|_| SliderState::new().min(0.0).max(100.0).step(1.0));
        subs.push(cx.subscribe_in(&opacity_slider, window, |v: &mut EditorView, _s, ev: &SliderEvent, _window, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = ev;
            if v.props.opacity_drag.is_none() {
                v.props.opacity_drag = Some(v.ed.doc.clone());
            }
            v.ed.preview_quick(crate::quick_bar::QuickSlider::Opacity, value.start() as f64);
            if matches!(ev, SliderEvent::Release(_))
                && let Some(origin) = v.props.opacity_drag.take()
            {
                v.ed.commit(origin);
            }
            v.after_edit(cx);
        }));
        let pencil_slider = cx.new(|_| SliderState::new().min(0.0).max(100.0).step(1.0));
        subs.push(cx.subscribe_in(&pencil_slider, window, |v: &mut EditorView, _s, ev: &SliderEvent, _window, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = ev;
            if v.props.pencil_drag.is_none() {
                v.props.pencil_drag = Some(v.ed.doc.clone());
            }
            v.ed.preview_pencil_smoothing(value.start() as f64 / 100.0);
            if matches!(ev, SliderEvent::Release(_))
                && let Some(origin) = v.props.pencil_drag.take()
            {
                v.ed.commit(origin);
            }
            v.after_edit(cx);
        }));
        let (palette_picker, palette_subs) = crate::palette_panel::new_palette_picker(window, cx);
        subs.extend(palette_subs);

        Self {
            inputs,
            text,
            font_search,
            layer_name,
            fill,
            stroke,
            background,
            gradient_stop: false,
            font_picker_open: false,
            palette_menu_open: false,
            palette_picker,
            palette_edit: Default::default(),
            picker_eyedrop: None,
            radius_slider,
            radius_drag: None,
            opacity_slider,
            opacity_drag: None,
            pencil_slider,
            pencil_drag: None,
            effects: Default::default(),
            scroll: ScrollHandle::new(),
            view: cx.entity().downgrade(),
            _subs: subs,
        }
    }

    pub(crate) fn input(&self, f: Field) -> &Entity<InputState> {
        &self.inputs.iter().find(|(k, _)| *k == f).unwrap().1
    }

    /// A number field, scrubbable.
    pub(crate) fn num(&self, f: Field) -> NumField {
        NumField { field: f, state: self.input(f).clone(), view: self.view.clone() }
    }

    pub fn focus_text(&self, window: &mut Window, cx: &mut App) {
        self.text.update(cx, |s, cx| {
            s.focus(window, cx);
            s.select_all(window, cx);
        });
    }
}

pub(crate) fn fmt(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.fract().abs() < 1e-9 { format!("{}", r as i64) } else { format!("{r}") }
}

fn same_color(a: Option<Hsla>, b: Color) -> bool {
    a.map(theme::from_hsla).is_some_and(|c| c.to_hex() == b.to_hex() && (c.a - b.a).abs() < 0.01)
}

impl EditorView {
    // ------------------------------------------------------------ syncing

    pub fn sync_props(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_palette_picker(window, cx);
        let ed = &self.ed;
        let first = ed.selected.first().and_then(|id| ed.doc.get(*id)).cloned();
        let bounds = ed.selection_bounds();
        let mut values: Vec<(Field, String)> = vec![
            (Field::DocName, ed.doc.name.clone()),
            (Field::DocW, fmt(ed.doc.width)),
            (Field::DocH, fmt(ed.doc.height)),
        ];
        if let Some(b) = bounds {
            values.extend([
                (Field::X, fmt(b.x0)),
                (Field::Y, fmt(b.y0)),
                (Field::W, fmt(b.width())),
                (Field::H, fmt(b.height())),
            ]);
        }
        if let Some(g) = ed.selection_gradient() {
            values.push((Field::GradientAngle, fmt(g.angle)));
        }
        if let Some(r) = ed.selection_rotation() {
            values.push((Field::Rotation, fmt(r)));
        }
        if let Some(o) = &first {
            values.push((Field::Opacity, fmt(o.opacity as f64 * 100.0)));
            if self.props.opacity_drag.is_none() {
                let pct = o.opacity * 100.0;
                self.props.opacity_slider.update(cx, |s, cx| s.set_value(pct, window, cx));
            }
            let stroke =
                o.stroke.or(if o.shape.is_closed_area() { ed.shape_style.stroke } else { ed.line_style.stroke });
            values.push((Field::StrokeW, fmt(stroke.map(|s| s.width).unwrap_or(1.0))));
            match &o.shape {
                Shape::Rect { .. } => {
                    if let Some((c, _, max_r)) = ed.rect_corners() {
                        let same = c.iter().all(|r| (r - c[0]).abs() < 1e-9);
                        values.push((Field::Radius, if same { fmt(c[0]) } else { String::new() }));
                        for (f, r) in
                            [Field::RadiusTL, Field::RadiusTR, Field::RadiusBR, Field::RadiusBL].into_iter().zip(c)
                        {
                            values.push((f, fmt(r)));
                        }
                        if self.props.radius_drag.is_none() {
                            let pct = if max_r > 0.0 { (c[0].min(max_r) / max_r * 100.0) as f32 } else { 0.0 };
                            self.props.radius_slider.update(cx, |s, cx| s.set_value(pct, window, cx));
                        }
                    }
                }
                Shape::Polygon { sides, .. } => values.push((Field::Sides, sides.to_string())),
                Shape::Star { points, inner, outer } => {
                    values.push((Field::Sides, points.to_string()));
                    values.push((Field::Inner, fmt(if *outer > 0.0 { inner / outer * 100.0 } else { 50.0 })));
                }
                Shape::Wave { waves, depth, .. } => {
                    values.push((Field::Sides, waves.to_string()));
                    values.push((Field::Inner, fmt(depth * 100.0)));
                }
                Shape::Blob { points, variance, .. } => {
                    values.push((Field::Sides, points.to_string()));
                    values.push((Field::Inner, fmt(variance * 100.0)));
                }
                Shape::Text(spec) => {
                    values.push((Field::FontSize, fmt(spec.size)));
                    values.push((Field::Letter, fmt(spec.letter_spacing)));
                    values.push((Field::LineHeight, fmt(spec.line_height)));
                    let content = spec.content.clone();
                    let text = self.props.text.clone();
                    if !text.read(cx).focus_handle(cx).is_focused(window) {
                        text.update(cx, |s, cx| s.set_value(content, window, cx));
                    }
                }
                _ => {}
            }
        } else {
            values.push((Field::StrokeW, fmt(ed.line_style.stroke.map(|s| s.width).unwrap_or(4.0))));
        }
        if self.props.pencil_drag.is_none() {
            let pct = (ed.pencil_smoothing * 100.0) as f32;
            self.props.pencil_slider.update(cx, |s, cx| s.set_value(pct, window, cx));
        }
        if let Some(w) = ed.selected_node_width() {
            values.push((Field::NodeW, fmt(w * 100.0)));
        }
        for (f, val) in values {
            let input = self.props.input(f).clone();
            if input.read(cx).focus_handle(cx).is_focused(window) {
                continue;
            }
            input.update(cx, |s, cx| s.set_value(val, window, cx));
        }

        self.sync_effect_sliders(window, cx);

        // Color pickers.
        let fill = if first.is_some() { self.fill_picker_color() } else { self.ed.shape_style.fill };
        let ed = &self.ed;
        let stroke = first.as_ref().map(|o| o.stroke).unwrap_or(ed.line_style.stroke);
        let bg = ed.doc.background;
        for (state, color) in
            [(&self.props.fill, fill), (&self.props.stroke, stroke.map(|s| s.color)), (&self.props.background, bg)]
        {
            if let Some(c) = color {
                let current = state.read(cx).value();
                if !same_color(current, c) {
                    state.update(cx, |s, cx| s.set_value(theme::to_hsla(c), window, cx));
                }
            }
        }
        self.sync_quick_bar(window, cx);
    }

    // ------------------------------------------------------------ applying

    pub fn apply_field(&mut self, f: Field, raw: &str, cx: &mut Context<Self>) {
        let num = raw.trim().replace(',', ".").trim_end_matches(['%', 'p', 'x', ' ', '°']).parse::<f64>().ok();
        let ed = &mut self.ed;
        match (f, num) {
            (Field::DocName, _) => {
                let name = raw.trim().to_string();
                if !name.is_empty() {
                    ed.edit(|ed| ed.doc.name = name);
                }
            }
            (Field::DocW, Some(v)) if v >= 1.0 => ed.edit(|ed| ed.doc.width = v),
            (Field::DocH, Some(v)) if v >= 1.0 => ed.edit(|ed| ed.doc.height = v),
            (Field::X | Field::Y, Some(v)) => {
                if let Some(b) = ed.selection_bounds() {
                    let d = if f == Field::X { Vec2::new(v - b.x0, 0.0) } else { Vec2::new(0.0, v - b.y0) };
                    ed.nudge(d);
                }
            }
            (Field::W | Field::H, Some(v)) if v > 0.0 => {
                if let Some(b) = ed.selection_bounds() {
                    let (sx, sy) = if f == Field::W {
                        (if b.width() > 0.0 { v / b.width() } else { 1.0 }, 1.0)
                    } else {
                        (1.0, if b.height() > 0.0 { v / b.height() } else { 1.0 })
                    };
                    let o = b.origin().to_vec2();
                    let t = Affine::translate(o) * Affine::scale_non_uniform(sx, sy) * Affine::translate(-o);
                    let ids = ed.selected.clone();
                    ed.edit(|ed| ed.doc.transform_objects(&ids, t));
                }
            }
            (Field::Radius, Some(v)) => ed.set_rect_radius(v),
            (Field::RadiusTL, Some(v)) => ed.set_rect_corner(0, v),
            (Field::RadiusTR, Some(v)) => ed.set_rect_corner(1, v),
            (Field::RadiusBR, Some(v)) => ed.set_rect_corner(2, v),
            (Field::RadiusBL, Some(v)) => ed.set_rect_corner(3, v),
            (Field::Sides, Some(v)) => ed.update_selected(|o, _| match &mut o.shape {
                Shape::Polygon { sides, .. } => *sides = (v as u32).clamp(3, 100),
                Shape::Star { points, .. } => *points = (v as u32).clamp(3, 100),
                Shape::Wave { waves, .. } => *waves = (v as u32).clamp(1, 100),
                Shape::Blob { points, .. } => *points = (v as u32).clamp(3, 64),
                _ => {}
            }),
            (Field::Inner, Some(v)) => ed.update_selected(|o, _| match &mut o.shape {
                Shape::Star { outer, inner, .. } => *inner = *outer * (v / 100.0).clamp(0.01, 1.0),
                Shape::Wave { depth, .. } => *depth = (v / 100.0).clamp(0.05, 1.0),
                Shape::Blob { variance, .. } => *variance = (v / 100.0).clamp(0.0, 0.9),
                _ => {}
            }),
            (Field::StrokeW, Some(v)) => {
                let w = v.max(0.0);
                ed.set_stroke(|s| match s {
                    Some(s) => s.width = w,
                    None => *s = Some(Stroke { width: w, ..Default::default() }),
                });
            }
            (Field::Rotation, Some(v)) => ed.set_rotation(v),
            (Field::GradientAngle, Some(v)) => ed.edit_gradient(|g| g.angle = v.rem_euclid(360.0)),
            (Field::Opacity, Some(v)) => ed.update_selected(|o, _| o.opacity = (v / 100.0).clamp(0.0, 1.0) as f32),
            (Field::NodeW, Some(v)) => ed.set_node_width(v / 100.0),
            (Field::FontSize, Some(v)) if v > 0.0 => ed.update_selected(|o, _| {
                if let Shape::Text(spec) = &mut o.shape {
                    spec.size = v;
                }
            }),
            (Field::Letter, Some(v)) => ed.update_selected(|o, _| {
                if let Shape::Text(spec) = &mut o.shape {
                    spec.letter_spacing = v;
                }
            }),
            (Field::LineHeight, Some(v)) if v > 0.0 => ed.update_selected(|o, _| {
                if let Shape::Text(spec) = &mut o.shape {
                    spec.line_height = v;
                }
            }),
            _ => {}
        }
        self.after_edit(cx);
    }

    pub(crate) fn apply_color(&mut self, kind: PickerKind, color: Color, cx: &mut Context<Self>) {
        match kind {
            PickerKind::Fill if self.ed.selection_gradient().is_some() => {
                // In gradient mode the color controls edit the selected stop.
                let end = self.props.gradient_stop;
                let stop = self.ed.selection_gradient().and_then(|g| g.end_stop(end).map(|i| g.stops[i].color));
                if stop.is_none_or(|c| same_color(Some(theme::to_hsla(c)), color)) {
                    return;
                }
                self.ed.edit_gradient(|g| {
                    if let Some(i) = g.end_stop(end) {
                        g.stops[i].color = color;
                    }
                });
            }
            PickerKind::Fill => {
                let current = self.ed.selected.first().and_then(|id| self.ed.doc.get(*id));
                let same = current.and_then(|o| o.fill).is_some_and(|c| same_color(Some(theme::to_hsla(c)), color));
                if same {
                    return;
                }
                self.ed.set_fill(Some(color));
            }
            PickerKind::Stroke => {
                let current = self.ed.selected.first().and_then(|id| self.ed.doc.get(*id)).and_then(|o| o.stroke);
                if current.is_some_and(|s| s.color.to_hex() == color.to_hex() && (s.color.a - color.a).abs() < 0.01) {
                    return;
                }
                let line = self.ed.line_style.stroke.unwrap_or_default();
                self.ed.set_stroke(|s| match s {
                    Some(s) => s.color = color,
                    None => *s = Some(Stroke { color, ..line }),
                });
            }
            PickerKind::Background => {
                if self.ed.doc.background.is_some_and(|c| c.to_hex() == color.to_hex()) {
                    return;
                }
                self.ed.edit(|ed| ed.doc.background = Some(color));
            }
        }
        self.after_edit(cx);
    }

    pub fn apply_font_family(&mut self, family: String, cx: &mut Context<Self>) {
        self.ed.update_selected(|o, _| {
            if let Shape::Text(spec) = &mut o.shape {
                spec.family = family.clone();
            }
        });
        self.after_edit(cx);
    }

    // ------------------------------------------------------------ rendering

    /// Side panel: the inspector as separate floating cards (scrolling
    /// together), the palette pinned at the bottom.
    pub fn right_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cards = self.inspector_cards(window, cx);
        v_flex()
            .absolute()
            .top(px(PANEL_TOP))
            .right(px(M))
            .bottom(px(M))
            .w(px(PANEL_W))
            .gap_2()
            .child(
                // Only as tall as its cards: the canvas stays reachable below.
                div()
                    .id("props-scroll")
                    .flex_shrink(1.0)
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.props.scroll)
                    .child(v_flex().gap_2().children(cards)),
            )
            .child(div().flex_1())
            .child(card().child(self.palette_panel(window, cx)))
    }

    fn inspector_cards(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut cards = Vec::new();
        if self.ed.tool == Tool::Pencil {
            cards.push(
                card()
                    .child(card_header("Crayon", None))
                    .child(
                        row()
                            .child(label("Lissage"))
                            .child(div().flex_1().px_1().child(Slider::new(&self.props.pencil_slider)))
                            .child(span_dim(format!("{:.0} %", self.ed.pencil_smoothing * 100.0))),
                    )
                    .into_any_element(),
            );
        }
        let first = self.ed.selected.first().and_then(|id| self.ed.doc.get(*id)).cloned();
        let Some(obj) = first else {
            cards.push(self.document_card(window, cx));
            return cards;
        };
        cards.push(self.position_card(&obj, cx));
        if let Some(shape) = self.shape_card(&obj, cx) {
            cards.push(shape);
        }
        cards.push(self.appearance_card(&obj, window, cx));
        cards
    }

    /// Nothing selected: the project and its background.
    fn document_card(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        card()
            .child(card_header("Projet", None))
            .child(row().child(text_field_full("Nom", &self.props.input(Field::DocName).clone())))
            .child(
                row()
                    .child(prefixed("L", &self.props.num(Field::DocW)))
                    .child(prefixed("H", &self.props.num(Field::DocH))),
            )
            .child(
                row()
                    .child(self.color_field(
                        &self.props.background,
                        FieldLook::Disc { none: self.ed.doc.background.is_none() },
                        "Couleur du fond",
                        true,
                        window,
                        cx,
                    ))
                    .child(div().flex_1().text_sm().child("Fond"))
                    .child(chip(
                        "bg-none",
                        "Transparent",
                        self.ed.doc.background.is_none(),
                        cx.listener(|v, _, _, cx| {
                            let next = if v.ed.doc.background.is_some() { None } else { Some(Color::WHITE) };
                            v.ed.edit(|ed| ed.doc.background = next);
                            v.after_edit(cx);
                        }),
                    )),
            )
            .into_any_element()
    }

    /// Alignment, position, size, rotation and flips.
    fn position_card(&self, obj: &desy_core::Object, cx: &mut Context<Self>) -> AnyElement {
        let count = self.ed.selected.len();
        let subtitle: SharedString = if count > 1 { format!("{count} objets").into() } else { obj.name.clone().into() };
        let input = |f: Field| self.props.num(f);
        let target = if count > 1 { "la sélection" } else { "le plan de travail" };
        let align = |a: Align, ic: Icon, tip: &'static str, i: usize| {
            seg_button(("align", i), ic, tip, cx.listener(move |v, _, _, cx| v.apply_align(a, cx)))
        };
        let h_aligns = segmented(vec![
            align(Align::Left, Icon::AlignLeft, "Aligner à gauche", 0),
            align(Align::HCenter, Icon::AlignHCenter, "Centrer horizontalement", 1),
            align(Align::Right, Icon::AlignRight, "Aligner à droite", 2),
        ]);
        let v_aligns = segmented(vec![
            align(Align::Top, Icon::AlignTop, "Aligner en haut", 3),
            align(Align::VCenter, Icon::AlignVCenter, "Centrer verticalement", 4),
            align(Align::Bottom, Icon::AlignBottom, "Aligner en bas", 5),
        ]);
        let transforms = segmented(vec![
            seg_button(
                "rotate-90",
                Icon::Rotate90,
                "Pivoter de 90°",
                cx.listener(|v, _, _, cx| {
                    v.ed.rotate_selection(90.0);
                    v.after_edit(cx)
                }),
            ),
            seg_button(
                "flip-h",
                Icon::FlipH,
                "Miroir horizontal ↔ (⇧H)",
                cx.listener(|v, _, _, cx| {
                    v.ed.flip(true);
                    v.after_edit(cx)
                }),
            ),
            seg_button(
                "flip-v",
                Icon::FlipV,
                "Miroir vertical ↕ (⇧V)",
                cx.listener(|v, _, _, cx| {
                    v.ed.flip(false);
                    v.after_edit(cx)
                }),
            ),
        ]);
        card()
            .child(card_header("Position", Some(subtitle)))
            .child(
                div().px_4().pb_1().text_xs().text_color(theme::text_faint()).child(format!("Alignement sur {target}")),
            )
            .child(row().child(h_aligns).child(v_aligns))
            .child(row().child(prefixed("X", &input(Field::X))).child(prefixed("Y", &input(Field::Y))))
            .child(row().child(prefixed("L", &input(Field::W))).child(prefixed("H", &input(Field::H))))
            .child(
                row()
                    .when(self.ed.can_rotate(), |r| {
                        r.child(
                            div().flex_1().child(scrubbable(
                                &input(Field::Rotation),
                                Input::new(&input(Field::Rotation).state)
                                    .prefix(icon(Icon::Angle, 16.0, theme::text_dim()))
                                    .suffix(div().text_sm().text_color(theme::text_dim()).child("°")),
                            )),
                        )
                    })
                    .when(!self.ed.can_rotate(), |r| r.child(div().flex_1()))
                    .child(transforms),
            )
            .into_any_element()
    }

    /// Settings proper to the kind of shape, when it has any.
    fn shape_card(&mut self, obj: &desy_core::Object, cx: &mut Context<Self>) -> Option<AnyElement> {
        let body = match &obj.shape {
            Shape::Polygon { .. } => v_flex()
                .child(card_header("Polygone", None))
                .child(row().child(prefixed("Côtés", &self.props.num(Field::Sides)))),
            Shape::Star { .. } => v_flex().child(card_header("Étoile", None)).child(
                row()
                    .child(prefixed("Branches", &self.props.num(Field::Sides)))
                    .child(prefixed("Creux %", &self.props.num(Field::Inner))),
            ),
            Shape::Wave { filled, .. } => {
                let filled = *filled;
                let mut kind = row();
                for (i, (on, name)) in [(false, "Ligne"), (true, "Remplie")].into_iter().enumerate() {
                    kind = kind.child(chip(
                        ("wave-kind", i),
                        name,
                        filled == on,
                        cx.listener(move |v, _, _, cx| {
                            v.ed.set_wave_filled(on);
                            v.after_edit(cx);
                        }),
                    ));
                }
                v_flex().child(card_header("Vague", None)).child(kind).child(
                    row()
                        .child(prefixed("Vagues", &self.props.num(Field::Sides)))
                        .when(filled, |r| r.child(prefixed("Hauteur %", &self.props.num(Field::Inner)))),
                )
            }
            Shape::Blob { .. } => v_flex()
                .child(card_header("Blob", None))
                .child(
                    row()
                        .child(prefixed("Points", &self.props.num(Field::Sides)))
                        .child(prefixed("Irrég. %", &self.props.num(Field::Inner))),
                )
                .child(row().child(chip(
                    "blob-shuffle",
                    "Nouvelle forme",
                    false,
                    cx.listener(|v, _, _, cx| {
                        v.ed.reshuffle_blobs();
                        v.after_edit(cx);
                    }),
                ))),
            Shape::Text(spec) => v_flex().child(self.text_section(spec.clone(), cx)),
            Shape::Image { asset, .. } => {
                let info = self.ed.doc.images.get(asset).map(|a| format!("{} — {} × {} px", a.name, a.width, a.height));
                v_flex().child(self.image_section(info, obj.opacity, cx))
            }
            Shape::Rect { .. } | Shape::Path(_) | Shape::Ellipse { .. } => return None,
        };
        Some(card().child(body).into_any_element())
    }

    /// Opacity and visibility, corner radius, fill and outline.
    fn appearance_card(&self, obj: &desy_core::Object, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let input = |f: Field| self.props.num(f);
        let hidden = self.ed.selected.iter().filter_map(|id| self.ed.doc.get(*id)).any(|o| !o.visible);
        let mut col = card().child(card_header("Apparence", None)).child(
            row().child(slider_field(Icon::Opacity, "%", &input(Field::Opacity), &self.props.opacity_slider)).child(
                square_toggle(
                    "visible",
                    if hidden { Icon::EyeOff } else { Icon::Eye },
                    if hidden { "Afficher" } else { "Masquer" },
                    false,
                    cx.listener(|v, _, _, cx| {
                        v.ed.toggle_selected_visible();
                        v.after_edit(cx);
                    }),
                ),
            ),
        );
        if let Some((_, independent, _)) = self.ed.rect_corners() {
            col = col
                .child(
                    row()
                        .child(slider_field(Icon::Corners, "px", &input(Field::Radius), &self.props.radius_slider))
                        .child(square_toggle(
                            "corners-independent",
                            Icon::Corners,
                            "Coins indépendants",
                            independent,
                            cx.listener(move |v, _, _, cx| {
                                v.ed.set_independent_corners(!independent);
                                v.after_edit(cx);
                            }),
                        )),
                )
                .when(independent, |d| {
                    d.child(
                        row()
                            .child(div().flex_1().child(icon_input(Icon::CornerTL, &input(Field::RadiusTL))))
                            .child(div().flex_1().child(icon_input(Icon::CornerTR, &input(Field::RadiusTR))))
                            .child(div().w(px(36.0)).flex_none()),
                    )
                    .child(
                        row()
                            .child(div().flex_1().child(icon_input(Icon::CornerBL, &input(Field::RadiusBL))))
                            .child(div().flex_1().child(icon_input(Icon::CornerBR, &input(Field::RadiusBR))))
                            .child(div().w(px(36.0)).flex_none()),
                    )
                });
        }
        if obj.is_image() {
            return col.into_any_element();
        }

        // Fill: plain color or gradient, both edited in the color popover.
        let fill = obj.fill;
        let gradient = obj.fill_gradient();
        let fill_detail = match gradient {
            Some(g) if g.kind == GradientKind::Radial => Some("Dégradé radial".to_string()),
            Some(_) => Some("Dégradé linéaire".to_string()),
            None => fill.map(|c| c.to_hex()),
        };
        let fill_open = self.props.fill.read(cx).is_open();
        col = col.child(
            row()
                .child(self.color_field_with(
                    &self.props.fill,
                    match gradient {
                        Some(g) => gradient_look(g),
                        None => FieldLook::Disc { none: fill.is_none() },
                    },
                    "Remplissage : couleur unie ou dégradé",
                    true,
                    fill_open.then(|| self.fill_header(window, cx)),
                    window,
                    cx,
                ))
                .child(paint_label("Remplissage", fill_detail))
                .child(square_toggle(
                    "fill-on",
                    if fill.is_some() { Icon::Minus } else { Icon::Plus },
                    if fill.is_some() { "Retirer le remplissage" } else { "Ajouter un remplissage" },
                    false,
                    cx.listener(move |v, _, _, cx| {
                        let next =
                            if fill.is_some() { None } else { v.ed.shape_style.fill.or(Color::from_hex("#d9d9d9")) };
                        v.ed.set_fill(next);
                        v.after_edit(cx);
                    }),
                )),
        );

        // Outline.
        let stroke = obj.stroke;
        col = col.child(
            row()
                .child(self.color_field(
                    &self.props.stroke,
                    FieldLook::Ring { none: stroke.is_none() },
                    "Couleur du contour",
                    true,
                    window,
                    cx,
                ))
                .child(paint_label("Contour", stroke.map(|s| s.color.to_hex())))
                .when(stroke.is_some(), |r| {
                    r.child(div().w(px(84.0)).flex_none().child(icon_input(Icon::StrokeWeight, &input(Field::StrokeW))))
                })
                .child(square_toggle(
                    "stroke-on",
                    if stroke.is_some() { Icon::Minus } else { Icon::Plus },
                    if stroke.is_some() { "Retirer le contour" } else { "Ajouter un contour" },
                    false,
                    cx.listener(move |v, _, _, cx| {
                        let line = v.ed.line_style.stroke.unwrap_or_default();
                        v.ed.set_stroke(|s| *s = if s.is_some() { None } else { Some(line) });
                        v.after_edit(cx);
                    }),
                )),
        );
        if let Some(s) = stroke {
            let caps = [
                (LineCap::Butt, Icon::CapButt, "Bout plat"),
                (LineCap::Round, Icon::CapRound, "Bout rond"),
                (LineCap::Square, Icon::CapSquare, "Bout carré"),
            ];
            let mut cap_buttons = Vec::new();
            for (i, (c, ic, tip)) in caps.into_iter().enumerate() {
                cap_buttons.push(seg_choice(
                    ("cap", i),
                    ic,
                    tip,
                    s.cap == c,
                    cx.listener(move |v, _, _, cx| {
                        v.ed.set_stroke(|st| {
                            if let Some(st) = st {
                                st.cap = c
                            }
                        });
                        v.after_edit(cx);
                    }),
                ));
            }
            let joins = [
                (LineJoin::Miter, Icon::JoinMiter, "Angle pointu"),
                (LineJoin::Round, Icon::JoinRound, "Angle arrondi"),
                (LineJoin::Bevel, Icon::JoinBevel, "Angle biseauté"),
            ];
            let mut join_buttons = Vec::new();
            for (i, (j, ic, tip)) in joins.into_iter().enumerate() {
                join_buttons.push(seg_choice(
                    ("join", i),
                    ic,
                    tip,
                    s.join == j,
                    cx.listener(move |v, _, _, cx| {
                        v.ed.set_stroke(|st| {
                            if let Some(st) = st {
                                st.join = j
                            }
                        });
                        v.after_edit(cx);
                    }),
                ));
            }
            col = col
                .child(row().child(label("Bouts")).child(segmented(cap_buttons)))
                .child(row().child(label("Angles")).child(segmented(join_buttons)));
        }
        col.into_any_element()
    }

    fn image_section(&mut self, info: Option<String>, opacity: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let mut presets = row();
        for (i, pct) in [25u8, 40, 60, 100].into_iter().enumerate() {
            let on = ((opacity * 100.0).round() as u8) == pct;
            let label: &'static str = match pct {
                25 => "25 %",
                40 => "40 %",
                60 => "60 %",
                _ => "100 %",
            };
            presets = presets.child(chip(
                ("img-op", i),
                label,
                on,
                cx.listener(move |v, _, _, cx| {
                    v.ed.update_selected(|o, _| o.opacity = pct as f32 / 100.0);
                    v.after_edit(cx);
                }),
            ));
        }
        v_flex()
            .child(section_info(
                "Image",
                "info-image",
                "Clic droit → « En faire une référence » : l'image va dans le calque « Références », en transparence et verrouillée, et n'est pas exportée. Cadenas dans les calques pour la modifier.",
            ))
            .children(info.map(|t| div().px_4().text_xs().text_color(theme::text_dim()).child(t)))
            .child(presets)
    }

    fn text_section(&mut self, spec: desy_core::TextSpec, cx: &mut Context<Self>) -> impl IntoElement {
        let weights = [
            (300, "Light"),
            (400, "Regular"),
            (500, "Medium"),
            (600, "Semi"),
            (700, "Bold"),
            (800, "Extra"),
            (900, "Black"),
        ];
        let mut weight_row = row().flex_wrap();
        for (i, (w, name)) in weights.into_iter().enumerate() {
            weight_row = weight_row.child(chip(
                ("weight", i),
                name,
                spec.weight == w,
                cx.listener(move |v, _, _, cx| {
                    v.ed.update_selected(|o, _| {
                        if let Shape::Text(s) = &mut o.shape {
                            s.weight = w;
                        }
                    });
                    v.after_edit(cx);
                }),
            ));
        }
        let aligns = [(TextAlign::Left, "Gauche"), (TextAlign::Center, "Centre"), (TextAlign::Right, "Droite")];
        let mut align_row = row().child(chip(
            "italic",
            "Italique",
            spec.italic,
            cx.listener(|v, _, _, cx| {
                v.ed.update_selected(|o, _| {
                    if let Shape::Text(s) = &mut o.shape {
                        s.italic = !s.italic;
                    }
                });
                v.after_edit(cx);
            }),
        ));
        for (i, (a, name)) in aligns.into_iter().enumerate() {
            align_row = align_row.child(chip(
                ("talign", i),
                name,
                spec.align == a,
                cx.listener(move |v, _, _, cx| {
                    v.ed.update_selected(|o, _| {
                        if let Shape::Text(s) = &mut o.shape {
                            s.align = a;
                        }
                    });
                    v.after_edit(cx);
                }),
            ));
        }
        let open = self.props.font_picker_open;
        let family = spec.family.clone();
        let missing = !self.ed.fonts.has_family(&family);
        v_flex()
            .child(section("Texte"))
            .child(div().px_4().py_1().child(Textarea::new(&self.props.text).h(px(64.0))))
            .child(
                row().child(
                    div()
                        .id("font-family")
                        .flex_1()
                        .px_2()
                        .py_1()
                        .rounded(px(10.0))
                        .bg(theme::panel_alt())
                        .border_1()
                        .border_color(if open { theme::accent() } else { theme::border() })
                        .hover(|d| d.border_color(theme::accent()))
                        .text_sm()
                        .font_family(family.clone())
                        .child(format!("{family}{}", if missing { "  (manquante)" } else { "" }))
                        .on_click(cx.listener(|v, _, _, cx| {
                            v.props.font_picker_open = !v.props.font_picker_open;
                            cx.notify();
                        })),
                ),
            )
            .when(open, |d| d.child(self.font_picker(cx)))
            .child(weight_row)
            .child(align_row)
            .child(
                row()
                    .child(field("Taille", &self.props.num(Field::FontSize)))
                    .child(field("Interl.", &self.props.num(Field::LineHeight))),
            )
            .child(row().child(field_full("Espacement lettres (px)", &self.props.num(Field::Letter))))
    }

    fn font_picker(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.props.font_search.read(cx).value().trim().to_lowercase();
        let installed = self.ed.fonts.families();
        let matches = |name: &str| query.is_empty() || name.to_lowercase().contains(&query);

        let mut google: Vec<String> =
            GOOGLE_FONTS.iter().filter(|f| matches(f) && !self.ed.fonts.has_family(f)).map(|s| s.to_string()).collect();
        // Any name typed can be fetched from Google Fonts.
        if !query.is_empty()
            && !installed.iter().any(|f| f.to_lowercase() == query)
            && !google.iter().any(|g| g.to_lowercase() == query)
        {
            let typed = self.props.font_search.read(cx).value().trim().to_string();
            google.insert(0, typed);
        }
        let local: Vec<String> = installed.iter().filter(|f| matches(f)).take(250).cloned().collect();

        let mut list = v_flex()
            .id("font-list")
            .max_h(px(260.0))
            .overflow_y_scroll()
            .mx_4()
            .p_1()
            .rounded(px(theme::RADIUS_MENU))
            .border_1()
            .border_color(theme::border())
            .bg(theme::panel_alt());
        if !google.is_empty() {
            list = list.child(div().px_2().pt_2().text_xs().text_color(theme::text_dim()).child("Google Fonts"));
            for (i, fam) in google.into_iter().enumerate() {
                let f = fam.clone();
                list = list.child(
                    div()
                        .id(("gf", i))
                        .px_2()
                        .py_1()
                        .text_sm()
                        .hover(|d| d.bg(theme::hover()))
                        .flex()
                        .justify_between()
                        .child(fam.clone())
                        .child(div().text_xs().text_color(theme::accent()).child("↓"))
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.props.font_picker_open = false;
                            v.install_google_font(f.clone(), cx);
                        })),
                );
            }
        }
        list = list.child(div().px_2().pt_2().text_xs().text_color(theme::text_dim()).child("Installées"));
        for (i, fam) in local.into_iter().enumerate() {
            let f = fam.clone();
            list = list.child(
                div()
                    .id(("lf", i))
                    .px_2()
                    .py_1()
                    .text_sm()
                    .font_family(fam.clone())
                    .hover(|d| d.bg(theme::hover()))
                    .child(fam)
                    .on_click(cx.listener(move |v, _, _, cx| {
                        v.props.font_picker_open = false;
                        v.apply_font_family(f.clone(), cx);
                    })),
            );
        }
        v_flex().gap_1().child(div().px_4().child(Input::new(&self.props.font_search))).child(list)
    }

    /// Variable width controls (Width tool frame).
    pub(crate) fn width_options(&self, cx: &mut Context<Self>) -> Div {
        let ed = &self.ed;
        let paths = ed.selected.iter().filter_map(|id| ed.doc.get(*id)).any(|o| matches!(o.shape, Shape::Path(_)));
        let mut col = v_flex();
        if !ed.nodes.is_empty() {
            col = col.child(row().child(field_full("Épaisseur au point (%)", &self.props.num(Field::NodeW))));
        }
        if paths || !ed.nodes.is_empty() {
            col = col.child(
                row()
                    .child(small_button(
                        "taper",
                        "Effiler (pinceau)",
                        cx.listener(|v, _, _, cx| {
                            v.ed.taper_selected();
                            v.after_edit(cx)
                        }),
                    ))
                    .child(small_button(
                        "uniform",
                        "Uniforme",
                        cx.listener(|v, _, _, cx| {
                            v.ed.reset_widths();
                            v.after_edit(cx)
                        }),
                    )),
            );
        } else {
            col = col
                .child(div().px_4().py_1().text_xs().text_color(theme::text_faint()).child("Aucun tracé sélectionné"));
        }
        col
    }

    /// Tooltip of the symmetry frame, depending on whether an axis exists.
    pub(crate) fn mirror_tip(&self) -> &'static str {
        let ed = &self.ed;
        if ed.doc.layers.get(ed.mirror_layer()).is_some_and(|l| l.mirror.is_some()) {
            "Dessine une moitié, l'autre suit. Les extrémités posées sur l'axe sont partagées avec leur reflet et y restent collées. « Appliquer » fige tout le calque."
        } else {
            "Tout le calque est reflété, y compris ce que tu dessines ensuite. Axe au centre, ou trace le tien avec l'outil Symétrie (X)."
        }
    }

    /// Layer mirror controls (layer of the selection, else the active layer),
    /// plus the selected shape's own mirror if it has one (Symmetry frame).
    pub(crate) fn mirror_options(&self, cx: &mut Context<Self>) -> Div {
        let ed = &self.ed;
        let obj = ed.selected.first().and_then(|id| ed.doc.get(*id));
        let Some(layer) = ed.doc.layers.get(ed.mirror_layer()) else { return v_flex() };
        let mut col = v_flex();
        col = col.child(
            div()
                .px_4()
                .pb_1()
                .text_xs()
                .text_color(theme::text_dim())
                .truncate()
                .child(format!("Calque « {} »", layer.name)),
        );
        match layer.mirror {
            None => {
                col = col.child(
                    row()
                        .child(small_button(
                            "mirror-v",
                            "Verticale ↔",
                            cx.listener(|v, _, _, cx| {
                                v.ed.mirror_layer_center(true);
                                v.after_edit(cx)
                            }),
                        ))
                        .child(small_button(
                            "mirror-h",
                            "Horizontale ↕",
                            cx.listener(|v, _, _, cx| {
                                v.ed.mirror_layer_center(false);
                                v.after_edit(cx)
                            }),
                        )),
                );
            }
            Some(m) => {
                col = col
                    .child(row().child(chip(
                        "mirror-merge",
                        "Lier les points sur l'axe",
                        m.merge,
                        cx.listener(|v, _, _, cx| {
                            v.ed.toggle_layer_mirror_merge();
                            v.after_edit(cx)
                        }),
                    )))
                    .child(
                        row()
                            .flex_wrap()
                            .child(small_button(
                                "mirror-apply",
                                "Appliquer",
                                cx.listener(|v, _, _, cx| {
                                    v.ed.apply_layer_mirror();
                                    v.after_edit(cx)
                                }),
                            ))
                            .child(small_button(
                                "mirror-off",
                                "Retirer",
                                cx.listener(|v, _, _, cx| {
                                    v.ed.set_layer_mirror(None);
                                    v.after_edit(cx)
                                }),
                            ))
                            .child(small_button(
                                "mirror-center",
                                "Centrer l'axe",
                                cx.listener(move |v, _, _, cx| {
                                    let vertical = (m.b - m.a).x.abs() < (m.b - m.a).y.abs();
                                    v.ed.mirror_layer_center(vertical);
                                    v.after_edit(cx)
                                }),
                            )),
                    );
            }
        }
        let Some(o) = obj.filter(|o| !o.is_image()) else { return col };
        if o.mirror.is_some() {
            col = col
                .child(section_info(
                    "Symétrie de la forme",
                    "info-obj-mirror",
                    "Cette forme a son propre axe, prioritaire sur celui du calque.",
                ))
                .child(
                    row()
                        .child(small_button(
                            "obj-mirror-apply",
                            "Appliquer",
                            cx.listener(|v, _, _, cx| {
                                v.ed.apply_mirror();
                                v.after_edit(cx)
                            }),
                        ))
                        .child(small_button(
                            "obj-mirror-off",
                            "Retirer",
                            cx.listener(|v, _, _, cx| {
                                v.ed.set_mirror(None);
                                v.after_edit(cx)
                            }),
                        )),
                );
        } else if layer.mirror.is_none() {
            col = col.child(row().child(small_button(
                "obj-mirror",
                "Cette forme seulement ↔",
                cx.listener(|v, _, _, cx| {
                    v.ed.mirror_artboard_center(true);
                    v.after_edit(cx)
                }),
            )));
        }
        col
    }

    pub(crate) fn layers_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let ed = &self.ed;
        let hint = if cx.has_active_drag() { self.drop_hint } else { None };
        let mut list = v_flex().id("layers-list").flex_1().min_h_0().overflow_y_scroll().px_2().pb_2().gap(px(2.0));
        for (li, layer) in ed.doc.layers.iter().enumerate().rev() {
            let lid = layer.id;
            let active = li == ed.active_layer;
            let expanded = self.layers_expanded.contains(&lid);
            let renaming = self.renaming_layer == Some(lid);
            let visible = layer.visible;
            let locked = layer.locked;
            let header = h_flex()
                .id(("layer", lid as usize))
                .h(px(36.0))
                .when(hint == Some(DropSpot::IntoLayer(lid)), |d| {
                    d.bg(theme::accent_soft()).border_1().border_color(theme::accent())
                })
                .when(hint == Some(DropSpot::AboveLayer(lid)), |d| d.border_t_2().border_color(theme::accent()))
                .when(hint == Some(DropSpot::BelowLayer(lid)), |d| d.border_b_2().border_color(theme::accent()))
                .on_drag(DragLayer { id: lid, label: layer.name.clone().into() }, |d, _, _, cx| {
                    cx.new(|_| DragGhost(d.label.clone()))
                })
                .on_drag_move(cx.listener(move |v, ev: &DragMoveEvent<DragLayer>, _, cx| {
                    let Some(up) = upper_half(ev.bounds, ev.event.position) else { return };
                    let spot = if up { DropSpot::AboveLayer(lid) } else { DropSpot::BelowLayer(lid) };
                    if v.drop_hint != Some(spot) {
                        v.drop_hint = Some(spot);
                        cx.notify();
                    }
                }))
                .on_drag_move(cx.listener(move |v, ev: &DragMoveEvent<DragObjects>, _, cx| {
                    if upper_half(ev.bounds, ev.event.position).is_some()
                        && v.drop_hint != Some(DropSpot::IntoLayer(lid))
                    {
                        v.drop_hint = Some(DropSpot::IntoLayer(lid));
                        cx.notify();
                    }
                }))
                .on_drop(cx.listener(move |v, d: &DragLayer, _, cx| {
                    let spot = match v.drop_hint.take() {
                        Some(s @ (DropSpot::AboveLayer(t) | DropSpot::BelowLayer(t))) if t == lid => s,
                        _ => DropSpot::AboveLayer(lid),
                    };
                    v.ed.move_layer_to(d.id, spot);
                    v.after_edit(cx);
                }))
                .on_drop(cx.listener(move |v, d: &DragObjects, _, cx| {
                    v.drop_hint = None;
                    v.ed.move_objects(&d.ids, DropSpot::IntoLayer(lid));
                    v.after_edit(cx);
                }))
                .pl_2()
                .pr_1()
                .gap_1()
                .flex_none()
                .items_center()
                .rounded(px(theme::RADIUS_ITEM))
                .when(active, |d| d.bg(theme::panel_alt()))
                .hover(|d| d.bg(theme::hover()))
                .child(
                    div()
                        .id(("layer-exp", lid as usize))
                        .w(px(14.0))
                        .text_xs()
                        .text_color(theme::text_dim())
                        .child(if expanded { "▾" } else { "▸" })
                        .on_click(cx.listener(move |v, _, _, cx| {
                            if let Some(i) = v.layers_expanded.iter().position(|x| *x == lid) {
                                v.layers_expanded.remove(i);
                            } else {
                                v.layers_expanded.push(lid);
                            }
                            cx.notify();
                        })),
                )
                .child(if renaming {
                    div().flex_1().child(Input::new(&self.props.layer_name)).into_any_element()
                } else {
                    h_flex()
                        .flex_1()
                        .gap_2()
                        .items_center()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).truncate().child(layer.name.clone()))
                        .child(span_dim(format!("{}", layer.objects.len())))
                        .into_any_element()
                })
                .when(layer.mirror.is_some(), |d| {
                    d.child(icon_button(
                        ("layer-mirror", lid as usize),
                        Icon::Mirror,
                        "Symétrie active — clic pour la retirer",
                        false,
                        cx.listener(move |v, _, _, cx| {
                            v.ed.edit(|ed| {
                                if let Some(i) = ed.doc.layer_index(lid) {
                                    ed.doc.layers[i].mirror = None;
                                }
                            });
                            v.after_edit(cx);
                        }),
                    ))
                })
                .child(icon_button(
                    ("layer-eye", lid as usize),
                    if visible { Icon::Eye } else { Icon::EyeOff },
                    "Afficher / masquer",
                    false,
                    cx.listener(move |v, _, _, cx| {
                        v.ed.edit(|ed| {
                            if let Some(i) = ed.doc.layer_index(lid) {
                                ed.doc.layers[i].visible = !ed.doc.layers[i].visible;
                            }
                        });
                        v.after_edit(cx);
                    }),
                ))
                .child(icon_button(
                    ("layer-lock", lid as usize),
                    if locked { Icon::Lock } else { Icon::Unlock },
                    "Verrouiller",
                    false,
                    cx.listener(move |v, _, _, cx| {
                        v.ed.edit(|ed| {
                            if let Some(i) = ed.doc.layer_index(lid) {
                                ed.doc.layers[i].locked = !ed.doc.layers[i].locked;
                            }
                        });
                        v.ed.selected.retain(|_| true);
                        v.after_edit(cx);
                    }),
                ))
                .on_click(cx.listener(move |v, ev: &ClickEvent, window, cx| {
                    if let Some(i) = v.ed.doc.layer_index(lid) {
                        v.ed.active_layer = i;
                        if ev.click_count() >= 2 {
                            v.renaming_layer = Some(lid);
                            let name = v.ed.doc.layers[i].name.clone();
                            v.props.layer_name.update(cx, |s, cx| {
                                s.set_value(name, window, cx);
                                s.focus(window, cx);
                            });
                        }
                    }
                    cx.notify();
                }));
            list = list.child(header);
            if expanded {
                for o in layer.objects.iter().rev() {
                    let id = o.id;
                    let selected = ed.selected.contains(&id);
                    let ovis = o.visible;
                    let olock = o.locked;
                    // Dragging a selected row moves the whole selection.
                    let dragged = if selected { ed.selected.clone() } else { vec![id] };
                    let label: SharedString = if dragged.len() > 1 {
                        format!("{} objets", dragged.len()).into()
                    } else {
                        o.name.clone().into()
                    };
                    list = list.child(
                        h_flex()
                            .id(("obj", id as usize))
                            .h(px(30.0))
                            .when(hint == Some(DropSpot::AboveObject(id)), |d| {
                                d.border_t_2().border_color(theme::accent())
                            })
                            .when(hint == Some(DropSpot::BelowObject(id)), |d| {
                                d.border_b_2().border_color(theme::accent())
                            })
                            .on_drag(DragObjects { ids: dragged, label }, |d, _, _, cx| {
                                cx.new(|_| DragGhost(d.label.clone()))
                            })
                            .on_drag_move(cx.listener(move |v, ev: &DragMoveEvent<DragObjects>, _, cx| {
                                let Some(up) = upper_half(ev.bounds, ev.event.position) else { return };
                                let spot = if up { DropSpot::AboveObject(id) } else { DropSpot::BelowObject(id) };
                                if v.drop_hint != Some(spot) {
                                    v.drop_hint = Some(spot);
                                    cx.notify();
                                }
                            }))
                            .on_drop(cx.listener(move |v, d: &DragObjects, _, cx| {
                                let spot = match v.drop_hint.take() {
                                    Some(s @ (DropSpot::AboveObject(t) | DropSpot::BelowObject(t))) if t == id => s,
                                    _ => DropSpot::AboveObject(id),
                                };
                                v.ed.move_objects(&d.ids, spot);
                                v.after_edit(cx);
                            }))
                            .pl(px(30.0))
                            .pr_1()
                            .gap_1()
                            .flex_none()
                            .items_center()
                            .rounded(px(theme::RADIUS_ITEM))
                            .when(selected, |d| d.bg(theme::active()))
                            .when(!selected, |d| d.hover(|d| d.bg(theme::hover())))
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .truncate()
                                    .text_color(if selected {
                                        theme::on_active()
                                    } else if ovis {
                                        theme::text()
                                    } else {
                                        theme::text_faint()
                                    })
                                    .child(o.name.clone()),
                            )
                            .when(o.reference, |d| {
                                d.child(
                                    div()
                                        .id(("obj-ref", id as usize))
                                        .px_1()
                                        .rounded_sm()
                                        .text_xs()
                                        .text_color(theme::accent())
                                        .bg(theme::accent_soft())
                                        .child("Réf.")
                                        .tooltip(|window, cx| {
                                            Tooltip::new(
                                                "Référence (non exportée) — clic pour en faire un objet normal",
                                            )
                                            .build(window, cx)
                                        })
                                        .on_click(cx.listener(move |v, _, _, cx| {
                                            cx.stop_propagation();
                                            v.ed.clear_reference(id);
                                            v.after_edit(cx);
                                        })),
                                )
                            })
                            .child(icon_button(
                                ("obj-eye", id as usize),
                                if ovis { Icon::Eye } else { Icon::EyeOff },
                                "Afficher / masquer",
                                false,
                                cx.listener(move |v, _, _, cx| {
                                    v.ed.edit(|ed| {
                                        if let Some(o) = ed.doc.get_mut(id) {
                                            o.visible = !o.visible;
                                        }
                                    });
                                    v.after_edit(cx);
                                }),
                            ))
                            .child(icon_button(
                                ("obj-lock", id as usize),
                                if olock { Icon::Lock } else { Icon::Unlock },
                                "Verrouiller",
                                false,
                                cx.listener(move |v, _, _, cx| {
                                    v.ed.edit(|ed| {
                                        if let Some(o) = ed.doc.get_mut(id) {
                                            o.locked = !o.locked;
                                        }
                                    });
                                    v.ed.selected.retain(|s| *s != id);
                                    v.after_edit(cx);
                                }),
                            ))
                            .on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
                                if ev.modifiers().shift || ev.modifiers().platform {
                                    v.ed.toggle_selected(id);
                                } else {
                                    v.ed.select_only(id);
                                }
                                v.after_edit(cx);
                            })),
                    );
                }
            }
        }

        let active = ed.active_layer;
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .h(px(48.0))
                    .pl_4()
                    .pr_2()
                    .items_center()
                    .justify_between()
                    .child(
                        div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme::accent()).child("Calques"),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(icon_button(
                                "layer-up",
                                Icon::Up,
                                "Monter la sélection (sinon le calque)",
                                false,
                                cx.listener(move |v, _, _, cx| {
                                    if v.ed.selected.is_empty() {
                                        v.ed.move_layer(active, true);
                                    } else {
                                        v.ed.reorder(1);
                                    }
                                    v.after_edit(cx)
                                }),
                            ))
                            .child(icon_button(
                                "layer-down",
                                Icon::Down,
                                "Descendre la sélection (sinon le calque)",
                                false,
                                cx.listener(move |v, _, _, cx| {
                                    if v.ed.selected.is_empty() {
                                        v.ed.move_layer(active, false);
                                    } else {
                                        v.ed.reorder(-1);
                                    }
                                    v.after_edit(cx)
                                }),
                            ))
                            .child(icon_button(
                                "layer-add",
                                Icon::Plus,
                                "Nouveau calque",
                                false,
                                cx.listener(|v, _, _, cx| {
                                    v.ed.add_layer();
                                    if let Some(l) = v.ed.doc.layers.last() {
                                        v.layers_expanded.push(l.id);
                                    }
                                    v.after_edit(cx)
                                }),
                            ))
                            .child(icon_button(
                                "layer-del",
                                Icon::Trash,
                                "Supprimer le calque",
                                false,
                                cx.listener(move |v, _, _, cx| {
                                    v.ed.delete_layer(active);
                                    v.after_edit(cx)
                                }),
                            )),
                    ),
            )
            .child(list)
    }
}

// ---------------------------------------------------------------- small widgets

/// Objects dragged in the layers panel.
struct DragObjects {
    ids: Vec<u64>,
    label: SharedString,
}

/// Layer dragged in the layers panel.
struct DragLayer {
    id: u64,
    label: SharedString,
}

/// Label following the cursor while dragging in the layers panel.
struct DragGhost(SharedString);

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .text_xs()
            .bg(theme::panel_alt())
            .border_1()
            .border_color(theme::accent())
            .text_color(theme::text())
            .child(self.0.clone())
    }
}

/// Upper half of the row → above (higher in z order).
fn upper_half(bounds: Bounds<Pixels>, p: Point<Pixels>) -> Option<bool> {
    bounds.contains(&p).then(|| p.y < bounds.center().y)
}

/// A floating inspector card.
fn card() -> Div {
    v_flex()
        .occlude()
        .w_full()
        .flex_none()
        .overflow_hidden()
        .pb_2()
        .rounded(px(20.0))
        .bg(theme::panel())
        .border_1()
        .border_color(theme::border())
        .shadow_lg()
}

/// Card title, with an optional dim detail on the right (object name).
fn card_header(title: &'static str, detail: Option<SharedString>) -> impl IntoElement {
    h_flex()
        .h(px(44.0))
        .px_4()
        .gap_3()
        .items_center()
        .child(div().flex_none().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme::accent()).child(title))
        .children(detail.map(|d| {
            div().flex_1().min_w_0().text_right().truncate().text_xs().text_color(theme::text_faint()).child(d)
        }))
}

/// Number input with a short text label inside, on the left ("X 120").
fn prefixed(name: &'static str, n: &NumField) -> impl IntoElement {
    scrubbable(n, Input::new(&n.state).prefix(div().pr_1().text_sm().text_color(theme::text_dim()).child(name)))
}

/// Joined icon buttons (alignments, rotate and flips).
fn segmented(buttons: Vec<AnyElement>) -> impl IntoElement {
    let mut d = h_flex().flex_1().h(px(34.0)).rounded(px(10.0)).overflow_hidden().bg(theme::panel_alt());
    for (i, b) in buttons.into_iter().enumerate() {
        if i > 0 {
            d = d.child(div().w(px(1.0)).h_full().flex_none().bg(theme::panel()));
        }
        d = d.child(b);
    }
    d
}

fn seg_button(
    id: impl Into<ElementId>,
    ic: Icon,
    tip: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id.into())
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .hover(|d| d.bg(theme::hover()))
        .active(|d| d.bg(theme::pressed()))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .on_click(on_click)
        .child(icon(ic, 18.0, theme::text_dim()))
        .into_any_element()
}

/// Segment of a joined group that shows a choice; filled when selected.
fn seg_choice(
    id: impl Into<ElementId>,
    ic: Icon,
    tip: &'static str,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id.into())
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .when(on, |d| d.bg(theme::active()))
        .when(!on, |d| d.hover(|d| d.bg(theme::hover())).active(|d| d.bg(theme::pressed())))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .on_click(on_click)
        .child(icon(ic, 20.0, if on { theme::on_active() } else { theme::text_dim() }))
        .into_any_element()
}

/// A value and its slider in one field: icon + number, then the slider.
fn slider_field(ic: Icon, unit: &'static str, input: &NumField, slider: &Entity<SliderState>) -> impl IntoElement {
    h_flex()
        .flex_1()
        .min_w_0()
        .h(px(34.0))
        .rounded(px(10.0))
        .bg(theme::panel_alt())
        .child(
            div().w(px(96.0)).flex_none().child(scrubbable(
                input,
                Input::new(&input.state)
                    .appearance(false)
                    .prefix(icon(ic, 16.0, theme::text_dim()))
                    .suffix(div().text_xs().text_color(theme::text_faint()).child(unit)),
            )),
        )
        .child(div().w(px(1.0)).h_full().flex_none().bg(theme::panel()))
        .child(div().flex_1().px_3().child(Slider::new(slider)))
}

/// Square icon button beside a field; filled when `on`.
fn square_toggle(
    id: &'static str,
    ic: Icon,
    tip: &'static str,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(34.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(10.0))
        .when(on, |d| d.bg(theme::active()))
        .when(!on, |d| d.hover(|d| d.bg(theme::hover())))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .on_click(on_click)
        .child(icon(ic, 18.0, if on { theme::on_active() } else { theme::text_dim() }))
}

/// Closed look of a fill field painted with a gradient.
pub(crate) fn gradient_look(g: &desy_core::Gradient) -> FieldLook {
    let stops = g.sorted_stops();
    let c = |s: Option<&desy_core::GradientStop>| s.map(|s| theme::to_hsla(s.color)).unwrap_or_default();
    let angle = if g.kind == GradientKind::Linear { g.angle as f32 + 90.0 } else { 90.0 };
    FieldLook::GradientDisc { from: c(stops.first()), to: c(stops.last()), angle }
}

/// Name of a paint (fill, outline) and its hex value, or "Aucun".
fn paint_label(name: &'static str, hex: Option<String>) -> impl IntoElement {
    v_flex()
        .flex_1()
        .min_w_0()
        .child(div().text_sm().child(name))
        .child(div().text_xs().text_color(theme::text_faint()).child(hex.unwrap_or_else(|| "Aucun".into())))
}

fn section(title: impl Into<SharedString>) -> impl IntoElement {
    div()
        .px_4()
        .pt_4()
        .pb_1()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::accent())
        .child(title.into())
}

/// Section title with an (i) whose tooltip explains the section.
fn section_info(title: &'static str, id: &'static str, tip: &'static str) -> impl IntoElement {
    h_flex()
        .px_4()
        .pt_4()
        .pb_1()
        .gap(px(6.0))
        .items_center()
        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme::accent()).child(title))
        .child(info(id, tip))
}

/// Small (i) icon showing an explanation on hover.
pub(crate) fn info(id: &'static str, tip: &'static str) -> impl IntoElement {
    div()
        .id(id)
        .size(px(20.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .hover(|d| d.bg(theme::hover()))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        // Hovering explains; a click must not trigger the row it sits in.
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(icon(Icon::Info, 15.0, theme::text_faint()))
}

fn row() -> Div {
    h_flex().px_4().py(px(4.0)).gap_2().items_center()
}

fn label(text: &'static str) -> impl IntoElement {
    div().w(px(44.0)).text_xs().text_color(theme::text_dim()).child(text)
}

fn span_dim(text: String) -> impl IntoElement {
    div().text_xs().text_color(theme::text_faint()).child(text)
}

fn field(name: &'static str, n: &NumField) -> impl IntoElement {
    h_flex()
        .flex_1()
        .gap_2()
        .items_center()
        .child(div().text_xs().text_color(theme::text_dim()).child(name))
        .child(scrubbable(n, Input::new(&n.state)))
}

/// Input with a leading icon instead of a text label.
fn icon_input(ic: Icon, n: &NumField) -> impl IntoElement {
    scrubbable(n, Input::new(&n.state).prefix(icon(ic, 16.0, theme::text_dim())))
}

fn field_full(name: &'static str, n: &NumField) -> impl IntoElement {
    v_flex()
        .flex_1()
        .gap_1()
        .child(div().text_xs().text_color(theme::text_dim()).child(name))
        .child(scrubbable(n, Input::new(&n.state)))
}

/// Labelled text (non-numeric) input on its own line.
fn text_field_full(name: &'static str, state: &Entity<InputState>) -> impl IntoElement {
    v_flex().flex_1().gap_1().child(div().text_xs().text_color(theme::text_dim()).child(name)).child(Input::new(state))
}

/// Material filter chip: outlined, filled with the secondary container when on.
pub(crate) fn chip(
    id: impl Into<ElementId>,
    text: &'static str,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    h_flex()
        .id(id.into())
        .h(px(28.0))
        .px(px(10.0))
        .gap_1()
        .items_center()
        .rounded(px(theme::RADIUS_CHIP))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .border_1()
        .when(on, |d| {
            d.bg(theme::active()).border_color(theme::active()).text_color(theme::on_active()).child(icon(
                Icon::Check,
                14.0,
                theme::on_active(),
            ))
        })
        .when(!on, |d| d.border_color(theme::border()).text_color(theme::text_dim()).hover(|d| d.bg(theme::hover())))
        .on_click(on_click)
        .child(text)
}

/// Tonal pill button.
pub(crate) fn small_button(
    id: &'static str,
    text: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(30.0))
        .px_4()
        .flex()
        .items_center()
        .rounded_full()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        // Reads on panels and on the raised effect cards alike.
        .bg(theme::panel_high())
        .hover(|d| d.bg(theme::border()))
        .on_click(on_click)
        .child(text)
}

/// Circular icon button.
pub(crate) fn icon_button(
    id: impl Into<ElementId>,
    ic: Icon,
    tip: &'static str,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let color = theme::text_dim();
    div()
        .id(id.into())
        .size(px(30.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(!disabled, |d| d.hover(|d| d.bg(theme::hover())).on_click(on_click))
        .when(disabled, |d| d.opacity(0.38))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .child(icon(ic, 18.0, color))
}

#[cfg(test)]
mod number_tests {
    use super::NumberKind::*;

    #[test]
    fn number_fields_reject_text() {
        for ok in ["", "12", "12.5", "12,5", "0.", ",5"] {
            assert!(Positive.accepts(ok), "{ok}");
        }
        for bad in ["abc", "12a", "1.2.3", "1,2.3", "-3", "12 %", "1e5"] {
            assert!(!Positive.accepts(bad), "{bad}");
        }
        assert!(Signed.accepts("-") && Signed.accepts("-12.5"));
        assert!(!Signed.accepts("--1") && !Signed.accepts("1-"));
        assert!(Count.accepts("12") && !Count.accepts("1.5") && !Count.accepts("-2"));
    }
}
