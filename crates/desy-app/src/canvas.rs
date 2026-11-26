//! The drawing surface: paints the document and the editing overlays, and
//! forwards pointer input to the editor.

use crate::editor::{CursorHint, Drag, Editor, HANDLE_RADIUS, Handle, Mods, ROTATE_HANDLE_OFFSET, Tool};
use crate::paint::{fill_bez, stroke_bez, stroke_bez_dashed};
use crate::theme;
use crate::view::EditorView;
use desy_core::Shape;
use desy_core::document::Rendered;
use desy_core::snap::Guide;
use gpui_kit::*;
use kurbo::{Affine, BezPath, Circle, Point as KPoint, Rect, Shape as _};
use std::rc::Rc;
use std::sync::Arc;

/// One object ready to paint (shared by the canvas and the project thumbnails).
pub struct Item {
    r: Rc<Rendered>,
    fill: Option<Hsla>,
    stroke: Option<Hsla>,
    even_odd: bool,
    image: Option<Arc<RenderImage>>,
    /// Pixels for gradients and render effects (canvas only; thumbnails
    /// fall back to plain colors).
    raster: Option<crate::raster::Raster>,
}

impl Item {
    pub fn new(o: &desy_core::Object, r: Rc<Rendered>, doc: &desy_core::Document) -> Self {
        let alpha = |c: desy_core::Color| {
            let mut h = theme::to_hsla(c);
            h.a *= o.opacity;
            h
        };
        let image = match &o.shape {
            Shape::Image { asset, .. } => doc.images.get(asset).and_then(|a| crate::images::texture(a, o.opacity)),
            _ => None,
        };
        // Without pixels, a gradient shows its first color.
        let fill =
            o.fill.map(|c| o.fill_gradient().and_then(|g| g.sorted_stops().first().map(|s| s.color)).unwrap_or(c));
        Item {
            r,
            fill: fill.map(alpha),
            stroke: o.stroke.filter(|s| s.width > 0.0).map(|s| alpha(s.color)),
            even_odd: o.fills_even_odd(),
            image,
            raster: None,
        }
    }
}

/// Paint objects bottom-to-top with a document → window transform.
pub fn paint_items(items: &[Item], view: Affine, window: &mut Window) {
    for item in items {
        if let Some(r) = &item.raster {
            let b = view.transform_rect_bbox(r.area);
            let bounds =
                Bounds::from_corners(point(px(b.x0 as f32), px(b.y0 as f32)), point(px(b.x1 as f32), px(b.y1 as f32)));
            let _ = window.paint_image(bounds, bounds, Corners::default(), r.image.clone(), 0, false);
            continue;
        }
        if let Some(img) = &item.image {
            let b = view.transform_rect_bbox(item.r.path.bounding_box());
            let bounds =
                Bounds::from_corners(point(px(b.x0 as f32), px(b.y0 as f32)), point(px(b.x1 as f32), px(b.y1 as f32)));
            let _ = window.paint_image(bounds, bounds, Corners::default(), img.clone(), 0, false);
        }
        if let Some(c) = item.fill {
            fill_bez(window, &item.r.fill, view, c, item.even_odd);
        }
        if let Some(c) = item.stroke {
            fill_bez(window, &item.r.stroke, view, c, false);
        }
    }
}

/// Everything the paint pass needs, in document coordinates.
pub struct Scene {
    artboard: Rect,
    background: Option<Hsla>,
    items: Vec<Item>,
    hover: Option<BezPath>,
    selected_paths: Vec<BezPath>,
    sel_bounds: Option<Rect>,
    show_handles: bool,
    rotatable: bool,
    nodes: Vec<(KPoint, bool, bool)>,
    handles: Vec<(KPoint, KPoint)>,
    width_markers: Vec<(KPoint, f64)>,
    guides: Vec<Guide>,
    marquee: Option<Rect>,
    preview: Option<BezPath>,
    /// Mirror axes to show: (a, b, being drawn).
    axes: Vec<(KPoint, KPoint, bool)>,
    /// Eyedropper: cursor (document space) and the color under it.
    eyedropper: Option<(KPoint, Hsla)>,
    zoom: f64,
}

fn kp(p: gpui_kit::Point<Pixels>) -> KPoint {
    KPoint::new(f32::from(p.x) as f64, f32::from(p.y) as f64)
}

pub fn mods(m: &Modifiers) -> Mods {
    Mods { shift: m.shift, alt: m.alt, cmd: m.platform || m.control }
}

impl Editor {
    pub fn scene(&self) -> Scene {
        let doc = &self.doc;
        let mut items = Vec::new();
        for o in doc.visible_effective() {
            let r = self.rendered(&o);
            let mut item = Item::new(&o, r.clone(), doc);
            if o.needs_pixels() {
                item.raster = crate::raster::raster(doc, &o, &r, self.zoom * self.pixel_scale);
            }
            items.push(item);
        }
        crate::raster::end_frame();

        let hover = self
            .hover
            .filter(|h| !self.selected.contains(h) && matches!(self.drag, Drag::None))
            .and_then(|h| doc.effective(h))
            .map(|o| self.rendered(&o).path.clone());

        let selected_paths =
            self.selected.iter().filter_map(|id| doc.effective(*id)).map(|o| self.rendered(&o).path.clone()).collect();

        let mut nodes = Vec::new();
        let mut handles = Vec::new();
        let mut width_markers = Vec::new();
        if matches!(self.tool, Tool::Direct | Tool::Pen | Tool::Width) {
            for &id in &self.selected {
                let Some(vp) = self.node_path(id) else { continue };
                let base = doc.get(id).and_then(|o| o.stroke).map(|s| s.width).unwrap_or(0.0);
                for (si, sp) in vp.subpaths.iter().enumerate() {
                    for (ni, n) in sp.nodes.iter().enumerate() {
                        let sel = self.nodes.iter().any(|r| r.id == id && r.sp == si && r.node == ni);
                        nodes.push((n.p, sel, n.kind != desy_core::NodeKind::Corner));
                        if self.tool == Tool::Width && (n.width - 1.0).abs() > 1e-6 || (self.tool == Tool::Width && sel)
                        {
                            width_markers.push((n.p, n.width * base * 0.5));
                        }
                    }
                }
            }
            for (_, _, p, h) in self.visible_handles() {
                handles.push((p, h));
            }
        }

        let marquee = match &self.drag {
            Drag::Marquee { start, current, .. } => Some(Rect::from_points(*start, *current)),
            _ => None,
        };

        // The layer's axis stays visible while drawing in it.
        let layer_axis = doc.layers.get(self.mirror_layer()).filter(|l| l.visible).and_then(|l| l.mirror);
        let own = self.selected.iter().filter_map(|id| doc.get(*id)).filter_map(|o| o.mirror);
        let mut axes: Vec<(KPoint, KPoint, bool)> =
            layer_axis.into_iter().chain(own).map(|m| (m.a, m.b, false)).collect();
        if let Drag::MirrorAxis { start, current } = &self.drag {
            let end = if (*current - *start).hypot() * self.zoom < 4.0 {
                *start + kurbo::Vec2::new(0.0, 1.0)
            } else {
                *current
            };
            axes.push((*start, end, true));
        }

        let preview = match &self.drag {
            Drag::Pencil { points, .. } if points.len() > 1 => {
                let mut p = BezPath::new();
                p.move_to(points[0]);
                for q in &points[1..] {
                    p.line_to(*q);
                }
                Some(p)
            }
            Drag::None if self.tool == Tool::Pen => self.pen.and_then(|s| {
                let Some(Shape::Path(vp)) = doc.get(s.id).map(|o| &o.shape) else { return None };
                let last = vp.subpaths.get(s.sp)?.nodes.last()?;
                let mut p = BezPath::new();
                p.move_to(last.p);
                p.curve_to(last.h_out, self.mouse, self.mouse);
                Some(p)
            }),
            _ => None,
        };

        let show_handles = self.tool == Tool::Select && !self.selected.is_empty();
        Scene {
            artboard: doc.artboard(),
            background: doc.background.map(theme::to_hsla),
            items,
            hover,
            selected_paths,
            sel_bounds: if self.tool == Tool::Select { self.selection_bounds() } else { None },
            show_handles,
            rotatable: self.can_rotate(),
            nodes,
            handles,
            width_markers,
            guides: self.guides.clone(),
            marquee,
            preview,
            axes,
            eyedropper: self
                .eyedropper_preview
                .filter(|_| self.tool == Tool::Eyedropper && matches!(self.drag, Drag::None) && !self.space_down)
                .map(|c| (self.mouse, theme::to_hsla(c))),
            zoom: self.zoom,
        }
    }
}

fn paint_scene(scene: &Scene, view: Affine, window: &mut Window) {
    // Artboard with a soft shadow.
    let ab = view.transform_rect_bbox(scene.artboard);
    let shadow = ab.inflate(1.0, 1.0);
    window.paint_quad(fill(
        Bounds::from_corners(
            point(px(shadow.x0 as f32), px(shadow.y0 as f32 + 2.0)),
            point(px(shadow.x1 as f32), px(shadow.y1 as f32 + 3.0)),
        ),
        hsla(0.0, 0.0, 0.0, 0.45),
    ));
    let ab_bounds =
        Bounds::from_corners(point(px(ab.x0 as f32), px(ab.y0 as f32)), point(px(ab.x1 as f32), px(ab.y1 as f32)));
    match scene.background {
        Some(bg) => window.paint_quad(fill(ab_bounds, bg)),
        None => paint_checker(window, ab),
    }

    paint_items(&scene.items, view, window);

    let accent = theme::selection();
    if let Some(h) = &scene.hover {
        stroke_bez(window, h, view, 1.5, accent);
    }
    for p in &scene.selected_paths {
        stroke_bez(window, p, view, 1.0, accent);
    }
    if let Some(b) = scene.sel_bounds {
        let r = view.transform_rect_bbox(b);
        stroke_bez(window, &r.to_path(0.1), Affine::IDENTITY, 1.0, accent);
        if scene.show_handles && scene.rotatable {
            // Rotation handle: a round knob on a stem above the top edge.
            let top = view * Handle::T.pos(b);
            let knob = top - kurbo::Vec2::new(0.0, ROTATE_HANDLE_OFFSET);
            let mut stem = BezPath::new();
            stem.move_to(top);
            stem.line_to(knob);
            stroke_bez(window, &stem, Affine::IDENTITY, 1.0, accent);
            let c = Circle::new(knob, HANDLE_RADIUS * 0.85).to_path(0.1);
            fill_bez(window, &c, Affine::IDENTITY, gpui_kit::white(), false);
            stroke_bez(window, &c, Affine::IDENTITY, 1.5, accent);
        }
        if scene.show_handles {
            for h in Handle::ALL {
                let p = view * h.pos(b);
                let s = HANDLE_RADIUS * 0.75;
                let sq = Rect::new(p.x - s, p.y - s, p.x + s, p.y + s).to_path(0.1);
                fill_bez(window, &sq, Affine::IDENTITY, gpui_kit::white(), false);
                stroke_bez(window, &sq, Affine::IDENTITY, 1.0, accent);
            }
        }
    }
    for (n, h) in &scene.handles {
        let a = view * *n;
        let b = view * *h;
        let mut line = BezPath::new();
        line.move_to(a);
        line.line_to(b);
        stroke_bez(window, &line, Affine::IDENTITY, 1.0, accent);
        let c = Circle::new(b, 3.5).to_path(0.1);
        fill_bez(window, &c, Affine::IDENTITY, accent, false);
    }
    for (p, sel, smooth) in &scene.nodes {
        let p = view * *p;
        let s = 3.5;
        let shape = if *smooth {
            Circle::new(p, s + 0.5).to_path(0.1)
        } else {
            Rect::new(p.x - s, p.y - s, p.x + s, p.y + s).to_path(0.1)
        };
        fill_bez(window, &shape, Affine::IDENTITY, if *sel { accent } else { gpui_kit::white() }, false);
        stroke_bez(window, &shape, Affine::IDENTITY, 1.0, accent);
    }
    for (p, r) in &scene.width_markers {
        let c = view * *p;
        let rr = r * scene.zoom;
        let mut line = BezPath::new();
        line.move_to(KPoint::new(c.x - rr, c.y));
        line.line_to(KPoint::new(c.x + rr, c.y));
        let ring = Circle::new(c, rr.max(2.0)).to_path(0.1);
        stroke_bez(window, &ring, Affine::IDENTITY, 1.0, theme::guide());
    }
    if let Some(p) = &scene.preview {
        stroke_bez(window, p, view, 1.0, accent);
    }
    for (a, b, drawing) in &scene.axes {
        let line = long_line(*a, *b, view);
        stroke_bez_dashed(window, &line, 1.5, if *drawing { theme::guide() } else { hsla(0.79, 0.9, 0.62, 1.0) });
    }

    let guide = theme::guide();
    for g in &scene.guides {
        let mut line = BezPath::new();
        match *g {
            Guide::Vertical { x, y0, y1 } => {
                line.move_to(view * KPoint::new(x, y0));
                line.line_to(view * KPoint::new(x, y1));
            }
            Guide::Horizontal { y, x0, x1 } => {
                line.move_to(view * KPoint::new(x0, y));
                line.line_to(view * KPoint::new(x1, y));
            }
            Guide::Line { a, b } => line = long_line(a, b, view),
        }
        stroke_bez(window, &line, Affine::IDENTITY, 1.0, guide);
    }

    if let Some(m) = scene.marquee {
        let r = view.transform_rect_bbox(m).to_path(0.1);
        fill_bez(window, &r, Affine::IDENTITY, theme::accent_soft(), false);
        stroke_bez(window, &r, Affine::IDENTITY, 1.0, accent);
    }

    // Eyedropper: the sampled color in a ring next to the cursor.
    if let Some((p, color)) = scene.eyedropper {
        let c = view * p + kurbo::Vec2::new(18.0, -18.0);
        let disc = Circle::new(c, 11.0).to_path(0.1);
        fill_bez(window, &disc, Affine::IDENTITY, color, false);
        stroke_bez(window, &Circle::new(c, 12.0).to_path(0.1), Affine::IDENTITY, 2.0, gpui_kit::white());
        stroke_bez(window, &Circle::new(c, 13.5).to_path(0.1), Affine::IDENTITY, 1.0, hsla(0.0, 0.0, 0.0, 0.45));
    }
}

/// A line through `a` and `b` long enough to cross the whole canvas, in window space.
fn long_line(a: KPoint, b: KPoint, view: Affine) -> BezPath {
    let (a, b) = (view * a, view * b);
    let d = b - a;
    let d = if d.hypot() < 1e-9 { kurbo::Vec2::new(0.0, 1.0) } else { d.normalize() };
    let mut line = BezPath::new();
    line.move_to(a - d * 1e5);
    line.line_to(a + d * 1e5);
    line
}

fn paint_checker(window: &mut Window, r: Rect) {
    window.paint_quad(fill(
        Bounds::from_corners(point(px(r.x0 as f32), px(r.y0 as f32)), point(px(r.x1 as f32), px(r.y1 as f32))),
        hsla(0.0, 0.0, 0.92, 1.0),
    ));
    let s = 10.0;
    let mut path = BezPath::new();
    let mut y = r.y0;
    let mut row = 0;
    while y < r.y1 {
        let mut x = r.x0 + if row % 2 == 0 { 0.0 } else { s };
        while x < r.x1 {
            path.extend(Rect::new(x, y, (x + s).min(r.x1), (y + s).min(r.y1)).to_path(0.1));
            x += 2.0 * s;
        }
        y += s;
        row += 1;
    }
    fill_bez(window, &path, Affine::IDENTITY, hsla(0.0, 0.0, 0.82, 1.0), false);
}

impl EditorView {
    pub fn render_canvas(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ed.pixel_scale = window.scale_factor() as f64;
        let scene = self.ed.scene();
        let entity = cx.entity();
        let cursor = match self.ed.cursor_hint() {
            CursorHint::Arrow => CursorStyle::Arrow,
            CursorHint::Crosshair | CursorHint::Rotate => CursorStyle::Crosshair,
            CursorHint::IBeam => CursorStyle::IBeam,
            CursorHint::Grab => CursorStyle::OpenHand,
            CursorHint::Grabbing => CursorStyle::ClosedHand,
            CursorHint::ResizeNS => CursorStyle::ResizeUpDown,
            CursorHint::ResizeEW => CursorStyle::ResizeLeftRight,
            CursorHint::ResizeNWSE => CursorStyle::ResizeUpLeftDownRight,
            CursorHint::ResizeNESW => CursorStyle::ResizeUpRightDownLeft,
        };
        let _ = window;

        let paint_entity = entity.clone();
        let surface = canvas(
            move |bounds, _window, cx| {
                entity.update(cx, |v, cx| {
                    let ed = &mut v.ed;
                    ed.canvas_origin = kp(bounds.origin);
                    let size =
                        kurbo::Vec2::new(f32::from(bounds.size.width) as f64, f32::from(bounds.size.height) as f64);
                    let resized = (size - ed.canvas_size).hypot() > 0.5;
                    ed.canvas_size = size;
                    if ed.needs_fit && size.x > 10.0 {
                        ed.fit();
                        ed.needs_fit = false;
                        cx.notify();
                    } else if resized {
                        cx.notify();
                    }
                    Affine::translate(kp(bounds.origin).to_vec2()) * ed.view()
                })
            },
            move |bounds, view, window, _cx| {
                window.paint_quad(fill(bounds, theme::canvas_bg()));
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    paint_scene(&scene, view, window);
                });

                // Moves and releases are tracked window-wide so drags keep
                // working when the pointer leaves the canvas.
                let e = paint_entity.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    e.update(cx, |v, cx| {
                        let dragging = !matches!(v.ed.drag, Drag::None);
                        if dragging || bounds.contains(&ev.position) {
                            v.ed.mouse_move(kp(ev.position), mods(&ev.modifiers));
                            cx.notify();
                        } else if v.ed.hover.take().is_some() {
                            cx.notify();
                        }
                    });
                });
                let e = paint_entity.clone();
                window.on_mouse_event(move |ev: &MouseUpEvent, phase, _window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    e.update(cx, |v, cx| {
                        if !matches!(v.ed.drag, Drag::None) {
                            v.ed.mouse_up(kp(ev.position), mods(&ev.modifiers));
                            v.after_edit(cx);
                        }
                    });
                });
            },
        )
        .size_full();

        div()
            .id("canvas")
            .key_context("Canvas")
            .track_focus(&self.canvas_focus)
            .relative()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .cursor(cursor)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|v, ev: &MouseDownEvent, window, cx| {
                    window.focus(&v.canvas_focus, cx);
                    if !v.ed.space_down && v.picker_eyedrop_click(kp(ev.position), window, cx) {
                        v.after_edit(cx);
                        return;
                    }
                    v.ed.mouse_down(kp(ev.position), mods(&ev.modifiers), ev.click_count, false);
                    v.after_edit(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|v, ev: &MouseDownEvent, window, cx| {
                    window.focus(&v.canvas_focus, cx);
                    if !matches!(v.ed.drag, Drag::None) || v.ed.pen.is_some() {
                        return;
                    }
                    v.tool_menu = None;
                    v.ed.context_select(kp(ev.position));
                    v.context_menu = Some(ev.position);
                    v.after_edit(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|v, ev: &MouseDownEvent, window, cx| {
                    window.focus(&v.canvas_focus, cx);
                    v.ed.mouse_down(kp(ev.position), mods(&ev.modifiers), ev.click_count, true);
                    cx.notify();
                }),
            )
            .on_scroll(cx.listener(|v, ev: &ScrollEvent, _window, cx| {
                let d = ev.delta.pixel_delta(px(20.0));
                let (dx, dy) = (f32::from(d.x) as f64, f32::from(d.y) as f64);
                if ev.modifiers.platform || ev.modifiers.control {
                    v.ed.zoom_at((1.0 + dy * 0.01).clamp(0.5, 2.0), kp(ev.position));
                } else {
                    v.ed.pan += kurbo::Vec2::new(dx, dy);
                }
                cx.notify();
            }))
            .on_drop(cx.listener(|v, paths: &ExternalPaths, window, cx| {
                let images: Vec<_> =
                    paths.paths().iter().filter(|p| crate::images::is_image_path(p)).cloned().collect();
                if !images.is_empty() {
                    v.import_images(images, Some(kp(window.mouse_position())), cx);
                }
                let palettes: Vec<_> =
                    paths.paths().iter().filter(|p| crate::palette_panel::is_palette_path(p)).cloned().collect();
                if !palettes.is_empty() {
                    v.import_palettes(palettes, cx);
                }
            }))
            .on_pinch(cx.listener(|v, ev: &PinchEvent, _window, cx| {
                v.ed.zoom_at((1.0 + ev.delta as f64).clamp(0.5, 2.0), kp(ev.position));
                cx.notify();
            }))
            .on_key_down(cx.listener(|v, ev: &KeyDownEvent, _window, cx| {
                if ev.keystroke.key == "space" && !v.ed.space_down {
                    v.ed.space_down = true;
                    cx.notify();
                }
            }))
            .on_key_up(cx.listener(|v, ev: &KeyUpEvent, _window, cx| {
                if ev.keystroke.key == "space" {
                    v.ed.space_down = false;
                    cx.notify();
                }
            }))
            .child(surface)
    }

    /// One-line help for the current tool.
    pub fn tool_hint(&self) -> &'static str {
        if self.picker_eyedrop_pending() && self.ed.tool == Tool::Eyedropper {
            return "Clic : prendre cette couleur dans le sélecteur · Échap : annuler";
        }
        match self.ed.tool {
            Tool::Pen if self.ed.pen.is_some() => {
                "Clic : point · Glisser : courbe · Clic sur le 1er point : fermer · Entrée/Échap : terminer"
            }
            Tool::Pen => "Clic : point · Glisser : courbe · Clic sur un tracé sélectionné : ajouter un point",
            Tool::Direct => {
                "Glisser un point ou une poignée · Alt+clic : lisse/angle · Double-clic sur le tracé : ajouter un point · Suppr : supprimer"
            }
            Tool::Width => "Glisser depuis un tracé pour régler son épaisseur à cet endroit",
            Tool::Pencil => "Dessine à main levée · Termine près du départ pour fermer",
            Tool::Select => {
                "Maj : contraindre · Alt+glisser : dupliquer · Poignée ronde ou coins : rotation (Maj : 15°) · ⌘ : sans magnétisme"
            }
            Tool::Hand => "Glisser pour déplacer la vue · ⌘ + molette ou pincement : zoom",
            Tool::Mirror => "Trace l'axe de symétrie du calque",
            Tool::Text => "Clic : placer un texte",
            Tool::Eyedropper => {
                "Clic : remplissage · Maj+clic : contour · Alt+clic : ajouter à la palette · Fonctionne aussi sur les images"
            }
            _ => "Maj : proportions · Alt : depuis le centre",
        }
    }
}
