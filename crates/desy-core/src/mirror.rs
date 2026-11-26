//! Live mirror (like Blender's Mirror modifier): an object keeps only one
//! half; the other half is its reflection across an axis line, recomputed on
//! every change. Path ends lying on the axis are merged with their reflection
//! so both halves form one continuous path ("linked" points).

use crate::path::{PathNode, SubPath, VectorPath};
use kurbo::{Affine, BezPath, Point, Vec2};
use serde::{Deserialize, Serialize};

/// Distance under which a point counts as lying on the axis.
pub const ON_AXIS: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mirror {
    /// Two distinct points of the axis line (world space).
    pub a: Point,
    pub b: Point,
    /// Merge path ends lying on the axis with their reflection.
    #[serde(default = "yes")]
    pub merge: bool,
}

fn yes() -> bool {
    true
}

impl Mirror {
    pub fn new(a: Point, b: Point) -> Self {
        Self { a, b, merge: true }
    }

    pub fn vertical(x: f64) -> Self {
        Self::new(Point::new(x, 0.0), Point::new(x, 1.0))
    }

    pub fn horizontal(y: f64) -> Self {
        Self::new(Point::new(0.0, y), Point::new(1.0, y))
    }

    fn dir(&self) -> Vec2 {
        let d = self.b - self.a;
        if d.hypot2() < 1e-18 { Vec2::new(0.0, 1.0) } else { d.normalize() }
    }

    /// Reflection across the axis.
    pub fn affine(&self) -> Affine {
        let d = self.dir();
        let (dx, dy) = (d.x, d.y);
        let r = Affine::new([2.0 * dx * dx - 1.0, 2.0 * dx * dy, 2.0 * dx * dy, 2.0 * dy * dy - 1.0, 0.0, 0.0]);
        Affine::translate(self.a.to_vec2()) * r * Affine::translate(-self.a.to_vec2())
    }

    pub fn project(&self, p: Point) -> Point {
        let d = self.dir();
        self.a + d * (p - self.a).dot(d)
    }

    pub fn distance(&self, p: Point) -> f64 {
        (p - self.project(p)).hypot()
    }

    pub fn on_axis(&self, p: Point) -> bool {
        self.distance(p) <= ON_AXIS
    }

    pub fn transform(&mut self, t: Affine) {
        self.a = t * self.a;
        self.b = t * self.b;
    }

    fn reflect_node(&self, m: Affine, n: &PathNode) -> PathNode {
        PathNode { p: m * n.p, h_in: m * n.h_in, h_out: m * n.h_out, ..*n }
    }

    /// Mirror a generic outline: original + reflection (reversed so that the
    /// winding direction matches and non-zero fills add up).
    pub fn apply_bez(&self, path: &BezPath) -> BezPath {
        let mut reflected = path.clone();
        reflected.apply_affine(self.affine());
        let mut out = path.clone();
        out.extend(reflected.reverse_subpaths().iter());
        out
    }

    /// Mirror an editable path (world space), merging ends on the axis.
    pub fn apply_path(&self, vp: &VectorPath) -> VectorPath {
        let m = self.affine();
        let mut out = Vec::new();
        for sp in &vp.subpaths {
            let n = sp.nodes.len();
            if n == 0 {
                continue;
            }
            let first_on = self.on_axis(sp.nodes[0].p);
            let last_on = self.on_axis(sp.nodes[n - 1].p);
            let mergeable = self.merge && !sp.closed && n >= 2 && (first_on || last_on);
            // A segment lying entirely on the axis would fold onto itself.
            let degenerate = n == 2 && first_on && last_on;
            if !mergeable || degenerate {
                out.push(sp.clone());
                out.push(SubPath {
                    nodes: reverse(&sp.nodes.iter().map(|x| self.reflect_node(m, x)).collect::<Vec<_>>()),
                    closed: sp.closed,
                });
                continue;
            }
            // Orient the original half so that its last node is on the axis.
            let half = if last_on { sp.nodes.clone() } else { reverse(&sp.nodes) };
            let mut merged = self.merge_at_end(m, &half);
            if first_on && last_on {
                // Both ends on the axis: the result ends where it started → close it.
                let end = merged.pop().unwrap();
                merged[0].h_in = end.h_in;
                merged[0].p = self.project(merged[0].p);
                out.push(SubPath { nodes: merged, closed: true });
            } else {
                out.push(SubPath { nodes: merged, closed: false });
            }
        }
        VectorPath { subpaths: out }
    }

    /// `half` ends on the axis: append its reflection, sharing the end node.
    fn merge_at_end(&self, m: Affine, half: &[PathNode]) -> Vec<PathNode> {
        let reflected = reverse(&half.iter().map(|x| self.reflect_node(m, x)).collect::<Vec<_>>());
        let mut merged = half.to_vec();
        let shared = merged.last_mut().unwrap();
        shared.p = self.project(shared.p);
        shared.h_out = reflected[0].h_out;
        merged.extend_from_slice(&reflected[1..]);
        merged
    }
}

/// Reverse a node list, swapping in/out handles so the curve is unchanged.
fn reverse(nodes: &[PathNode]) -> Vec<PathNode> {
    nodes.iter().rev().map(|n| PathNode { h_in: n.h_out, h_out: n.h_in, ..*n }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    fn node(x: f64, y: f64) -> PathNode {
        PathNode::corner(Point::new(x, y))
    }

    #[test]
    fn reflection_matrix() {
        let m = Mirror::vertical(10.0).affine();
        assert!((m * Point::new(4.0, 7.0) - Point::new(16.0, 7.0)).hypot() < 1e-9);
        let diag = Mirror::new(Point::ZERO, Point::new(1.0, 1.0)).affine();
        assert!((diag * Point::new(3.0, 1.0) - Point::new(1.0, 3.0)).hypot() < 1e-9);
    }

    #[test]
    fn half_shape_with_both_ends_on_axis_closes() {
        // Left half of a "roof": (50,0) → (0,50) → (50,100), axis x = 50.
        let vp = VectorPath {
            subpaths: vec![SubPath { nodes: vec![node(50.0, 0.0), node(0.0, 50.0), node(50.0, 100.0)], closed: false }],
        };
        let out = Mirror::vertical(50.0).apply_path(&vp);
        assert_eq!(out.subpaths.len(), 1);
        let sp = &out.subpaths[0];
        assert!(sp.closed);
        assert_eq!(sp.nodes.len(), 4, "the two axis points are shared");
        assert!((out.to_bezpath().area().abs() - 5000.0).abs() < 1e-6);
    }

    #[test]
    fn one_end_on_axis_makes_one_open_path() {
        let vp =
            VectorPath { subpaths: vec![SubPath { nodes: vec![node(50.0, 10.0), node(20.0, 30.0)], closed: false }] };
        let out = Mirror::vertical(50.0).apply_path(&vp);
        assert_eq!(out.subpaths.len(), 1);
        let xs: Vec<f64> = out.subpaths[0].nodes.iter().map(|n| n.p.x).collect();
        assert_eq!(xs, vec![20.0, 50.0, 80.0]);
    }

    #[test]
    fn nearly_on_axis_snaps_and_curves_stay_smooth() {
        let mut a = node(49.8, 0.0);
        a.h_out = Point::new(30.0, 0.0);
        let mut b = node(10.0, 40.0);
        b.h_in = Point::new(10.0, 20.0);
        let vp = VectorPath { subpaths: vec![SubPath { nodes: vec![a, b], closed: false }] };
        let out = Mirror::vertical(50.0).apply_path(&vp);
        let shared =
            out.subpaths[0].nodes.iter().find(|n| (n.p.x - 50.0).abs() < 1e-9).expect("shared node on the axis");
        // Mirrored handles are collinear through the shared node.
        let (i, o) = (shared.h_in - shared.p, shared.h_out - shared.p);
        assert!((i.x + o.x).abs() < 1e-9 && (i.y - o.y).abs() < 1e-9);
    }

    #[test]
    fn closed_and_off_axis_paths_are_duplicated() {
        let vp = VectorPath {
            subpaths: vec![SubPath { nodes: vec![node(0.0, 0.0), node(10.0, 0.0), node(10.0, 10.0)], closed: true }],
        };
        let out = Mirror::vertical(50.0).apply_path(&vp);
        assert_eq!(out.subpaths.len(), 2);
        // Same winding for both halves so non-zero fills add up.
        let a = VectorPath { subpaths: vec![out.subpaths[0].clone()] }.to_bezpath().area();
        let b = VectorPath { subpaths: vec![out.subpaths[1].clone()] }.to_bezpath().area();
        assert!(a * b > 0.0);
    }
}
