//! Colors, shapes and conversions shared by the UI.
//!
//! A Material 3 (Expressive) dark scheme: tonal surfaces stacked by
//! elevation, a light blue primary, a deep blue secondary container for
//! selected states, and generous rounded shapes. The canvas selection keeps
//! its own vivid blue so it reads on any artwork.

use desy_core::Color;
use gpui_kit::{App, Hsla, Rgba, px, rgb};

fn alpha(hex: u32, a: f32) -> Hsla {
    let mut c: Hsla = rgb(hex).into();
    c.a = a;
    c
}

// ---------------------------------------------------------------- surfaces

/// Workspace behind the artboard (surface dim).
pub fn canvas_bg() -> Hsla {
    rgb(0x111213).into()
}
/// Floating toolbars and panels (surface container).
pub fn panel() -> Hsla {
    rgb(0x1e1f21).into()
}
/// Raised elements inside a panel: inputs, tonal buttons (container high).
pub fn panel_alt() -> Hsla {
    rgb(0x2a2b2e).into()
}
/// Menus and flyouts (container highest).
pub fn panel_high() -> Hsla {
    rgb(0x333438).into()
}
/// Outline variant: hairlines and panel edges.
pub fn border() -> Hsla {
    rgb(0x3c3e41).into()
}

// ---------------------------------------------------------------- content

pub fn text() -> Hsla {
    rgb(0xe3e3e3).into()
}
/// On-surface-variant: labels and secondary text.
pub fn text_dim() -> Hsla {
    rgb(0xc4c7c5).into()
}
/// Text on the inverse surface (snackbars).
pub fn on_surface_inverse() -> Hsla {
    rgb(0x2f3033).into()
}
/// Hints and counters.
pub fn text_faint() -> Hsla {
    rgb(0x8e918f).into()
}
/// Hover state layer (on-surface at 8 %), works over any surface.
pub fn hover() -> Hsla {
    alpha(0xe3e3e3, 0.08)
}
/// Pressed / open state layer (on-surface at 12 %).
pub fn pressed() -> Hsla {
    alpha(0xe3e3e3, 0.12)
}

// ---------------------------------------------------------------- roles

/// Primary: filled buttons, focus, links, section titles.
pub fn accent() -> Hsla {
    rgb(0xa8c7fa).into()
}
pub fn on_accent() -> Hsla {
    rgb(0x062e6f).into()
}
/// Primary at 16 %: soft highlight.
pub fn accent_soft() -> Hsla {
    alpha(0xa8c7fa, 0.16)
}
/// Secondary container: active tool, selected rows, selected chips.
pub fn active() -> Hsla {
    rgb(0x004a77).into()
}
pub fn on_active() -> Hsla {
    rgb(0xc2e7ff).into()
}
/// Selection on the canvas (bounds, handles, nodes).
pub fn selection() -> Hsla {
    rgb(0x2f8cff).into()
}
pub fn guide() -> Hsla {
    rgb(0xff2e88).into()
}
pub fn danger() -> Hsla {
    rgb(0xffb4ab).into()
}
pub fn danger_container() -> Hsla {
    rgb(0x8c1d18).into()
}

// ---------------------------------------------------------------- shapes

/// Large containers: floating panels (M3 "extra large").
pub const RADIUS_PANEL: f32 = 24.0;
/// Menus and cards ("large").
pub const RADIUS_MENU: f32 = 16.0;
/// Active tool indicator, list selection ("medium").
pub const RADIUS_ITEM: f32 = 12.0;
/// Chips and fields ("small").
pub const RADIUS_CHIP: f32 = 8.0;

/// Align the component library (inputs, pickers, tooltips) with the scheme.
pub fn install(cx: &mut App) {
    gpui_kit::component::Theme::update(cx, |t| {
        let c = &mut t.colors;
        c.background = canvas_bg();
        c.foreground = text();
        c.border = border();
        c.input = border();
        c.ring = accent();
        c.caret = accent();
        c.selection = alpha(0xa8c7fa, 0.3);
        c.muted = panel_alt();
        c.muted_foreground = text_dim();
        c.popover = panel_high();
        c.popover_foreground = text();
        c.accent = hover();
        c.accent_foreground = text();
        c.primary = accent();
        c.primary_hover = rgb(0xbcd4fb).into();
        c.primary_active = rgb(0x93b8f8).into();
        c.primary_foreground = on_accent();
        c.secondary = panel_alt();
        c.secondary_hover = panel_high();
        c.secondary_active = panel_high();
        c.secondary_foreground = text();
        c.list_active = active();
        c.list_hover = hover();
        c.scrollbar = alpha(0x000000, 0.0);
        c.scrollbar_thumb = alpha(0xe3e3e3, 0.18);
        c.scrollbar_thumb_hover = alpha(0xe3e3e3, 0.3);
        c.slider_bar = accent();
        c.slider_thumb = accent();
        c.switch = panel_high();
        t.radius = px(10.0);
        t.radius_lg = px(RADIUS_MENU);
        t.shadow = true;
    });
}

pub fn to_hsla(c: Color) -> Hsla {
    Rgba { r: c.r, g: c.g, b: c.b, a: c.a }.into()
}

pub fn from_hsla(h: Hsla) -> Color {
    let c = h.to_rgb();
    Color::rgba(c.r, c.g, c.b, c.a)
}
