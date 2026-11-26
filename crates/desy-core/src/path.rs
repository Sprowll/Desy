//! Editable vector paths: anchor points with Bézier handles and a per-point
//! stroke width factor (used for variable-width / tapered strokes).

use kurbo::{BezPath, CubicBez, Line, ParamCurve, PathEl, PathSeg, Point, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// Handles move independently.
    #[default]
    Corner,
    /// Handles stay collinear (lengths independent).
    Smooth,
    /// Handles stay collinear and equal length.
    Symmetric,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathNode {
    pub p: Point,
    /// Incoming handle, absolute. Equal to `p` when absent.
    pub h_in: Point,
    /// Outgoing handle, absolute. Equal to `p` when absent.
    pub h_out: Point,
    #[serde(default)]
    pub kind: NodeKind,
    /// Stroke width multiplier at this node (1.0 = the object's stroke width).
    #[serde(default = "one")]
    pub width: f64,
}

fn one() -> f64 {
    1.0
}

impl PathNode {
    pub fn corner(p: Point) -> Self {
        Self { p, h_in: p, h_out: p, kind: NodeKind::Corner, width: 1.0 }
    }

    pub fn has_in(&self) -> bool {
        self.h_in.distance_squared(self.p) > 1e-12
    }

    pub fn has_out(&self) -> bool {
        self.h_out.distance_squared(self.p) > 1e-12
    }

    pub fn translate(&mut self, d: Vec2) {
        self.p += d;
        self.h_in += d;
        self.h_out += d;
    }

    /// Move the outgoing handle and update the incoming one according to the node kind.
    pub fn set_out(&mut self, h: Point) {
        self.h_out = h;
        self.mirror_into_in();
    }

    /// Move the incoming handle and update the outgoing one according to the node kind.
    pub fn set_in(&mut self, h: Point) {
        self.h_in = h;
        self.mirror_into_out();
    }

    fn mirror_into_in(&mut self) {
        let d = self.h_out - self.p;
        match self.kind {
            NodeKind::Corner => {}
            NodeKind::Symmetric => self.h_in = self.p - d,
            NodeKind::Smooth => {
                let len = (self.h_in - self.p).hypot();
                if d.hypot() > 1e-9 {
                    self.h_in = self.p - d.normalize() * len;
                }
            }
        }
    }

    fn mirror_into_out(&mut self) {
        let d = self.h_in - self.p;
        match self.kind {
            NodeKind::Corner => {}
            NodeKind::Symmetric => self.h_out = self.p - d,
            NodeKind::Smooth => {
                let len = (self.h_out - self.p).hypot();
                if d.hypot() > 1e-9 {
                    self.h_out = self.p - d.normalize() * len;
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SubPath {
    pub nodes: Vec<PathNode>,
    pub closed: bool,
}

/// One drawable segment of a subpath, with the width factors at its ends.
#[derive(Clone, Copy, Debug)]
pub struct WidthSeg {
    pub seg: PathSeg,
    pub w0: f64,
    pub w1: f64,
    /// Index of the start node; the end node is `(start + 1) % len`.
    pub start: usize,
}

impl SubPath {
    pub fn segment(a: &PathNode, b: &PathNode) -> PathSeg {
        if !a.has_out() && !b.has_in() {
            PathSeg::Line(Line::new(a.p, b.p))
        } else {
            PathSeg::Cubic(CubicBez::new(a.p, a.h_out, b.h_in, b.p))
        }
    }

    pub fn width_segments(&self) -> Vec<WidthSeg> {
        let n = self.nodes.len();
        if n < 2 {
            return Vec::new();
        }
        let count = if self.closed { n } else { n - 1 };
        (0..count)
            .map(|i| {
                let a = &self.nodes[i];
                let b = &self.nodes[(i + 1) % n];
                WidthSeg { seg: Self::segment(a, b), w0: a.width, w1: b.width, start: i }
            })
            .collect()
    }

    pub fn append_to(&self, out: &mut BezPath) {
        let Some(first) = self.nodes.first() else { return };
        out.move_to(first.p);
        for ws in self.width_segments() {
            match ws.seg {
                PathSeg::Line(l) => out.line_to(l.p1),
                PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
                PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
            }
        }
        if self.closed {
            out.close_path();
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VectorPath {
    pub subpaths: Vec<SubPath>,
}

impl VectorPath {
    pub fn to_bezpath(&self) -> BezPath {
        let mut out = BezPath::new();
        for sp in &self.subpaths {
            sp.append_to(&mut out);
        }
        out
    }

    pub fn from_bezpath(path: &BezPath) -> Self {
        let mut subpaths: Vec<SubPath> = Vec::new();
        let mut cur: Option<SubPath> = None;
        let finish = |sp: Option<SubPath>, list: &mut Vec<SubPath>| {
            if let Some(sp) = sp
                && !sp.nodes.is_empty()
            {
                list.push(sp);
            }
        };
        for el in path.elements() {
            match *el {
                PathEl::MoveTo(p) => {
                    finish(cur.take(), &mut subpaths);
                    cur = Some(SubPath { nodes: vec![PathNode::corner(p)], closed: false });
                }
                PathEl::LineTo(p) => {
                    let sp = cur.get_or_insert_with(SubPath::default);
                    sp.nodes.push(PathNode::corner(p));
                }
                PathEl::QuadTo(c, p) => {
                    let sp = cur.get_or_insert_with(SubPath::default);
                    let p0 = sp.nodes.last().map(|n| n.p).unwrap_or(c);
                    let c1 = p0 + (c - p0) * (2.0 / 3.0);
                    let c2 = p + (c - p) * (2.0 / 3.0);
                    if let Some(last) = sp.nodes.last_mut() {
                        last.h_out = c1;
                    }
                    let mut n = PathNode::corner(p);
                    n.h_in = c2;
                    sp.nodes.push(n);
                }
                PathEl::CurveTo(c1, c2, p) => {
                    let sp = cur.get_or_insert_with(SubPath::default);
                    if let Some(last) = sp.nodes.last_mut() {
                        last.h_out = c1;
                    }
                    let mut n = PathNode::corner(p);
                    n.h_in = c2;
                    sp.nodes.push(n);
                }
                PathEl::ClosePath => {
                    if let Some(mut sp) = cur.take() {
                        sp.closed = true;
                        // Merge a duplicated end point into the start node.
                        if sp.nodes.len() > 1 {
                            let first = sp.nodes[0].p;
                            let last = *sp.nodes.last().unwrap();
                            if last.p.distance(first) < 1e-6 {
                                sp.nodes.pop();
                                sp.nodes[0].h_in = last.h_in;
                            }
                        }
                        // A following segment without MoveTo starts at the same point.
                        let start = sp.nodes[0].p;
                        subpaths.push(sp);
                        cur = Some(SubPath { nodes: vec![PathNode::corner(start)], closed: false });
                    }
                }
            }
        }
        // A lone point left after a ClosePath is not a real subpath.
        if let Some(sp) = cur.take()
            && sp.nodes.len() > 1
        {
            subpaths.push(sp);
        }
        let mut vp = VectorPath { subpaths };
        vp.infer_kinds();
        vp
    }

    /// Mark nodes whose handles are collinear as smooth.
    pub fn infer_kinds(&mut self) {
        for sp in &mut self.subpaths {
            for n in &mut sp.nodes {
                if n.has_in() && n.has_out() {
                    let a = (n.p - n.h_in).normalize();
                    let b = (n.h_out - n.p).normalize();
                    if a.dot(b) > 0.999 {
                        n.kind = NodeKind::Smooth;
                    }
                }
            }
        }
    }

    pub fn transform(&mut self, a: kurbo::Affine) {
        for sp in &mut self.subpaths {
            for n in &mut sp.nodes {
                n.p = a * n.p;
                n.h_in = a * n.h_in;
                n.h_out = a * n.h_out;
            }
        }
    }

    pub fn has_variable_width(&self) -> bool {
        self.subpaths.iter().flat_map(|s| &s.nodes).any(|n| (n.width - 1.0).abs() > 1e-6)
    }

    pub fn node_count(&self) -> usize {
        self.subpaths.iter().map(|s| s.nodes.len()).sum()
    }

    /// Insert a node on segment `seg_start` of subpath `sp` at parameter `t`,
    /// splitting the curve so the shape is unchanged. Returns the new node index.
    pub fn insert_node(&mut self, sp: usize, seg_start: usize, t: f64) -> Option<usize> {
        let sub = self.subpaths.get_mut(sp)?;
        let n = sub.nodes.len();
        if n < 2 {
            return None;
        }
        let a = sub.nodes[seg_start];
        let bi = (seg_start + 1) % n;
        let b = sub.nodes[bi];
        let width = a.width + (b.width - a.width) * t;
        let new = match SubPath::segment(&a, &b) {
            PathSeg::Line(l) => {
                let mut nn = PathNode::corner(l.eval(t));
                nn.width = width;
                nn
            }
            PathSeg::Cubic(c) => {
                let (l, r) = c.subdivide_at(t);
                sub.nodes[seg_start].h_out = l.p1;
                sub.nodes[bi].h_in = r.p2;
                PathNode { p: l.p3, h_in: l.p2, h_out: r.p1, kind: NodeKind::Smooth, width }
            }
            PathSeg::Quad(_) => unreachable!(),
        };
        let idx = seg_start + 1;
        sub.nodes.insert(idx, new);
        Some(idx)
    }
}

trait SubdivideAt {
    fn subdivide_at(&self, t: f64) -> (CubicBez, CubicBez);
}

impl SubdivideAt for CubicBez {
    fn subdivide_at(&self, t: f64) -> (CubicBez, CubicBez) {
        (self.subsegment(0.0..t), self.subsegment(t..1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    #[test]
    fn roundtrip_closed_curve() {
        let circle = kurbo::Circle::new((50.0, 50.0), 20.0).to_path(0.1);
        let vp = VectorPath::from_bezpath(&circle);
        assert_eq!(vp.subpaths.len(), 1);
        assert!(vp.subpaths[0].closed);
        assert_eq!(vp.subpaths[0].nodes.len(), 4);
        assert!(vp.subpaths[0].nodes.iter().all(|n| n.kind == NodeKind::Smooth));
        let back = vp.to_bezpath();
        assert!((back.area() - circle.area()).abs() < 1e-6);
    }

    #[test]
    fn roundtrip_rect_lines() {
        let rect = kurbo::Rect::new(0.0, 0.0, 10.0, 20.0).to_path(0.1);
        let vp = VectorPath::from_bezpath(&rect);
        assert_eq!(vp.subpaths[0].nodes.len(), 4);
        assert!((vp.to_bezpath().area().abs() - 200.0).abs() < 1e-9);
    }

    #[test]
    fn insert_node_keeps_shape() {
        let circle = kurbo::Circle::new((0.0, 0.0), 10.0).to_path(0.1);
        let mut vp = VectorPath::from_bezpath(&circle);
        let before = vp.to_bezpath().area();
        vp.insert_node(0, 1, 0.5).unwrap();
        assert_eq!(vp.subpaths[0].nodes.len(), 5);
        assert!((vp.to_bezpath().area() - before).abs() < 1e-6);
    }

    #[test]
    fn symmetric_handles_mirror() {
        let mut n = PathNode::corner(Point::new(0.0, 0.0));
        n.kind = NodeKind::Symmetric;
        n.set_out(Point::new(5.0, 0.0));
        assert_eq!(n.h_in, Point::new(-5.0, 0.0));
    }
}
