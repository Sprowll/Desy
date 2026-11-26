//! Pixels for what GPUI cannot draw as plain paths: gradient fills and
//! render effects (shadows, glows, blur, noise). The object's own SVG — the
//! export's, so the canvas matches the files — is rasterized by resvg at the
//! current zoom and cached.
//!
//! The SVG is written with the object moved next to the origin and serves as
//! the cache key: moving an object reuses its pixels, only a change of look,
//! shape or zoom renders it again.

use desy_core::document::Rendered;
use desy_core::{Document, Object, svg};
use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};
use kurbo::{Affine, Rect};
use resvg::{tiny_skia, usvg};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Longest side of a raster in pixels; beyond, it is drawn at a lower
/// resolution and stretched.
const MAX_SIDE: f64 = 4096.0;

/// An object's pixels and the document area they cover.
#[derive(Clone)]
pub struct Raster {
    pub area: Rect,
    pub image: Arc<RenderImage>,
}

struct Cached {
    /// Covered area relative to the object's paint area origin.
    local: Rect,
    image: Arc<RenderImage>,
}

thread_local! {
    static CACHE: RefCell<HashMap<u64, Cached>> = RefCell::new(HashMap::new());
    static USED: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
}

/// Pixels of `o` (as rendered in `r`) at `scale` screen pixels per document
/// unit.
pub fn raster(doc: &Document, o: &Object, r: &Rendered, scale: f64) -> Option<Raster> {
    let area = svg::paint_area(o, r)?;
    let origin = area.origin().to_vec2();
    let shift = Affine::translate(-origin);
    let moved = |p: &kurbo::BezPath| {
        let mut p = p.clone();
        p.apply_affine(shift);
        p
    };
    let local = Rendered { fill: moved(&r.fill), stroke: moved(&r.stroke), path: moved(&r.path) };
    let (text, local_area) = svg::object_svg(doc, o, &local)?;

    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    scale.to_bits().hash(&mut h);
    let key = h.finish();
    USED.with(|u| u.borrow_mut().insert(key));
    let hit = CACHE.with(|c| c.borrow().get(&key).map(|c| (c.local, c.image.clone())));
    let (local, image) = match hit {
        Some(hit) => hit,
        None => {
            let (local, image) = rasterize(&text, local_area, scale)?;
            CACHE.with(|c| c.borrow_mut().insert(key, Cached { local, image: image.clone() }));
            (local, image)
        }
    };
    Some(Raster { area: local + origin, image })
}

/// Forget the pixels of objects not drawn since the last call (once per
/// canvas frame).
pub fn end_frame() {
    let used = USED.with(|u| std::mem::take(&mut *u.borrow_mut()));
    CACHE.with(|c| c.borrow_mut().retain(|k, _| used.contains(k)));
}

fn rasterize(text: &str, area: Rect, scale: f64) -> Option<(Rect, Arc<RenderImage>)> {
    let s = scale.min(MAX_SIDE / area.width().max(area.height()).max(1.0));
    let w = (area.width() * s).ceil().max(1.0) as u32;
    let h = (area.height() * s).ceil().max(1.0) as u32;
    let tree = usvg::Tree::from_str(text, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(s as f32, s as f32), &mut pixmap.as_mut());
    // tiny-skia keeps premultiplied RGBA; GPUI wants straight BGRA.
    let mut buf = Vec::with_capacity((w * h * 4) as usize);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        buf.extend_from_slice(&[c.blue(), c.green(), c.red(), c.alpha()]);
    }
    let img = RgbaImage::from_raw(w, h, buf)?;
    // The whole pixels cover slightly more than the area.
    let covered = Rect::new(area.x0, area.y0, area.x0 + w as f64 / s, area.y0 + h as f64 / s);
    Some((covered, Arc::new(RenderImage::new([Frame::new(img)]))))
}

#[cfg(test)]
mod tests {
    use super::{CACHE, end_frame, raster};
    use desy_core::effects::{Effect, EffectKind};
    use desy_core::{Color, Document, FontLibrary, Shape, document::new_object};
    use kurbo::Affine;

    #[test]
    fn moving_reuses_the_pixels() {
        let mut doc = Document::new("t", 400.0, 400.0);
        let mut o = new_object(
            Shape::Rect { w: 50.0, h: 50.0, radius: 0.0, corners: None },
            Affine::translate((20.0, 20.0)),
            Some(Color::BLACK),
            None,
        );
        o.effects = vec![Effect::new(EffectKind::Blur { radius: 4.0 })];
        let id = doc.insert(0, o);
        let fonts = FontLibrary::empty();
        let o = doc.get(id).unwrap().clone();
        let a = raster(&doc, &o, &o.render(&fonts), 2.0).unwrap();
        let mut moved = o.clone();
        moved.apply_transform(Affine::translate((100.0, 30.0)));
        let b = raster(&doc, &moved, &moved.render(&fonts), 2.0).unwrap();
        assert!(std::sync::Arc::ptr_eq(&a.image, &b.image));
        assert!((b.area.x0 - a.area.x0 - 100.0).abs() < 1e-6);
        // Blurred: the pixels reach past the 50 px square.
        assert!(a.area.width() > 55.0);
        end_frame();
        end_frame();
        assert!(CACHE.with(|c| c.borrow().is_empty()));
    }
}
