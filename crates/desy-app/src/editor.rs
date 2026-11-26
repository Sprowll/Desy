//! Editor state and every canvas interaction (tools, selection, node editing,
//! snapping). Rendering lives in `canvas.rs` and `panels.rs`.

use crate::icons::Icon;
use desy_core::document::{Rendered, new_object};
use desy_core::history::History;
use desy_core::snap::{self, Guide};
use desy_core::stroke::LineCap;
use desy_core::{
    BoolOp, Color, Document, FontLibrary, Id, NodeKind, Object, PathNode, Shape, Stroke, SubPath, TextSpec, VectorPath,
};
use kurbo::{Affine, ParamCurveNearest, Point, Rect, Shape as _, Vec2};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Direct,
    Pen,
    Pencil,
    Line,
    Rect,
    Ellipse,
    Polygon,
    Star,
    Wave,
    Blob,
    Text,
    Width,
    Mirror,
    Eyedropper,
    Hand,
}

impl Tool {
    #[cfg(test)]
    pub const ALL: [Tool; 16] = [
        Tool::Select,
        Tool::Direct,
        Tool::Pen,
        Tool::Pencil,
        Tool::Width,
        Tool::Mirror,
        Tool::Line,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Polygon,
        Tool::Star,
        Tool::Wave,
        Tool::Blob,
        Tool::Text,
        Tool::Eyedropper,
        Tool::Hand,
    ];

    pub fn icon(self) -> Icon {
        match self {
            Tool::Select => Icon::Select,
            Tool::Direct => Icon::Direct,
            Tool::Pen => Icon::Pen,
            Tool::Pencil => Icon::Pencil,
            Tool::Line => Icon::Line,
            Tool::Rect => Icon::Rect,
            Tool::Ellipse => Icon::Ellipse,
            Tool::Polygon => Icon::Polygon,
            Tool::Star => Icon::Star,
            Tool::Wave => Icon::Wave,
            Tool::Blob => Icon::Blob,
            Tool::Text => Icon::Text,
            Tool::Width => Icon::Width,
            Tool::Mirror => Icon::Mirror,
            Tool::Eyedropper => Icon::Eyedropper,
            Tool::Hand => Icon::Hand,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Sélection (V)",
            Tool::Direct => "Sélection directe — points (A)",
            Tool::Pen => "Plume (P)",
            Tool::Pencil => "Crayon (N)",
            Tool::Line => "Ligne (L)",
            Tool::Rect => "Rectangle (M)",
            Tool::Ellipse => "Ellipse (E)",
            Tool::Polygon => "Polygone (Y)",
            Tool::Star => "Étoile (S)",
            Tool::Wave => "Vague (U)",
            Tool::Blob => "Blob — forme organique (B)",
            Tool::Text => "Texte (T)",
            Tool::Width => "Largeur — épaisseur par point (W)",
            Tool::Mirror => "Symétrie du calque — trace l'axe miroir (X)",
            Tool::Eyedropper => "Pipette — prélever une couleur (I)",
            Tool::Hand => "Main (H / Espace)",
        }
    }

    /// Short name for menus.
    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Sélection",
            Tool::Direct => "Sélection directe",
            Tool::Pen => "Plume",
            Tool::Pencil => "Crayon",
            Tool::Line => "Ligne",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Polygon => "Polygone",
            Tool::Star => "Étoile",
            Tool::Wave => "Vague",
            Tool::Blob => "Blob",
            Tool::Text => "Texte",
            Tool::Width => "Largeur",
            Tool::Mirror => "Symétrie",
            Tool::Eyedropper => "Pipette",
            Tool::Hand => "Main",
        }
    }

    /// Keyboard shortcut shown in menus.
    pub fn key(self) -> &'static str {
        match self {
            Tool::Select => "V",
            Tool::Direct => "A",
            Tool::Pen => "P",
            Tool::Pencil => "N",
            Tool::Line => "L",
            Tool::Rect => "M",
            Tool::Ellipse => "E",
            Tool::Polygon => "Y",
            Tool::Star => "S",
            Tool::Wave => "U",
            Tool::Blob => "B",
            Tool::Text => "T",
            Tool::Width => "W",
            Tool::Mirror => "X",
            Tool::Eyedropper => "I",
            Tool::Hand => "H",
        }
    }

    /// Tools sharing one slot of the tool rail (the slot shows the last one
    /// used). Symmetry has no slot: it is reached with X or from its panel.
    pub const GROUPS: [&'static [Tool]; 6] = [
        &[Tool::Select, Tool::Direct],
        &[Tool::Pen, Tool::Pencil, Tool::Width],
        &[Tool::Rect, Tool::Ellipse, Tool::Polygon, Tool::Star, Tool::Blob, Tool::Wave, Tool::Line],
        &[Tool::Text],
        &[Tool::Eyedropper],
        &[Tool::Hand],
    ];

    /// Rail slot of the tool, if it has one.
    pub fn group(self) -> Option<usize> {
        Tool::GROUPS.iter().position(|g| g.contains(&self))
    }

    fn draws_lines(self) -> bool {
        matches!(self, Tool::Pen | Tool::Pencil | Tool::Line)
    }

    /// Tools drawing a shape from center to edge (radius + rotation).
    fn draws_radial(self) -> bool {
        matches!(self, Tool::Polygon | Tool::Star | Tool::Blob)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeRef {
    pub id: Id,
    pub sp: usize,
    pub node: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    In,
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    TL,
    T,
    TR,
    R,
    BR,
    B,
    BL,
    L,
}

impl Handle {
    pub const ALL: [Handle; 8] =
        [Handle::TL, Handle::T, Handle::TR, Handle::R, Handle::BR, Handle::B, Handle::BL, Handle::L];

    pub fn pos(self, r: Rect) -> Point {
        let cx = (r.x0 + r.x1) * 0.5;
        let cy = (r.y0 + r.y1) * 0.5;
        match self {
            Handle::TL => Point::new(r.x0, r.y0),
            Handle::T => Point::new(cx, r.y0),
            Handle::TR => Point::new(r.x1, r.y0),
            Handle::R => Point::new(r.x1, cy),
            Handle::BR => Point::new(r.x1, r.y1),
            Handle::B => Point::new(cx, r.y1),
            Handle::BL => Point::new(r.x0, r.y1),
            Handle::L => Point::new(r.x0, cy),
        }
    }

    /// The opposite point, which stays fixed while resizing.
    fn anchor(self, r: Rect) -> Point {
        let opposite = match self {
            Handle::TL => Handle::BR,
            Handle::T => Handle::B,
            Handle::TR => Handle::BL,
            Handle::R => Handle::L,
            Handle::BR => Handle::TL,
            Handle::B => Handle::T,
            Handle::BL => Handle::TR,
            Handle::L => Handle::R,
        };
        opposite.pos(r)
    }

    fn moves_x(self) -> bool {
        !matches!(self, Handle::T | Handle::B)
    }

    fn moves_y(self) -> bool {
        !matches!(self, Handle::L | Handle::R)
    }

    pub fn is_corner(self) -> bool {
        matches!(self, Handle::TL | Handle::TR | Handle::BR | Handle::BL)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
}

pub enum Drag {
    None,
    Pan { start: Point, start_pan: Vec2 },
    Marquee { start: Point, current: Point, additive: bool, nodes: bool },
    Move { start: Point, origin: Document, ids: Vec<Id>, bounds: Rect },
    Resize { handle: Handle, bounds: Rect, origin: Document, ids: Vec<Id> },
    Rotate { center: Point, start_angle: f64, origin: Document, ids: Vec<Id> },
    Create { tool: Tool, start: Point, id: Id, origin: Document },
    Pencil { points: Vec<Point>, origin: Document },
    PenHandle { node: NodeRef },
    Nodes { start: Point, origin: Document, primary: NodeRef },
    Handle { node: NodeRef, side: Side, origin: Document },
    Width { node: NodeRef, origin: Document },
    MirrorAxis { start: Point, current: Point },
}

/// Raw samples of the last pencil stroke, kept so the smoothing can be
/// changed after drawing as long as the path is untouched.
#[derive(Clone, Debug)]
pub struct PencilStroke {
    pub id: Id,
    pub points: Vec<Point>,
    pub closed: bool,
    /// Zoom at drawing time (the tolerances are in screen pixels).
    pub zoom: f64,
    /// The path as created, to detect later edits.
    pub path: VectorPath,
}

/// The path currently being drawn with the pen.
#[derive(Clone, Copy, Debug)]
pub struct PenSession {
    pub id: Id,
    pub sp: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DefaultStyle {
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
}

/// Screen-space tolerances, in pixels.
pub const HANDLE_RADIUS: f64 = 6.0;
/// Distance (screen px) between the top of the selection box and the
/// rotation handle.
pub const ROTATE_HANDLE_OFFSET: f64 = 28.0;
pub const SNAP_PX: f64 = 6.0;

pub struct Editor {
    pub doc: Document,
    pub file: Option<PathBuf>,
    pub saved_doc: Document,
    pub history: History,
    pub fonts: FontLibrary,
    pub tool: Tool,
    pub selected: Vec<Id>,
    pub nodes: Vec<NodeRef>,
    pub active_layer: usize,
    pub zoom: f64,
    pub pan: Vec2,
    /// Canvas origin in window coordinates and its size, updated every frame.
    pub canvas_origin: Point,
    pub canvas_size: Vec2,
    /// Canvas area covered by floating panels (left, top, right, bottom),
    /// kept clear when fitting or zooming on the center.
    pub view_insets: (f64, f64, f64, f64),
    pub drag: Drag,
    pub hover: Option<Id>,
    pub mouse: Point,
    pub guides: Vec<Guide>,
    pub pen: Option<PenSession>,
    pub space_down: bool,
    pub snapping: bool,
    pub shape_style: DefaultStyle,
    pub line_style: DefaultStyle,
    pub needs_fit: bool,
    /// Screen pixels per window unit (2 on Retina): resolution of the
    /// gradient and effect pixels.
    pub pixel_scale: f64,
    pub status: Option<String>,
    /// Ask the properties panel to focus the text content field.
    pub focus_text: bool,
    /// Color under the cursor while the eyedropper is active.
    pub eyedropper_preview: Option<Color>,
    /// Pencil smoothing, 0 (follows the hand) to 1 (round, few points).
    pub pencil_smoothing: f64,
    pub pencil_last: Option<PencilStroke>,
    clipboard: Vec<Object>,
    cache: RefCell<HashMap<Id, (Object, Rc<Rendered>)>>,
    /// Bumped whenever the document or selection changes outside the panels,
    /// so the property inputs know to refresh.
    pub revision: u64,
}

impl Editor {
    pub fn new(doc: Document, file: Option<PathBuf>, fonts: FontLibrary) -> Self {
        Self {
            saved_doc: doc.clone(),
            doc,
            file,
            history: History::default(),
            fonts,
            tool: Tool::Select,
            selected: Vec::new(),
            nodes: Vec::new(),
            active_layer: 0,
            zoom: 1.0,
            pan: Vec2::ZERO,
            canvas_origin: Point::ZERO,
            canvas_size: Vec2::new(800.0, 600.0),
            view_insets: (0.0, 0.0, 0.0, 0.0),
            drag: Drag::None,
            hover: None,
            mouse: Point::ZERO,
            guides: Vec::new(),
            pen: None,
            space_down: false,
            snapping: true,
            shape_style: DefaultStyle { fill: Color::from_hex("#d9d9d9"), stroke: None },
            line_style: DefaultStyle {
                fill: None,
                stroke: Some(Stroke { color: Color::from_hex("#1d1d1f").unwrap(), width: 6.0, ..Default::default() }),
            },
            needs_fit: true,
            pixel_scale: 2.0,
            status: None,
            focus_text: false,
            eyedropper_preview: None,
            pencil_smoothing: 0.5,
            pencil_last: None,
            clipboard: Vec::new(),
            cache: RefCell::new(HashMap::new()),
            revision: 0,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.doc != self.saved_doc
    }

    pub fn title(&self) -> String {
        format!("{}{} — Desy", if self.is_dirty() { "• " } else { "" }, self.doc.name)
    }

    // ---------------------------------------------------------------- view

    pub fn view(&self) -> Affine {
        Affine::translate(self.pan) * Affine::scale(self.zoom)
    }

    /// Window coordinates → document coordinates.
    pub fn to_doc(&self, window_pt: Point) -> Point {
        let local = window_pt - self.canvas_origin.to_vec2();
        Point::new((local.x - self.pan.x) / self.zoom, (local.y - self.pan.y) / self.zoom)
    }

    /// Document → canvas-local pixels.
    pub fn to_screen(&self, p: Point) -> Point {
        Point::new(p.x * self.zoom + self.pan.x, p.y * self.zoom + self.pan.y)
    }

    /// The part of the canvas not covered by floating panels: (origin, size),
    /// relative to the canvas. Falls back to the whole canvas when too small.
    fn visible_area(&self) -> (Vec2, Vec2) {
        let (l, t, r, b) = self.view_insets;
        let size = Vec2::new(self.canvas_size.x - l - r, self.canvas_size.y - t - b);
        if size.x < 200.0 || size.y < 200.0 { (Vec2::ZERO, self.canvas_size) } else { (Vec2::new(l, t), size) }
    }

    pub fn fit(&mut self) {
        let margin = 40.0;
        let (origin, size) = self.visible_area();
        let sx = (size.x - margin * 2.0) / self.doc.width;
        let sy = (size.y - margin * 2.0) / self.doc.height;
        self.zoom = sx.min(sy).clamp(0.02, 64.0);
        self.pan = origin
            + Vec2::new((size.x - self.doc.width * self.zoom) * 0.5, (size.y - self.doc.height * self.zoom) * 0.5);
    }

    pub fn zoom_at(&mut self, factor: f64, window_pt: Point) {
        let before = self.to_doc(window_pt);
        self.zoom = (self.zoom * factor).clamp(0.02, 64.0);
        let local = window_pt - self.canvas_origin.to_vec2();
        self.pan = Vec2::new(local.x - before.x * self.zoom, local.y - before.y * self.zoom);
    }

    pub fn zoom_center(&mut self, factor: f64) {
        let (origin, size) = self.visible_area();
        let c = self.canvas_origin + origin + size * 0.5;
        self.zoom_at(factor, c);
    }

    fn px(&self, screen_px: f64) -> f64 {
        screen_px / self.zoom
    }

    // ---------------------------------------------------------------- rendering cache

    pub fn rendered(&self, o: &Object) -> Rc<Rendered> {
        let mut cache = self.cache.borrow_mut();
        if let Some((cached, r)) = cache.get(&o.id)
            && cached == o
        {
            return r.clone();
        }
        let r = Rc::new(o.render(&self.fonts));
        cache.insert(o.id, (o.clone(), r.clone()));
        r
    }

    /// Topmost editable object under a document point, using the render cache
    /// (effects can be costly to recompute on every pointer move).
    pub fn hit_test(&self, p: Point) -> Option<Id> {
        let tol = self.px(4.0);
        self.doc.hit_test_with(|o| o.hit_rendered(&self.rendered(o), p, tol))
    }

    pub fn invalidate_text(&mut self) {
        self.cache.borrow_mut().retain(|_, (o, _)| !matches!(o.shape, Shape::Text(_)));
    }

    pub fn prune_cache(&self) {
        let live: std::collections::HashSet<Id> = self.doc.objects().map(|o| o.id).collect();
        self.cache.borrow_mut().retain(|id, _| live.contains(id));
    }

    pub fn bounds_of(&self, id: Id) -> Option<Rect> {
        let o = self.doc.effective(id)?;
        let r = self.rendered(&o);
        if r.path.is_empty() { Some(o.bounds(&self.fonts)) } else { Some(r.path.bounding_box()) }
    }

    pub fn selection_bounds(&self) -> Option<Rect> {
        self.selected.iter().filter_map(|&id| self.bounds_of(id)).reduce(|a, b| a.union(b))
    }

    // ---------------------------------------------------------------- history

    /// Record `origin` as an undo step if the document changed since.
    pub fn commit(&mut self, origin: Document) {
        if origin != self.doc {
            self.history.push(origin);
        }
        self.touch();
    }

    /// Run a document edit as one undo step.
    pub fn edit(&mut self, f: impl FnOnce(&mut Self)) {
        let origin = self.doc.clone();
        f(self);
        self.commit(origin);
    }

    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.selected.retain(|&id| self.doc.find(id).is_some());
        let doc = &self.doc;
        self.nodes.retain(|n| match doc.get(n.id).map(|o| &o.shape) {
            Some(Shape::Path(vp)) => vp.subpaths.get(n.sp).is_some_and(|s| n.node < s.nodes.len()),
            _ => false,
        });
        if self.active_layer >= self.doc.layers.len() {
            self.active_layer = self.doc.layers.len().saturating_sub(1);
        }
    }

    pub fn undo(&mut self) {
        self.finish_pen();
        if let Some(prev) = self.history.undo(&self.doc) {
            self.doc = prev;
            self.touch();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.history.redo(&self.doc) {
            self.doc = next;
            self.touch();
        }
    }

    // ---------------------------------------------------------------- selection

    pub fn select_only(&mut self, id: Id) {
        self.selected = vec![id];
        self.nodes.clear();
        if let Some(loc) = self.doc.find(id) {
            self.active_layer = loc.layer;
        }
        self.revision += 1;
    }

    pub fn toggle_selected(&mut self, id: Id) {
        if let Some(i) = self.selected.iter().position(|&s| s == id) {
            self.selected.remove(i);
        } else {
            self.selected.push(id);
        }
        self.revision += 1;
    }

    /// Right-click: select what is under the pointer (unless it is already
    /// part of the selection); on empty canvas outside the selection box,
    /// clear the selection.
    pub fn context_select(&mut self, window_pt: Point) {
        let p = self.to_doc(window_pt);
        match self.hit_test(p) {
            Some(id) if !self.selected.contains(&id) => self.select_only(id),
            Some(_) => {}
            None => {
                if !self.selection_bounds().is_some_and(|b| b.contains(p)) {
                    self.clear_selection();
                }
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.nodes.clear();
        self.revision += 1;
    }

    pub fn select_all(&mut self) {
        self.selected = self
            .doc
            .layers
            .iter()
            .filter(|l| l.visible && !l.locked)
            .flat_map(|l| l.objects.iter())
            .filter(|o| o.visible && !o.locked)
            .map(|o| o.id)
            .collect();
        self.revision += 1;
    }

    pub fn set_tool(&mut self, tool: Tool) {
        if self.tool != tool {
            self.finish_pen();
            self.tool = tool;
            if tool != Tool::Direct {
                self.nodes.clear();
            }
        }
    }

    // ---------------------------------------------------------------- snapping

    fn snap_targets(&self, exclude: &[Id]) -> Vec<Rect> {
        let mut t = vec![self.doc.artboard()];
        for o in self.doc.visible_effective() {
            if exclude.contains(&o.id) {
                continue;
            }
            let r = self.rendered(&o);
            if !r.path.is_empty() {
                t.push(r.path.bounding_box());
            }
        }
        t
    }

    fn snap_point(&mut self, p: Point, exclude: &[Id], mods: Mods) -> Point {
        self.guides.clear();
        if !self.snapping || mods.cmd {
            return p;
        }
        let res = snap::snap_point(p, &self.snap_targets(exclude), self.px(SNAP_PX));
        self.guides = res.guides;
        let snapped = p + res.offset;
        // Mirror axes are magnetic, so points can be placed exactly on them.
        let threshold = self.px(SNAP_PX);
        let axis = self
            .doc
            .visible_mirrors()
            .into_iter()
            .map(|m| (m, m.distance(p)))
            .filter(|(_, d)| *d <= threshold)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((m, _)) = axis {
            self.guides.retain(|g| !matches!(g, Guide::Line { .. }));
            self.guides.push(Guide::Line { a: m.a, b: m.b });
            // Keep an alignment along the axis if there was one, else project.
            let along = m.project(snapped);
            return if m.on_axis(snapped) { snapped } else { along };
        }
        snapped
    }

    // ---------------------------------------------------------------- hit testing (screen space)

    pub fn handle_at(&self, doc_pt: Point) -> Option<Handle> {
        if self.selected.is_empty() || self.tool != Tool::Select {
            return None;
        }
        let b = self.selection_bounds()?;
        let p = self.to_screen(doc_pt);
        Handle::ALL.into_iter().find(|h| {
            let hp = self.to_screen(h.pos(b));
            (hp - p).hypot() <= HANDLE_RADIUS + 2.0
        })
    }

    /// Whether the selection can rotate (images always stay upright).
    pub fn can_rotate(&self) -> bool {
        self.tool == Tool::Select && self.selected.iter().filter_map(|id| self.doc.get(*id)).any(|o| !o.is_image())
    }

    /// Rotation handle, in canvas screen coordinates: a point above the
    /// middle of the selection box's top edge.
    pub fn rotate_handle(&self) -> Option<Point> {
        if !self.can_rotate() {
            return None;
        }
        let b = self.selection_bounds()?;
        Some(self.to_screen(Handle::T.pos(b)) - Vec2::new(0.0, ROTATE_HANDLE_OFFSET))
    }

    pub fn rotate_handle_at(&self, doc_pt: Point) -> bool {
        self.rotate_handle().is_some_and(|h| (h - self.to_screen(doc_pt)).hypot() <= HANDLE_RADIUS + 3.0)
    }

    pub fn rotate_zone_at(&self, doc_pt: Point) -> bool {
        if self.selected.is_empty() || self.tool != Tool::Select {
            return false;
        }
        let Some(b) = self.selection_bounds() else { return false };
        let p = self.to_screen(doc_pt);
        let sb = Rect::from_points(self.to_screen(b.origin()), self.to_screen(Point::new(b.x1, b.y1)));
        if sb.contains(p) {
            return false;
        }
        [Handle::TL, Handle::TR, Handle::BR, Handle::BL].into_iter().any(|h| {
            let d = (self.to_screen(h.pos(b)) - p).hypot();
            d > HANDLE_RADIUS + 2.0 && d < 24.0
        })
    }

    /// Editable path nodes for an object (world space). Shapes are shown as
    /// the path they would become; text has none.
    pub fn node_path(&self, id: Id) -> Option<VectorPath> {
        let o = self.doc.get(id)?;
        match &o.shape {
            Shape::Path(vp) => {
                let mut vp = vp.clone();
                vp.transform(o.transform);
                Some(vp)
            }
            Shape::Text(_) | Shape::Image { .. } => None,
            _ => Some(VectorPath::from_bezpath(&o.base_world_path(&self.fonts))),
        }
    }

    fn node_at(&self, doc_pt: Point) -> Option<NodeRef> {
        let p = self.to_screen(doc_pt);
        for &id in self.selected.iter().rev() {
            let Some(vp) = self.node_path(id) else { continue };
            for (si, sp) in vp.subpaths.iter().enumerate() {
                for (ni, n) in sp.nodes.iter().enumerate() {
                    if (self.to_screen(n.p) - p).hypot() <= HANDLE_RADIUS {
                        return Some(NodeRef { id, sp: si, node: ni });
                    }
                }
            }
        }
        None
    }

    /// Nodes whose handles are displayed: selected nodes and their neighbors' facing handles.
    pub fn visible_handles(&self) -> Vec<(NodeRef, Side, Point, Point)> {
        let mut out = Vec::new();
        for nr in &self.nodes {
            let Some(vp) = self.node_path(nr.id) else { continue };
            let Some(sp) = vp.subpaths.get(nr.sp) else { continue };
            let len = sp.nodes.len();
            let Some(n) = sp.nodes.get(nr.node) else { continue };
            if n.has_in() {
                out.push((*nr, Side::In, n.p, n.h_in));
            }
            if n.has_out() {
                out.push((*nr, Side::Out, n.p, n.h_out));
            }
            let prev = if nr.node > 0 {
                Some(nr.node - 1)
            } else if sp.closed {
                Some(len - 1)
            } else {
                None
            };
            let next = if nr.node + 1 < len {
                Some(nr.node + 1)
            } else if sp.closed {
                Some(0)
            } else {
                None
            };
            if let Some(pi) = prev {
                let pn = &sp.nodes[pi];
                if pn.has_out() {
                    out.push((NodeRef { node: pi, ..*nr }, Side::Out, pn.p, pn.h_out));
                }
            }
            if let Some(ni) = next {
                let nn = &sp.nodes[ni];
                if nn.has_in() {
                    out.push((NodeRef { node: ni, ..*nr }, Side::In, nn.p, nn.h_in));
                }
            }
        }
        out
    }

    fn handle_point_at(&self, doc_pt: Point) -> Option<(NodeRef, Side)> {
        let p = self.to_screen(doc_pt);
        self.visible_handles()
            .into_iter()
            .find(|(_, _, _, h)| (self.to_screen(*h) - p).hypot() <= HANDLE_RADIUS)
            .map(|(n, s, _, _)| (n, s))
    }

    /// Nearest point on a path object: (subpath, segment start node, t, distance).
    fn nearest_on_path(&self, id: Id, p: Point) -> Option<(usize, usize, f64, f64)> {
        let vp = self.node_path(id)?;
        let mut best: Option<(usize, usize, f64, f64)> = None;
        for (si, sp) in vp.subpaths.iter().enumerate() {
            for ws in sp.width_segments() {
                let n = ws.seg.nearest(p, 0.01);
                let d = n.distance_sq.sqrt();
                if best.is_none_or(|b| d < b.3) {
                    best = Some((si, ws.start, n.t, d));
                }
            }
        }
        best
    }

    // ---------------------------------------------------------------- path mutation helpers

    /// Make sure the object is an editable path (converting shapes) and return it.
    fn ensure_path(&mut self, id: Id) -> Option<&mut VectorPath> {
        let fonts = &self.fonts;
        let o = self.doc.get_mut(id)?;
        if matches!(o.shape, Shape::Text(_) | Shape::Image { .. }) {
            return None;
        }
        o.convert_to_path(fonts);
        o.normalize();
        match &mut o.shape {
            Shape::Path(vp) => Some(vp),
            _ => None,
        }
    }

    fn node_mut(&mut self, n: NodeRef) -> Option<&mut PathNode> {
        self.ensure_path(n.id)?.subpaths.get_mut(n.sp)?.nodes.get_mut(n.node)
    }

    pub fn delete_selected_nodes(&mut self) {
        let mut by_obj: HashMap<Id, Vec<(usize, usize)>> = HashMap::new();
        for n in &self.nodes {
            by_obj.entry(n.id).or_default().push((n.sp, n.node));
        }
        for (id, mut list) in by_obj {
            list.sort_unstable_by(|a, b| b.cmp(a));
            let Some(vp) = self.ensure_path(id) else { continue };
            for (sp, node) in list {
                if let Some(s) = vp.subpaths.get_mut(sp)
                    && node < s.nodes.len()
                {
                    s.nodes.remove(node);
                }
            }
            vp.subpaths.retain(|s| s.nodes.len() >= 2);
            if vp.subpaths.is_empty() {
                self.doc.remove(id);
            }
        }
        self.nodes.clear();
    }

    fn toggle_node_smooth(&mut self, n: NodeRef) {
        let Some(vp) = self.ensure_path(n.id) else { return };
        let Some(sp) = vp.subpaths.get_mut(n.sp) else { return };
        let len = sp.nodes.len();
        let closed = sp.closed;
        let prev = if n.node > 0 {
            Some(sp.nodes[n.node - 1].p)
        } else if closed {
            Some(sp.nodes[len - 1].p)
        } else {
            None
        };
        let next = if n.node + 1 < len {
            Some(sp.nodes[n.node + 1].p)
        } else if closed {
            Some(sp.nodes[0].p)
        } else {
            None
        };
        let node = &mut sp.nodes[n.node];
        if node.has_in() || node.has_out() {
            node.h_in = node.p;
            node.h_out = node.p;
            node.kind = NodeKind::Corner;
        } else {
            let a = prev.unwrap_or(node.p);
            let b = next.unwrap_or(node.p);
            let dir = b - a;
            if dir.hypot() < 1e-9 {
                return;
            }
            let dir = dir.normalize();
            let len_in = (node.p - a).hypot() / 3.0;
            let len_out = (b - node.p).hypot() / 3.0;
            node.h_in = node.p - dir * len_in;
            node.h_out = node.p + dir * len_out;
            node.kind = NodeKind::Smooth;
        }
    }

    // ---------------------------------------------------------------- object creation

    fn style_for(&self, tool: Tool) -> DefaultStyle {
        // A wave starts as a wavy line.
        if tool.draws_lines() || tool == Tool::Wave { self.line_style } else { self.shape_style }
    }

    fn target_layer(&mut self) -> usize {
        if self.doc.layers.is_empty() {
            self.doc.add_layer(None);
        }
        let li = self.active_layer.min(self.doc.layers.len() - 1);
        let l = &mut self.doc.layers[li];
        // Drawing on a hidden or locked layer would be invisible: reveal it.
        l.visible = true;
        l.locked = false;
        li
    }

    fn create(&mut self, tool: Tool, shape: Shape, transform: Affine) -> Id {
        let style = self.style_for(tool);
        let li = self.target_layer();
        let id = self.doc.insert(li, new_object(shape, transform, style.fill, style.stroke));
        self.selected = vec![id];
        self.nodes.clear();
        id
    }

    fn update_created(&mut self, tool: Tool, id: Id, start: Point, cur: Point, mods: Mods) {
        let Some(o) = self.doc.get_mut(id) else { return };
        match tool {
            Tool::Rect | Tool::Ellipse | Tool::Wave => {
                let mut d = cur - start;
                if mods.shift {
                    let m = d.x.abs().max(d.y.abs());
                    d = Vec2::new(m.copysign(d.x), m.copysign(d.y));
                }
                let (p0, p1) = if mods.alt { (start - d, start + d) } else { (start, start + d) };
                let r = Rect::from_points(p0, p1);
                o.shape = match &o.shape {
                    Shape::Rect { radius, corners, .. } => {
                        Shape::Rect { w: r.width(), h: r.height(), radius: *radius, corners: *corners }
                    }
                    Shape::Wave { waves, filled, depth, .. } => {
                        Shape::Wave { w: r.width(), h: r.height(), waves: *waves, filled: *filled, depth: *depth }
                    }
                    _ => Shape::Ellipse { w: r.width(), h: r.height() },
                };
                o.transform = Affine::translate(r.origin().to_vec2());
            }
            Tool::Polygon | Tool::Star | Tool::Blob => {
                let d = cur - start;
                let radius = d.hypot();
                let angle = if mods.shift { 0.0 } else { d.y.atan2(d.x) + std::f64::consts::FRAC_PI_2 };
                o.shape = match &o.shape {
                    Shape::Polygon { sides, .. } => Shape::Polygon { radius, sides: *sides },
                    Shape::Star { outer, inner, points } => {
                        let ratio = if *outer > 0.0 { inner / outer } else { 0.5 };
                        Shape::Star { outer: radius, inner: radius * ratio, points: *points }
                    }
                    Shape::Blob { points, variance, seed, .. } => {
                        Shape::Blob { radius, points: *points, variance: *variance, seed: *seed }
                    }
                    s => s.clone(),
                };
                o.transform = Affine::translate(start.to_vec2()) * Affine::rotate(angle);
            }
            Tool::Line => {
                let end = if mods.shift { constrain_45(start, cur) } else { cur };
                o.shape = Shape::Path(VectorPath {
                    subpaths: vec![SubPath {
                        nodes: vec![PathNode::corner(start), PathNode::corner(end)],
                        closed: false,
                    }],
                });
                o.transform = Affine::IDENTITY;
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- pen

    pub fn finish_pen(&mut self) {
        if let Some(sess) = self.pen.take() {
            if let Some(Shape::Path(vp)) = self.doc.get(sess.id).map(|o| &o.shape)
                && vp.node_count() < 2
            {
                self.doc.remove(sess.id);
            }
            self.touch();
        }
    }

    fn pen_down(&mut self, p: Point, mods: Mods) {
        let p = self.snap_point(p, &[], mods);
        let tol = HANDLE_RADIUS;
        if let Some(sess) = self.pen {
            let Some(Shape::Path(vp)) = self.doc.get(sess.id).map(|o| &o.shape) else {
                self.pen = None;
                return;
            };
            let sp = &vp.subpaths[sess.sp];
            let first = sp.nodes[0].p;
            let last = sp.nodes.last().map(|n| n.p).unwrap_or(first);
            // Click on the first point closes the path.
            if sp.nodes.len() >= 2 && (self.to_screen(first) - self.to_screen(p)).hypot() <= tol {
                let origin = self.doc.clone();
                if let Some(Shape::Path(vp)) = self.doc.get_mut(sess.id).map(|o| &mut o.shape) {
                    vp.subpaths[sess.sp].closed = true;
                }
                let style = self.shape_style;
                if let Some(o) = self.doc.get_mut(sess.id)
                    && o.fill.is_none()
                {
                    o.fill = style.fill;
                }
                self.commit(origin);
                self.pen = None;
                self.drag = Drag::PenHandle { node: NodeRef { id: sess.id, sp: sess.sp, node: 0 } };
                return;
            }
            let p = if mods.shift { constrain_45(last, p) } else { p };
            let origin = self.doc.clone();
            let idx = if let Some(Shape::Path(vp)) = self.doc.get_mut(sess.id).map(|o| &mut o.shape) {
                let nodes = &mut vp.subpaths[sess.sp].nodes;
                nodes.push(PathNode::corner(p));
                nodes.len() - 1
            } else {
                return;
            };
            self.commit(origin);
            self.drag = Drag::PenHandle { node: NodeRef { id: sess.id, sp: sess.sp, node: idx } };
            return;
        }

        // Not drawing yet: continue an open path from one of its ends, or insert a
        // point on a selected path's segment, or start a new path.
        for &id in self.selected.clone().iter() {
            let Some(Shape::Path(vp)) = self.doc.get(id).map(|o| &o.shape) else { continue };
            for (si, sp) in vp.subpaths.iter().enumerate() {
                if sp.closed || sp.nodes.len() < 2 {
                    continue;
                }
                let first = sp.nodes[0].p;
                let last = sp.nodes[sp.nodes.len() - 1].p;
                let near = |q: Point| (self.to_screen(q) - self.to_screen(p)).hypot() <= tol;
                if near(last) || near(first) {
                    let reverse = near(first) && !near(last);
                    if reverse && let Some(Shape::Path(vp)) = self.doc.get_mut(id).map(|o| &mut o.shape) {
                        let s = &mut vp.subpaths[si];
                        s.nodes.reverse();
                        for n in &mut s.nodes {
                            std::mem::swap(&mut n.h_in, &mut n.h_out);
                        }
                    }
                    self.pen = Some(PenSession { id, sp: si });
                    return;
                }
            }
            if let Some((si, seg, t, d)) = self.nearest_on_path(id, p)
                && d * self.zoom <= tol
            {
                self.edit(|ed| {
                    if let Some(vp) = ed.ensure_path(id)
                        && let Some(ni) = vp.insert_node(si, seg, t)
                    {
                        ed.nodes = vec![NodeRef { id, sp: si, node: ni }];
                    }
                });
                return;
            }
        }

        let origin = self.doc.clone();
        let id = self.create(
            Tool::Pen,
            Shape::Path(VectorPath { subpaths: vec![SubPath { nodes: vec![PathNode::corner(p)], closed: false }] }),
            Affine::IDENTITY,
        );
        self.commit(origin);
        self.pen = Some(PenSession { id, sp: 0 });
        self.drag = Drag::PenHandle { node: NodeRef { id, sp: 0, node: 0 } };
    }

    // ---------------------------------------------------------------- mouse

    pub fn mouse_down(&mut self, window_pt: Point, mods: Mods, click_count: usize, middle: bool) {
        let p = self.to_doc(window_pt);
        self.mouse = p;
        if middle || self.tool == Tool::Hand || self.space_down {
            self.drag = Drag::Pan { start: window_pt, start_pan: self.pan };
            return;
        }
        match self.tool {
            Tool::Select => self.select_down(p, mods, click_count),
            Tool::Direct => self.direct_down(p, mods, click_count),
            Tool::Pen => self.pen_down(p, mods),
            Tool::Pencil => {
                let p = self.snap_point(p, &[], mods);
                self.drag = Drag::Pencil { points: vec![p], origin: self.doc.clone() };
            }
            Tool::Width => self.width_down(p),
            Tool::Eyedropper => self.eyedrop(p, mods),
            Tool::Mirror => {
                if self.doc.layers.get(self.mirror_layer()).is_none_or(|l| l.locked) {
                    self.status = Some("Ce calque est verrouillé".into());
                    return;
                }
                let start = self.snap_point(p, &[], mods);
                self.drag = Drag::MirrorAxis { start, current: start };
            }
            Tool::Text => {
                let p = self.snap_point(p, &[], mods);
                if let Some(id) = self.hit_test(p)
                    && matches!(self.doc.get(id).map(|o| &o.shape), Some(Shape::Text(_)))
                {
                    self.select_only(id);
                    self.focus_text = true;
                    return;
                }
                let origin = self.doc.clone();
                let spec = TextSpec { family: self.default_family(), ..Default::default() };
                let fill = self.shape_style.fill.filter(|c| c.to_hex() != "#d9d9d9").or(Color::from_hex("#1d1d1f"));
                let li = self.target_layer();
                let mut obj = new_object(Shape::Text(spec), Affine::translate(p.to_vec2()), fill, None);
                obj.name = "Texte".into();
                let id = self.doc.insert(li, obj);
                self.select_only(id);
                self.commit(origin);
                self.tool = Tool::Select;
                self.focus_text = true;
            }
            Tool::Hand => {}
            t @ (Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Polygon | Tool::Star | Tool::Wave | Tool::Blob) => {
                let origin = self.doc.clone();
                let start = self.snap_point(p, &[], mods);
                let shape = match t {
                    Tool::Rect => Shape::Rect { w: 0.0, h: 0.0, radius: 0.0, corners: None },
                    Tool::Ellipse => Shape::Ellipse { w: 0.0, h: 0.0 },
                    Tool::Polygon => Shape::Polygon { radius: 0.0, sides: 6 },
                    Tool::Star => Shape::Star { outer: 0.0, inner: 0.0, points: 5 },
                    Tool::Wave => Shape::Wave {
                        w: 0.0,
                        h: 0.0,
                        waves: 3,
                        filled: false,
                        depth: desy_core::document::DEFAULT_WAVE_DEPTH,
                    },
                    Tool::Blob => {
                        Shape::Blob { radius: 0.0, points: 6, variance: 0.35, seed: desy_core::document::random_seed() }
                    }
                    _ => Shape::Path(VectorPath::default()),
                };
                let id = self.create(t, shape, Affine::translate(start.to_vec2()));
                if t == Tool::Star
                    && let Some(Shape::Star { inner, outer, .. }) = self.doc.get_mut(id).map(|o| &mut o.shape)
                {
                    *outer = 1.0;
                    *inner = 0.45;
                }
                self.drag = Drag::Create { tool: t, start, id, origin };
            }
        }
    }

    pub fn default_family(&self) -> String {
        let prefs = ["Inter", "Helvetica Neue", "Helvetica", "Arial", "DejaVu Sans", "Segoe UI"];
        prefs
            .iter()
            .find(|f| self.fonts.has_family(f))
            .map(|s| s.to_string())
            .or_else(|| self.fonts.families().first().cloned())
            .unwrap_or_else(|| "sans-serif".into())
    }

    fn select_down(&mut self, p: Point, mods: Mods, click_count: usize) {
        if let Some(h) = self.handle_at(p) {
            let bounds = self.selection_bounds().unwrap_or_default();
            self.drag = Drag::Resize { handle: h, bounds, origin: self.doc.clone(), ids: self.selected.clone() };
            return;
        }
        if self.rotate_handle_at(p) || self.rotate_zone_at(p) {
            let b = self.selection_bounds().unwrap_or_default();
            let c = b.center();
            self.drag = Drag::Rotate {
                center: c,
                start_angle: (p - c).atan2(),
                origin: self.doc.clone(),
                // Images always stay upright.
                ids: self
                    .selected
                    .iter()
                    .copied()
                    .filter(|id| !self.doc.get(*id).is_some_and(|o| o.is_image()))
                    .collect(),
            };
            return;
        }
        match self.hit_test(p) {
            Some(id) => {
                if click_count >= 2 {
                    match self.doc.get(id).map(|o| &o.shape) {
                        Some(Shape::Text(_)) => {
                            self.select_only(id);
                            self.focus_text = true;
                        }
                        Some(_) => {
                            self.select_only(id);
                            self.tool = Tool::Direct;
                        }
                        None => {}
                    }
                    return;
                }
                if mods.shift {
                    self.toggle_selected(id);
                    if !self.selected.contains(&id) {
                        return;
                    }
                } else if !self.selected.contains(&id) {
                    self.select_only(id);
                }
                self.start_move(p, mods);
            }
            None => {
                if !mods.shift {
                    self.clear_selection();
                }
                self.drag = Drag::Marquee { start: p, current: p, additive: mods.shift, nodes: false };
            }
        }
    }

    fn start_move(&mut self, p: Point, mods: Mods) {
        let origin = self.doc.clone();
        if mods.alt {
            // Alt-drag duplicates.
            let ids = self.doc.sort_by_z(&self.selected);
            let mut new_ids = Vec::new();
            for id in ids {
                if let (Some(o), Some(loc)) = (self.doc.get(id).cloned(), self.doc.find(id)) {
                    new_ids.push(self.doc.insert(loc.layer, o));
                }
            }
            self.selected = new_ids;
        }
        let bounds = self.selection_bounds().unwrap_or_default();
        self.drag = Drag::Move { start: p, origin, ids: self.selected.clone(), bounds };
    }

    fn direct_down(&mut self, p: Point, mods: Mods, click_count: usize) {
        if let Some((node, side)) = self.handle_point_at(p) {
            let origin = self.doc.clone();
            self.ensure_path(node.id);
            self.drag = Drag::Handle { node, side, origin };
            return;
        }
        if let Some(node) = self.node_at(p) {
            let origin = self.doc.clone();
            if mods.alt {
                self.toggle_node_smooth(node);
                self.nodes = vec![node];
                self.commit(origin);
                return;
            }
            if mods.shift {
                if let Some(i) = self.nodes.iter().position(|n| *n == node) {
                    self.nodes.remove(i);
                } else {
                    self.nodes.push(node);
                }
            } else if !self.nodes.contains(&node) {
                self.nodes = vec![node];
            }
            self.revision += 1;
            self.ensure_path(node.id);
            self.drag = Drag::Nodes { start: p, origin, primary: node };
            return;
        }
        // Double-click on a selected path's segment adds a point.
        if click_count >= 2 {
            for &id in self.selected.clone().iter() {
                if let Some((si, seg, t, d)) = self.nearest_on_path(id, p)
                    && d * self.zoom <= HANDLE_RADIUS
                {
                    self.edit(|ed| {
                        if let Some(vp) = ed.ensure_path(id)
                            && let Some(ni) = vp.insert_node(si, seg, t)
                        {
                            ed.nodes = vec![NodeRef { id, sp: si, node: ni }];
                        }
                    });
                    return;
                }
            }
        }
        match self.hit_test(p) {
            Some(id) => {
                if mods.shift {
                    self.toggle_selected(id);
                } else if !self.selected.contains(&id) {
                    self.select_only(id);
                }
                self.nodes.retain(|n| n.id == id || mods.shift);
                self.start_move(p, mods);
            }
            None => {
                if !mods.shift {
                    self.nodes.clear();
                }
                self.drag = Drag::Marquee { start: p, current: p, additive: mods.shift, nodes: true };
            }
        }
    }

    fn width_down(&mut self, p: Point) {
        let tol = self.px(10.0);
        // Selected objects first, then the others from the top of the stack.
        let mut candidates = self.selected.clone();
        let others: Vec<Id> = self.doc.objects().map(|o| o.id).filter(|id| !self.selected.contains(id)).collect();
        candidates.extend(others.into_iter().rev());
        candidates.retain(|&id| self.doc.is_editable(id));
        for id in candidates {
            if matches!(self.doc.get(id).map(|o| &o.shape), Some(Shape::Text(_) | Shape::Image { .. }) | None) {
                continue;
            }
            let Some(vp) = self.node_path(id) else { continue };
            // Existing node close by?
            let mut hit: Option<NodeRef> = None;
            for (si, sp) in vp.subpaths.iter().enumerate() {
                for (ni, n) in sp.nodes.iter().enumerate() {
                    if (n.p - p).hypot() <= tol.max(self.px(HANDLE_RADIUS)) {
                        hit = Some(NodeRef { id, sp: si, node: ni });
                    }
                }
            }
            let origin = self.doc.clone();
            if hit.is_none()
                && let Some((si, seg, t, d)) = self.nearest_on_path(id, p)
                && d <= tol
            {
                let ni = self.ensure_path(id).and_then(|vp| vp.insert_node(si, seg, t));
                hit = ni.map(|ni| NodeRef { id, sp: si, node: ni });
            }
            if let Some(node) = hit {
                self.ensure_path(id);
                let line = self.line_style.stroke;
                if let Some(o) = self.doc.get_mut(id)
                    && o.stroke.is_none()
                {
                    o.stroke = line;
                }
                self.select_only(id);
                self.nodes = vec![node];
                self.drag = Drag::Width { node, origin };
                return;
            }
        }
    }

    pub fn mouse_move(&mut self, window_pt: Point, mods: Mods) {
        let p = self.to_doc(window_pt);
        self.mouse = p;
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        self.drag = match drag {
            Drag::None => {
                self.hover = match self.tool {
                    Tool::Select | Tool::Direct | Tool::Width => self.hit_test(p),
                    _ => None,
                };
                self.eyedropper_preview = if self.tool == Tool::Eyedropper { self.sample_color(p) } else { None };
                Drag::None
            }
            Drag::Pan { start, start_pan } => {
                self.pan = start_pan + (window_pt - start);
                Drag::Pan { start, start_pan }
            }
            Drag::Marquee { start, additive, nodes, .. } => Drag::Marquee { start, current: p, additive, nodes },
            Drag::Move { start, origin, ids, bounds } => {
                let mut d = p - start;
                if mods.shift {
                    if d.x.abs() > d.y.abs() { d.y = 0.0 } else { d.x = 0.0 }
                }
                self.guides.clear();
                if self.snapping && !mods.cmd {
                    let res = snap::snap_rect(bounds + d, &self.snap_targets(&ids), self.px(SNAP_PX));
                    d += res.offset;
                    self.guides = res.guides;
                }
                self.doc = origin.clone();
                self.doc.translate_objects(&ids, d);
                Drag::Move { start, origin, ids, bounds }
            }
            Drag::Resize { handle, bounds, origin, ids } => {
                self.resize(handle, bounds, &origin, &ids, p, mods);
                Drag::Resize { handle, bounds, origin, ids }
            }
            Drag::Rotate { center, start_angle, origin, ids } => {
                let mut a = (p - center).atan2() - start_angle;
                if mods.shift {
                    let step = std::f64::consts::PI / 12.0;
                    a = (a / step).round() * step;
                }
                self.doc = origin.clone();
                self.doc.transform_objects(&ids, Affine::rotate_about(a, center));
                Drag::Rotate { center, start_angle, origin, ids }
            }
            Drag::Create { tool, start, id, origin } => {
                let cur = self.snap_point(p, &[id], mods);
                self.update_created(tool, id, start, cur, mods);
                Drag::Create { tool, start, id, origin }
            }
            Drag::Pencil { mut points, origin } => {
                if points.last().is_none_or(|l| (*l - p).hypot() * self.zoom > 1.5) {
                    points.push(p);
                }
                Drag::Pencil { points, origin }
            }
            Drag::PenHandle { node } => {
                let zoom = self.zoom;
                if let Some(n) = self.node_mut(node)
                    && (p - n.p).hypot() * zoom > 2.0
                {
                    n.kind = NodeKind::Symmetric;
                    n.set_out(p);
                }
                Drag::PenHandle { node }
            }
            Drag::Nodes { start, origin, primary } => {
                self.doc = origin.clone();
                let base = self
                    .node_path(primary.id)
                    .and_then(|vp| vp.subpaths.get(primary.sp).and_then(|s| s.nodes.get(primary.node)).map(|n| n.p))
                    .unwrap_or(start);
                let mut target = base + (p - start);
                if mods.shift {
                    target = constrain_45(base, target);
                }
                let ids: Vec<Id> = self.nodes.iter().map(|n| n.id).collect();
                let snapped = self.snap_point(target, &ids, mods);
                let d = snapped - base;
                for n in self.nodes.clone() {
                    let mirror = self.doc.mirror_of(n.id);
                    if let Some(node) = self.node_mut(n) {
                        // Points on the mirror axis stay on it (shared with their reflection).
                        let pinned = mirror.filter(|m| m.on_axis(node.p));
                        node.translate(d);
                        if let Some(m) = pinned {
                            let on = m.project(node.p);
                            node.translate(on - node.p);
                        }
                    }
                }
                Drag::Nodes { start, origin, primary }
            }
            Drag::Handle { node, side, origin } => {
                let p = if mods.shift {
                    self.node_path(node.id)
                        .and_then(|vp| {
                            vp.subpaths.get(node.sp).and_then(|s| s.nodes.get(node.node)).map(|n| constrain_45(n.p, p))
                        })
                        .unwrap_or(p)
                } else {
                    p
                };
                let alt = mods.alt;
                if let Some(n) = self.node_mut(node) {
                    if alt {
                        n.kind = NodeKind::Corner;
                    }
                    match side {
                        Side::In => n.set_in(p),
                        Side::Out => n.set_out(p),
                    }
                }
                Drag::Handle { node, side, origin }
            }
            Drag::MirrorAxis { start, .. } => {
                let mut cur = self.snap_point(p, &[], mods);
                let d = cur - start;
                if mods.shift {
                    cur = constrain_45(start, cur);
                } else if d.hypot() > 1e-9 {
                    // Gently straighten nearly vertical / horizontal axes.
                    let a = d.atan2().rem_euclid(std::f64::consts::FRAC_PI_2);
                    if a < 0.07 || a > std::f64::consts::FRAC_PI_2 - 0.07 {
                        cur = constrain_45(start, cur);
                    }
                }
                Drag::MirrorAxis { start, current: cur }
            }
            Drag::Width { node, origin } => {
                let base = self.doc.get(node.id).and_then(|o| o.stroke).map(|s| s.width).unwrap_or(1.0).max(0.01);
                if let Some(n) = self.node_mut(node) {
                    let w = (p - n.p).hypot() * 2.0 / base;
                    n.width = ((w * 100.0).round() / 100.0).clamp(0.0, 40.0);
                }
                Drag::Width { node, origin }
            }
        };
    }

    fn resize(&mut self, handle: Handle, b: Rect, origin: &Document, ids: &[Id], p: Point, mods: Mods) {
        self.guides.clear();
        let mut p = p;
        if self.snapping && !mods.cmd {
            let res = snap::snap_values(
                handle.moves_x().then_some(p.x),
                handle.moves_y().then_some(p.y),
                b,
                &self.snap_targets(ids),
                self.px(SNAP_PX),
            );
            p += res.offset;
            self.guides = res.guides;
        }
        let anchor = if mods.alt { b.center() } else { handle.anchor(b) };
        let from = handle.pos(b);
        let sx = if handle.moves_x() && (from.x - anchor.x).abs() > 1e-9 {
            (p.x - anchor.x) / (from.x - anchor.x)
        } else {
            1.0
        };
        let sy = if handle.moves_y() && (from.y - anchor.y).abs() > 1e-9 {
            (p.y - anchor.y) / (from.y - anchor.y)
        } else {
            1.0
        };
        let (mut sx, mut sy) = (sx, sy);
        if mods.shift || (ids.len() == 1 && self.keeps_ratio(origin, ids[0])) {
            if handle.is_corner() {
                let s = if sx.abs() > sy.abs() { sx } else { sy };
                sx = s.abs() * sx.signum();
                sy = s.abs() * sy.signum();
            } else if handle.moves_x() {
                sy = sx.abs();
            } else {
                sx = sy.abs();
            }
        }
        // Avoid collapsing to zero, which would lose the geometry.
        let min = 1e-3;
        if sx.abs() < min {
            sx = min.copysign(sx + f64::EPSILON);
        }
        if sy.abs() < min {
            sy = min.copysign(sy + f64::EPSILON);
        }
        let t = Affine::translate(anchor.to_vec2())
            * Affine::scale_non_uniform(sx, sy)
            * Affine::translate(-anchor.to_vec2());
        self.doc = origin.clone();
        self.doc.transform_objects(ids, t);
    }

    fn keeps_ratio(&self, doc: &Document, id: Id) -> bool {
        matches!(doc.get(id).map(|o| &o.shape), Some(Shape::Text(_)))
    }

    pub fn mouse_up(&mut self, window_pt: Point, mods: Mods) {
        let p = self.to_doc(window_pt);
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        self.guides.clear();
        match drag {
            Drag::None | Drag::Pan { .. } => {}
            Drag::Marquee { start, current, additive, nodes } => {
                let r = Rect::from_points(start, current);
                if nodes {
                    let mut found = Vec::new();
                    for o in self.doc.visible_objects() {
                        if !self.doc.is_editable(o.id) {
                            continue;
                        }
                        let Some(vp) = self.node_path(o.id) else { continue };
                        for (si, sp) in vp.subpaths.iter().enumerate() {
                            for (ni, n) in sp.nodes.iter().enumerate() {
                                if r.contains(n.p) {
                                    found.push(NodeRef { id: o.id, sp: si, node: ni });
                                }
                            }
                        }
                    }
                    if !additive {
                        self.nodes.clear();
                    }
                    for n in found {
                        if !self.selected.contains(&n.id) {
                            self.selected.push(n.id);
                        }
                        if !self.nodes.contains(&n) {
                            self.nodes.push(n);
                        }
                    }
                } else if r.width() * self.zoom > 2.0 || r.height() * self.zoom > 2.0 {
                    let ids = self.doc.objects_in_rect(&self.fonts, r);
                    if !additive {
                        self.selected.clear();
                    }
                    for id in ids {
                        if !self.selected.contains(&id) {
                            self.selected.push(id);
                        }
                    }
                }
                self.revision += 1;
            }
            Drag::Move { origin, .. }
            | Drag::Resize { origin, .. }
            | Drag::Rotate { origin, .. }
            | Drag::Nodes { origin, .. }
            | Drag::Handle { origin, .. }
            | Drag::Width { origin, .. } => self.commit(origin),
            Drag::Create { tool, start, id, origin } => {
                let tiny = (p - start).hypot() * self.zoom < 3.0;
                if tiny {
                    if tool == Tool::Line {
                        self.doc.remove(id);
                        self.selected.clear();
                    } else {
                        // Click without drag: a 100 px shape centered on the click.
                        let s = 50.0;
                        self.update_created(
                            tool,
                            id,
                            start - Vec2::new(s, s),
                            start + Vec2::new(s, s),
                            Mods::default(),
                        );
                        if tool.draws_radial() {
                            self.update_created(tool, id, start, start - Vec2::new(0.0, s), Mods::default());
                        }
                    }
                }
                self.commit(origin);
            }
            Drag::Pencil { mut points, origin } => {
                points.push(p);
                if points.len() >= 2 {
                    let closed = points.len() > 4 && (points[0] - p).hypot() * self.zoom < 10.0;
                    let vp = Self::pencil_path(&points, closed, self.pencil_smoothing, self.zoom);
                    if vp.node_count() >= 2 {
                        let id = self.create(Tool::Pencil, Shape::Path(vp.clone()), Affine::IDENTITY);
                        if closed {
                            let fill = self.shape_style.fill;
                            if let Some(o) = self.doc.get_mut(id) {
                                o.fill = fill;
                            }
                        }
                        self.pencil_last = Some(PencilStroke { id, points, closed, zoom: self.zoom, path: vp });
                        self.commit(origin);
                    }
                }
            }
            Drag::MirrorAxis { start, current } => {
                // A click without drag gives a vertical axis through that point.
                let end =
                    if (current - start).hypot() * self.zoom < 4.0 { start + Vec2::new(0.0, 1.0) } else { current };
                self.set_layer_mirror(Some(desy_core::Mirror::new(start, end)));
                // Edit the selection's points, or draw the first half.
                self.set_tool(if self.selected.is_empty() { Tool::Pen } else { Tool::Direct });
            }
            Drag::PenHandle { .. } => {
                // The point was committed on mouse down; handle edits are part of it.
                self.touch();
            }
        }
        let _ = mods;
    }

    pub fn cursor_hint(&self) -> CursorHint {
        if self.space_down || self.tool == Tool::Hand {
            return if matches!(self.drag, Drag::Pan { .. }) { CursorHint::Grabbing } else { CursorHint::Grab };
        }
        match self.tool {
            Tool::Select => {
                if let Some(h) = self.handle_at(self.mouse) {
                    return match h {
                        Handle::T | Handle::B => CursorHint::ResizeNS,
                        Handle::L | Handle::R => CursorHint::ResizeEW,
                        Handle::TL | Handle::BR => CursorHint::ResizeNWSE,
                        Handle::TR | Handle::BL => CursorHint::ResizeNESW,
                    };
                }
                if self.rotate_handle_at(self.mouse) || self.rotate_zone_at(self.mouse) {
                    return CursorHint::Rotate;
                }
                CursorHint::Arrow
            }
            Tool::Direct => CursorHint::Arrow,
            Tool::Text => CursorHint::IBeam,
            _ => CursorHint::Crosshair,
        }
    }

    // ---------------------------------------------------------------- commands

    pub fn delete(&mut self) {
        if self.tool == Tool::Direct && !self.nodes.is_empty() {
            self.edit(|ed| ed.delete_selected_nodes());
            return;
        }
        let ids = self.selected.clone();
        self.edit(|ed| {
            for id in ids {
                ed.doc.remove(id);
            }
        });
        self.selected.clear();
    }

    pub fn duplicate(&mut self) {
        let ids = self.doc.sort_by_z(&self.selected);
        let mut new_ids = Vec::new();
        self.edit(|ed| {
            for id in ids {
                if let (Some(mut o), Some(loc)) = (ed.doc.get(id).cloned(), ed.doc.find(id)) {
                    o.apply_transform(Affine::translate((10.0, 10.0)));
                    new_ids.push(ed.doc.insert(loc.layer, o));
                }
            }
        });
        self.selected = new_ids;
    }

    pub fn copy(&mut self) {
        let ids = self.doc.sort_by_z(&self.selected);
        self.clipboard = ids.iter().filter_map(|&id| self.doc.get(id).cloned()).collect();
    }

    pub fn cut(&mut self) {
        self.copy();
        self.delete();
    }

    pub fn paste(&mut self) {
        let objs = self.clipboard.clone();
        if objs.is_empty() {
            return;
        }
        let mut ids = Vec::new();
        self.edit(|ed| {
            let li = ed.target_layer();
            for mut o in objs {
                o.apply_transform(Affine::translate((10.0, 10.0)));
                ids.push(ed.doc.insert(li, o));
            }
        });
        // Paste again offsets further.
        for o in &mut self.clipboard {
            o.apply_transform(Affine::translate((10.0, 10.0)));
        }
        self.selected = ids;
    }

    pub fn nudge(&mut self, d: Vec2) {
        if self.tool == Tool::Direct && !self.nodes.is_empty() {
            self.edit(|ed| {
                for n in ed.nodes.clone() {
                    if let Some(node) = ed.node_mut(n) {
                        node.translate(d);
                    }
                }
            });
            return;
        }
        let ids = self.selected.clone();
        self.edit(|ed| ed.doc.translate_objects(&ids, d));
    }

    pub fn boolean(&mut self, op: BoolOp) {
        if self.selected.len() < 2 {
            self.status = Some("Sélectionne au moins 2 objets pour une opération booléenne".into());
            return;
        }
        let ids = self.selected.clone();
        let mut result = None;
        self.edit(|ed| result = ed.doc.boolean(&ed.fonts, &ids, op));
        if let Some(id) = result {
            self.select_only(id);
        }
    }

    pub fn convert_to_path(&mut self) {
        let ids = self.selected.clone();
        self.edit(|ed| {
            for id in ids {
                let fonts = &ed.fonts;
                if let Some(o) = ed.doc.get_mut(id) {
                    o.convert_to_path(fonts);
                }
            }
        });
    }

    pub fn outline_stroke(&mut self) {
        let ids = self.selected.clone();
        self.edit(|ed| {
            for id in ids {
                let fonts = &ed.fonts;
                if let Some(o) = ed.doc.get_mut(id) {
                    o.outline_stroke(fonts);
                }
            }
        });
    }

    /// Reorder within each object's layer, keeping the relative order of the
    /// selection. `delta`: +1 forward, -1 backward, `i64::MAX` front, `i64::MIN` back.
    pub fn reorder(&mut self, delta: i64) {
        let ids = self.selected.clone();
        if ids.is_empty() {
            return;
        }
        self.edit(|ed| {
            for layer in &mut ed.doc.layers {
                let objs = &mut layer.objects;
                let sel = |o: &Object| ids.contains(&o.id);
                if !objs.iter().any(sel) {
                    continue;
                }
                match delta {
                    i64::MAX | i64::MIN => {
                        let (picked, rest): (Vec<Object>, Vec<Object>) = objs.drain(..).partition(sel);
                        *objs = if delta == i64::MAX { [rest, picked] } else { [picked, rest] }.concat();
                    }
                    d if d > 0 => {
                        // From the top: each selected object hops over the unselected one above.
                        for i in (0..objs.len().saturating_sub(1)).rev() {
                            if sel(&objs[i]) && !sel(&objs[i + 1]) {
                                objs.swap(i, i + 1);
                            }
                        }
                    }
                    _ => {
                        for i in 1..objs.len() {
                            if sel(&objs[i]) && !sel(&objs[i - 1]) {
                                objs.swap(i, i - 1);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Drag & drop in the layers panel: move objects (z order kept) next to
    /// another object or on top of a layer.
    pub fn move_objects(&mut self, ids: &[Id], spot: DropSpot) {
        let ids = self.doc.sort_by_z(ids);
        let target_layer = match spot {
            DropSpot::AboveObject(t) | DropSpot::BelowObject(t) => {
                if ids.contains(&t) {
                    return;
                }
                self.doc.find(t).map(|l| l.layer)
            }
            DropSpot::IntoLayer(lid) => self.doc.layer_index(lid),
            DropSpot::AboveLayer(_) | DropSpot::BelowLayer(_) => None,
        };
        let Some(li) = target_layer else { return };
        if self.doc.layers[li].locked || ids.is_empty() {
            self.status = Some("Ce calque est verrouillé".into());
            return;
        }
        self.edit(|ed| {
            let objs: Vec<Object> = ids.iter().filter_map(|id| ed.doc.remove(*id)).collect();
            let index = match spot {
                DropSpot::AboveObject(t) => ed.doc.find(t).map_or(0, |l| l.index + 1),
                DropSpot::BelowObject(t) => ed.doc.find(t).map_or(0, |l| l.index),
                _ => ed.doc.layers[li].objects.len(),
            };
            let list = &mut ed.doc.layers[li].objects;
            let index = index.min(list.len());
            list.splice(index..index, objs);
            ed.active_layer = li;
        });
        self.selected = ids;
    }

    /// Drag & drop of a whole layer above or below another one.
    pub fn move_layer_to(&mut self, layer: Id, spot: DropSpot) {
        let (target, above) = match spot {
            DropSpot::AboveLayer(t) => (t, true),
            DropSpot::BelowLayer(t) => (t, false),
            _ => return,
        };
        if target == layer || self.doc.layer_index(layer).is_none() || self.doc.layer_index(target).is_none() {
            return;
        }
        let active = self.doc.layers.get(self.active_layer).map(|l| l.id);
        self.edit(|ed| {
            let from = ed.doc.layer_index(layer).unwrap();
            let l = ed.doc.layers.remove(from);
            let ti = ed.doc.layer_index(target).unwrap();
            ed.doc.layers.insert(if above { ti + 1 } else { ti }, l);
            ed.active_layer = active.and_then(|id| ed.doc.layer_index(id)).unwrap_or(0);
        });
    }

    /// Align selection: to each other when ≥2 objects, to the artboard otherwise.
    pub fn align(&mut self, how: Align) {
        let ids = self.selected.clone();
        if ids.is_empty() {
            return;
        }
        let target = if ids.len() >= 2 { self.selection_bounds() } else { Some(self.doc.artboard()) };
        let Some(t) = target else { return };
        let moves: Vec<(Id, Vec2)> = ids
            .iter()
            .filter_map(|&id| {
                let b = self.bounds_of(id)?;
                let d = match how {
                    Align::Left => Vec2::new(t.x0 - b.x0, 0.0),
                    Align::HCenter => Vec2::new(t.center().x - b.center().x, 0.0),
                    Align::Right => Vec2::new(t.x1 - b.x1, 0.0),
                    Align::Top => Vec2::new(0.0, t.y0 - b.y0),
                    Align::VCenter => Vec2::new(0.0, t.center().y - b.center().y),
                    Align::Bottom => Vec2::new(0.0, t.y1 - b.y1),
                };
                Some((id, d))
            })
            .collect();
        self.edit(|ed| {
            for (id, d) in moves {
                ed.doc.translate_objects(&[id], d);
            }
        });
    }

    pub fn flip(&mut self, horizontal: bool) {
        let Some(b) = self.selection_bounds() else { return };
        let c = b.center().to_vec2();
        let s = if horizontal { Affine::scale_non_uniform(-1.0, 1.0) } else { Affine::scale_non_uniform(1.0, -1.0) };
        let t = Affine::translate(c) * s * Affine::translate(-c);
        let ids = self.selected.clone();
        self.edit(|ed| ed.doc.transform_objects(&ids, t));
    }

    /// Angle in degrees of the first selected object's own transform.
    /// Paths bake rotations into their points, so they read 0.
    pub fn selection_rotation(&self) -> Option<f64> {
        let o = self.selected.first().and_then(|id| self.doc.get(*id))?;
        let [a, b, ..] = o.transform.as_coeffs();
        let deg = b.atan2(a).to_degrees();
        Some(if deg.abs() < 1e-9 { 0.0 } else { deg })
    }

    /// Rotate the selection about its center so its first object reads
    /// `deg` (for paths, which read 0: rotate by `deg`).
    pub fn set_rotation(&mut self, deg: f64) {
        if let Some(cur) = self.selection_rotation() {
            self.rotate_selection(deg - cur);
        }
    }

    /// Rotate the selection by `deg` about its center (one undo step).
    pub fn rotate_selection(&mut self, deg: f64) {
        let Some(b) = self.selection_bounds() else { return };
        if deg.abs() < 1e-9 {
            return;
        }
        let ids = self.selected.clone();
        let t = Affine::rotate_about(deg.to_radians(), b.center());
        self.edit(|ed| ed.doc.transform_objects(&ids, t));
    }

    /// Hide the selection, or show it again if part of it is hidden.
    pub fn toggle_selected_visible(&mut self) {
        let show = self.selected.iter().filter_map(|id| self.doc.get(*id)).any(|o| !o.visible);
        self.update_selected(|o, _| o.visible = show);
    }

    /// Apply a style change to the selection and remember it for new objects.
    /// A plain fill (or none): replaces a gradient.
    pub fn set_fill(&mut self, fill: Option<Color>) {
        let ids = self.selected.clone();
        self.edit(|ed| {
            for id in &ids {
                if let Some(o) = ed.doc.get_mut(*id) {
                    o.fill = fill;
                    o.gradient = None;
                }
            }
        });
        if ids.is_empty() || self.selection_is_shapes() {
            self.shape_style.fill = fill;
        }
    }

    /// Fill the selection with a gradient (`None`: back to its plain
    /// color). A shape without fill gets one.
    pub fn set_gradient(&mut self, g: Option<desy_core::Gradient>) {
        self.update_selected(|o, _| {
            if o.is_image() {
                return;
            }
            if let Some(g) = &g
                && o.fill.is_none()
            {
                o.fill = g.sorted_stops().first().map(|s| s.color);
            }
            o.gradient = g.clone();
        });
    }

    /// Change the gradients of the selection (one undo step).
    pub fn edit_gradient(&mut self, f: impl Fn(&mut desy_core::Gradient)) {
        self.update_selected(|o, _| {
            if let Some(g) = &mut o.gradient {
                f(g);
            }
        });
    }

    /// Gradient of the first selected object.
    pub fn selection_gradient(&self) -> Option<&desy_core::Gradient> {
        self.selected.first().and_then(|id| self.doc.get(*id)).and_then(|o| o.fill_gradient())
    }

    pub fn set_stroke(&mut self, f: impl Fn(&mut Option<Stroke>)) {
        let ids = self.selected.clone();
        self.edit(|ed| {
            for id in &ids {
                if let Some(o) = ed.doc.get_mut(*id) {
                    f(&mut o.stroke);
                }
            }
        });
        if ids.is_empty() {
            f(&mut self.line_style.stroke);
        } else if let Some(o) = ids.first().and_then(|id| self.doc.get(*id)) {
            if self.selection_is_shapes() {
                self.shape_style.stroke = o.stroke;
            } else {
                self.line_style.stroke = o.stroke;
            }
        }
    }

    fn selection_is_shapes(&self) -> bool {
        self.selected.iter().filter_map(|id| self.doc.get(*id)).all(|o| o.shape.is_closed_area())
    }

    // ---------------------------------------------------------------- rectangle corners

    /// Corner radii of the first selected rectangle (TL, TR, BR, BL), whether
    /// they are independent, and the largest radius it can take.
    pub fn rect_corners(&self) -> Option<([f64; 4], bool, f64)> {
        self.selected.iter().filter_map(|id| self.doc.get(*id)).find_map(|o| match &o.shape {
            Shape::Rect { w, h, radius, corners } => {
                Some((corners.unwrap_or([*radius; 4]), corners.is_some(), w.abs().min(h.abs()) * 0.5))
            }
            _ => None,
        })
    }

    fn set_radius_in(doc: &mut Document, ids: &[Id], r: f64) {
        for id in ids {
            if let Some(Shape::Rect { radius, corners, .. }) = doc.get_mut(*id).map(|o| &mut o.shape) {
                *radius = r.max(0.0);
                *corners = None;
            }
        }
    }

    /// Same radius on every corner of the selected rectangles (one undo step).
    pub fn set_rect_radius(&mut self, r: f64) {
        let ids = self.selected.clone();
        self.edit(|ed| Self::set_radius_in(&mut ed.doc, &ids, r));
    }

    /// Live radius while dragging the slider: no undo step, the caller
    /// commits the document it started from.
    /// Smooth editable path from raw pencil samples (`zoom` sets the
    /// screen-pixel tolerances).
    fn pencil_path(points: &[Point], closed: bool, smoothing: f64, zoom: f64) -> VectorPath {
        let bez = desy_core::freehand::fit_freehand(points, closed, smoothing, 1.0 / zoom);
        let mut vp = VectorPath::from_bezpath(&bez);
        // The fitter starts and ends a loop at the first sample: make that
        // join smooth like the others.
        if let Some(sp) = vp.subpaths.first_mut()
            && sp.closed
            && let Some(n) = sp.nodes.first_mut()
            && n.has_in()
            && n.has_out()
        {
            let dir = ((n.h_out - n.p).normalize() - (n.h_in - n.p).normalize()).normalize();
            if dir.is_finite() {
                n.h_out = n.p + dir * (n.h_out - n.p).hypot();
                n.h_in = n.p - dir * (n.h_in - n.p).hypot();
                n.kind = NodeKind::Smooth;
            }
        }
        vp
    }

    /// Change the pencil smoothing and refit the last stroke if it is still
    /// as drawn. No undo step: the caller commits.
    pub fn preview_pencil_smoothing(&mut self, smoothing: f64) {
        self.pencil_smoothing = smoothing.clamp(0.0, 1.0);
        let Some(last) = &mut self.pencil_last else { return };
        let Some(o) = self.doc.get_mut(last.id) else { return };
        if o.transform != Affine::IDENTITY || !matches!(&o.shape, Shape::Path(vp) if *vp == last.path) {
            return;
        }
        let vp = Self::pencil_path(&last.points, last.closed, self.pencil_smoothing, last.zoom);
        if vp.node_count() >= 2 {
            o.shape = Shape::Path(vp.clone());
            last.path = vp;
            self.touch();
        }
    }

    pub fn preview_rect_radius(&mut self, r: f64) {
        let ids = self.selected.clone();
        Self::set_radius_in(&mut self.doc, &ids, r);
        self.touch();
    }

    /// One corner (0 = top-left, clockwise) of the selected rectangles.
    pub fn set_rect_corner(&mut self, corner: usize, r: f64) {
        self.update_selected(|o, _| {
            if let Shape::Rect { radius, corners, .. } = &mut o.shape {
                let c = corners.get_or_insert([*radius; 4]);
                c[corner.min(3)] = r.max(0.0);
            }
        });
    }

    /// Wavy line or filled wave for the selected waves. A filled wave gets a
    /// fill, a line a stroke, so the result is never invisible.
    pub fn set_wave_filled(&mut self, on: bool) {
        let fill = self.shape_style.fill.or(Color::from_hex("#d9d9d9"));
        let stroke = self.line_style.stroke.unwrap_or_default();
        self.update_selected(|o, _| {
            if let Shape::Wave { filled, .. } = &mut o.shape {
                *filled = on;
                if on && o.fill.is_none() {
                    o.fill = fill;
                }
                if !on && o.stroke.is_none() {
                    o.stroke = Some(stroke);
                }
            }
        });
    }

    /// New random variation for the selected blobs.
    pub fn reshuffle_blobs(&mut self) {
        let base = desy_core::document::random_seed();
        let ids = self.selected.clone();
        self.update_selected(|o, _| {
            if let Shape::Blob { seed, .. } = &mut o.shape {
                let i = ids.iter().position(|id| *id == o.id).unwrap_or(0) as u32;
                *seed = base.wrapping_add(i.wrapping_mul(0x9e37_79b9));
            }
        });
    }

    /// Switch between one radius and independent corners.
    pub fn set_independent_corners(&mut self, on: bool) {
        self.update_selected(|o, _| {
            if let Shape::Rect { radius, corners, .. } = &mut o.shape {
                match (on, *corners) {
                    (true, None) => *corners = Some([*radius; 4]),
                    (false, Some(c)) => {
                        *radius = c[0];
                        *corners = None;
                    }
                    _ => {}
                }
            }
        });
    }

    pub fn update_selected(&mut self, f: impl Fn(&mut Object, &FontLibrary)) {
        let ids = self.selected.clone();
        let fonts = std::mem::take(&mut self.fonts);
        self.edit(|ed| {
            for id in &ids {
                if let Some(o) = ed.doc.get_mut(*id) {
                    f(o, &fonts);
                }
            }
        });
        self.fonts = fonts;
    }

    /// Set the width factor of the selected nodes.
    pub fn set_node_width(&mut self, w: f64) {
        let nodes = self.nodes.clone();
        self.edit(|ed| {
            for n in nodes {
                if let Some(node) = ed.node_mut(n) {
                    node.width = w.clamp(0.0, 40.0);
                }
            }
        });
    }

    pub fn selected_node_width(&self) -> Option<f64> {
        let n = self.nodes.first()?;
        let vp = self.node_path(n.id)?;
        Some(vp.subpaths.get(n.sp)?.nodes.get(n.node)?.width)
    }

    /// Sets the stroke of the selected paths to a classic tapered brush.
    pub fn taper_selected(&mut self) {
        let ids = self.selected.clone();
        let line = self.line_style.stroke;
        self.edit(|ed| {
            for id in &ids {
                let Some(vp) = ed.ensure_path(*id) else { continue };
                for sp in &mut vp.subpaths {
                    let n = sp.nodes.len();
                    if sp.closed || n < 2 {
                        continue;
                    }
                    for (i, node) in sp.nodes.iter_mut().enumerate() {
                        let t = i as f64 / (n - 1) as f64;
                        node.width = (std::f64::consts::PI * t).sin().max(0.0) * 1.4;
                    }
                    sp.nodes[0].width = 0.0;
                    sp.nodes[n - 1].width = 0.0;
                }
                if let Some(o) = ed.doc.get_mut(*id) {
                    if o.stroke.is_none() {
                        o.stroke = line;
                    }
                    if let Some(s) = &mut o.stroke {
                        s.cap = LineCap::Round;
                    }
                }
            }
        });
    }

    /// Place an image centered in the visible area, scaled down to fit the artboard.
    pub fn add_image(&mut self, asset: desy_core::ImageAsset) -> Id {
        let (iw, ih) = (asset.width.max(1) as f64, asset.height.max(1) as f64);
        let name = asset.name.clone();
        let limit = self.doc.width.min(self.doc.height) * 0.8;
        let s = (limit / iw.max(ih)).min(1.0);
        let (w, h) = (iw * s, ih * s);
        let (origin, size) = self.visible_area();
        let center = self.to_doc(self.canvas_origin + origin + size * 0.5);
        let mut id = 0;
        self.edit(|ed| {
            let asset_id = ed.doc.add_image_asset(asset);
            let li = ed.target_layer();
            let mut obj = new_object(
                Shape::Image { asset: asset_id, w, h },
                Affine::translate((center.x - w / 2.0, center.y - h / 2.0)),
                None,
                None,
            );
            obj.name = name;
            id = ed.doc.insert(li, obj);
        });
        self.select_only(id);
        self.tool = Tool::Select;
        id
    }

    /// Reset selected images to their pixel size.
    pub fn image_original_size(&mut self) {
        let sizes: Vec<_> = self
            .selected
            .iter()
            .filter_map(|id| match self.doc.get(*id).map(|o| &o.shape) {
                Some(Shape::Image { asset, .. }) => {
                    self.doc.images.get(asset).map(|a| (*id, a.width as f64, a.height as f64))
                }
                _ => None,
            })
            .collect();
        if sizes.is_empty() {
            return;
        }
        self.edit(|ed| {
            for (id, w0, h0) in &sizes {
                if let Some(Shape::Image { w, h, .. }) = ed.doc.get_mut(*id).map(|o| &mut o.shape) {
                    *w = *w0;
                    *h = *h0;
                }
            }
        });
    }

    /// Turn the selection into tracing references: bottom "Références" layer,
    /// semi-transparent, locked, not exported. Unlock from the layers panel.
    pub fn make_reference(&mut self) {
        let ids = self.doc.sort_by_z(&self.selected);
        if ids.is_empty() {
            return;
        }
        let active = self.doc.layers.get(self.active_layer).map(|l| l.id);
        self.edit(|ed| {
            let li = ed.doc.reference_layer();
            for id in &ids {
                if let Some(mut o) = ed.doc.remove(*id) {
                    o.reference = true;
                    o.locked = true;
                    if o.opacity >= 0.99 {
                        o.opacity = 0.4;
                    }
                    ed.doc.layers[li].objects.push(o);
                }
            }
            // Creating the reference layer shifts indices: keep drawing where we were,
            // never on the reference layer itself.
            let active_index = active.and_then(|id| ed.doc.layer_index(id)).filter(|&i| i != li);
            ed.active_layer = match active_index {
                Some(i) => i,
                None if ed.doc.layers.len() > 1 => ed.doc.layers.len() - 1,
                None => {
                    ed.doc.add_layer(None);
                    ed.doc.layers.len() - 1
                }
            };
        });
        self.clear_selection();
        self.status = Some("Référence verrouillée — déverrouille-la dans les calques pour la modifier".into());
    }

    pub fn clear_reference(&mut self, id: Id) {
        self.edit(|ed| {
            if let Some(o) = ed.doc.get_mut(id) {
                o.reference = false;
                o.opacity = 1.0;
                o.locked = false;
            }
        });
    }

    /// Layer targeted by the mirror controls: the selection's, else the active one.
    pub fn mirror_layer(&self) -> usize {
        self.selected
            .first()
            .and_then(|id| self.doc.find(*id))
            .map_or(self.active_layer, |l| l.layer)
            .min(self.doc.layers.len().saturating_sub(1))
    }

    /// Set (or remove) the mirror of the whole layer: everything it holds,
    /// including what is drawn afterwards, is reflected.
    pub fn set_layer_mirror(&mut self, mirror: Option<desy_core::Mirror>) {
        let li = self.mirror_layer();
        if self.doc.layers.get(li).is_none_or(|l| l.locked) {
            self.status = Some("Ce calque est verrouillé".into());
            return;
        }
        self.edit(|ed| {
            let l = &mut ed.doc.layers[li];
            // Keep the merge preference when moving an existing axis.
            l.mirror = mirror.map(|m| desy_core::Mirror { merge: l.mirror.map_or(m.merge, |old| old.merge), ..m });
        });
    }

    /// Mirror the layer across the artboard's vertical or horizontal center.
    pub fn mirror_layer_center(&mut self, vertical: bool) {
        let c = self.doc.artboard().center();
        self.set_layer_mirror(Some(if vertical {
            desy_core::Mirror::vertical(c.x)
        } else {
            desy_core::Mirror::horizontal(c.y)
        }));
    }

    pub fn toggle_layer_mirror_merge(&mut self) {
        let li = self.mirror_layer();
        self.edit(|ed| {
            if let Some(m) = ed.doc.layers.get_mut(li).and_then(|l| l.mirror.as_mut()) {
                m.merge = !m.merge;
            }
        });
    }

    /// Bake the layer mirror into its objects.
    pub fn apply_layer_mirror(&mut self) {
        let li = self.mirror_layer();
        self.edit(|ed| ed.doc.apply_layer_mirror(li, &ed.fonts));
    }

    /// Set (or remove) the mirror of the selected shapes only.
    pub fn set_mirror(&mut self, mirror: Option<desy_core::Mirror>) {
        let ids = self.selected.clone();
        self.edit(|ed| {
            for id in &ids {
                if let Some(o) = ed.doc.get_mut(*id)
                    && !o.is_image()
                {
                    // Keep the merge preference when moving an existing axis.
                    o.mirror =
                        mirror.map(|m| desy_core::Mirror { merge: o.mirror.map_or(m.merge, |old| old.merge), ..m });
                }
            }
        });
    }

    /// Mirror the selection across the artboard's vertical or horizontal center.
    pub fn mirror_artboard_center(&mut self, vertical: bool) {
        let c = self.doc.artboard().center();
        self.set_mirror(Some(if vertical {
            desy_core::Mirror::vertical(c.x)
        } else {
            desy_core::Mirror::horizontal(c.y)
        }));
    }

    pub fn apply_mirror(&mut self) {
        self.update_selected(|o, fonts| o.apply_mirror(fonts));
    }

    pub fn reset_widths(&mut self) {
        let ids = self.selected.clone();
        self.edit(|ed| {
            for id in &ids {
                if let Some(Shape::Path(vp)) = ed.doc.get_mut(*id).map(|o| &mut o.shape) {
                    for n in vp.subpaths.iter_mut().flat_map(|s| s.nodes.iter_mut()) {
                        n.width = 1.0;
                    }
                }
            }
        });
    }

    // ---------------------------------------------------------------- layers

    pub fn add_layer(&mut self) {
        self.edit(|ed| {
            ed.doc.add_layer(None);
            ed.active_layer = ed.doc.layers.len() - 1;
        });
    }

    pub fn delete_layer(&mut self, li: usize) {
        if self.doc.layers.len() <= 1 {
            return;
        }
        self.edit(|ed| {
            ed.doc.layers.remove(li);
        });
    }

    pub fn move_layer(&mut self, li: usize, up: bool) {
        let target = if up { li + 1 } else { li.wrapping_sub(1) };
        if target >= self.doc.layers.len() {
            return;
        }
        self.edit(|ed| {
            ed.doc.layers.swap(li, target);
            ed.active_layer = target;
        });
    }

    /// Move the selection to a layer (top of it).
    pub fn move_selection_to_layer(&mut self, li: usize) {
        let ids = self.doc.sort_by_z(&self.selected);
        self.edit(|ed| {
            for id in ids {
                if let Some(o) = ed.doc.remove(id) {
                    ed.doc.layers[li].objects.push(o);
                }
            }
            ed.active_layer = li;
        });
    }
}

/// Drop position in the layers panel ("above" = higher in z order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropSpot {
    AboveObject(Id),
    BelowObject(Id),
    /// On a layer header: top of that layer.
    IntoLayer(Id),
    AboveLayer(Id),
    BelowLayer(Id),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorHint {
    Arrow,
    Crosshair,
    IBeam,
    Grab,
    Grabbing,
    ResizeNS,
    ResizeEW,
    ResizeNWSE,
    ResizeNESW,
    Rotate,
}

/// Constrain `p` to 45° increments around `origin`.
pub fn constrain_45(origin: Point, p: Point) -> Point {
    let d = p - origin;
    let step = std::f64::consts::FRAC_PI_4;
    let a = (d.atan2() / step).round() * step;
    let len = d.hypot();
    origin + Vec2::new(a.cos(), a.sin()) * len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        let mut e = Editor::new(Document::new("t", 500.0, 500.0), None, FontLibrary::empty());
        e.canvas_size = Vec2::new(500.0, 500.0);
        e.zoom = 1.0;
        e.pan = Vec2::ZERO;
        e
    }

    fn drag(e: &mut Editor, a: (f64, f64), b: (f64, f64), mods: Mods) {
        e.mouse_down(Point::new(a.0, a.1), mods, 1, false);
        e.mouse_move(Point::new((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), mods);
        e.mouse_move(Point::new(b.0, b.1), mods);
        e.mouse_up(Point::new(b.0, b.1), mods);
    }

    #[test]
    fn corners_can_be_set_together_or_one_by_one() {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        drag(&mut e, (100.0, 100.0), (200.0, 160.0), Mods::default());
        assert_eq!(e.rect_corners(), Some(([0.0; 4], false, 30.0)));
        e.set_rect_radius(12.0);
        assert_eq!(e.rect_corners().unwrap().0, [12.0; 4]);
        e.set_rect_corner(2, 5.0);
        assert_eq!(e.rect_corners(), Some(([12.0, 12.0, 5.0, 12.0], true, 30.0)));
        e.set_independent_corners(false);
        assert_eq!(e.rect_corners(), Some(([12.0; 4], false, 30.0)));
        // The slider previews without undo steps.
        let before = e.history.can_undo();
        let origin = e.doc.clone();
        e.preview_rect_radius(20.0);
        e.preview_rect_radius(25.0);
        e.commit(origin);
        assert_eq!(e.rect_corners().unwrap().0, [25.0; 4]);
        e.undo();
        assert_eq!(e.rect_corners().unwrap().0, [12.0; 4]);
        assert!(before);
    }

    #[test]
    fn pencil_strokes_are_smoothed_and_can_be_refit() {
        let mut e = editor();
        e.set_tool(Tool::Pencil);
        // A shaky wave drawn by hand: 200 samples with ±1.5 px jitter.
        let pt = |i: usize| {
            let x = 50.0 + i as f64 * 2.0;
            let y = 250.0 + 60.0 * (x / 60.0).sin() + if i.is_multiple_of(2) { 1.5 } else { -1.5 };
            Point::new(x, y)
        };
        e.mouse_down(pt(0), Mods::default(), 1, false);
        for i in 1..200 {
            e.mouse_move(pt(i), Mods::default());
        }
        e.mouse_up(pt(199), Mods::default());
        let count = |e: &Editor| match &e.doc.get(e.selected[0]).unwrap().shape {
            Shape::Path(vp) => vp.node_count(),
            _ => panic!("not a path"),
        };
        let drawn = count(&e);
        assert!(drawn <= 12, "{drawn} nodes");
        // Rounder after the fact, in one undo step.
        let origin = e.doc.clone();
        e.preview_pencil_smoothing(0.0);
        let sharp = count(&e);
        e.preview_pencil_smoothing(1.0);
        e.commit(origin);
        let round = count(&e);
        assert!(round <= drawn && drawn <= sharp, "{sharp} ≥ {drawn} ≥ {round}");
        e.undo();
        assert_eq!(count(&e), drawn);
    }

    #[test]
    fn dragging_the_rotation_handle_rotates_the_selection() {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        drag(&mut e, (100.0, 100.0), (200.0, 200.0), Mods::default());
        e.set_tool(Tool::Select);
        let h = e.rotate_handle().unwrap();
        assert_eq!(h, Point::new(150.0, 100.0 - ROTATE_HANDLE_OFFSET));
        assert!(e.rotate_handle_at(h));
        // Drag the handle a quarter turn around the center (150, 150).
        drag(&mut e, (h.x, h.y), (150.0 + 150.0 - h.y, 150.0), Mods::default());
        let o = e.doc.get(e.selected[0]).unwrap();
        let [a, b, ..] = o.transform.as_coeffs();
        assert!(a.abs() < 1e-6 && (b - 1.0).abs() < 1e-6, "rotated 90°: {:?}", o.transform);
        let c = e.selection_bounds().unwrap().center();
        assert!((c - Point::new(150.0, 150.0)).hypot() < 1e-6);
    }

    #[test]
    fn right_click_selects_what_is_under_the_pointer() {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        drag(&mut e, (10.0, 10.0), (60.0, 60.0), Mods::default());
        let a = e.selected[0];
        drag(&mut e, (100.0, 100.0), (150.0, 150.0), Mods::default());
        let b = e.selected[0];
        e.set_tool(Tool::Select);

        e.context_select(Point::new(30.0, 30.0));
        assert_eq!(e.selected, vec![a]);
        // Already selected: the selection is kept.
        e.selected = vec![a, b];
        e.context_select(Point::new(120.0, 120.0));
        assert_eq!(e.selected, vec![a, b]);
        // Empty canvas outside the selection box clears it.
        e.context_select(Point::new(400.0, 400.0));
        assert!(e.selected.is_empty());
    }

    #[test]
    fn wave_tool_draws_a_line_that_can_be_filled() {
        let mut e = editor();
        e.line_style.stroke = Some(Stroke::default());
        e.shape_style.fill = Color::from_hex("#3366ff");
        e.set_tool(Tool::Wave);
        drag(&mut e, (50.0, 100.0), (350.0, 160.0), Mods::default());
        let id = e.selected[0];
        let o = e.doc.get(id).unwrap();
        assert!(matches!(o.shape, Shape::Wave { waves: 3, filled: false, .. }));
        assert!(o.fill.is_none() && o.stroke.is_some(), "starts as a stroked line");
        let b = e.selection_bounds().unwrap();
        assert!((b.x0 - 50.0).abs() < 1.0 && (b.x1 - 350.0).abs() < 1.0 && (b.height() - 60.0).abs() < 1.0, "{b:?}");

        e.set_wave_filled(true);
        let o = e.doc.get(id).unwrap();
        assert!(o.is_closed_area() && o.fill == Color::from_hex("#3366ff"));
        e.undo();
        assert!(!e.doc.get(id).unwrap().is_closed_area());
    }

    #[test]
    fn blob_tool_draws_from_the_center_and_reshuffles() {
        let mut e = editor();
        e.set_tool(Tool::Blob);
        drag(&mut e, (200.0, 200.0), (200.0, 120.0), Mods::default());
        let id = e.selected[0];
        let Some(Shape::Blob { radius, seed, .. }) = e.doc.get(id).map(|o| o.shape.clone()) else { panic!() };
        assert!((radius - 80.0).abs() < 1e-9);
        let c = e.selection_bounds().unwrap().center();
        assert!((c - Point::new(200.0, 200.0)).hypot() < 40.0, "{c:?}");

        e.reshuffle_blobs();
        assert!(matches!(e.doc.get(id).unwrap().shape, Shape::Blob { seed: s, .. } if s != seed));

        // A click without drag makes a 100 px blob.
        drag(&mut e, (400.0, 400.0), (400.0, 400.0), Mods::default());
        assert!(matches!(e.doc.get(e.selected[0]).unwrap().shape, Shape::Blob { radius, .. } if radius == 50.0));
    }

    #[test]
    fn every_tool_but_symmetry_is_in_exactly_one_rail_group() {
        for t in Tool::ALL {
            let n = Tool::GROUPS.iter().filter(|g| g.contains(&t)).count();
            if t == Tool::Mirror {
                assert_eq!(n, 0);
                assert_eq!(t.group(), None);
                continue;
            }
            assert_eq!(n, 1, "{t:?}");
            assert!(Tool::GROUPS[t.group().unwrap()].contains(&t));
        }
    }

    #[test]
    fn fit_centers_the_artboard_in_the_area_left_by_the_panels() {
        let mut e = editor();
        e.canvas_size = Vec2::new(1400.0, 900.0);
        e.view_insets = (80.0, 72.0, 324.0, 48.0);
        e.fit();
        let center = e.pan + Vec2::new(250.0, 250.0) * e.zoom;
        assert!((center.x - (80.0 + (1400.0 - 80.0 - 324.0) / 2.0)).abs() < 1e-6);
        assert!((center.y - (72.0 + (900.0 - 72.0 - 48.0) / 2.0)).abs() < 1e-6);
        assert!(e.pan.x >= 80.0 && e.pan.y >= 72.0);
    }

    #[test]
    fn draw_rect_then_move_with_snap_and_undo() {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        drag(&mut e, (100.0, 100.0), (200.0, 150.0), Mods::default());
        assert_eq!(e.doc.objects().count(), 1);
        let id = e.selected[0];
        assert_eq!(e.bounds_of(id).unwrap(), Rect::new(100.0, 100.0, 200.0, 150.0));

        e.set_tool(Tool::Select);
        // Center would land at x=248; snaps to the artboard center (250).
        drag(&mut e, (150.0, 120.0), (248.0, 120.0), Mods::default());
        let b = e.bounds_of(id).unwrap();
        assert!((b.center().x - 250.0).abs() < 1e-9, "{b:?}");

        e.undo();
        assert_eq!(e.bounds_of(id).unwrap(), Rect::new(100.0, 100.0, 200.0, 150.0));
        e.redo();
        assert!((e.bounds_of(id).unwrap().center().x - 250.0).abs() < 1e-9);
    }

    #[test]
    fn pen_draws_and_closes() {
        let mut e = editor();
        e.snapping = false;
        e.set_tool(Tool::Pen);
        for p in [(10.0, 10.0), (100.0, 10.0), (100.0, 100.0)] {
            e.mouse_down(Point::new(p.0, p.1), Mods::default(), 1, false);
            e.mouse_up(Point::new(p.0, p.1), Mods::default());
        }
        e.mouse_down(Point::new(10.0, 10.0), Mods::default(), 1, false);
        e.mouse_up(Point::new(10.0, 10.0), Mods::default());
        assert!(e.pen.is_none());
        let o = e.doc.objects().next().unwrap();
        let Shape::Path(vp) = &o.shape else { panic!() };
        assert!(vp.subpaths[0].closed);
        assert_eq!(vp.subpaths[0].nodes.len(), 3);
    }

    #[test]
    fn resize_from_corner_and_boolean() {
        let mut e = editor();
        e.snapping = false;
        e.set_tool(Tool::Rect);
        drag(&mut e, (0.0, 0.0), (100.0, 100.0), Mods::default());
        e.set_tool(Tool::Ellipse);
        drag(&mut e, (50.0, 50.0), (150.0, 150.0), Mods::default());
        e.set_tool(Tool::Select);
        e.select_all();
        e.boolean(BoolOp::Union);
        assert_eq!(e.doc.objects().count(), 1);
        let id = e.selected[0];
        let b = e.bounds_of(id).unwrap();
        assert!((b.x1 - 150.0).abs() < 1e-6);
        // Drag bottom-right handle to double the size.
        drag(&mut e, (150.0, 150.0), (300.0, 300.0), Mods::default());
        let b = e.bounds_of(id).unwrap();
        assert!((b.x1 - 300.0).abs() < 1e-6 && b.x0.abs() < 1e-6, "{b:?}");
    }

    #[test]
    fn image_import_and_reference() {
        let mut e = editor();
        let asset = desy_core::ImageAsset {
            name: "chat.png".into(),
            width: 1000,
            height: 500,
            data: std::sync::Arc::new(vec![1, 2, 3]),
        };
        let id = e.add_image(asset);
        // Scaled to 80% of the 500px artboard, centered in the view.
        assert_eq!(e.bounds_of(id).unwrap(), Rect::new(50.0, 150.0, 450.0, 350.0));
        e.make_reference();
        let o = e.doc.get(id).unwrap();
        assert!(o.reference && o.locked && (o.opacity - 0.4).abs() < 1e-6);
        assert_eq!(e.doc.layers[0].name, "Références");
        assert!(e.selected.is_empty());
        // New content does not land in the reference layer.
        assert_ne!(e.doc.layers[e.active_layer].name, "Références");
        // Locked: clicking it selects nothing.
        e.set_tool(Tool::Select);
        e.mouse_down(Point::new(250.0, 250.0), Mods::default(), 1, false);
        e.mouse_up(Point::new(250.0, 250.0), Mods::default());
        assert!(e.selected.is_empty());
        e.undo();
        assert!(!e.doc.get(id).unwrap().reference);
    }

    #[test]
    fn mirror_tool_links_points_on_axis() {
        let mut e = editor();
        e.snapping = false;
        // Left half of a shape: both ends at x = 250.
        e.set_tool(Tool::Pen);
        for p in [(250.0, 100.0), (150.0, 200.0), (250.0, 300.0)] {
            e.mouse_down(Point::new(p.0, p.1), Mods::default(), 1, false);
            e.mouse_up(Point::new(p.0, p.1), Mods::default());
        }
        e.finish_pen();
        let id = e.selected[0];
        // Click with the mirror tool: vertical axis through x = 250.
        e.set_tool(Tool::Mirror);
        e.mouse_down(Point::new(250.0, 400.0), Mods::default(), 1, false);
        e.mouse_up(Point::new(250.0, 400.0), Mods::default());
        assert_eq!(e.tool, Tool::Direct);
        assert!(e.doc.layers[0].mirror.is_some(), "the axis belongs to the layer");
        assert!(e.doc.get(id).unwrap().mirror.is_none());
        let o = e.doc.effective(id).unwrap();
        assert!(o.is_closed_area(), "ends on the axis are merged into one closed path");
        assert_eq!(e.bounds_of(id).unwrap(), Rect::new(150.0, 100.0, 350.0, 300.0));

        // Dragging the top point (on the axis) sideways keeps it on the axis.
        e.mouse_down(Point::new(250.0, 100.0), Mods::default(), 1, false);
        e.mouse_move(Point::new(280.0, 80.0), Mods::default());
        e.mouse_up(Point::new(280.0, 80.0), Mods::default());
        let Shape::Path(vp) = &e.doc.get(id).unwrap().shape else { panic!() };
        assert_eq!(vp.subpaths[0].nodes[0].p, Point::new(250.0, 80.0));
        // Moving an off-axis point moves its reflection too.
        e.mouse_down(Point::new(150.0, 200.0), Mods::default(), 1, false);
        e.mouse_move(Point::new(100.0, 200.0), Mods::default());
        e.mouse_up(Point::new(100.0, 200.0), Mods::default());
        assert_eq!(e.bounds_of(id).unwrap(), Rect::new(100.0, 80.0, 400.0, 300.0));
    }

    #[test]
    fn layer_mirror_reflects_new_drawings() {
        let mut e = editor();
        e.snapping = false;
        // No selection: the axis goes on the active layer, then draw.
        e.set_tool(Tool::Mirror);
        e.mouse_down(Point::new(250.0, 50.0), Mods::default(), 1, false);
        e.mouse_up(Point::new(250.0, 50.0), Mods::default());
        assert_eq!(e.tool, Tool::Pen, "ready to draw the first half");
        e.set_tool(Tool::Rect);
        drag(&mut e, (100.0, 100.0), (150.0, 150.0), Mods::default());
        let id = e.selected[0];
        assert_eq!(e.bounds_of(id).unwrap(), Rect::new(100.0, 100.0, 400.0, 150.0));
        // The reflection can be clicked to select the shape.
        e.set_tool(Tool::Select);
        e.clear_selection();
        e.mouse_down(Point::new(375.0, 125.0), Mods::default(), 1, false);
        e.mouse_up(Point::new(375.0, 125.0), Mods::default());
        assert_eq!(e.selected, vec![id]);
        // Baking keeps the picture and frees the layer.
        e.apply_layer_mirror();
        assert!(e.doc.layers[0].mirror.is_none());
        assert_eq!(e.bounds_of(id).unwrap(), Rect::new(100.0, 100.0, 400.0, 150.0));
        e.undo();
        assert!(e.doc.layers[0].mirror.is_some());
    }

    #[test]
    fn reorder_and_drag_in_layers() {
        let mut e = editor();
        e.snapping = false;
        e.set_tool(Tool::Rect);
        let mut ids = Vec::new();
        for i in 0..4 {
            let x = 50.0 + i as f64 * 60.0;
            drag(&mut e, (x, 50.0), (x + 40.0, 90.0), Mods::default());
            ids.push(e.selected[0]);
        }
        let order = |e: &Editor| e.doc.layers.iter().flat_map(|l| l.objects.iter().map(|o| o.id)).collect::<Vec<_>>();
        let [a, b, c, d] = ids[..] else { panic!() };
        // Two neighbours forward: they keep their relative order.
        e.selected = vec![a, b];
        e.reorder(1);
        assert_eq!(order(&e), vec![c, a, b, d]);
        e.reorder(i64::MAX);
        assert_eq!(order(&e), vec![c, d, a, b]);
        e.reorder(i64::MIN);
        assert_eq!(order(&e), vec![a, b, c, d]);
        e.reorder(-1);
        assert_eq!(order(&e), vec![a, b, c, d], "already at the back");
        // Drag d below a, then into a new layer.
        e.move_objects(&[d], DropSpot::BelowObject(a));
        assert_eq!(order(&e), vec![d, a, b, c]);
        e.move_objects(&[c], DropSpot::AboveObject(d));
        assert_eq!(order(&e), vec![d, c, a, b]);
        e.add_layer();
        let top = e.doc.layers[1].id;
        e.move_objects(&[a, d], DropSpot::IntoLayer(top));
        assert_eq!(e.doc.layers[1].objects.iter().map(|o| o.id).collect::<Vec<_>>(), vec![d, a]);
        // Layers: drag the bottom one above the top one.
        let bottom = e.doc.layers[0].id;
        e.move_layer_to(bottom, DropSpot::AboveLayer(top));
        assert_eq!(e.doc.layers[1].id, bottom);
        e.undo();
        assert_eq!(e.doc.layers[0].id, bottom);
    }

    #[test]
    fn width_tool_sets_node_width() {
        let mut e = editor();
        e.snapping = false;
        e.set_tool(Tool::Line);
        drag(&mut e, (100.0, 100.0), (300.0, 100.0), Mods::default());
        e.set_tool(Tool::Width);
        // Grab the middle of the line and pull 12px away: width 24 at that point.
        drag(&mut e, (200.0, 100.0), (200.0, 112.0), Mods::default());
        let o = e.doc.objects().next().unwrap();
        let Shape::Path(vp) = &o.shape else { panic!() };
        assert_eq!(vp.subpaths[0].nodes.len(), 3);
        let base = o.stroke.unwrap().width;
        assert!((vp.subpaths[0].nodes[1].width * base - 24.0).abs() < 0.1);
    }
}
