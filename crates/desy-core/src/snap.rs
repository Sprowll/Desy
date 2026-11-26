//! Smart guides: magnetic alignment of edges and centers against other
//! objects and the artboard.

use kurbo::{Point, Rect, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Guide {
    /// Vertical line at `x`, from `y0` to `y1`.
    Vertical { x: f64, y0: f64, y1: f64 },
    /// Horizontal line at `y`, from `x0` to `x1`.
    Horizontal { y: f64, x0: f64, x1: f64 },
    /// Any line (e.g. a mirror axis), drawn infinitely long.
    Line { a: Point, b: Point },
}

#[derive(Clone, Debug, Default)]
pub struct SnapResult {
    pub offset: Vec2,
    pub guides: Vec<Guide>,
}

fn xs(r: &Rect) -> [f64; 3] {
    [r.x0, (r.x0 + r.x1) * 0.5, r.x1]
}

fn ys(r: &Rect) -> [f64; 3] {
    [r.y0, (r.y0 + r.y1) * 0.5, r.y1]
}

/// Best delta (smallest |delta| ≤ threshold) bringing one of `values` onto one of `candidates`.
fn best_delta(values: &[f64], candidates: impl Iterator<Item = f64> + Clone, threshold: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for &v in values {
        for c in candidates.clone() {
            let d = c - v;
            if d.abs() <= threshold && best.is_none_or(|b| d.abs() < b.abs()) {
                best = Some(d);
            }
        }
    }
    best
}

/// Snap a moving rectangle (edges + center) to target rectangles.
pub fn snap_rect(moving: Rect, targets: &[Rect], threshold: f64) -> SnapResult {
    snap_features(&xs(&moving), &ys(&moving), moving, targets, threshold, true, true)
}

/// Snap a single point (drawing, resizing, node editing).
pub fn snap_point(p: Point, targets: &[Rect], threshold: f64) -> SnapResult {
    snap_features(&[p.x], &[p.y], Rect::from_points(p, p), targets, threshold, true, true)
}

/// Snap only the given x / y values (e.g. the edges being resized).
pub fn snap_values(x: Option<f64>, y: Option<f64>, around: Rect, targets: &[Rect], threshold: f64) -> SnapResult {
    let xv: Vec<f64> = x.into_iter().collect();
    let yv: Vec<f64> = y.into_iter().collect();
    snap_features(&xv, &yv, around, targets, threshold, x.is_some(), y.is_some())
}

fn snap_features(
    mx: &[f64],
    my: &[f64],
    moving: Rect,
    targets: &[Rect],
    threshold: f64,
    do_x: bool,
    do_y: bool,
) -> SnapResult {
    let tx = targets.iter().flat_map(xs);
    let ty = targets.iter().flat_map(ys);
    let dx = if do_x { best_delta(mx, tx, threshold) } else { None };
    let dy = if do_y { best_delta(my, ty, threshold) } else { None };
    let offset = Vec2::new(dx.unwrap_or(0.0), dy.unwrap_or(0.0));
    let moved = moving + offset;

    let mut guides = Vec::new();
    if dx.is_some() {
        for &v in mx {
            let v = v + offset.x;
            for t in targets {
                if xs(t).iter().any(|c| (c - v).abs() < 1e-6) {
                    guides.push(Guide::Vertical { x: v, y0: moved.y0.min(t.y0), y1: moved.y1.max(t.y1) });
                }
            }
        }
    }
    if dy.is_some() {
        for &v in my {
            let v = v + offset.y;
            for t in targets {
                if ys(t).iter().any(|c| (c - v).abs() < 1e-6) {
                    guides.push(Guide::Horizontal { y: v, x0: moved.x0.min(t.x0), x1: moved.x1.max(t.x1) });
                }
            }
        }
    }
    SnapResult { offset, guides }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_center_to_artboard_center() {
        let artboard = Rect::new(0.0, 0.0, 100.0, 100.0);
        let moving = Rect::new(38.0, 10.0, 58.0, 20.0); // center x = 48
        let r = snap_rect(moving, &[artboard], 3.0);
        assert_eq!(r.offset, Vec2::new(2.0, 0.0));
        assert!(r.guides.iter().any(|g| matches!(g, Guide::Vertical { x, .. } if (*x - 50.0).abs() < 1e-9)));
    }

    #[test]
    fn no_snap_beyond_threshold() {
        let r = snap_rect(Rect::new(10.0, 10.0, 20.0, 20.0), &[Rect::new(40.0, 40.0, 60.0, 60.0)], 3.0);
        assert_eq!(r.offset, Vec2::ZERO);
        assert!(r.guides.is_empty());
    }

    #[test]
    fn picks_closest_candidate() {
        let targets = [Rect::new(0.0, 0.0, 10.0, 10.0), Rect::new(11.0, 0.0, 30.0, 10.0)];
        let r = snap_point(Point::new(10.4, 50.0), &targets, 3.0);
        assert!((r.offset.x + 0.4).abs() < 1e-9);
    }
}
