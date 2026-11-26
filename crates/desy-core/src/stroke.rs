//! Stroke outlines. Uniform strokes use kurbo's stroker (exact joins/caps);
//! variable-width strokes are built from per-segment strips plus round
//! joins, all oriented the same way so a non-zero fill renders them correctly.

use crate::path::VectorPath;
use kurbo::{BezPath, Cap, Circle, Join, ParamCurve, ParamCurveArclen, ParamCurveDeriv, PathSeg, Point, Shape, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineCap {
    Butt,
    #[default]
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineJoin {
    Miter,
    #[default]
    Round,
    Bevel,
}

impl From<LineCap> for Cap {
    fn from(c: LineCap) -> Self {
        match c {
            LineCap::Butt => Cap::Butt,
            LineCap::Round => Cap::Round,
            LineCap::Square => Cap::Square,
        }
    }
}

impl From<LineJoin> for Join {
    fn from(j: LineJoin) -> Self {
        match j {
            LineJoin::Miter => Join::Miter,
            LineJoin::Round => Join::Round,
            LineJoin::Bevel => Join::Bevel,
        }
    }
}

pub const TOLERANCE: f64 = 0.05;

/// Outline of a uniform stroke.
pub fn uniform_outline(path: &BezPath, width: f64, cap: LineCap, join: LineJoin) -> BezPath {
    let style = kurbo::Stroke::new(width).with_caps(cap.into()).with_join(join.into()).with_miter_limit(8.0);
    kurbo::stroke(path.iter(), &style, &kurbo::StrokeOpts::default(), TOLERANCE)
}

fn smoothstep(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn tangent(seg: &PathSeg, t: f64) -> Vec2 {
    let d = match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.deriv().eval(t).to_vec2(),
        PathSeg::Cubic(c) => {
            let d = c.deriv().eval(t).to_vec2();
            if d.hypot2() > 1e-12 {
                d
            } else {
                // Degenerate handle at an end point: look slightly inside.
                if t < 0.5 { c.eval((t + 1e-3).min(1.0)) - c.eval(t) } else { c.eval(t) - c.eval((t - 1e-3).max(0.0)) }
            }
        }
    };
    if d.hypot2() < 1e-18 { Vec2::new(1.0, 0.0) } else { d.normalize() }
}

fn push_polygon(out: &mut BezPath, pts: &[Point]) {
    if pts.len() < 3 {
        return;
    }
    // Keep every piece counter-clockwise (positive area) so pieces add up under non-zero.
    let area: f64 = pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum();
    let iter: Box<dyn Iterator<Item = &Point>> =
        if area < 0.0 { Box::new(pts.iter().rev()) } else { Box::new(pts.iter()) };
    let mut first = true;
    for p in iter {
        if first {
            out.move_to(*p);
            first = false;
        } else {
            out.line_to(*p);
        }
    }
    out.close_path();
}

fn push_circle(out: &mut BezPath, c: Point, r: f64) {
    if r <= 1e-6 {
        return;
    }
    let circle = Circle::new(c, r).to_path(TOLERANCE);
    // kurbo circles are clockwise in y-down space with positive signed area check below.
    if circle.area() < 0.0 {
        out.extend(circle.reverse_subpaths());
    } else {
        out.extend(circle);
    }
}

/// Outline of a stroke whose width varies per node (`node.width * base_width`).
/// Joins are always round; caps are round or butt.
pub fn variable_outline(path: &VectorPath, base_width: f64, cap: LineCap) -> BezPath {
    let mut out = BezPath::new();
    let half = base_width * 0.5;
    for sp in &path.subpaths {
        let segs = sp.width_segments();
        for ws in &segs {
            let len = ws.seg.arclen(0.1);
            let n = ((len / 2.0).ceil() as usize).clamp(4, 256);
            let mut left = Vec::with_capacity(n + 1);
            let mut right = Vec::with_capacity(n + 1);
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let p = ws.seg.eval(t);
                let tan = tangent(&ws.seg, t);
                let normal = Vec2::new(-tan.y, tan.x);
                let w = (ws.w0 + (ws.w1 - ws.w0) * smoothstep(t)).max(0.0) * half;
                left.push(p + normal * w);
                right.push(p - normal * w);
            }
            right.reverse();
            left.extend(right);
            push_polygon(&mut out, &left);
        }
        // Round joins at every interior node; caps at the ends of open paths.
        let count = sp.nodes.len();
        for (i, node) in sp.nodes.iter().enumerate() {
            let is_end = !sp.closed && (i == 0 || i + 1 == count);
            if is_end && cap != LineCap::Round {
                continue;
            }
            push_circle(&mut out, node.p, node.width.max(0.0) * half);
        }
    }
    out
}

/// Union-cleaned version of an outline (no overlaps), for export and booleans.
pub fn clean(path: &BezPath) -> BezPath {
    crate::boolean::simplify(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::{PathNode, SubPath};

    #[test]
    fn tapered_line_has_expected_area() {
        let mut a = PathNode::corner(Point::new(0.0, 0.0));
        let mut b = PathNode::corner(Point::new(100.0, 0.0));
        a.width = 1.0;
        b.width = 1.0;
        let vp = VectorPath { subpaths: vec![SubPath { nodes: vec![a, b], closed: false }] };
        let outline = clean(&variable_outline(&vp, 10.0, LineCap::Butt));
        assert!((outline.area().abs() - 1000.0).abs() < 1.0, "area {}", outline.area());
    }

    #[test]
    fn zero_width_end_tapers() {
        let a = PathNode::corner(Point::new(0.0, 0.0));
        let mut b = PathNode::corner(Point::new(100.0, 0.0));
        b.width = 0.0;
        let vp = VectorPath { subpaths: vec![SubPath { nodes: vec![a, b], closed: false }] };
        let outline = clean(&variable_outline(&vp, 10.0, LineCap::Butt));
        // Smoothstep taper averages to half width.
        assert!((outline.area().abs() - 500.0).abs() < 5.0, "area {}", outline.area());
    }
}
