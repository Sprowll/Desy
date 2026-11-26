//! Entrance motion for floating surfaces: menus, flyouts, windows, cards and
//! the snackbar fade in while gliding a few pixels into place, with a slight
//! overshoot so they land rather than just appear. Menu rows follow one
//! after the other.
//!
//! GPUI has no element transform, so the glide is a pair of opposite
//! margins: the surface moves, its siblings and its anchor (`top`, `bottom`,
//! `left`…) do not. The animated element must therefore have no margins of
//! its own. A motion plays when its element id first renders: a closed
//! surface stops rendering, so the next opening plays again. There is no
//! exit motion for the same reason.
//!
//! Reduced motion is handled by GPUI: the end state shows at once.

use gpui_kit::base::v_flex;
use gpui_kit::*;
use std::time::Duration;

/// Side the surface comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum From {
    Above,
    Below,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct Enter {
    from: From,
    distance: f32,
    duration: Duration,
    delay: Duration,
    /// Fade the elevation shadow with the surface: a half transparent panel
    /// does not hide its own shadow, which would show through as a dark slab.
    lifted: bool,
}

impl Enter {
    /// Menus and popovers: a short, quick drop.
    pub fn menu(from: From) -> Self {
        Self { from, distance: 10.0, duration: Duration::from_millis(260), delay: Duration::ZERO, lifted: true }
    }

    /// Windows, cards and bars: a longer glide.
    pub fn card(from: From) -> Self {
        Self { from, distance: 18.0, duration: Duration::from_millis(340), delay: Duration::ZERO, lifted: true }
    }

    /// Row `i` of a menu: rows cascade in after the surface.
    pub fn row(i: usize) -> Self {
        Self {
            from: From::Left,
            distance: 8.0,
            duration: Duration::from_millis(240),
            delay: Duration::from_millis(30 + 22 * i.min(10) as u64),
            lifted: false,
        }
    }

    /// Content of a popover: the surface shows at once, its content glides.
    pub fn content() -> Self {
        Self {
            from: From::Above,
            distance: 6.0,
            duration: Duration::from_millis(220),
            delay: Duration::ZERO,
            lifted: false,
        }
    }

    /// Start `ms` milliseconds later (the surface stays hidden meanwhile).
    pub fn after(mut self, ms: u64) -> Self {
        self.delay = Duration::from_millis(ms);
        self
    }

    /// For an element without its own shadow.
    pub fn flat(mut self) -> Self {
        self.lifted = false;
        self
    }

    /// Animate `el` inside a stretching column: for elements with margins or
    /// an opacity of their own, which the motion would override.
    pub fn wrap(self, id: impl Into<ElementId>, el: impl IntoElement) -> AnimationElement<Div> {
        self.flat().run(id, v_flex().flex_none().child(el))
    }

    pub fn run<E: Styled + IntoElement + 'static>(self, id: impl Into<ElementId>, el: E) -> AnimationElement<E> {
        let mut steps = Vec::with_capacity(2);
        if !self.delay.is_zero() {
            steps.push(Animation::new(self.delay));
        }
        steps.push(Animation::new(self.duration));
        let delayed = steps.len() > 1;
        el.with_animations(id, steps, move |el, ix, t| {
            let t = if delayed && ix == 0 { 0.0 } else { t };
            self.frame(el, t)
        })
    }

    fn frame<E: Styled>(self, el: E, t: f32) -> E {
        // 1 → 0, dipping just below 0 before settling: the landing.
        let d = self.distance * (1.0 - ease_out_back(t));
        // The fade finishes well before the glide.
        let fade = ease_out_cubic((t * 1.8).min(1.0));
        let el = match self.from {
            From::Above => el.mt(px(-d)).mb(px(d)),
            From::Below => el.mt(px(d)).mb(px(-d)),
            From::Left => el.ml(px(-d)).mr(px(d)),
            From::Right => el.ml(px(d)).mr(px(-d)),
        };
        let el = el.opacity(fade);
        if self.lifted { el.shadow(shadow_lg(fade * fade * fade)) } else { el }
    }
}

/// Morph played when an item becomes active: its corners go from `from` to
/// `to` radius with a springy overshoot while its fill fades in.
pub fn morph<E: Styled + IntoElement + 'static>(
    id: impl Into<ElementId>,
    el: E,
    from: f32,
    to: f32,
    fill: Hsla,
) -> AnimationElement<E> {
    el.with_animation(id, Animation::new(Duration::from_millis(380)), move |el, t| {
        let r = from + (to - from) * ease_out_back(t);
        let mut c = fill;
        c.a *= ease_out_cubic((t * 2.0).min(1.0));
        el.rounded(px(r.max(0.0))).bg(c)
    })
}

/// GPUI's `shadow_lg` at `strength` of its ink.
fn shadow_lg(strength: f32) -> Vec<BoxShadow> {
    let ink = hsla(0., 0., 0., 0.1 * strength.clamp(0.0, 1.0));
    vec![
        BoxShadow::new(px(0.), px(10.), ink).blur_radius(px(15.)).spread_radius(px(-3.)),
        BoxShadow::new(px(0.), px(4.), ink).blur_radius(px(6.)).spread_radius(px(-4.)),
    ]
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Overshoots by about 8 % around t = 0.6, then settles on 1.
fn ease_out_back(t: f32) -> f32 {
    const C1: f32 = 1.5;
    const C3: f32 = C1 + 1.0;
    let x = t.clamp(0.0, 1.0) - 1.0;
    1.0 + C3 * x * x * x + C1 * x * x
}

#[cfg(test)]
mod tests {
    use super::{ease_out_back, ease_out_cubic};

    #[test]
    fn easings_start_at_zero_and_settle_on_one() {
        for f in [ease_out_back as fn(f32) -> f32, ease_out_cubic] {
            assert!(f(0.0).abs() < 1e-6);
            assert!((f(1.0) - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn the_landing_overshoots_a_little() {
        let peak = (0..=100).map(|i| ease_out_back(i as f32 / 100.0)).fold(0.0, f32::max);
        assert!(peak > 1.02 && peak < 1.12, "peak {peak}");
    }
}
