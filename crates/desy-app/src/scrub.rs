//! Scrubbing number fields: press on a field and drag left or right to
//! change its value (Shift: steps of 10). A press without a drag focuses
//! the field for typing; while it has focus, the mouse edits text.

use crate::panels::{Field, fmt};
use crate::view::EditorView;
use desy_core::Document;
use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::*;

/// Pointer travel (px) per step.
const STEP_PX: f32 = 2.0;
/// Travel (px) before a press turns into a scrub.
const DEAD_ZONE: f32 = 3.0;

/// A numeric field and what is needed to scrub it.
#[derive(Clone)]
pub struct NumField {
    pub field: Field,
    pub state: Entity<InputState>,
    pub view: WeakEntity<EditorView>,
}

/// Wraps a field's input so a press on it starts a scrub (unless the input
/// is being edited, then the press goes to the text).
pub fn scrubbable(n: &NumField, input: impl IntoElement) -> Div {
    let (f, state, view) = (n.field, n.state.clone(), n.view.clone());
    div().flex_1().min_w_0().child(input).capture_any_mouse_down(move |ev: &MouseDownEvent, window, cx| {
        if ev.button != MouseButton::Left || state.read(cx).focus_handle(cx).is_focused(window) {
            return;
        }
        cx.stop_propagation();
        let x = f32::from(ev.position.x);
        let _ = view.update(cx, |v, cx| v.scrub_start(f, x, cx));
    })
}

pub struct Scrub {
    field: Field,
    start_x: f32,
    last_x: f32,
    /// Pointer travel not yet turned into steps.
    rest: f32,
    value: f64,
    /// Past the dead zone: the press is a scrub, not a click.
    moved: bool,
    /// Document and history before the scrub (one undo step per scrub).
    origin: Option<(Document, desy_core::history::History)>,
}

impl EditorView {
    pub fn scrub_start(&mut self, field: Field, x: f32, cx: &mut Context<Self>) {
        let text = self.props.input(field).read(cx).value().to_string();
        let value = parse(&text).unwrap_or(0.0);
        self.scrub = Some(Scrub { field, start_x: x, last_x: x, rest: 0.0, value, moved: false, origin: None });
        cx.notify();
    }

    pub fn scrub_move(&mut self, x: f32, shift: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = &mut self.scrub else { return };
        if !s.moved {
            if (x - s.start_x).abs() < DEAD_ZONE {
                return;
            }
            s.moved = true;
            s.last_x = x;
            // Intermediate values go to a throwaway history.
            let history = std::mem::take(&mut self.ed.history);
            s.origin = Some((self.ed.doc.clone(), history));
            return;
        }
        s.rest += x - s.last_x;
        s.last_x = x;
        let steps = (s.rest / STEP_PX).trunc();
        if steps == 0.0 {
            return;
        }
        s.rest -= steps * STEP_PX;
        let field = s.field;
        let mut value = s.value + steps as f64 * if shift { 10.0 } else { 1.0 };
        if field.number() != Some(crate::panels::NumberKind::Signed) {
            value = value.max(0.0);
        }
        self.apply_field(field, &fmt(value), cx);
        self.synced_revision = self.ed.revision;
        self.sync_props(window, cx);
        // The field shows what was really applied (clamps, rounding).
        let shown = parse(&self.props.input(field).read(cx).value()).unwrap_or(value);
        if let Some(s) = &mut self.scrub {
            s.value = shown;
        }
    }

    pub fn scrub_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.scrub.take() else { return };
        match s.origin {
            Some((doc, history)) => {
                self.ed.history = history;
                self.ed.commit(doc);
                self.after_edit(cx);
            }
            None => {
                // A plain click: edit the value by typing.
                let input = self.props.input(s.field).clone();
                input.update(cx, |st, cx| {
                    st.focus(window, cx);
                    st.select_all(window, cx);
                });
                cx.notify();
            }
        }
    }

    /// Window-wide pointer tracking while a scrub is under way, so it keeps
    /// going when the pointer leaves the field.
    pub fn scrub_tracker(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.scrub.as_ref()?;
        let e = cx.entity();
        Some(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    window.set_window_cursor_style(CursorStyle::ResizeLeftRight);
                    let m = e.clone();
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        // A release missed before this tracker existed (very quick click).
                        if ev.pressed_button != Some(MouseButton::Left) {
                            m.update(cx, |v, cx| v.scrub_end(window, cx));
                            return;
                        }
                        let x = f32::from(ev.position.x);
                        m.update(cx, |v, cx| v.scrub_move(x, ev.modifiers.shift, window, cx));
                    });
                    let u = e.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble {
                            u.update(cx, |v, cx| v.scrub_end(window, cx));
                        }
                    });
                },
            )
            .absolute()
            .size_0()
            .into_any_element(),
        )
    }
}

fn parse(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse().ok()
}
