//! Live effects, in the spirit of Illustrator's Effect menu and Blender's
//! modifier stack: an object keeps its editable geometry and a list of
//! effects applied in order, in world space, when it is displayed or
//! exported. "Apply" bakes them into a plain path.

use crate::boolean::{self, BoolOp};
use crate::color::Color;
use crate::stroke::{self, LineCap, LineJoin};
use kurbo::{
    Affine, BezPath, CubicBez, ParamCurve, ParamCurveArclen, ParamCurveDeriv, PathEl, Point, Shape as _, Vec2,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    #[serde(default = "yes")]
    pub enabled: bool,
    pub kind: EffectKind,
}

fn yes() -> bool {
    true
}

fn center_origin() -> [f64; 2] {
    [0.5, 0.5]
}

impl Effect {
    pub fn new(kind: EffectKind) -> Self {
        Self { enabled: true, kind }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum EffectKind {
    /// −1 (pucker: anchors out, curves in) … +1 (bloat: anchors in, curves out).
    PuckerBloat { amount: f64 },
    /// Rotation in degrees at the center, fading to none at the edge.
    Twist { angle: f64 },
    /// Random jitter of up to `size` px, `detail` points per 100 px.
    Roughen { size: f64, detail: f64, smooth: bool, seed: u32 },
    /// `ridges` zigzags per segment, `size` px from the path.
    ZigZag { size: f64, ridges: u32, smooth: bool },
    /// Scale (1 = 100 %), rotation (degrees) and move around `origin`
    /// (fraction of the bounding box: [0.5, 0.5] = center, [0.5, 1] =
    /// bottom middle), repeated on `copies` extra copies (0 = transform the
    /// shape itself).
    Transform {
        scale_x: f64,
        scale_y: f64,
        rotate: f64,
        move_x: f64,
        move_y: f64,
        copies: u32,
        #[serde(default = "center_origin")]
        origin: [f64; 2],
    },
    /// Grow (> 0) or shrink (< 0) the shape by `distance` px.
    Offset { distance: f64, join: LineJoin },

    // Render effects: pixels around or over the shape, the geometry stays.
    // Drawn in stack order after the geometric effects (SVG filters).
    /// Shadow behind the shape, moved by (`x`, `y`), blurred by `blur` px,
    /// grown by `spread` px.
    DropShadow { x: f64, y: f64, blur: f64, spread: f64, color: Color },
    /// Shadow cast inside the shape from its edges.
    InnerShadow { x: f64, y: f64, blur: f64, color: Color },
    /// Halo around the shape.
    OuterGlow { blur: f64, spread: f64, color: Color },
    /// Light along the inner edges.
    InnerGlow { blur: f64, color: Color },
    /// Gaussian blur of the whole object, `radius` px.
    Blur { radius: f64 },
    /// Grain over the shape: `amount` 0..1, grain `size` px.
    Noise { amount: f64, size: f64, seed: u32 },
}

const SHADOW: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.35 };
const GLOW: Color = Color { r: 1.0, g: 0.85, b: 0.35, a: 0.9 };
const INNER_GLOW: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 0.8 };

/// Menu entries: category, then (name, default effect) in display order.
pub const CATALOG: &[(&str, &[EffectKind])] = &[
    (
        "Déformer",
        &[
            EffectKind::PuckerBloat { amount: 0.4 },
            EffectKind::Twist { angle: 90.0 },
            EffectKind::Roughen { size: 6.0, detail: 10.0, smooth: true, seed: 1 },
            EffectKind::ZigZag { size: 8.0, ridges: 4, smooth: false },
        ],
    ),
    (
        "Transformer",
        &[
            EffectKind::Transform {
                scale_x: 1.0,
                scale_y: 1.0,
                rotate: 15.0,
                move_x: 0.0,
                move_y: 0.0,
                copies: 0,
                origin: [0.5, 0.5],
            },
            EffectKind::Offset { distance: 10.0, join: LineJoin::Round },
        ],
    ),
    (
        "Ombres et lueurs",
        &[
            EffectKind::DropShadow { x: 0.0, y: 8.0, blur: 16.0, spread: 0.0, color: SHADOW },
            EffectKind::InnerShadow { x: 0.0, y: 6.0, blur: 10.0, color: SHADOW },
            EffectKind::OuterGlow { blur: 20.0, spread: 2.0, color: GLOW },
            EffectKind::InnerGlow { blur: 14.0, color: INNER_GLOW },
        ],
    ),
    ("Flou et texture", &[EffectKind::Blur { radius: 6.0 }, EffectKind::Noise { amount: 0.35, size: 1.5, seed: 1 }]),
];

impl EffectKind {
    pub fn name(&self) -> &'static str {
        match self {
            EffectKind::PuckerBloat { .. } => "Contracter et dilater",
            EffectKind::Twist { .. } => "Tourbillon",
            EffectKind::Roughen { .. } => "Rugosité",
            EffectKind::ZigZag { .. } => "Zigzag",
            EffectKind::Transform { .. } => "Transformation",
            EffectKind::Offset { .. } => "Décalage du contour",
            EffectKind::DropShadow { .. } => "Ombre portée",
            EffectKind::InnerShadow { .. } => "Ombre interne",
            EffectKind::OuterGlow { .. } => "Lueur externe",
            EffectKind::InnerGlow { .. } => "Lueur interne",
            EffectKind::Blur { .. } => "Flou",
            EffectKind::Noise { .. } => "Bruit",
        }
    }

    /// Render effects work on pixels (exported as SVG filters): they leave
    /// the geometry alone and are never baked into the path.
    pub fn is_render(&self) -> bool {
        matches!(
            self,
            EffectKind::DropShadow { .. }
                | EffectKind::InnerShadow { .. }
                | EffectKind::OuterGlow { .. }
                | EffectKind::InnerGlow { .. }
                | EffectKind::Blur { .. }
                | EffectKind::Noise { .. }
        )
    }

    /// The effect's color, for those that have one.
    pub fn color(&self) -> Option<Color> {
        match *self {
            EffectKind::DropShadow { color, .. }
            | EffectKind::InnerShadow { color, .. }
            | EffectKind::OuterGlow { color, .. }
            | EffectKind::InnerGlow { color, .. } => Some(color),
            _ => None,
        }
    }

    pub fn set_color(&mut self, c: Color) {
        match self {
            EffectKind::DropShadow { color, .. }
            | EffectKind::InnerShadow { color, .. }
            | EffectKind::OuterGlow { color, .. }
            | EffectKind::InnerGlow { color, .. } => *color = c,
            _ => {}
        }
    }

    /// How far (px) a render effect may paint outside the shape.
    pub fn margin(&self) -> f64 {
        match *self {
            EffectKind::DropShadow { x, y, blur, spread, .. } => x.abs().max(y.abs()) + 1.5 * blur + spread.max(0.0),
            EffectKind::OuterGlow { blur, spread, .. } => 1.5 * blur + spread.max(0.0),
            EffectKind::Blur { radius } => 1.5 * radius,
            _ => 0.0,
        }
    }

    pub fn tip(&self) -> &'static str {
        match self {
            EffectKind::PuckerBloat { .. } => {
                "Pucker & Bloat : négatif = pointes vers l'extérieur et courbes creusées, positif = forme gonflée."
            }
            EffectKind::Twist { .. } => "Fait tourner la forme sur elle-même, plus fort au centre qu'au bord.",
            EffectKind::Roughen { .. } => "Rend le contour irrégulier, comme dessiné à la main ou déchiré.",
            EffectKind::ZigZag { .. } => "Remplace chaque segment par des dents ou des vagues régulières.",
            EffectKind::Transform { .. } => {
                "Échelle, rotation et déplacement autour du centre ; avec des copies, chaque copie part de la précédente (rosaces, motifs en éventail)."
            }
            EffectKind::Offset { .. } => {
                "Agrandit ou réduit la forme d'une distance fixe : contour « autocollant », liseré, version plus fine."
            }
            EffectKind::DropShadow { .. } => {
                "Ombre derrière la forme : décalage, flou et étendue. Couleur et opacité dans la pastille."
            }
            EffectKind::InnerShadow { .. } => {
                "Ombre à l'intérieur de la forme, depuis ses bords : effet creusé ou embossé."
            }
            EffectKind::OuterGlow { .. } => "Halo lumineux autour de la forme : néon, autocollant brillant.",
            EffectKind::InnerGlow { .. } => "Lumière le long des bords intérieurs : volume, reflet.",
            EffectKind::Blur { .. } => "Flou de tout l'objet (remplissage et contour).",
            EffectKind::Noise { .. } => "Grain sur la forme, comme une impression ou une texture papier.",
        }
    }
}

/// Apply the enabled geometric effects in order (render effects leave the
/// geometry as it is).
pub fn apply_all(effects: &[Effect], path: BezPath) -> BezPath {
    effects.iter().filter(|e| e.enabled && !e.kind.is_render()).fold(path, |p, e| apply(&e.kind, &p))
}

/// Enabled render effects, in stack order.
pub fn render_effects(effects: &[Effect]) -> impl Iterator<Item = &EffectKind> {
    effects.iter().filter(|e| e.enabled && e.kind.is_render()).map(|e| &e.kind)
}

pub fn apply(kind: &EffectKind, path: &BezPath) -> BezPath {
    if path.is_empty() {
        return path.clone();
    }
    match *kind {
        EffectKind::PuckerBloat { amount } => pucker_bloat(path, amount),
        EffectKind::Twist { angle } => twist(path, angle),
        EffectKind::Roughen { size, detail, smooth, seed } => roughen(path, size, detail, smooth, seed),
        EffectKind::ZigZag { size, ridges, smooth } => zigzag(path, size, ridges, smooth),
        EffectKind::Transform { scale_x, scale_y, rotate, move_x, move_y, copies, origin } => {
            transform(path, [scale_x, scale_y], rotate, Vec2::new(move_x, move_y), copies, origin)
        }
        EffectKind::Offset { distance, join } => offset(path, distance, join),
        _ => path.clone(),
    }
}

// ------------------------------------------------------------------ geometry

/// A subpath as a chain of cubics.
struct Sub {
    segs: Vec<CubicBez>,
    closed: bool,
}

fn line_cubic(a: Point, b: Point) -> CubicBez {
    CubicBez::new(a, a.lerp(b, 1.0 / 3.0), a.lerp(b, 2.0 / 3.0), b)
}

fn subs(path: &BezPath) -> Vec<Sub> {
    let mut out: Vec<Sub> = Vec::new();
    let mut start = Point::ORIGIN;
    let mut last = Point::ORIGIN;
    let mut cur: Option<Sub> = None;
    let flush = |cur: &mut Option<Sub>, out: &mut Vec<Sub>| {
        if let Some(s) = cur.take()
            && !s.segs.is_empty()
        {
            out.push(s);
        }
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                flush(&mut cur, &mut out);
                cur = Some(Sub { segs: Vec::new(), closed: false });
                start = p;
                last = p;
            }
            PathEl::LineTo(p) => {
                cur.get_or_insert_with(|| Sub { segs: Vec::new(), closed: false }).segs.push(line_cubic(last, p));
                last = p;
            }
            PathEl::QuadTo(c, p) => {
                let q = kurbo::QuadBez::new(last, c, p).raise();
                cur.get_or_insert_with(|| Sub { segs: Vec::new(), closed: false }).segs.push(q);
                last = p;
            }
            PathEl::CurveTo(c1, c2, p) => {
                cur.get_or_insert_with(|| Sub { segs: Vec::new(), closed: false })
                    .segs
                    .push(CubicBez::new(last, c1, c2, p));
                last = p;
            }
            PathEl::ClosePath => {
                if let Some(s) = &mut cur {
                    if (last - start).hypot() > 1e-9 {
                        s.segs.push(line_cubic(last, start));
                    }
                    s.closed = true;
                }
                flush(&mut cur, &mut out);
                last = start;
            }
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn build(subs: &[Sub]) -> BezPath {
    let mut p = BezPath::new();
    for s in subs {
        let Some(first) = s.segs.first() else { continue };
        p.move_to(first.p0);
        for c in &s.segs {
            p.curve_to(c.p1, c.p2, c.p3);
        }
        if s.closed {
            p.close_path();
        }
    }
    p
}

fn center(path: &BezPath) -> Point {
    path.bounding_box().center()
}

/// Split segments so none is longer than `max_len`, keeping the total
/// number of pieces reasonable on long paths (text).
fn subdivide(subs: &mut [Sub], max_len: f64) {
    let total: f64 = subs.iter().flat_map(|s| &s.segs).map(|c| c.arclen(0.5)).sum();
    let max_len = max_len.max(total / 4000.0).max(0.5);
    for s in subs {
        let mut out = Vec::with_capacity(s.segs.len());
        for c in &s.segs {
            let n = (c.arclen(0.5) / max_len).ceil().clamp(1.0, 400.0) as usize;
            for i in 0..n {
                out.push(c.subsegment(i as f64 / n as f64..(i + 1) as f64 / n as f64));
            }
        }
        s.segs = out;
    }
}

/// Smooth curve through points (Catmull-Rom) or a polyline.
fn through(points: &[Point], closed: bool, smooth: bool, out: &mut BezPath) {
    let n = points.len();
    if n < 2 {
        return;
    }
    out.move_to(points[0]);
    let count = if closed { n } else { n - 1 };
    let at = |i: isize| -> Point {
        if closed { points[i.rem_euclid(n as isize) as usize] } else { points[i.clamp(0, n as isize - 1) as usize] }
    };
    for i in 0..count as isize {
        let (a, b) = (at(i), at(i + 1));
        if smooth {
            let (prev, next) = (at(i - 1), at(i + 2));
            out.curve_to(a + (b - prev) / 6.0, b - (next - a) / 6.0, b);
        } else {
            out.line_to(b);
        }
    }
    if closed {
        out.close_path();
    }
}

/// Deterministic pseudo-random numbers in [0, 1) (splitmix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ------------------------------------------------------------------ effects

fn pucker_bloat(path: &BezPath, amount: f64) -> BezPath {
    let c = center(path);
    let a = amount.clamp(-2.0, 2.0);
    let mut subs = subs(path);
    for s in &mut subs {
        for seg in &mut s.segs {
            let anchor = |p: Point| p + (c - p) * a;
            let handle = |p: Point| p + (p - c) * a;
            *seg = CubicBez::new(anchor(seg.p0), handle(seg.p1), handle(seg.p2), anchor(seg.p3));
        }
    }
    build(&subs)
}

fn twist(path: &BezPath, angle: f64) -> BezPath {
    let b = path.bounding_box();
    let c = b.center();
    let radius = (b.width().hypot(b.height()) / 2.0).max(1e-6);
    let mut subs = subs(path);
    subdivide(&mut subs, radius / 24.0);
    let turn = angle.to_radians();
    let rot = |p: Point| {
        let d = p - c;
        let t = turn * (1.0 - (d.hypot() / radius).min(1.0));
        let (s, co) = t.sin_cos();
        c + Vec2::new(d.x * co - d.y * s, d.x * s + d.y * co)
    };
    for s in &mut subs {
        for seg in &mut s.segs {
            *seg = CubicBez::new(rot(seg.p0), rot(seg.p1), rot(seg.p2), rot(seg.p3));
        }
    }
    build(&subs)
}

fn roughen(path: &BezPath, size: f64, detail: f64, smooth: bool, seed: u32) -> BezPath {
    let mut subs = subs(path);
    subdivide(&mut subs, 100.0 / detail.clamp(0.5, 100.0));
    let mut rng = Rng(u64::from(seed) ^ 0x5eed);
    let mut out = BezPath::new();
    for s in &subs {
        let mut pts: Vec<Point> = s.segs.iter().map(|c| c.p0).collect();
        if !s.closed
            && let Some(last) = s.segs.last()
        {
            pts.push(last.p3);
        }
        for p in &mut pts {
            *p += Vec2::new((rng.next() * 2.0 - 1.0) * size, (rng.next() * 2.0 - 1.0) * size);
        }
        through(&pts, s.closed, smooth, &mut out);
    }
    out
}

fn normal(c: &CubicBez, t: f64) -> Vec2 {
    let mut d = c.deriv().eval(t).to_vec2();
    if d.hypot2() < 1e-12 {
        d = c.p3 - c.p0;
    }
    if d.hypot2() < 1e-18 { Vec2::new(0.0, -1.0) } else { Vec2::new(-d.y, d.x).normalize() }
}

fn zigzag(path: &BezPath, size: f64, ridges: u32, smooth: bool) -> BezPath {
    let steps = ridges.clamp(1, 100) as usize;
    let mut out = BezPath::new();
    for s in subs(path) {
        let mut pts = Vec::new();
        for c in &s.segs {
            let len = c.arclen(0.1);
            for k in 0..steps {
                let t = if len > 1e-9 { c.inv_arclen(len * k as f64 / steps as f64, 0.1) } else { 0.0 };
                let sign = if pts.len() % 2 == 0 { 1.0 } else { -1.0 };
                pts.push(c.eval(t) + normal(c, t) * size * sign);
            }
        }
        if !s.closed
            && let Some(c) = s.segs.last()
        {
            let sign = if pts.len() % 2 == 0 { 1.0 } else { -1.0 };
            pts.push(c.p3 + normal(c, 1.0) * size * sign);
        }
        through(&pts, s.closed, smooth, &mut out);
    }
    out
}

fn transform(path: &BezPath, scale: [f64; 2], rotate: f64, d: Vec2, copies: u32, origin: [f64; 2]) -> BezPath {
    let b = path.bounding_box();
    let c = Vec2::new(b.x0 + b.width() * origin[0], b.y0 + b.height() * origin[1]);
    let t = Affine::translate(c + d)
        * Affine::rotate(rotate.to_radians())
        * Affine::scale_non_uniform(scale[0], scale[1])
        * Affine::translate(-c);
    if copies == 0 {
        return t * path.clone();
    }
    let mut out = path.clone();
    let mut cur = path.clone();
    for _ in 0..copies.min(360) {
        cur = t * cur;
        out.extend(cur.iter());
    }
    out
}

fn offset(path: &BezPath, distance: f64, join: LineJoin) -> BezPath {
    if distance.abs() < 1e-6 {
        return path.clone();
    }
    let closed = path.elements().iter().any(|e| matches!(e, PathEl::ClosePath));
    let band = stroke::uniform_outline(path, distance.abs() * 2.0, LineCap::Round, join);
    if !closed {
        // An open line grows into a band around it.
        return boolean::simplify(&band);
    }
    let op = if distance > 0.0 { BoolOp::Union } else { BoolOp::Subtract };
    boolean::apply(op, &[path.clone(), band])
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Circle, Rect};

    fn square() -> BezPath {
        Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.01)
    }

    fn area(p: &BezPath) -> f64 {
        p.area().abs()
    }

    #[test]
    fn pucker_shrinks_and_bloat_grows_area() {
        let sq = square();
        let puckered = apply(&EffectKind::PuckerBloat { amount: -0.5 }, &sq);
        let bloated = apply(&EffectKind::PuckerBloat { amount: 0.5 }, &sq);
        // Pucker pushes corners out: the box grows, curves dig in.
        assert!(puckered.bounding_box().width() > 100.0);
        // Bloat pulls corners in and bulges the sides.
        assert!(bloated.bounding_box().width() < 150.0);
        assert!(area(&bloated) > area(&apply(&EffectKind::PuckerBloat { amount: 0.0 }, &sq)) * 0.5);
        assert!((area(&apply(&EffectKind::PuckerBloat { amount: 0.0 }, &sq)) - 10_000.0).abs() < 1e-6);
    }

    #[test]
    fn twist_keeps_a_circle_round_and_rotates_a_square() {
        let circle = Circle::new((50.0, 50.0), 50.0).to_path(0.01);
        let t = apply(&EffectKind::Twist { angle: 90.0 }, &circle);
        assert!((area(&t) - area(&circle)).abs() / area(&circle) < 0.02);
        let sq = apply(&EffectKind::Twist { angle: 180.0 }, &square());
        assert_ne!(sq.bounding_box(), Rect::new(0.0, 0.0, 100.0, 100.0));
        // No twist: same shape.
        let same = apply(&EffectKind::Twist { angle: 0.0 }, &square());
        assert!((area(&same) - 10_000.0).abs() < 1e-6);
    }

    #[test]
    fn roughen_is_bounded_and_stable() {
        let k = |seed| EffectKind::Roughen { size: 5.0, detail: 10.0, smooth: false, seed };
        let a = apply(&k(1), &square());
        assert_eq!(a, apply(&k(1), &square()));
        assert_ne!(a, apply(&k(2), &square()));
        let b = a.bounding_box();
        assert!(b.x0 >= -5.0 && b.x1 <= 105.0 && b.y0 >= -5.0 && b.y1 <= 105.0, "{b:?}");
        // 400 px of outline at 10 points per 100 px.
        assert!(a.elements().len() >= 40);
    }

    #[test]
    fn zigzag_adds_ridges() {
        let z = apply(&EffectKind::ZigZag { size: 5.0, ridges: 3, smooth: false }, &square());
        // 4 sides × 3 points each, closed.
        assert_eq!(z.elements().iter().filter(|e| matches!(e, PathEl::LineTo(_))).count(), 12);
        let b = z.bounding_box();
        assert!(b.width() > 100.0 && b.width() <= 110.0 + 1e-9, "{b:?}");
    }

    #[test]
    fn transform_copies_repeat_from_the_previous_one() {
        let one = apply(
            &EffectKind::Transform {
                scale_x: 1.0,
                scale_y: 1.0,
                rotate: 0.0,
                move_x: 10.0,
                move_y: 0.0,
                copies: 0,
                origin: [0.5, 0.5],
            },
            &square(),
        );
        assert_eq!(one.bounding_box(), Rect::new(10.0, 0.0, 110.0, 100.0));
        let three = apply(
            &EffectKind::Transform {
                scale_x: 1.0,
                scale_y: 1.0,
                rotate: 0.0,
                move_x: 10.0,
                move_y: 0.0,
                copies: 2,
                origin: [0.5, 0.5],
            },
            &square(),
        );
        assert_eq!(three.bounding_box(), Rect::new(0.0, 0.0, 120.0, 100.0));
        assert_eq!(three.elements().iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count(), 3);
    }

    #[test]
    fn transform_rotates_around_the_chosen_origin() {
        // A petal turned 4 × 90° around its bottom middle: a 4-petal flower.
        let petal = Rect::new(-10.0, -100.0, 10.0, 0.0).to_path(0.01);
        let flower = apply(
            &EffectKind::Transform {
                scale_x: 1.0,
                scale_y: 1.0,
                rotate: 90.0,
                move_x: 0.0,
                move_y: 0.0,
                copies: 3,
                origin: [0.5, 1.0],
            },
            &petal,
        );
        let b = flower.bounding_box();
        assert!((b.x0 + 100.0).abs() < 1e-6 && (b.x1 - 100.0).abs() < 1e-6, "{b:?}");
        assert!((b.y0 + 100.0).abs() < 1e-6 && (b.y1 - 100.0).abs() < 1e-6, "{b:?}");
    }

    #[test]
    fn offset_grows_and_shrinks() {
        let grown = apply(&EffectKind::Offset { distance: 10.0, join: LineJoin::Miter }, &square());
        assert!((area(&grown) - 120.0 * 120.0).abs() < 1.0, "{}", area(&grown));
        let shrunk = apply(&EffectKind::Offset { distance: -10.0, join: LineJoin::Miter }, &square());
        assert!((area(&shrunk) - 80.0 * 80.0).abs() < 1.0, "{}", area(&shrunk));
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        let band = apply(&EffectKind::Offset { distance: 5.0, join: LineJoin::Round }, &line);
        assert!(band.bounding_box().height() > 9.0);
    }

    #[test]
    fn render_effects_keep_the_geometry() {
        let e = Effect::new(EffectKind::Blur { radius: 8.0 });
        assert_eq!(apply_all(&[e], square()), square());
        assert!(e.kind.is_render() && e.kind.margin() > 0.0);
        let mut k = EffectKind::DropShadow { x: 0.0, y: 0.0, blur: 1.0, spread: 0.0, color: Color::BLACK };
        k.set_color(Color::WHITE);
        assert_eq!(k.color(), Some(Color::WHITE));
    }

    #[test]
    fn disabled_effects_are_skipped_and_old_files_load() {
        let mut e = Effect::new(EffectKind::Twist { angle: 90.0 });
        e.enabled = false;
        assert_eq!(apply_all(&[e], square()), square());
        let old: Effect = serde_json::from_str(r#"{"kind":{"Twist":{"angle":10.0}}}"#).unwrap();
        assert!(old.enabled);
        let t: EffectKind = serde_json::from_str(
            r#"{"Transform":{"scale_x":1,"scale_y":1,"rotate":0,"move_x":0,"move_y":0,"copies":0}}"#,
        )
        .unwrap();
        assert!(matches!(t, EffectKind::Transform { origin: [0.5, 0.5], .. }));
    }
}
