//! Color field: a swatch opening our color popover — eyedropper, hex value,
//! project palette and HSLA sliders.
//!
//! gpui-kit's `ColorPicker` popover cannot host extra controls, so this one
//! rebuilds it from public parts. It keeps `ColorPickerState` as its model:
//! changes still arrive as `ColorPickerEvent`s to the existing subscriptions.

use crate::editor::Tool;
use crate::icons::{Icon, icon};
use crate::theme;
use crate::view::EditorView;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Selectable;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::color_picker::ColorPickerState;
use gpui_kit::component::input::Input;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// A picker waiting for the eyedropper to sample the canvas.
pub struct PickerEyedrop {
    pub state: Entity<ColorPickerState>,
    /// Tool to restore once the color is picked.
    prev_tool: Tool,
}

/// What the field shows while closed.
#[derive(Clone, Copy, PartialEq)]
pub enum FieldLook {
    /// A dashed "+" (new palette color).
    Plus,
    /// A large disc (quick bar fill); `none` shows a crossed-out disc.
    Disc { none: bool },
    /// A ring in the color (quick bar stroke); `none` is crossed out.
    Ring { none: bool },
    /// A disc showing a gradient fill: start, end, CSS angle (degrees).
    GradientDisc { from: Hsla, to: Hsla, angle: f32 },
}

impl EditorView {
    /// A color swatch opening the color popover. `with_palette` lists the
    /// project palette inside the popover.
    pub fn color_field(
        &self,
        state: &Entity<ColorPickerState>,
        look: FieldLook,
        tip: &'static str,
        with_palette: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.color_field_with(state, look, tip, with_palette, None, window, cx)
    }

    /// Like [`Self::color_field`], with `extra` controls at the top of the
    /// popover (the quick bar puts its "None" chip there).
    #[allow(clippy::too_many_arguments)]
    pub fn color_field_with(
        &self,
        state: &Entity<ColorPickerState>,
        look: FieldLook,
        tip: &'static str,
        with_palette: bool,
        extra: Option<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let s = state.read(cx);
        let open = s.is_open();
        let value = s.value();
        let st = state.clone();
        Popover::new(("color-field", state.entity_id()))
            .open(open)
            .w(px(300.0))
            .on_open_change(move |open, _, cx| st.update(cx, |s, cx| s.set_open(*open, cx)))
            .trigger(Trigger { value, look, open, tip })
            .when(open, |p| p.children(extra).child(self.color_panel(state, with_palette, window, cx)))
    }

    fn color_panel(
        &self,
        state: &Entity<ColorPickerState>,
        with_palette: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        state.update(cx, |s, cx| s.sync_pending_value(window, cx));
        let s = state.read(cx);
        let color = s.displayed_color().unwrap_or(hsla(0.0, 0.0, 0.0, 1.0));
        let hex_input = s.hex_input().clone();
        let sliders = s.sliders().clone();

        let st = state.clone();
        let header = h_flex()
            .gap_2()
            .items_center()
            .child(
                div()
                    .id("picker-eyedropper")
                    .size(px(30.0))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::border())
                    .hover(|d| d.bg(theme::hover()).border_color(theme::accent()))
                    .tooltip(|window, cx| {
                        Tooltip::new("Pipette : clique sur le canvas pour prélever une couleur (Échap : annuler)")
                            .build(window, cx)
                    })
                    .on_click(cx.listener(move |v, _, window, cx| v.start_picker_eyedrop(st.clone(), window, cx)))
                    .child(icon(Icon::Eyedropper, 16.0, theme::text())),
            )
            .child(div().size(px(30.0)).flex_none().rounded_md().border_1().border_color(theme::border()).bg(color))
            .child(div().flex_1().child(Input::new(&hex_input).small()));

        let mut panel = v_flex().p_1().gap_3().child(header);

        // Editing a gradient takes several colors: stay open.
        let keep_open = self.is_fill_picker(state) && self.ed.selection_gradient().is_some();
        let palette = &self.ed.doc.palette;
        if with_palette && !palette.is_empty() {
            let mut grid = h_flex().flex_wrap().gap_1();
            for (i, c) in palette.iter().enumerate() {
                let h = theme::to_hsla(*c);
                let st = state.clone();
                let tip: SharedString = c.to_hex().into();
                grid = grid.child(
                    div()
                        .id(("picker-swatch", i))
                        .size(px(20.0))
                        .rounded_full()
                        .bg(h)
                        .border_1()
                        .border_color(theme::border())
                        .hover(|d| d.border_color(theme::text()))
                        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                        .on_click(move |_, window, cx| {
                            st.update(cx, |s, cx| {
                                if keep_open { s.update_color(h, window, cx) } else { s.select_color(h, window, cx) }
                            })
                        }),
                );
            }
            panel = panel.child(
                v_flex()
                    .gap_1()
                    .child(div().text_xs().text_color(theme::text_dim()).child("Palette du projet"))
                    .child(grid),
            );
        }

        let steps = 48;
        let hue: Vec<Hsla> = (0..steps).map(|i| hsla(i as f32 / (steps - 1) as f32, 1.0, 0.5, 1.0)).collect();
        let light: Vec<Hsla> = (0..steps).map(|i| hsla(color.h, 1.0, i as f32 / (steps - 1) as f32, 1.0)).collect();
        panel.child(
            v_flex()
                .gap_1()
                .child(slider_row("Teinte", Slider::new(sliders.hue()), track_steps(hue), color.h * 360.0))
                .child(slider_row(
                    "Saturation",
                    Slider::new(sliders.saturation()),
                    track_gradient(hsla(color.h, 0.0, color.l, 1.0), hsla(color.h, 1.0, color.l, 1.0)),
                    color.s * 100.0,
                ))
                .child(slider_row("Luminosité", Slider::new(sliders.lightness()), track_steps(light), color.l * 100.0))
                .child(slider_row(
                    "Opacité",
                    Slider::new(sliders.alpha()),
                    track_gradient(color.opacity(0.0), color.opacity(1.0)),
                    color.a * 100.0,
                )),
        )
    }

    /// Hide the picker and let the next canvas click choose its color.
    pub fn start_picker_eyedrop(
        &mut self,
        state: Entity<ColorPickerState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prev_tool = if self.ed.tool == Tool::Eyedropper { Tool::Select } else { self.ed.tool };
        self.props.picker_eyedrop = Some(PickerEyedrop { state: state.clone(), prev_tool });
        self.ed.set_tool(Tool::Eyedropper);
        state.update(cx, |s, cx| s.set_open(false, cx));
        window.focus(&self.canvas_focus, cx);
        cx.notify();
    }

    /// Canvas click: hand the sampled color to the waiting picker and reopen
    /// it. Returns whether the click was consumed.
    pub fn picker_eyedrop_click(
        &mut self,
        window_pt: kurbo::Point,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pe) = self.props.picker_eyedrop.take() else { return false };
        if self.ed.tool != Tool::Eyedropper {
            // The tool was changed meanwhile: drop the request.
            self.finish_idle_palette_edit(cx);
            return false;
        }
        let color = self.ed.sample_color(self.ed.to_doc(window_pt));
        self.ed.set_tool(pe.prev_tool);
        pe.state.update(cx, |s, cx| {
            s.set_open(true, cx);
            if let Some(c) = color {
                s.update_color(theme::to_hsla(c), window, cx);
            }
        });
        if color.is_none() {
            self.notify_user("Aucune couleur ici", true, cx);
        }
        true
    }

    /// Escape while the picker waits for a color: back to the picker.
    pub fn cancel_picker_eyedrop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pe) = self.props.picker_eyedrop.take() else { return false };
        if self.ed.tool == Tool::Eyedropper {
            self.ed.set_tool(pe.prev_tool);
        }
        pe.state.update(cx, |s, cx| s.set_open(true, cx));
        true
    }

    pub fn picker_eyedrop_pending(&self) -> bool {
        self.props.picker_eyedrop.is_some()
    }

    /// Whether this picker is hidden only while the eyedropper samples for it.
    pub fn picker_waits_for_eyedrop(&self, state: &Entity<ColorPickerState>) -> bool {
        self.props.picker_eyedrop.as_ref().is_some_and(|p| p.state == *state)
    }
}

#[derive(IntoElement)]
struct Trigger {
    value: Option<Hsla>,
    look: FieldLook,
    open: bool,
    tip: &'static str,
}

impl Selectable for Trigger {
    fn selected(mut self, selected: bool) -> Self {
        self.open = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.open
    }
}

impl RenderOnce for Trigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let tip = self.tip;
        let border = if self.open { theme::accent() } else { theme::border() };
        let d = div()
            .id("color-field-trigger")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx));
        match self.look {
            FieldLook::Plus => d
                .size(px(22.0))
                .rounded_full()
                .border_1()
                .border_dashed()
                .border_color(border)
                .text_sm()
                .text_color(theme::text_dim())
                .hover(|d| d.bg(theme::hover()).text_color(theme::text()))
                .child("+"),
            FieldLook::GradientDisc { from, to, angle } => d
                .size(px(36.0))
                .rounded_full()
                .when(self.open, |d| d.bg(theme::pressed()))
                .hover(|d| d.bg(theme::hover()))
                .child(
                    div()
                        .size(px(26.0))
                        .rounded_full()
                        .border_1()
                        .border_color(if self.open { theme::accent() } else { theme::text_faint().opacity(0.6) })
                        .bg(linear_gradient(angle, linear_color_stop(from, 0.0), linear_color_stop(to, 1.0))),
                ),
            FieldLook::Disc { none } | FieldLook::Ring { none } => {
                let ring = matches!(self.look, FieldLook::Ring { .. });
                let color = self.value.filter(|_| !none);
                let swatch = div()
                    .relative()
                    .size(px(26.0))
                    .rounded_full()
                    .border_color(if self.open { theme::accent() } else { theme::text_faint().opacity(0.6) })
                    .when(!ring, |d| d.border_1().when_some(color, |d, c| d.bg(c)))
                    .when(ring, |d| match color {
                        // Thick ring inside a light hairline so dark colors read.
                        Some(c) => d
                            .border_1()
                            .p(px(1.0))
                            .child(div().size_full().rounded_full().border(px(5.0)).border_color(c)),
                        None => d.border_2(),
                    })
                    .when(color.is_none(), |d| {
                        d.flex().items_center().justify_center().child(icon(Icon::Line, 22.0, theme::danger()))
                    });
                d.size(px(36.0))
                    .rounded_full()
                    .when(self.open, |d| d.bg(theme::pressed()))
                    .hover(|d| d.bg(theme::hover()))
                    .child(swatch)
            }
        }
    }
}

fn slider_row(label: &'static str, slider: Slider, track: AnyElement, value: f32) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(div().w(px(68.0)).flex_none().text_xs().text_color(theme::text_dim()).child(label))
        .child(
            div()
                .relative()
                .flex()
                .items_center()
                .flex_1()
                .h(px(28.0))
                .child(track)
                .child(slider.flex_1().bg(transparent_black())),
        )
        .child(
            div()
                .w(px(30.0))
                .flex_none()
                .text_xs()
                .text_color(theme::text_dim())
                .text_align(TextAlign::Right)
                .child(format!("{value:.0}")),
        )
}

fn track_steps(colors: Vec<Hsla>) -> AnyElement {
    h_flex()
        .absolute()
        .left_0()
        .right_0()
        .h(px(10.0))
        .rounded_full()
        .overflow_hidden()
        .children(colors.into_iter().map(|c| div().flex_1().h_full().bg(c)))
        .into_any_element()
}

fn track_gradient(start: Hsla, end: Hsla) -> AnyElement {
    div()
        .absolute()
        .left_0()
        .right_0()
        .h(px(10.0))
        .rounded_full()
        .bg(linear_gradient(90.0, linear_color_stop(start, 0.0), linear_color_stop(end, 1.0)))
        .into_any_element()
}
