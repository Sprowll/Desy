//! Eyedropper tool (I): samples the color under the cursor (stroke, fill,
//! image pixel or artboard background) and applies it.

use crate::editor::{Editor, Mods};
use desy_core::{Color, Object, Shape, Stroke};
use kurbo::{Point, Shape as _};

/// Image pixels more transparent than this let the eyedropper see below.
const MIN_ALPHA: f32 = 0.05;

impl Editor {
    /// Color visible at a document point: topmost visible object (locked ones
    /// included), else the artboard background.
    pub fn sample_color(&self, p: Point) -> Option<Color> {
        let objects: Vec<_> = self.doc.visible_effective().collect();
        for o in objects.iter().rev() {
            if let Some(c) = self.sample_object(o, p) {
                return Some(c);
            }
        }
        if self.doc.artboard().contains(p) { self.doc.background } else { None }
    }

    fn sample_object(&self, o: &Object, p: Point) -> Option<Color> {
        if let Shape::Image { asset, w, h } = o.shape {
            let local = o.transform.inverse() * p;
            if w <= 0.0 || h <= 0.0 || !(0.0..w).contains(&local.x) || !(0.0..h).contains(&local.y) {
                return None;
            }
            let c = crate::images::pixel(self.doc.images.get(&asset)?, local.x / w, local.y / h)?;
            return (c.a > MIN_ALPHA).then_some(c);
        }
        let r = self.rendered(o);
        if let Some(s) = o.stroke
            && !r.stroke.is_empty()
            && r.stroke.winding(p) != 0
        {
            return Some(s.color);
        }
        let fill = o.fill?;
        let w = r.fill.winding(p);
        let inside = if o.fills_even_odd() { w % 2 != 0 } else { w != 0 };
        inside.then_some(fill)
    }

    /// Fill (or stroke) of the selection; without a selection, the color of
    /// the next drawings.
    pub fn apply_color(&mut self, color: Color, to_stroke: bool) {
        if to_stroke {
            let line = self.line_style.stroke.unwrap_or_default();
            self.set_stroke(|s| match s {
                Some(s) => s.color = color,
                None => *s = Some(Stroke { color, ..line }),
            });
        } else {
            self.set_fill(Some(color));
        }
    }

    /// Click with the eyedropper: fill, ⇧ stroke, ⌥ add to the palette.
    pub fn eyedrop(&mut self, p: Point, mods: Mods) {
        let Some(color) = self.sample_color(p) else {
            self.status = Some("Aucune couleur ici".into());
            return;
        };
        if mods.alt {
            let mut added = false;
            self.edit(|ed| added = ed.doc.palette_add(color));
            self.status = Some(if added {
                format!("{} ajoutée à la palette", color.to_hex())
            } else {
                format!("{} est déjà dans la palette", color.to_hex())
            });
        } else {
            self.apply_color(color, mods.shift);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::{Editor, Mods, Tool};
    use desy_core::document::new_object;
    use desy_core::{Color, Document, FontLibrary, Shape, Stroke};
    use kurbo::{Affine, Point, Vec2};

    fn hex(h: &str) -> Color {
        Color::from_hex(h).unwrap()
    }

    fn editor() -> Editor {
        let mut doc = Document::new("t", 200.0, 200.0);
        doc.background = Some(Color::WHITE);
        let mut e = Editor::new(doc, None, FontLibrary::empty());
        e.canvas_size = Vec2::new(200.0, 200.0);
        e.zoom = 1.0;
        e.pan = Vec2::ZERO;
        e
    }

    #[test]
    fn samples_stroke_fill_locked_and_background() {
        let mut e = editor();
        let below = e.doc.insert(
            0,
            new_object(
                Shape::Rect { w: 100.0, h: 100.0, radius: 0.0, corners: None },
                Affine::translate((0.0, 0.0)),
                Some(hex("#ff0000")),
                None,
            ),
        );
        e.doc.get_mut(below).unwrap().locked = true;
        e.doc.insert(
            0,
            new_object(
                Shape::Rect { w: 40.0, h: 40.0, radius: 0.0, corners: None },
                Affine::translate((50.0, 50.0)),
                None,
                Some(Stroke { color: hex("#0000ff"), width: 10.0, ..Default::default() }),
            ),
        );
        // Inside the unfilled square: the red rectangle below shows through.
        assert_eq!(e.sample_color(Point::new(70.0, 70.0)), Some(hex("#ff0000")));
        assert_eq!(e.sample_color(Point::new(50.0, 70.0)), Some(hex("#0000ff")));
        assert_eq!(e.sample_color(Point::new(150.0, 150.0)), Some(Color::WHITE));
        assert_eq!(e.sample_color(Point::new(-10.0, 10.0)), None);
    }

    #[test]
    fn click_applies_fill_stroke_or_adds_to_palette() {
        let mut e = editor();
        let green = hex("#00aa00");
        e.doc.insert(
            0,
            new_object(
                Shape::Rect { w: 50.0, h: 50.0, radius: 0.0, corners: None },
                Affine::IDENTITY,
                Some(green),
                None,
            ),
        );
        let target = e.doc.insert(
            0,
            new_object(
                Shape::Ellipse { w: 20.0, h: 20.0 },
                Affine::translate((150.0, 150.0)),
                Some(Color::BLACK),
                None,
            ),
        );
        e.select_only(target);
        e.set_tool(Tool::Eyedropper);

        e.mouse_down(Point::new(10.0, 10.0), Mods::default(), 1, false);
        e.mouse_up(Point::new(10.0, 10.0), Mods::default());
        assert_eq!(e.doc.get(target).unwrap().fill, Some(green));

        let shift = Mods { shift: true, ..Default::default() };
        e.mouse_down(Point::new(10.0, 10.0), shift, 1, false);
        assert_eq!(e.doc.get(target).unwrap().stroke.map(|s| s.color), Some(green));

        e.doc.palette.clear();
        let alt = Mods { alt: true, ..Default::default() };
        e.mouse_down(Point::new(10.0, 10.0), alt, 1, false);
        assert_eq!(e.doc.palette, vec![green]);
        e.undo();
        assert!(e.doc.palette.is_empty());
    }
}
