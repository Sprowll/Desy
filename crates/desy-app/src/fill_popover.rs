//! Top of the fill color popover (quick bar and inspector): paint mode tabs
//! (none, solid, gradient) and, for a gradient, its editor. The usual color
//! controls below edit the solid color, or the selected gradient stop.

use crate::icons::{Icon, icon};
use crate::panels::{Field, chip};
use crate::scrub::scrubbable;
use crate::theme;
use crate::view::EditorView;
use desy_core::{Color, Gradient, GradientKind};
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::color_picker::ColorPickerState;
use gpui_kit::component::input::Input;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// How the selection is filled.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum FillMode {
    None,
    Solid,
    Gradient,
}

const KNOB: f32 = 26.0;

impl EditorView {
    /// Fill mode of the first selected object.
    pub(crate) fn fill_mode(&self) -> FillMode {
        let o = self.ed.selected.first().and_then(|id| self.ed.doc.get(*id));
        match o {
            Some(o) if o.fill_gradient().is_some() => FillMode::Gradient,
            Some(o) if o.fill.is_some() => FillMode::Solid,
            _ => FillMode::None,
        }
    }

    /// Color shown by the fill pickers: the plain fill, or the gradient
    /// stop being edited.
    pub(crate) fn fill_picker_color(&self) -> Option<Color> {
        let o = self.ed.selected.first().and_then(|id| self.ed.doc.get(*id))?;
        match o.fill_gradient() {
            Some(g) => g.end_stop(self.props.gradient_stop).map(|i| g.stops[i].color),
            None => o.fill,
        }
    }

    /// Whether `state` is one of the fill pickers (quick bar, inspector).
    pub(crate) fn is_fill_picker(&self, state: &Entity<ColorPickerState>) -> bool {
        *state == self.props.fill || *state == self.quick.fill
    }

    /// Edit the first (`false`) or last stop with the color controls.
    pub(crate) fn select_gradient_stop(&mut self, end: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.props.gradient_stop = end;
        self.push_fill_picker_color(window, cx);
        cx.notify();
    }

    fn push_fill_picker_color(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.fill_picker_color() else { return };
        for state in [self.props.fill.clone(), self.quick.fill.clone()] {
            state.update(cx, |s, cx| s.set_value(theme::to_hsla(c), window, cx));
        }
    }

    pub(crate) fn set_fill_mode(&mut self, mode: FillMode, window: &mut Window, cx: &mut Context<Self>) {
        if mode == self.fill_mode() {
            return;
        }
        let fallback = || self.ed.shape_style.fill.or(Color::from_hex("#d9d9d9"));
        match mode {
            FillMode::None => self.ed.set_fill(None),
            FillMode::Solid => {
                // Leaving a gradient keeps the color being edited.
                let c = self.fill_picker_color().or_else(fallback);
                self.ed.set_fill(c);
            }
            FillMode::Gradient => {
                let base = self.fill_picker_color().or_else(fallback).unwrap_or(Color::BLACK);
                self.ed.set_gradient(Some(Gradient::from_color(base)));
                self.props.gradient_stop = false;
            }
        }
        self.after_edit(cx);
        self.push_fill_picker_color(window, cx);
    }

    /// Tabs, then the gradient editor in gradient mode.
    pub(crate) fn fill_header(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.fill_mode();
        let mut tabs = h_flex().h(px(32.0)).p(px(3.0)).gap(px(2.0)).rounded(px(10.0)).bg(theme::panel_alt());
        for (i, (m, name)) in [(FillMode::None, "Aucun"), (FillMode::Solid, "Uni"), (FillMode::Gradient, "Dégradé")]
            .into_iter()
            .enumerate()
        {
            let on = m == mode;
            tabs = tabs.child(
                div()
                    .id(("fill-mode", i))
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .when(on, |d| d.bg(theme::active()).text_color(theme::on_active()))
                    .when(!on, |d| d.text_color(theme::text_dim()).hover(|d| d.bg(theme::hover())))
                    .on_click(cx.listener(move |v, _, window, cx| v.set_fill_mode(m, window, cx)))
                    .child(name),
            );
        }
        let mut col = v_flex().px_1().pb_3().gap_3().child(tabs);
        if let Some(g) = self.ed.selection_gradient() {
            col = col.child(self.gradient_editor(g, window, cx));
        }
        col.into_any_element()
    }

    fn gradient_editor(&self, g: &Gradient, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stops = g.sorted_stops();
        let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return v_flex() };
        let (from, to) = (theme::to_hsla(first.color), theme::to_hsla(last.color));
        let selected = self.props.gradient_stop;

        // Stops laid out left to right, a knob on each end.
        let knob = |end: bool, c: Hsla| {
            let on = selected == end;
            div()
                .id(("gradient-knob", end as usize))
                .absolute()
                .top_0()
                .when(!end, |d| d.left_0())
                .when(end, |d| d.right_0())
                .size(px(KNOB))
                .rounded_full()
                .border(px(if on { 3.0 } else { 2.0 }))
                .border_color(if on { theme::accent() } else { gpui_kit::white() })
                .shadow_sm()
                .bg(c)
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    Tooltip::new(if end { "Couleur d'arrivée" } else { "Couleur de départ" }).build(window, cx)
                })
                .on_click(cx.listener(move |v, _, window, cx| v.select_gradient_stop(end, window, cx)))
        };
        let bar = div()
            .relative()
            .h(px(KNOB))
            .child(
                div()
                    .absolute()
                    .left(px(KNOB / 2.0))
                    .right(px(KNOB / 2.0))
                    .top(px((KNOB - 14.0) / 2.0))
                    .h(px(14.0))
                    .rounded(px(7.0))
                    .border_1()
                    .border_color(theme::border())
                    .bg(linear_gradient(90.0, linear_color_stop(from, 0.0), linear_color_stop(to, 1.0))),
            )
            .child(knob(false, from))
            .child(knob(true, to));

        let mut kinds = h_flex().gap_1();
        for (i, (k, name)) in
            [(GradientKind::Linear, "Linéaire"), (GradientKind::Radial, "Radial")].into_iter().enumerate()
        {
            kinds = kinds.child(chip(
                ("gradient-kind", i),
                name,
                g.kind == k,
                cx.listener(move |v, _, _, cx| {
                    v.ed.edit_gradient(|g| g.kind = k);
                    v.after_edit(cx);
                }),
            ));
        }
        let angle = (g.kind == GradientKind::Linear).then(|| {
            let n = self.props.num(Field::GradientAngle);
            scrubbable(
                &n,
                Input::new(&n.state)
                    .small()
                    .prefix(icon(Icon::Angle, 14.0, theme::text_dim()))
                    .suffix(div().text_xs().text_color(theme::text_dim()).child("°")),
            )
        });
        let swap = div()
            .id("gradient-swap")
            .size(px(28.0))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded_full()
            .hover(|d| d.bg(theme::hover()))
            .tooltip(|window, cx| Tooltip::new("Inverser les couleurs").build(window, cx))
            .on_click(cx.listener(|v, _, window, cx| {
                v.ed.edit_gradient(|g| g.reverse());
                v.after_edit(cx);
                v.push_fill_picker_color(window, cx);
            }))
            .child(icon(Icon::Swap, 16.0, theme::text_dim()));

        // Saved gradients, and saving this one.
        let saved = &self.ed.doc.palette_gradients;
        let is_saved = saved.contains(g);
        let mut swatches = h_flex().flex_wrap().gap_1().items_center();
        for (i, s) in saved.iter().enumerate() {
            let s2 = s.clone();
            swatches = swatches.child(
                gradient_swatch(("picker-gradient", i), s, 22.0, s == g)
                    .tooltip(|window, cx| Tooltip::new("Appliquer · ⌥ clic : retirer").build(window, cx))
                    .on_click(cx.listener(move |v, ev: &ClickEvent, window, cx| {
                        if ev.modifiers().alt {
                            v.remove_palette_gradient(i, cx);
                        } else {
                            v.apply_palette_gradient(s2.clone(), window, cx);
                        }
                    })),
            );
        }
        let g2 = g.clone();
        swatches = swatches.child(
            div()
                .id("gradient-save")
                .size(px(22.0))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_dashed()
                .border_color(theme::border())
                .text_sm()
                .text_color(theme::text_dim())
                .when(is_saved, |d| d.opacity(0.38))
                .when(!is_saved, |d| {
                    d.hover(|d| d.bg(theme::hover()).text_color(theme::text())).on_click(cx.listener(
                        move |v, _, _, cx| {
                            let g = g2.clone();
                            v.ed.edit(|ed| ed.doc.palette_gradients.push(g));
                            v.after_edit(cx);
                        },
                    ))
                })
                .tooltip(move |window, cx| {
                    Tooltip::new(if is_saved { "Déjà dans la palette" } else { "Enregistrer dans la palette" })
                        .build(window, cx)
                })
                .child("+"),
        );

        v_flex()
            .gap_3()
            .child(bar)
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(kinds)
                    .child(div().flex_1().min_w_0().children(angle))
                    .child(swap),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(theme::text_dim()).child("Dégradés du projet"))
                    .child(swatches),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .pt_1()
                    .border_t_1()
                    .border_color(theme::border())
                    .child(div().size(px(10.0)).rounded_full().bg(if selected { to } else { from }))
                    .child(div().text_xs().text_color(theme::text_dim()).child(if selected {
                        "Couleur d'arrivée"
                    } else {
                        "Couleur de départ"
                    })),
            )
    }

    /// Fill the selection with a saved gradient.
    pub(crate) fn apply_palette_gradient(&mut self, g: Gradient, window: &mut Window, cx: &mut Context<Self>) {
        if self.ed.selected.is_empty() {
            self.notify_user("Sélectionne une forme pour lui appliquer le dégradé", true, cx);
            return;
        }
        self.ed.set_gradient(Some(g));
        self.after_edit(cx);
        self.push_fill_picker_color(window, cx);
    }

    pub(crate) fn remove_palette_gradient(&mut self, i: usize, cx: &mut Context<Self>) {
        self.ed.edit(|ed| {
            if i < ed.doc.palette_gradients.len() {
                ed.doc.palette_gradients.remove(i);
            }
        });
        self.after_edit(cx);
    }
}

/// CSS-style background showing a gradient (radials as center → edge).
pub(crate) fn gradient_background(g: &Gradient) -> Background {
    let stops = g.sorted_stops();
    let c = |s: Option<&desy_core::GradientStop>| s.map(|s| theme::to_hsla(s.color)).unwrap_or_default();
    let angle = if g.kind == GradientKind::Linear { g.angle as f32 + 90.0 } else { 90.0 };
    linear_gradient(angle, linear_color_stop(c(stops.first()), 0.0), linear_color_stop(c(stops.last()), 1.0))
}

/// Round gradient swatch; `current` rings it.
pub(crate) fn gradient_swatch(id: impl Into<ElementId>, g: &Gradient, size: f32, current: bool) -> Stateful<Div> {
    let bg = gradient_background(g);
    div().id(id.into()).size(px(size)).flex().flex_none().items_center().justify_center().rounded_full().map(|d| {
        if current {
            d.border_2().border_color(theme::text()).child(div().size(px(size - 8.0)).rounded_full().bg(bg))
        } else {
            d.bg(bg).border_1().border_color(theme::border()).hover(|d| d.border_color(theme::text_dim()))
        }
    })
}
