//! Document model: a project holds an artboard and a stack of layers; each
//! layer holds objects (index 0 = bottom). Every object is local geometry +
//! an affine transform + style.

use crate::boolean::{self, BoolOp};
use crate::color::Color;
use crate::effects::{self, Effect};
use crate::image::ImageAsset;
use crate::mirror::Mirror;
use crate::palette;
use crate::path::VectorPath;
use crate::stroke::{self, LineCap, LineJoin};
use crate::text::{FontLibrary, TextSpec};
use kurbo::{Affine, BezPath, Point, Rect, RoundedRect, RoundedRectRadii, Shape as _, Vec2};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;

pub type Id = u64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    /// Origin at top-left.
    Rect {
        w: f64,
        h: f64,
        radius: f64,
        /// Independent corner radii (top-left, top-right, bottom-right,
        /// bottom-left); `radius` is used when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        corners: Option<[f64; 4]>,
    },
    /// Origin at top-left of the bounding box.
    Ellipse {
        w: f64,
        h: f64,
    },
    /// Regular polygon centered on the origin.
    Polygon {
        radius: f64,
        sides: u32,
    },
    /// Star centered on the origin.
    Star {
        outer: f64,
        inner: f64,
        points: u32,
    },
    /// Wave running across its box, origin at top-left. Open wavy line, or
    /// with `filled` the top edge of a closed shape (water, banner) whose
    /// wave takes `depth` (0–1) of the height.
    Wave {
        w: f64,
        h: f64,
        /// Number of full periods (crests).
        waves: u32,
        #[serde(default)]
        filled: bool,
        #[serde(default = "default_wave_depth")]
        depth: f64,
    },
    /// Organic rounded shape centered on the origin: a smooth closed curve
    /// through `points` points at random distances. `variance` 0 = circle;
    /// `seed` picks the random variation.
    Blob {
        radius: f64,
        points: u32,
        variance: f64,
        seed: u32,
    },
    /// Free path (pen, pencil, lines, booleans). Nodes are in local space.
    Path(VectorPath),
    Text(TextSpec),
    /// Embedded raster image (see [`Document::images`]), origin at top-left.
    /// Always kept axis-aligned: only moves and resizes apply.
    Image {
        asset: Id,
        w: f64,
        h: f64,
    },
}

impl Shape {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Shape::Rect { .. } => "Rectangle",
            Shape::Ellipse { .. } => "Ellipse",
            Shape::Polygon { .. } => "Polygone",
            Shape::Star { .. } => "Étoile",
            Shape::Wave { .. } => "Vague",
            Shape::Blob { .. } => "Blob",
            Shape::Path(_) => "Tracé",
            Shape::Text(_) => "Texte",
            Shape::Image { .. } => "Image",
        }
    }

    pub fn local_path(&self, fonts: &FontLibrary) -> BezPath {
        match self {
            Shape::Rect { w, h, radius, corners } => {
                let r = Rect::new(0.0, 0.0, *w, *h).abs();
                let max_r = r.width().min(r.height()) * 0.5;
                let [tl, tr, br, bl] = corners.unwrap_or([*radius; 4]).map(|c| c.clamp(0.0, max_r));
                if tl + tr + br + bl > 0.0 {
                    RoundedRect::from_rect(r, RoundedRectRadii::new(tl, tr, br, bl)).to_path(0.01)
                } else {
                    r.to_path(0.01)
                }
            }
            Shape::Ellipse { w, h } => {
                let r = Rect::new(0.0, 0.0, *w, *h).abs();
                kurbo::Ellipse::from_rect(r).to_path(0.01)
            }
            Shape::Polygon { radius, sides } => {
                let n = (*sides).max(3);
                polygon_path((0..n).map(|i| polar(*radius, i as f64 / n as f64)))
            }
            Shape::Star { outer, inner, points } => {
                let n = (*points).max(3) * 2;
                polygon_path((0..n).map(|i| {
                    let r = if i % 2 == 0 { *outer } else { *inner };
                    polar(r, i as f64 / n as f64)
                }))
            }
            Shape::Wave { w, h, waves, filled, depth } => wave_path(*w, *h, *waves, *filled, *depth),
            Shape::Blob { radius, points, variance, seed } => blob_path(*radius, *points, *variance, *seed),
            Shape::Path(vp) => vp.to_bezpath(),
            Shape::Text(spec) => fonts.outline(spec),
            Shape::Image { w, h, .. } => Rect::new(0.0, 0.0, *w, *h).to_path(0.01),
        }
    }

    pub fn is_closed_area(&self) -> bool {
        match self {
            Shape::Path(vp) => vp.subpaths.iter().any(|s| s.closed),
            Shape::Wave { filled, .. } => *filled,
            _ => true,
        }
    }
}

fn polar(r: f64, turn: f64) -> Point {
    let a = turn * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
    Point::new(r * a.cos(), r * a.sin())
}

pub const DEFAULT_WAVE_DEPTH: f64 = 0.35;

fn default_wave_depth() -> f64 {
    DEFAULT_WAVE_DEPTH
}

fn wave_path(w: f64, h: f64, waves: u32, filled: bool, depth: f64) -> BezPath {
    let halves = waves.clamp(1, 100) * 2;
    let amp = if filled { h * depth.clamp(0.05, 1.0) } else { h };
    let dx = w / halves as f64;
    // Horizontal handles of this length make each half period match a sine
    // (same slope at the zero crossing).
    let k = dx * (std::f64::consts::PI - 2.0) / std::f64::consts::PI;
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    for i in 0..halves {
        let x0 = i as f64 * dx;
        let (y0, y1) = if i % 2 == 0 { (0.0, amp) } else { (amp, 0.0) };
        p.curve_to((x0 + k, y0), (x0 + dx - k, y1), (x0 + dx, y1));
    }
    if filled {
        p.line_to((w, h));
        p.line_to((0.0, h));
        p.close_path();
    }
    p
}

fn blob_path(radius: f64, points: u32, variance: f64, seed: u32) -> BezPath {
    let n = points.clamp(3, 64) as usize;
    let variance = variance.clamp(0.0, 0.9);
    let mut state = u64::from(seed);
    let mut rand = move || {
        // splitmix64: tiny, deterministic, good enough for shapes.
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    };
    let pts: Vec<Point> = (0..n)
        .map(|i| {
            let shift = (rand() - 0.5) * variance * 0.6;
            let r = radius * (1.0 - variance * rand());
            polar(r, (i as f64 + shift) / n as f64)
        })
        .collect();
    // Catmull-Rom through the points, with handles scaled so that equal
    // distances give a circle.
    let step = std::f64::consts::PI / n as f64;
    let f = 4.0 * (step / 2.0).tan() / (2.0 * step).sin() / 6.0;
    let at = |i: usize| pts[i % n];
    let mut p = BezPath::new();
    p.move_to(pts[0]);
    for i in 0..n {
        let (prev, a, b, next) = (at(i + n - 1), at(i), at(i + 1), at(i + 2));
        p.curve_to(a + (b - prev) * f, b - (next - a) * f, b);
    }
    p.close_path();
    p
}

/// A seed for new random shapes (blobs).
pub fn random_seed() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.finish() as u32
}

fn polygon_path(points: impl Iterator<Item = Point>) -> BezPath {
    let mut p = BezPath::new();
    for (i, pt) in points.enumerate() {
        if i == 0 {
            p.move_to(pt);
        } else {
            p.line_to(pt);
        }
    }
    p.close_path();
    p
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Color,
    pub width: f64,
    #[serde(default)]
    pub cap: LineCap,
    #[serde(default)]
    pub join: LineJoin,
}

impl Default for Stroke {
    fn default() -> Self {
        Self { color: Color::BLACK, width: 4.0, cap: LineCap::Round, join: LineJoin::Round }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: Id,
    pub name: String,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    pub shape: Shape,
    pub transform: Affine,
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub fill_rule: FillRule,
    /// Reference (tracing template): shown on the canvas, never exported.
    #[serde(default)]
    pub reference: bool,
    /// Live mirror: the object is drawn together with its reflection.
    #[serde(default)]
    pub mirror: Option<Mirror>,
    /// Live effects, applied in order after the mirror (not to images).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
    /// Gradient painting the fill instead of the plain `fill` color (kept
    /// to come back to it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
}

/// A color at a position (0..1) along a gradient.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Color,
}

/// Fill gradient over the shape's bounding box: linear along `angle`
/// (degrees, 0 = left to right, 90 = top to bottom), or radial from the
/// center.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub kind: GradientKind,
    pub angle: f64,
    pub stops: Vec<GradientStop>,
}

impl Gradient {
    /// Two-stop gradient from `a` to `b`.
    pub fn two(kind: GradientKind, a: Color, b: Color) -> Self {
        Self {
            kind,
            angle: 90.0,
            stops: vec![GradientStop { offset: 0.0, color: a }, GradientStop { offset: 1.0, color: b }],
        }
    }

    /// A gradient from `c` to a lighter (or, for light colors, darker)
    /// shade of it.
    pub fn from_color(c: Color) -> Self {
        let light = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
        let (target, t) = if light < 0.6 { (1.0, 0.55) } else { (0.0, 0.45) };
        let mix = |v: f32| v + (target - v) * t;
        Self::two(GradientKind::Linear, c, Color { r: mix(c.r), g: mix(c.g), b: mix(c.b), a: c.a })
    }

    /// Index of the first (`end` false) or last stop by position.
    pub fn end_stop(&self, end: bool) -> Option<usize> {
        let it = self.stops.iter().enumerate();
        if end {
            it.max_by(|a, b| a.1.offset.total_cmp(&b.1.offset)).map(|(i, _)| i)
        } else {
            it.min_by(|a, b| a.1.offset.total_cmp(&b.1.offset)).map(|(i, _)| i)
        }
    }

    /// Swap the colors of the first and last stops.
    pub fn reverse(&mut self) {
        for s in &mut self.stops {
            s.offset = 1.0 - s.offset;
        }
    }

    /// Stops sorted by position.
    pub fn sorted_stops(&self) -> Vec<GradientStop> {
        let mut s = self.stops.clone();
        s.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        s
    }

    /// Linear: the gradient line (start, end) across `bounds` along the
    /// angle, long enough for the corners to get the end colors.
    pub fn line(&self, bounds: kurbo::Rect) -> (kurbo::Point, kurbo::Point) {
        let a = self.angle.to_radians();
        let d = kurbo::Vec2::new(a.cos(), a.sin());
        let half = (bounds.width() * d.x.abs() + bounds.height() * d.y.abs()) / 2.0;
        let c = bounds.center();
        (c - d * half, c + d * half)
    }
}

fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}

/// World-space geometry of an object, ready to render or export.
#[derive(Clone, Debug, Default)]
pub struct Rendered {
    /// Fill area (empty if no fill).
    pub fill: BezPath,
    /// Stroke outline to be filled with the non-zero rule (empty if no stroke).
    pub stroke: BezPath,
    /// The object's centerline path.
    pub path: BezPath,
}

impl Object {
    /// The object's own geometry, without its mirror half.
    pub fn base_world_path(&self, fonts: &FontLibrary) -> BezPath {
        let mut p = self.shape.local_path(fonts);
        p.apply_affine(self.transform);
        p
    }

    fn active_mirror(&self) -> Option<&Mirror> {
        self.mirror.as_ref().filter(|_| !self.is_image())
    }

    /// Editable path in world space including the mirror (paths only).
    pub fn world_vector(&self) -> Option<VectorPath> {
        let Shape::Path(vp) = &self.shape else { return None };
        let mut vp = vp.clone();
        vp.transform(self.transform);
        Some(match self.active_mirror() {
            Some(m) => m.apply_path(&vp),
            None => vp,
        })
    }

    /// Full geometry, mirror half included.
    pub fn world_path(&self, fonts: &FontLibrary) -> BezPath {
        if let Some(vp) = self.world_vector() {
            return vp.to_bezpath();
        }
        let base = self.base_world_path(fonts);
        match self.active_mirror() {
            Some(m) => m.apply_bez(&base),
            None => base,
        }
    }

    /// Even-odd fill only without a mirror: overlapping mirror halves would cancel out.
    pub fn fills_even_odd(&self) -> bool {
        self.fill_rule == FillRule::EvenOdd && self.active_mirror().is_none()
    }

    /// Whether the (mirrored) geometry encloses an area.
    pub fn is_closed_area(&self) -> bool {
        match self.world_vector() {
            Some(vp) => vp.subpaths.iter().any(|s| s.closed),
            None => self.shape.is_closed_area(),
        }
    }

    /// Bake the mirror into the geometry.
    pub fn apply_mirror(&mut self, fonts: &FontLibrary) {
        if self.active_mirror().is_none() {
            return;
        }
        let vp = self.world_vector().unwrap_or_else(|| VectorPath::from_bezpath(&self.world_path(fonts)));
        self.shape = Shape::Path(vp);
        self.transform = Affine::IDENTITY;
        self.mirror = None;
    }

    /// Whether enabled effects change the displayed geometry.
    pub fn has_effects(&self) -> bool {
        !self.is_image() && self.effects.iter().any(|e| e.enabled && !e.kind.is_render())
    }

    /// Whether enabled render effects (shadows, glows, blur, noise) are on.
    pub fn has_render_effects(&self) -> bool {
        !self.is_image() && self.effects.iter().any(|e| e.enabled && e.kind.is_render())
    }

    /// The gradient painting the fill, if any (a gradient needs a fill).
    pub fn fill_gradient(&self) -> Option<&Gradient> {
        self.gradient.as_ref().filter(|_| self.fill.is_some() && !self.is_image())
    }

    /// Whether the object needs pixels to be drawn: gradient or render
    /// effects.
    pub fn needs_pixels(&self) -> bool {
        self.fill_gradient().is_some() || self.has_render_effects()
    }

    /// Geometry as displayed: mirror, then effects.
    pub fn display_path(&self, fonts: &FontLibrary) -> BezPath {
        let path = self.world_path(fonts);
        if self.has_effects() { effects::apply_all(&self.effects, path) } else { path }
    }

    /// Whether the stroke is drawn with per-node widths (not after effects,
    /// which reshape the path).
    pub fn variable_stroke(&self) -> bool {
        matches!(&self.shape, Shape::Path(vp) if vp.has_variable_width()) && !self.has_effects()
    }

    /// Bake the mirror and the enabled effects up to `upto` (included) into
    /// a plain path; later and disabled effects stay live.
    pub fn apply_effects(&mut self, upto: usize, fonts: &FontLibrary) {
        if self.is_image() || upto >= self.effects.len() {
            return;
        }
        let mut baked = self.clone();
        baked.effects = self.effects[..=upto].to_vec();
        let path = baked.display_path(fonts);
        let is_text = matches!(self.shape, Shape::Text(_));
        // Render effects cannot become a path: they stay live.
        let rest: Vec<Effect> = self
            .effects
            .iter()
            .enumerate()
            .filter(|(i, e)| *i > upto || !e.enabled || e.kind.is_render())
            .map(|(_, e)| *e)
            .collect();
        self.shape = Shape::Path(VectorPath::from_bezpath(&path));
        self.transform = Affine::IDENTITY;
        self.mirror = None;
        self.effects = rest;
        if is_text {
            self.fill_rule = FillRule::NonZero;
        }
    }

    pub fn render(&self, fonts: &FontLibrary) -> Rendered {
        let path = self.display_path(fonts);
        let fill = if self.fill.is_some() { path.clone() } else { BezPath::new() };
        let stroke = match &self.stroke {
            Some(s) if s.width > 0.0 => self.stroke_outline(&path, s),
            _ => BezPath::new(),
        };
        Rendered { fill, stroke, path }
    }

    fn stroke_outline(&self, world: &BezPath, s: &Stroke) -> BezPath {
        if self.variable_stroke()
            && let Some(vp) = self.world_vector()
        {
            return stroke::variable_outline(&vp, s.width, s.cap);
        }
        stroke::uniform_outline(world, s.width, s.cap, s.join)
    }

    /// Area covered by the object (used by boolean operations).
    pub fn area(&self, fonts: &FontLibrary) -> BezPath {
        let r = self.render(fonts);
        let closed = self.is_closed_area();
        let mut area = if self.fill.is_some() && closed { r.fill } else { BezPath::new() };
        if !r.stroke.is_empty() && (area.is_empty() || !closed) {
            area.extend(r.stroke.iter());
        }
        if area.is_empty() {
            area = r.path;
        }
        area
    }

    pub fn bounds(&self, fonts: &FontLibrary) -> Rect {
        let p = self.world_path(fonts);
        if p.is_empty() {
            return Rect::from_origin_size(self.transform * Point::ORIGIN, (0.0, 0.0));
        }
        p.bounding_box()
    }

    /// Bounds including stroke thickness.
    pub fn visual_bounds(&self, fonts: &FontLibrary) -> Rect {
        let r = self.render(fonts);
        let mut b = if r.path.is_empty() { self.bounds(fonts) } else { r.path.bounding_box() };
        if !r.stroke.is_empty() {
            b = b.union(r.stroke.bounding_box());
        }
        b
    }

    pub fn hit(&self, fonts: &FontLibrary, pt: Point, tolerance: f64) -> bool {
        self.hit_rendered(&self.render(fonts), pt, tolerance)
    }

    /// Hit test against already rendered geometry (see [`Object::render`]).
    pub fn hit_rendered(&self, r: &Rendered, pt: Point, tolerance: f64) -> bool {
        if self.fill.is_some() && r.fill.winding(pt) != 0 {
            return true;
        }
        if !r.stroke.is_empty() && r.stroke.winding(pt) != 0 {
            return true;
        }
        if matches!(self.shape, Shape::Text(_) | Shape::Image { .. })
            && r.path.bounding_box().inflate(2.0, 2.0).contains(pt)
        {
            return true;
        }
        let tol2 = tolerance * tolerance;
        r.path.segments().any(|s| kurbo::ParamCurveNearest::nearest(&s, pt, 0.1).distance_sq <= tol2)
    }

    /// Apply a world-space transform. Paths bake it into their nodes so they
    /// stay directly editable; parametric shapes keep it in `transform` and
    /// absorb pure axis-aligned scaling into their dimensions.
    pub fn apply_transform(&mut self, t: Affine) {
        // The mirror axis belongs to the object: it moves with it.
        if let Some(m) = &mut self.mirror {
            m.transform(t);
        }
        self.transform = t * self.transform;
        self.normalize();
    }

    pub fn normalize(&mut self) {
        let [a, b, c, d, e, f] = self.transform.as_coeffs();
        let axis_aligned = b.abs() < 1e-9 && c.abs() < 1e-9;
        match &mut self.shape {
            Shape::Path(vp) => {
                vp.transform(self.transform);
                self.transform = Affine::IDENTITY;
            }
            Shape::Rect { w, h, radius, corners } if axis_aligned => {
                *w *= a;
                *h *= d;
                let k = a.abs().min(d.abs());
                *radius *= k;
                if let Some(c) = corners {
                    for r in c.iter_mut() {
                        *r *= k;
                    }
                    // A flip swaps which corner is where.
                    if *w < 0.0 {
                        *c = [c[1], c[0], c[3], c[2]];
                    }
                    if *h < 0.0 {
                        *c = [c[3], c[2], c[1], c[0]];
                    }
                }
                // Keep width/height positive by moving the origin.
                let mut ox = e;
                let mut oy = f;
                if *w < 0.0 {
                    ox += *w;
                    *w = -*w;
                }
                if *h < 0.0 {
                    oy += *h;
                    *h = -*h;
                }
                self.transform = Affine::translate((ox, oy));
            }
            Shape::Ellipse { w, h } if axis_aligned => {
                *w *= a;
                *h *= d;
                let mut ox = e;
                let mut oy = f;
                if *w < 0.0 {
                    ox += *w;
                    *w = -*w;
                }
                if *h < 0.0 {
                    oy += *h;
                    *h = -*h;
                }
                self.transform = Affine::translate((ox, oy));
            }
            // A wave reads the same mirrored left/right (whole periods), not
            // upside down: a vertical flip stays in the transform.
            Shape::Wave { w, h, .. } if axis_aligned && d > 0.0 => {
                *w *= a.abs();
                *h *= d;
                let ox = if a < 0.0 { e - *w } else { e };
                self.transform = Affine::translate((ox, f));
            }
            Shape::Image { w, h, .. } => {
                // Images stay upright and unflipped: keep the transformed bounds.
                let r = self.transform.transform_rect_bbox(Rect::new(0.0, 0.0, *w, *h));
                *w = r.width();
                *h = r.height();
                self.transform = Affine::translate((r.x0, r.y0));
            }
            _ => {}
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self.shape, Shape::Image { .. })
    }

    /// Replace the shape by an editable path with the same look.
    pub fn convert_to_path(&mut self, fonts: &FontLibrary) {
        if matches!(self.shape, Shape::Path(_) | Shape::Image { .. }) {
            return;
        }
        let is_text = matches!(self.shape, Shape::Text(_));
        let world = self.base_world_path(fonts);
        self.shape = Shape::Path(VectorPath::from_bezpath(&world));
        self.transform = Affine::IDENTITY;
        if is_text {
            self.fill_rule = FillRule::NonZero;
        }
    }

    /// Turn the stroke into a filled shape.
    pub fn outline_stroke(&mut self, fonts: &FontLibrary) {
        let Some(s) = self.stroke else { return };
        if self.is_image() {
            return;
        }
        let outline = boolean::simplify(&self.render(fonts).stroke);
        self.shape = Shape::Path(VectorPath::from_bezpath(&outline));
        self.transform = Affine::IDENTITY;
        self.fill = Some(s.color);
        self.stroke = None;
        self.fill_rule = FillRule::EvenOdd;
        // The outline already went through the mirror and the effects.
        self.mirror = None;
        self.effects.clear();
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: Id,
    pub name: String,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    pub objects: Vec<Object>,
    /// Layer-wide live mirror: every object of the layer (drawn before or
    /// after) gets reflected, unless it has its own mirror. Images and
    /// references are not mirrored.
    #[serde(default)]
    pub mirror: Option<Mirror>,
}

impl Layer {
    pub fn new(id: Id, name: impl Into<String>) -> Self {
        Self { id, name: name.into(), visible: true, locked: false, objects: Vec::new(), mirror: None }
    }

    /// Whether the layer mirror applies to this object.
    fn mirrors(&self, o: &Object) -> bool {
        self.mirror.is_some() && o.mirror.is_none() && !o.is_image() && !o.reference
    }

    /// The object as displayed: with the layer mirror when it applies.
    pub fn effective<'a>(&self, o: &'a Object) -> Cow<'a, Object> {
        if self.mirrors(o) {
            let mut o = o.clone();
            o.mirror = self.mirror;
            Cow::Owned(o)
        } else {
            Cow::Borrowed(o)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub background: Option<Color>,
    pub layers: Vec<Layer>,
    pub next_id: Id,
    /// Embedded images, referenced by [`Shape::Image`].
    #[serde(default)]
    pub images: BTreeMap<Id, ImageAsset>,
    /// Project color palette (swatches), in display order.
    #[serde(default)]
    pub palette: Vec<Color>,
    /// Saved gradients, shown after the palette colors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette_gradients: Vec<Gradient>,
}

/// Address of an object inside the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loc {
    pub layer: usize,
    pub index: usize,
}

impl Document {
    pub const VERSION: u32 = 1;

    pub fn new(name: impl Into<String>, width: f64, height: f64) -> Self {
        let mut doc = Self {
            version: Self::VERSION,
            name: name.into(),
            width,
            height,
            background: Some(Color::WHITE),
            layers: Vec::new(),
            next_id: 1,
            images: BTreeMap::new(),
            palette: palette::default_palette(),
            palette_gradients: Vec::new(),
        };
        let id = doc.alloc_id();
        doc.layers.push(Layer::new(id, "Calque 1"));
        doc
    }

    pub fn artboard(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width, self.height)
    }

    pub fn alloc_id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_layer(&mut self, name: Option<String>) -> Id {
        let id = self.alloc_id();
        let name = name.unwrap_or_else(|| format!("Calque {}", self.layers.len() + 1));
        self.layers.push(Layer::new(id, name));
        id
    }

    pub fn layer_index(&self, id: Id) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    pub fn find(&self, id: Id) -> Option<Loc> {
        self.layers
            .iter()
            .enumerate()
            .find_map(|(li, l)| l.objects.iter().position(|o| o.id == id).map(|index| Loc { layer: li, index }))
    }

    pub fn get(&self, id: Id) -> Option<&Object> {
        let loc = self.find(id)?;
        Some(&self.layers[loc.layer].objects[loc.index])
    }

    pub fn get_mut(&mut self, id: Id) -> Option<&mut Object> {
        let loc = self.find(id)?;
        Some(&mut self.layers[loc.layer].objects[loc.index])
    }

    /// Insert an object on top of the given layer; returns its id.
    pub fn insert(&mut self, layer: usize, mut obj: Object) -> Id {
        obj.id = self.alloc_id();
        let id = obj.id;
        let li = layer.min(self.layers.len().saturating_sub(1));
        self.layers[li].objects.push(obj);
        id
    }

    pub fn remove(&mut self, id: Id) -> Option<Object> {
        let loc = self.find(id)?;
        Some(self.layers[loc.layer].objects.remove(loc.index))
    }

    /// An object as displayed (layer mirror included).
    pub fn effective(&self, id: Id) -> Option<Cow<'_, Object>> {
        let loc = self.find(id)?;
        let l = &self.layers[loc.layer];
        Some(l.effective(&l.objects[loc.index]))
    }

    /// The mirror acting on an object: its own, else its layer's.
    pub fn mirror_of(&self, id: Id) -> Option<Mirror> {
        let loc = self.find(id)?;
        let l = &self.layers[loc.layer];
        let o = &l.objects[loc.index];
        if o.is_image() {
            return None;
        }
        o.mirror.or(if l.mirrors(o) { l.mirror } else { None })
    }

    /// Visible objects bottom-to-top, as displayed (layer mirrors included).
    pub fn visible_effective(&self) -> impl Iterator<Item = Cow<'_, Object>> {
        self.layers
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| l.objects.iter().filter(|o| o.visible).map(move |o| l.effective(o)))
    }

    /// Axes of every visible mirror (layers and objects).
    pub fn visible_mirrors(&self) -> Vec<Mirror> {
        let mut out: Vec<Mirror> = Vec::new();
        for l in self.layers.iter().filter(|l| l.visible) {
            let own = l.objects.iter().filter(|o| o.visible && !o.is_image()).filter_map(|o| o.mirror);
            for m in l.mirror.into_iter().chain(own) {
                if !out.iter().any(|x| x.a == m.a && x.b == m.b) {
                    out.push(m);
                }
            }
        }
        out
    }

    /// Bake a layer's mirror into its objects and remove it.
    pub fn apply_layer_mirror(&mut self, li: usize, fonts: &FontLibrary) {
        let Some(l) = self.layers.get_mut(li) else { return };
        let Some(m) = l.mirror.take() else { return };
        for o in &mut l.objects {
            if o.mirror.is_none() && !o.is_image() && !o.reference {
                o.mirror = Some(m);
                o.apply_mirror(fonts);
            }
        }
    }

    /// All objects bottom-to-top.
    pub fn objects(&self) -> impl Iterator<Item = &Object> {
        self.layers.iter().flat_map(|l| l.objects.iter())
    }

    /// Visible objects bottom-to-top.
    pub fn visible_objects(&self) -> impl Iterator<Item = &Object> {
        self.layers.iter().filter(|l| l.visible).flat_map(|l| l.objects.iter().filter(|o| o.visible))
    }

    pub fn is_editable(&self, id: Id) -> bool {
        let Some(loc) = self.find(id) else { return false };
        let l = &self.layers[loc.layer];
        let o = &l.objects[loc.index];
        l.visible && !l.locked && o.visible && !o.locked
    }

    /// Topmost editable object under the point.
    pub fn hit_test(&self, fonts: &FontLibrary, pt: Point, tolerance: f64) -> Option<Id> {
        self.hit_test_with(|o| o.hit(fonts, pt, tolerance))
    }

    /// Topmost editable object (as displayed) for which `hit` is true.
    pub fn hit_test_with(&self, hit: impl Fn(&Object) -> bool) -> Option<Id> {
        for l in self.layers.iter().rev() {
            if !l.visible || l.locked {
                continue;
            }
            for o in l.objects.iter().rev() {
                if o.visible && !o.locked && hit(&l.effective(o)) {
                    return Some(o.id);
                }
            }
        }
        None
    }

    /// Editable objects whose bounds intersect the rectangle.
    pub fn objects_in_rect(&self, fonts: &FontLibrary, r: Rect) -> Vec<Id> {
        self.layers
            .iter()
            .filter(|l| l.visible && !l.locked)
            .flat_map(|l| l.objects.iter().map(move |o| (l, o)))
            .filter(|(_, o)| o.visible && !o.locked)
            .filter(|(l, o)| {
                let b = l.effective(o).bounds(fonts);
                b.intersect(r).area() > 0.0 || (r.contains(b.origin()) && r.contains(Point::new(b.x1, b.y1)))
            })
            .map(|(_, o)| o.id)
            .collect()
    }

    /// Sort ids by z-order (bottom first).
    pub fn sort_by_z(&self, ids: &[Id]) -> Vec<Id> {
        let mut locs: Vec<(Loc, Id)> = ids.iter().filter_map(|&id| self.find(id).map(|l| (l, id))).collect();
        locs.sort_by_key(|(l, _)| (l.layer, l.index));
        locs.into_iter().map(|(_, id)| id).collect()
    }

    pub fn selection_bounds(&self, fonts: &FontLibrary, ids: &[Id]) -> Option<Rect> {
        ids.iter().filter_map(|&id| self.effective(id)).map(|o| o.bounds(fonts)).reduce(|a, b| a.union(b))
    }

    pub fn transform_objects(&mut self, ids: &[Id], t: Affine) {
        for &id in ids {
            if let Some(o) = self.get_mut(id) {
                o.apply_transform(t);
            }
        }
    }

    pub fn translate_objects(&mut self, ids: &[Id], d: Vec2) {
        self.transform_objects(ids, Affine::translate(d));
    }

    /// Combine objects with a boolean operation. The result takes the place
    /// and style of the bottom-most object. Returns the new object id.
    pub fn boolean(&mut self, fonts: &FontLibrary, ids: &[Id], op: BoolOp) -> Option<Id> {
        let ids = self.sort_by_z(ids);
        if ids.len() < 2 {
            return None;
        }
        // Within one layer the result stays under the layer mirror, so the
        // halves are combined; across layers, combine what is displayed.
        let same_layer = ids.iter().filter_map(|&id| self.find(id)).map(|l| l.layer).collect::<Vec<_>>();
        let same_layer = same_layer.windows(2).all(|w| w[0] == w[1]);
        let areas: Vec<BezPath> = ids
            .iter()
            .filter_map(|&id| if same_layer { self.get(id).map(Cow::Borrowed) } else { self.effective(id) })
            .map(|o| o.area(fonts))
            .collect();
        let result = boolean::apply(op, &areas);
        let base_loc = self.find(ids[0])?;
        let base = self.get(ids[0])?.clone();
        let mut obj = base.clone();
        // Strokes of open lines become area: their color becomes the fill.
        if obj.fill.is_none() || !base.is_closed_area() {
            obj.fill = obj.stroke.map(|s| s.color).or(obj.fill).or(Some(Color::BLACK));
            obj.stroke = None;
        }
        obj.shape = Shape::Path(VectorPath::from_bezpath(&result));
        obj.transform = Affine::IDENTITY;
        obj.fill_rule = FillRule::EvenOdd;
        // The result already contains the mirrored halves and the effects.
        obj.mirror = None;
        obj.effects.clear();
        obj.name = match op {
            BoolOp::Union => "Union",
            BoolOp::Subtract => "Soustraction",
            BoolOp::Intersect => "Intersection",
            BoolOp::Exclude => "Exclusion",
        }
        .into();
        for &id in &ids[1..] {
            self.remove(id);
        }
        let new_id = self.alloc_id();
        obj.id = new_id;
        // Base index is unaffected by removals above it in the same layer, but
        // objects below may have been removed from that layer: re-find base.
        let loc = self.find(base.id).unwrap_or(base_loc);
        self.layers[loc.layer].objects[loc.index] = obj;
        Some(new_id)
    }

    /// Store an image (deduplicated by content) and return its asset id.
    pub fn add_image_asset(&mut self, asset: ImageAsset) -> Id {
        if let Some((id, _)) = self.images.iter().find(|(_, a)| a.data == asset.data) {
            return *id;
        }
        let id = self.alloc_id();
        self.images.insert(id, asset);
        id
    }

    /// Drop images no object uses any more.
    pub fn remove_unused_images(&mut self) {
        let used: std::collections::HashSet<Id> = self
            .objects()
            .filter_map(|o| match o.shape {
                Shape::Image { asset, .. } => Some(asset),
                _ => None,
            })
            .collect();
        self.images.retain(|id, _| used.contains(id));
    }

    /// Add a swatch unless the palette already has it. Returns whether it was added.
    pub fn palette_add(&mut self, color: Color) -> bool {
        if self.palette.iter().any(|c| palette::same(*c, color)) {
            return false;
        }
        self.palette.push(color);
        true
    }

    /// Distinct fill and stroke colors used by the objects (references
    /// excluded), bottom-to-top.
    pub fn used_colors(&self) -> Vec<Color> {
        let mut out: Vec<Color> = Vec::new();
        for o in self.objects().filter(|o| !o.reference) {
            for c in [o.fill, o.stroke.map(|s| s.color)].into_iter().flatten() {
                if !out.iter().any(|x| palette::same(*x, c)) {
                    out.push(c);
                }
            }
        }
        out
    }

    /// Bottom layer dedicated to references; created (and returned) if missing.
    pub fn reference_layer(&mut self) -> usize {
        const NAME: &str = "Références";
        if let Some(i) = self.layers.iter().position(|l| l.name == NAME) {
            return i;
        }
        let id = self.alloc_id();
        self.layers.insert(0, Layer::new(id, NAME));
        0
    }

    pub fn to_json(&self) -> anyhow::Result<String> {
        let mut doc = self.clone();
        doc.remove_unused_images();
        Ok(serde_json::to_string_pretty(&doc)?)
    }

    pub fn from_json(s: &str) -> anyhow::Result<Self> {
        let doc: Document = serde_json::from_str(s)?;
        anyhow::ensure!(doc.version <= Self::VERSION, "fichier créé par une version plus récente de Desy");
        Ok(doc)
    }
}

pub fn new_object(shape: Shape, transform: Affine, fill: Option<Color>, stroke: Option<Stroke>) -> Object {
    Object {
        id: 0,
        name: shape.kind_name().into(),
        visible: true,
        locked: false,
        shape,
        transform,
        fill,
        stroke,
        opacity: 1.0,
        fill_rule: FillRule::NonZero,
        reference: false,
        mirror: None,
        effects: Vec::new(),
        gradient: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageAsset;

    fn rect_obj(x: f64, y: f64, w: f64, h: f64) -> Object {
        new_object(
            Shape::Rect { w, h, radius: 0.0, corners: None },
            Affine::translate((x, y)),
            Some(Color::BLACK),
            None,
        )
    }

    fn assert_bbox(p: &BezPath, expect: Rect) {
        let b = p.bounding_box();
        let ok = [b.x0 - expect.x0, b.y0 - expect.y0, b.x1 - expect.x1, b.y1 - expect.y1].iter().all(|d| d.abs() < 0.5);
        assert!(ok, "{b:?} != {expect:?}");
    }

    #[test]
    fn applying_effects_keeps_render_effects_and_gradients() {
        use crate::effects::{Effect, EffectKind};
        let fonts = FontLibrary::empty();
        let mut o = rect_obj(0.0, 0.0, 100.0, 100.0);
        o.gradient = Some(Gradient::from_color(Color::BLACK));
        o.effects =
            vec![Effect::new(EffectKind::Blur { radius: 4.0 }), Effect::new(EffectKind::PuckerBloat { amount: 0.3 })];
        assert!(o.needs_pixels() && o.has_effects());
        o.apply_effects(1, &fonts);
        assert!(matches!(o.shape, Shape::Path(_)));
        assert_eq!(o.effects.len(), 1, "the blur stays live");
        assert!(o.effects[0].kind.is_render() && !o.has_effects() && o.needs_pixels());
        // No fill: the gradient has nothing to paint.
        o.fill = None;
        o.effects.clear();
        assert!(!o.needs_pixels());
    }

    #[test]
    fn palette_gradients_are_saved_and_optional() {
        let mut doc = Document::new("t", 100.0, 100.0);
        let json = serde_json::to_string(&doc).unwrap();
        assert!(!json.contains("palette_gradients"), "empty list is not written");
        assert!(serde_json::from_str::<Document>(&json).unwrap().palette_gradients.is_empty());
        doc.palette_gradients.push(Gradient::from_color(Color::BLACK));
        let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(back.palette_gradients, doc.palette_gradients);
    }

    #[test]
    fn gradient_line_spans_the_box() {
        let mut g = Gradient::from_color(Color::WHITE);
        g.angle = 0.0;
        let (a, b) = g.line(kurbo::Rect::new(0.0, 0.0, 200.0, 100.0));
        assert_eq!((a, b), (kurbo::Point::new(0.0, 50.0), kurbo::Point::new(200.0, 50.0)));
        g.reverse();
        assert_eq!(g.end_stop(false).map(|i| g.stops[i].offset), Some(0.0));
    }

    #[test]
    fn wave_fills_its_box() {
        let fonts = FontLibrary::empty();
        let line = Shape::Wave { w: 300.0, h: 40.0, waves: 3, filled: false, depth: DEFAULT_WAVE_DEPTH };
        assert!(!line.is_closed_area());
        assert_bbox(&line.local_path(&fonts), Rect::new(0.0, 0.0, 300.0, 40.0));
        // Crest, trough… crest: 3 periods = 6 half periods.
        assert_eq!(line.local_path(&fonts).segments().count(), 6);

        let water = Shape::Wave { w: 300.0, h: 100.0, waves: 2, filled: true, depth: 0.2 };
        assert!(water.is_closed_area());
        let p = water.local_path(&fonts);
        assert_bbox(&p, Rect::new(0.0, 0.0, 300.0, 100.0));
        assert!((p.area().abs() - 300.0 * 90.0).abs() < 1.0, "wave halfway down its 20 px band");
    }

    #[test]
    fn wave_keeps_vertical_flip() {
        let shape = Shape::Wave { w: 100.0, h: 50.0, waves: 2, filled: true, depth: 0.3 };
        let mut o = new_object(shape.clone(), Affine::translate((10.0, 10.0)), Some(Color::BLACK), None);
        o.apply_transform(
            Affine::translate((60.0, 0.0)) * Affine::scale_non_uniform(-2.0, 1.0) * Affine::translate((-60.0, 0.0)),
        );
        assert_eq!(o.transform, Affine::translate((-40.0, 10.0)));
        assert!(matches!(o.shape, Shape::Wave { w, .. } if (w - 200.0).abs() < 1e-9));
        o.apply_transform(Affine::FLIP_Y);
        assert!(o.transform.as_coeffs()[3] < 0.0, "upside-down wave stays upside down");
    }

    #[test]
    fn blob_is_smooth_bounded_and_stable() {
        let fonts = FontLibrary::empty();
        let circle = Shape::Blob { radius: 100.0, points: 6, variance: 0.0, seed: 1 }.local_path(&fonts);
        assert_bbox(&circle, Rect::new(-100.0, -100.0, 100.0, 100.0));
        let circle_area = std::f64::consts::PI * 100.0 * 100.0;
        assert!((circle.area().abs() - circle_area).abs() / circle_area < 0.001);

        let blob = |seed| Shape::Blob { radius: 100.0, points: 7, variance: 0.5, seed }.local_path(&fonts);
        assert_eq!(blob(42), blob(42));
        assert_ne!(blob(42), blob(43));
        let b = blob(42).bounding_box();
        assert!(b.x0 > -115.0 && b.x1 < 115.0 && b.y0 > -115.0 && b.y1 < 115.0, "{b:?}");
        assert!(b.width() > 70.0 && b.height() > 70.0, "{b:?}");
    }

    #[test]
    fn resize_bakes_into_rect() {
        let mut o = rect_obj(10.0, 10.0, 100.0, 50.0);
        let anchor = Vec2::new(10.0, 10.0);
        o.apply_transform(
            Affine::translate(anchor) * Affine::scale_non_uniform(2.0, -1.0) * Affine::translate(-anchor),
        );
        assert_eq!(o.shape, Shape::Rect { w: 200.0, h: 50.0, radius: 0.0, corners: None });
        let b = o.bounds(&FontLibrary::empty());
        assert!((b.x0 - 10.0).abs() < 1e-9 && (b.y0 + 40.0).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn independent_corners_follow_a_flip_and_old_files_load() {
        let mut o = new_object(
            Shape::Rect { w: 100.0, h: 50.0, radius: 0.0, corners: Some([1.0, 2.0, 3.0, 4.0]) },
            Affine::scale_non_uniform(-2.0, 1.0),
            Some(Color::BLACK),
            None,
        );
        o.normalize();
        // Mirrored left/right and scaled by min(|sx|, |sy|) = 1.
        assert_eq!(o.shape, Shape::Rect { w: 200.0, h: 50.0, radius: 0.0, corners: Some([2.0, 1.0, 4.0, 3.0]) });
        let old: Shape = serde_json::from_str(r#"{"Rect":{"w":10.0,"h":10.0,"radius":2.0}}"#).unwrap();
        assert_eq!(old, Shape::Rect { w: 10.0, h: 10.0, radius: 2.0, corners: None });
    }

    #[test]
    fn boolean_union_replaces_objects() {
        let fonts = FontLibrary::empty();
        let mut doc = Document::new("t", 100.0, 100.0);
        let a = doc.insert(0, rect_obj(0.0, 0.0, 10.0, 10.0));
        let b = doc.insert(0, rect_obj(5.0, 0.0, 10.0, 10.0));
        let c = doc.insert(0, rect_obj(50.0, 50.0, 1.0, 1.0));
        let u = doc.boolean(&fonts, &[b, a], BoolOp::Union).unwrap();
        let ids: Vec<Id> = doc.objects().map(|o| o.id).collect();
        assert_eq!(ids, vec![u, c]);
        assert!((doc.get(u).unwrap().world_path(&fonts).area().abs() - 150.0).abs() < 1e-6);
    }

    fn image_doc() -> (Document, Id) {
        let mut doc = Document::new("t", 100.0, 100.0);
        let asset = doc.add_image_asset(ImageAsset {
            name: "ref.png".into(),
            width: 2,
            height: 1,
            data: std::sync::Arc::new(b"\x89PNG fake".to_vec()),
        });
        let id = doc.insert(
            0,
            new_object(Shape::Image { asset, w: 200.0, h: 100.0 }, Affine::translate((10.0, 10.0)), None, None),
        );
        (doc, id)
    }

    #[test]
    fn images_roundtrip_and_dedupe() {
        let (mut doc, _) = image_doc();
        let again = doc.add_image_asset(doc.images.values().next().unwrap().clone());
        assert_eq!(doc.images.len(), 1);
        assert_eq!(Some(&again), doc.images.keys().next());
        let back = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn images_stay_upright_and_unused_ones_are_dropped() {
        let fonts = FontLibrary::empty();
        let (mut doc, id) = image_doc();
        doc.transform_objects(&[id], Affine::rotate_about(0.3, Point::new(50.0, 50.0)));
        doc.transform_objects(&[id], Affine::scale_non_uniform(-1.0, 1.0));
        let o = doc.get(id).unwrap();
        assert_eq!(o.transform.as_coeffs()[0..4], [1.0, 0.0, 0.0, 1.0]);
        assert!(o.hit(&fonts, o.bounds(&fonts).center(), 1.0));
        doc.remove(id);
        let back = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert!(back.images.is_empty());
    }

    #[test]
    fn mirror_moves_with_object_and_bakes() {
        let fonts = FontLibrary::empty();
        // Left half of a square, ends on the axis x = 50.
        let half = VectorPath {
            subpaths: vec![crate::path::SubPath {
                nodes: vec![
                    crate::PathNode::corner(Point::new(50.0, 0.0)),
                    crate::PathNode::corner(Point::new(0.0, 0.0)),
                    crate::PathNode::corner(Point::new(0.0, 100.0)),
                    crate::PathNode::corner(Point::new(50.0, 100.0)),
                ],
                closed: false,
            }],
        };
        let mut o = new_object(Shape::Path(half), Affine::IDENTITY, Some(Color::BLACK), None);
        o.mirror = Some(Mirror::vertical(50.0));
        assert!(o.is_closed_area());
        assert_eq!(o.bounds(&fonts), Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(o.hit(&fonts, Point::new(90.0, 50.0), 1.0), "mirror half is hittable");
        o.apply_transform(Affine::translate((10.0, 0.0)));
        assert_eq!(o.bounds(&fonts), Rect::new(10.0, 0.0, 110.0, 100.0));
        o.apply_mirror(&fonts);
        assert!(o.mirror.is_none());
        assert!((o.world_path(&fonts).area().abs() - 10_000.0).abs() < 1e-6);
    }

    #[test]
    fn layer_mirror_reflects_every_object_until_applied() {
        let fonts = FontLibrary::empty();
        let mut doc = Document::new("t", 100.0, 100.0);
        doc.layers[0].mirror = Some(Mirror::vertical(50.0));
        // Drawn after the axis was set: still mirrored.
        let a = doc.insert(0, rect_obj(10.0, 10.0, 10.0, 10.0));
        assert!(doc.get(a).unwrap().mirror.is_none(), "the object itself is untouched");
        assert_eq!(doc.selection_bounds(&fonts, &[a]), Some(Rect::new(10.0, 10.0, 90.0, 20.0)));
        assert_eq!(doc.hit_test(&fonts, Point::new(85.0, 15.0), 1.0), Some(a), "reflection is hittable");
        assert_eq!(doc.mirror_of(a), Some(Mirror::vertical(50.0)));
        assert!(crate::svg::export(&doc, &fonts).matches("<path").count() >= 1);
        // Another layer is not affected.
        doc.add_layer(None);
        let b = doc.insert(1, rect_obj(10.0, 30.0, 10.0, 10.0));
        assert_eq!(doc.selection_bounds(&fonts, &[b]), Some(Rect::new(10.0, 30.0, 20.0, 40.0)));
        assert_eq!(doc.visible_mirrors().len(), 1);
        // Baking leaves the same picture without the layer mirror.
        doc.apply_layer_mirror(0, &fonts);
        assert!(doc.layers[0].mirror.is_none());
        assert_eq!(doc.selection_bounds(&fonts, &[a]), Some(Rect::new(10.0, 10.0, 90.0, 20.0)));
        let back = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn effects_change_display_until_applied() {
        let fonts = FontLibrary::empty();
        let mut o = rect_obj(0.0, 0.0, 100.0, 100.0);
        let grow = Effect::new(effects::EffectKind::Offset { distance: 10.0, join: crate::stroke::LineJoin::Miter });
        let mut off = Effect::new(effects::EffectKind::Twist { angle: 90.0 });
        off.enabled = false;
        o.effects = vec![grow, off];
        // Handles stay on the editable shape; display and hits follow the effect.
        assert_eq!(o.bounds(&fonts), Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!((o.render(&fonts).path.area().abs() - 14_400.0).abs() < 1.0);
        assert!(o.hit(&fonts, Point::new(-5.0, 50.0), 0.5));
        let json = serde_json::to_string(&o).unwrap();
        assert_eq!(serde_json::from_str::<Object>(&json).unwrap(), o);
        o.apply_effects(0, &fonts);
        assert!(matches!(o.shape, Shape::Path(_)));
        assert_eq!(o.effects, vec![off], "disabled effects stay");
        assert!((o.world_path(&fonts).area().abs() - 14_400.0).abs() < 1.0);
        // Old files have no effects.
        let plain = serde_json::to_string(&rect_obj(0.0, 0.0, 1.0, 1.0)).unwrap();
        assert!(!plain.contains("effects"));
    }

    #[test]
    fn json_roundtrip() {
        let mut doc = Document::new("Logo", 512.0, 512.0);
        doc.insert(0, rect_obj(1.0, 2.0, 3.0, 4.0));
        let back = Document::from_json(&doc.to_json().unwrap()).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn palette_dedupes_and_old_files_load() {
        let mut doc = Document::new("t", 100.0, 100.0);
        assert!(!doc.palette.is_empty());
        doc.palette.clear();
        assert!(doc.palette_add(Color::from_hex("#ff0000").unwrap()));
        assert!(!doc.palette_add(Color::from_hex("#FF0000").unwrap()));
        assert!(doc.palette_add(Color::from_hex("#ff0000").unwrap().with_alpha(0.5)));
        assert_eq!(doc.palette.len(), 2);

        let mut json: serde_json::Value = serde_json::from_str(&doc.to_json().unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("palette");
        let old = Document::from_json(&json.to_string()).unwrap();
        assert!(old.palette.is_empty());
    }

    #[test]
    fn used_colors_are_distinct_and_skip_references() {
        let mut doc = Document::new("t", 100.0, 100.0);
        let red = Color::from_hex("#ff0000").unwrap();
        let mut a = rect_obj(0.0, 0.0, 1.0, 1.0);
        a.fill = Some(red);
        a.stroke = Some(Stroke { color: Color::BLACK, ..Default::default() });
        let mut b = rect_obj(0.0, 0.0, 1.0, 1.0);
        b.fill = Some(red);
        let mut r = rect_obj(0.0, 0.0, 1.0, 1.0);
        r.fill = Some(Color::WHITE);
        r.reference = true;
        for o in [a, b, r] {
            doc.insert(0, o);
        }
        assert_eq!(doc.used_colors(), vec![red, Color::BLACK]);
    }

    #[test]
    fn hit_test_respects_locks_and_order() {
        let fonts = FontLibrary::empty();
        let mut doc = Document::new("t", 100.0, 100.0);
        let a = doc.insert(0, rect_obj(0.0, 0.0, 10.0, 10.0));
        let b = doc.insert(0, rect_obj(0.0, 0.0, 10.0, 10.0));
        assert_eq!(doc.hit_test(&fonts, Point::new(5.0, 5.0), 1.0), Some(b));
        doc.get_mut(b).unwrap().locked = true;
        assert_eq!(doc.hit_test(&fonts, Point::new(5.0, 5.0), 1.0), Some(a));
        assert_eq!(doc.hit_test(&fonts, Point::new(50.0, 50.0), 1.0), None);
    }
}
