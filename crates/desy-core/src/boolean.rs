//! Boolean operations (union, subtract, intersect, exclude) on Bézier paths,
//! keeping curves (no flattening) thanks to `linesweeper`.

use kurbo::{BezPath, PathEl};
use linesweeper::{BinaryOp, FillRule, binary_op};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoolOp {
    Union,
    /// Bottom-most shape minus everything above it (Illustrator's "Minus Front").
    Subtract,
    Intersect,
    Exclude,
}

/// Close every open subpath so the sweep-line accepts it.
pub fn close_all(path: &BezPath) -> BezPath {
    let mut out = BezPath::new();
    let mut open = false;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(_) => {
                if open {
                    out.close_path();
                }
                open = true;
            }
            PathEl::ClosePath => open = false,
            _ => {}
        }
        out.push(*el);
    }
    if open {
        out.close_path();
    }
    out
}

fn op(a: &BezPath, b: &BezPath, fill: FillRule, op: BinaryOp) -> BezPath {
    let a = close_all(a);
    let b = close_all(b);
    match binary_op(&a, &b, fill, op) {
        Ok(contours) => {
            let mut out = BezPath::new();
            for c in contours.contours() {
                out.extend(c.path.iter());
            }
            out
        }
        Err(_) => BezPath::new(),
    }
}

/// Remove self-overlaps using the non-zero rule.
pub fn simplify(path: &BezPath) -> BezPath {
    op(path, &BezPath::new(), FillRule::NonZero, BinaryOp::Union)
}

/// Combine areas in z-order (index 0 = bottom).
pub fn apply(op_kind: BoolOp, areas: &[BezPath]) -> BezPath {
    let Some((first, rest)) = areas.split_first() else { return BezPath::new() };
    let mut acc = simplify(first);
    match op_kind {
        BoolOp::Union => {
            for a in rest {
                acc = op(&acc, a, FillRule::NonZero, BinaryOp::Union);
            }
        }
        BoolOp::Intersect => {
            for a in rest {
                acc = op(&acc, a, FillRule::NonZero, BinaryOp::Intersection);
            }
        }
        BoolOp::Exclude => {
            for a in rest {
                acc = op(&acc, a, FillRule::NonZero, BinaryOp::Xor);
            }
        }
        BoolOp::Subtract => {
            let mut cutter = BezPath::new();
            for a in rest {
                cutter = op(&cutter, a, FillRule::NonZero, BinaryOp::Union);
            }
            acc = op(&acc, &cutter, FillRule::NonZero, BinaryOp::Difference);
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Circle, Rect, Shape};

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        Rect::new(x0, y0, x1, y1).to_path(0.1)
    }

    /// Area of a set given as nested contours; robust to contour orientation.
    fn set_area(p: &BezPath) -> f64 {
        // Linesweeper returns outer contours and holes with opposite orientation.
        p.area().abs()
    }

    #[test]
    fn union_of_overlapping_squares() {
        let r = apply(BoolOp::Union, &[rect(0., 0., 10., 10.), rect(5., 5., 15., 15.)]);
        assert!((set_area(&r) - 175.0).abs() < 1e-6, "{}", set_area(&r));
    }

    #[test]
    fn subtract_front_from_back() {
        let r = apply(BoolOp::Subtract, &[rect(0., 0., 10., 10.), rect(5., 0., 15., 10.)]);
        assert!((set_area(&r) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn intersect_and_exclude() {
        let shapes = [rect(0., 0., 10., 10.), rect(5., 5., 15., 15.)];
        let i = apply(BoolOp::Intersect, &shapes);
        assert!((set_area(&i) - 25.0).abs() < 1e-6);
        let x = apply(BoolOp::Exclude, &shapes);
        assert!((set_area(&x) - 150.0).abs() < 1e-6);
    }

    #[test]
    fn curves_survive_union() {
        let a = Circle::new((0.0, 0.0), 10.0).to_path(0.01);
        let b = Circle::new((10.0, 0.0), 10.0).to_path(0.01);
        let r = apply(BoolOp::Union, &[a, b]);
        assert!(r.elements().iter().any(|e| matches!(e, PathEl::CurveTo(..))));
        // Two r=10 circles 10 apart: 2πr² − lens area.
        let lens = 2.0 * 100.0 * (0.5f64).acos() - 5.0 * (400.0f64 - 100.0).sqrt();
        let expected = 2.0 * std::f64::consts::PI * 100.0 - lens;
        assert!((set_area(&r) - expected).abs() < 0.5, "{} vs {}", set_area(&r), expected);
    }

    #[test]
    fn subtract_makes_hole() {
        let r = apply(BoolOp::Subtract, &[rect(0., 0., 10., 10.), rect(3., 3., 7., 7.)]);
        assert!((set_area(&r) - 84.0).abs() < 1e-6, "{}", set_area(&r));
    }
}
