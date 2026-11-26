//! SVG export. Uniform strokes are exported as SVG strokes; variable-width
//! strokes are exported as filled outlines. Gradients become SVG gradients
//! and render effects (shadows, glows, blur, noise) one SVG filter chain
//! per object.

use crate::color::Color;
use crate::document::{Document, Gradient, GradientKind, Object, Rendered, Shape};
use crate::effects::{self, Effect, EffectKind};
use crate::stroke::{LineCap, LineJoin};
use crate::text::FontLibrary;
use kurbo::{Rect, Shape as _};
use std::fmt::Write;

fn paint(c: Color) -> String {
    if c.a < 1.0 { format!("{}\" fill-opacity=\"{:.3}", c.to_hex(), c.a) } else { c.to_hex() }
}

fn stroke_paint(c: Color) -> String {
    if c.a < 1.0 { format!("{}\" stroke-opacity=\"{:.3}", c.to_hex(), c.a) } else { c.to_hex() }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn export(doc: &Document, fonts: &FontLibrary) -> String {
    let mut body = String::new();
    let mut defs = String::new();
    if let Some(bg) = doc.background {
        let _ = writeln!(body, r#"  <rect width="100%" height="100%" fill="{}"/>"#, paint(bg));
    }
    for layer in doc.layers.iter().filter(|l| l.visible) {
        let _ = writeln!(body, r#"  <g id="{}">"#, escape(&layer.name));
        for o in layer.objects.iter().filter(|o| o.visible && !o.reference) {
            let o = &*layer.effective(o);
            let r = o.render(fonts);
            write_object(&mut body, &mut defs, doc, o, &r);
        }
        let _ = writeln!(body, "  </g>");
    }
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
        w = doc.width,
        h = doc.height
    );
    if !defs.is_empty() {
        let _ = write!(s, "  <defs>\n{defs}  </defs>\n");
    }
    s.push_str(&body);
    s.push_str("</svg>\n");
    s
}

/// One object alone as an SVG document whose view box is the area it
/// paints (render effects included), for drawing it as pixels. Returns the
/// SVG and that area in document coordinates.
pub fn object_svg(doc: &Document, o: &Object, r: &Rendered) -> Option<(String, Rect)> {
    let area = paint_area(o, r)?;
    let mut body = String::new();
    let mut defs = String::new();
    write_object(&mut body, &mut defs, doc, o, r);
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{x} {y} {w} {h}">"#,
        x = area.x0,
        y = area.y0,
        w = area.width(),
        h = area.height()
    );
    if !defs.is_empty() {
        let _ = write!(s, "<defs>\n{defs}</defs>\n");
    }
    s.push_str(&body);
    s.push_str("</svg>\n");
    Some((s, area))
}

/// Area an object paints: its fill and stroke, grown by its render effects.
pub fn paint_area(o: &Object, r: &Rendered) -> Option<Rect> {
    let mut b: Option<Rect> = None;
    for p in [&r.fill, &r.stroke] {
        if !p.elements().is_empty() {
            let pb = p.bounding_box();
            b = Some(b.map_or(pb, |x| x.union(pb)));
        }
    }
    let margin: f64 = effects::render_effects(&o.effects).map(|k| k.margin()).sum();
    b.map(|b| b.inflate(margin + 2.0, margin + 2.0))
}

/// Write an object (and its gradient and filter into `defs`).
fn write_object(s: &mut String, defs: &mut String, doc: &Document, o: &Object, r: &Rendered) {
    let opacity = if o.opacity < 1.0 { format!(r#" opacity="{:.3}""#, o.opacity) } else { String::new() };
    if let Shape::Image { asset, w, h } = &o.shape {
        if let Some(a) = doc.images.get(asset) {
            let [.., x, y] = o.transform.as_coeffs();
            let _ = writeln!(
                s,
                r#"    <image id="{}" x="{x}" y="{y}" width="{w}" height="{h}" preserveAspectRatio="none" href="{}"{opacity}/>"#,
                escape(&o.name),
                a.data_uri()
            );
        }
        return;
    }
    let variable = o.variable_stroke();
    let rule = if o.fills_even_odd() { r#" fill-rule="evenodd""# } else { "" };
    let fill = match (o.fill, o.fill_gradient()) {
        (Some(_), Some(g)) if !r.fill.elements().is_empty() => {
            let id = format!("gradient-{}", o.id);
            write_gradient(defs, &id, g, r.fill.bounding_box());
            format!("url(#{id})")
        }
        (Some(c), _) => paint(c),
        (None, _) => "none".into(),
    };
    let filter = paint_area(o, r).and_then(|area| {
        let id = format!("effects-{}", o.id);
        write_filter(defs, &id, &o.effects, area).then(|| format!(r#" filter="url(#{id})""#))
    });
    let mut attrs = format!(r#"fill="{fill}"{rule}"#);
    if let (Some(st), false) = (o.stroke, variable) {
        let cap = match st.cap {
            LineCap::Butt => "butt",
            LineCap::Round => "round",
            LineCap::Square => "square",
        };
        let join = match st.join {
            LineJoin::Miter => "miter",
            LineJoin::Round => "round",
            LineJoin::Bevel => "bevel",
        };
        let _ = write!(
            attrs,
            r#" stroke="{}" stroke-width="{}" stroke-linecap="{cap}" stroke-linejoin="{join}""#,
            stroke_paint(st.color),
            st.width
        );
    }
    let label = escape(&o.name);
    if variable || filter.is_some() {
        let _ = writeln!(s, r#"    <g id="{label}"{opacity}{}>"#, filter.unwrap_or_default());
        if variable {
            if o.fill.is_some() {
                let _ = writeln!(s, r#"      <path d="{}" fill="{fill}"{rule}/>"#, r.path.to_svg());
            }
            if let Some(st) = o.stroke {
                let outline = crate::stroke::clean(&r.stroke);
                let _ = writeln!(
                    s,
                    r#"      <path d="{}" fill="{}" fill-rule="evenodd"/>"#,
                    outline.to_svg(),
                    paint(st.color)
                );
            }
        } else {
            let _ = writeln!(s, r#"      <path d="{}" {attrs}/>"#, r.path.to_svg());
        }
        let _ = writeln!(s, "    </g>");
    } else {
        let _ = writeln!(s, r#"    <path id="{label}" d="{}" {attrs}{opacity}/>"#, r.path.to_svg());
    }
}

fn write_gradient(defs: &mut String, id: &str, g: &Gradient, bounds: Rect) {
    let stops: String = g
        .sorted_stops()
        .iter()
        .map(|st| {
            format!(
                r#"<stop offset="{:.4}" stop-color="{}" stop-opacity="{:.3}"/>"#,
                st.offset.clamp(0.0, 1.0),
                st.color.to_hex(),
                st.color.a
            )
        })
        .collect();
    match g.kind {
        GradientKind::Linear => {
            let (a, b) = g.line(bounds);
            let _ = writeln!(
                defs,
                r#"    <linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}">{stops}</linearGradient>"#,
                a.x, a.y, b.x, b.y
            );
        }
        GradientKind::Radial => {
            let c = bounds.center();
            let radius = bounds.width().max(bounds.height()) / 2.0;
            let _ = writeln!(
                defs,
                r#"    <radialGradient id="{id}" gradientUnits="userSpaceOnUse" cx="{:.3}" cy="{:.3}" r="{:.3}">{stops}</radialGradient>"#,
                c.x, c.y, radius
            );
        }
    }
}

/// Flood `color` into the alpha of `mask`, named `out`.
fn colorize(f: &mut String, mask: &str, color: Color, out: &str) {
    let _ = write!(
        f,
        r#"<feFlood flood-color="{}" flood-opacity="{:.3}" result="{out}-c"/><feComposite in="{out}-c" in2="{mask}" operator="in" result="{out}"/>"#,
        color.to_hex(),
        color.a
    );
}

/// The render effects as one SVG filter chain, in stack order. Returns
/// false when there are none.
fn write_filter(defs: &mut String, id: &str, effects: &[Effect], area: Rect) -> bool {
    let mut f = String::new();
    let mut cur = "SourceGraphic".to_string();
    for (i, kind) in effects::render_effects(effects).enumerate() {
        let n = format!("e{i}");
        // Glows are shadows that do not move.
        let kind = match *kind {
            EffectKind::OuterGlow { blur, spread, color } => {
                EffectKind::DropShadow { x: 0.0, y: 0.0, blur, spread, color }
            }
            EffectKind::InnerGlow { blur, color } => EffectKind::InnerShadow { x: 0.0, y: 0.0, blur, color },
            k => k,
        };
        match kind {
            EffectKind::DropShadow { x, y, blur, spread, color } => {
                let mut src = cur.clone();
                if spread.abs() > 1e-6 {
                    let op = if spread > 0.0 { "dilate" } else { "erode" };
                    let _ = write!(
                        f,
                        r#"<feMorphology in="{cur}" operator="{op}" radius="{:.3}" result="{n}-s"/>"#,
                        spread.abs()
                    );
                    src = format!("{n}-s");
                }
                let _ = write!(
                    f,
                    r#"<feGaussianBlur in="{src}" stdDeviation="{:.3}" result="{n}-b"/><feOffset in="{n}-b" dx="{x:.3}" dy="{y:.3}" result="{n}-o"/>"#,
                    blur / 2.0
                );
                colorize(&mut f, &format!("{n}-o"), color, &format!("{n}-k"));
                let _ =
                    write!(f, r#"<feMerge result="{n}"><feMergeNode in="{n}-k"/><feMergeNode in="{cur}"/></feMerge>"#);
            }
            EffectKind::InnerShadow { x, y, blur, color } => {
                // Shape alpha minus its moved, blurred self: a band along
                // the edges facing away from the offset.
                let _ = write!(
                    f,
                    r#"<feOffset in="{cur}" dx="{x:.3}" dy="{y:.3}" result="{n}-o"/><feGaussianBlur in="{n}-o" stdDeviation="{:.3}" result="{n}-b"/><feComposite in="{cur}" in2="{n}-b" operator="out" result="{n}-m"/>"#,
                    blur / 2.0
                );
                colorize(&mut f, &format!("{n}-m"), color, &format!("{n}-k"));
                let _ =
                    write!(f, r#"<feMerge result="{n}"><feMergeNode in="{cur}"/><feMergeNode in="{n}-k"/></feMerge>"#);
            }
            EffectKind::Blur { radius } => {
                let _ = write!(f, r#"<feGaussianBlur in="{cur}" stdDeviation="{:.3}" result="{n}"/>"#, radius / 2.0);
            }
            EffectKind::Noise { amount, size, seed } => {
                // Gray fractal noise, contrasted, at `amount` opacity, kept
                // inside the shape and blended over it.
                let _ = write!(
                    f,
                    r#"<feTurbulence type="fractalNoise" baseFrequency="{:.4}" numOctaves="2" seed="{seed}" result="{n}-t"/><feColorMatrix in="{n}-t" type="saturate" values="0" result="{n}-g"/><feComponentTransfer in="{n}-g" result="{n}-a"><feFuncR type="linear" slope="2.5" intercept="-0.75"/><feFuncG type="linear" slope="2.5" intercept="-0.75"/><feFuncB type="linear" slope="2.5" intercept="-0.75"/><feFuncA type="linear" slope="0" intercept="{:.3}"/></feComponentTransfer><feComposite in="{n}-a" in2="{cur}" operator="in" result="{n}-m"/><feBlend in="{n}-m" in2="{cur}" mode="overlay" result="{n}"/>"#,
                    0.5 / size.max(0.1),
                    amount.clamp(0.0, 1.0)
                );
            }
            _ => continue,
        }
        cur = n;
    }
    if f.is_empty() {
        return false;
    }
    let _ = writeln!(
        defs,
        r#"    <filter id="{id}" filterUnits="userSpaceOnUse" x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" color-interpolation-filters="sRGB">{f}</filter>"#,
        area.x0,
        area.y0,
        area.width(),
        area.height()
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Stroke, new_object};
    use kurbo::Affine;

    #[test]
    fn exports_shapes() {
        let mut doc = Document::new("t", 100.0, 50.0);
        doc.insert(
            0,
            new_object(
                Shape::Ellipse { w: 10.0, h: 10.0 },
                Affine::IDENTITY,
                Some(Color::from_hex("#ff0000").unwrap()),
                Some(Stroke::default()),
            ),
        );
        let mut reference = new_object(
            Shape::Rect { w: 1.0, h: 1.0, radius: 0.0, corners: None },
            Affine::IDENTITY,
            Some(Color::BLACK),
            None,
        );
        reference.reference = true;
        reference.name = "calque-modele".into();
        doc.insert(0, reference);
        let svg = export(&doc, &FontLibrary::empty());
        assert!(!svg.contains("calque-modele"), "references are not exported");
        assert!(svg.contains(r#"viewBox="0 0 100 50""#));
        assert!(svg.contains(r##"fill="#ff0000""##));
        assert!(svg.contains("stroke-width=\"4\""));
    }

    #[test]
    fn exports_gradients_and_render_effects() {
        use crate::document::{Gradient, GradientKind};
        use crate::effects::{Effect, EffectKind};
        let mut doc = Document::new("t", 200.0, 200.0);
        let mut o = new_object(
            Shape::Rect { w: 100.0, h: 50.0, radius: 0.0, corners: None },
            Affine::translate((50.0, 50.0)),
            Some(Color::BLACK),
            None,
        );
        o.gradient = Some(Gradient::two(GradientKind::Linear, Color::WHITE, Color::BLACK));
        o.effects = vec![
            Effect::new(EffectKind::DropShadow { x: 0.0, y: 8.0, blur: 16.0, spread: 2.0, color: Color::BLACK }),
            Effect::new(EffectKind::Noise { amount: 0.3, size: 2.0, seed: 4 }),
        ];
        let id = doc.insert(0, o);
        let svg = export(&doc, &FontLibrary::empty());
        assert!(svg.contains("<defs>"));
        assert!(svg.contains(&format!(r#"fill="url(#gradient-{id})""#)));
        // 90°: top to bottom across the 50 px height.
        assert!(svg.contains(r#"x1="100.000" y1="50.000" x2="100.000" y2="100.000""#), "{svg}");
        assert!(svg.contains(&format!(r#"filter="url(#effects-{id})""#)));
        assert!(svg.contains("feMorphology") && svg.contains("feTurbulence"));

        // A disabled effect or a gradient without fill paints nothing extra.
        let o = doc.get_mut(id).unwrap();
        o.fill = None;
        o.effects.iter_mut().for_each(|e| e.enabled = false);
        let svg = export(&doc, &FontLibrary::empty());
        assert!(!svg.contains("<defs>"), "{svg}");
    }

    #[test]
    fn object_svg_covers_the_shadow() {
        use crate::effects::{Effect, EffectKind};
        let mut doc = Document::new("t", 200.0, 200.0);
        let mut o = new_object(
            Shape::Rect { w: 100.0, h: 100.0, radius: 0.0, corners: None },
            Affine::translate((50.0, 50.0)),
            Some(Color::BLACK),
            None,
        );
        o.effects =
            vec![Effect::new(EffectKind::DropShadow { x: 0.0, y: 10.0, blur: 20.0, spread: 0.0, color: Color::BLACK })];
        let id = doc.insert(0, o);
        let o = doc.get(id).unwrap();
        let (_, area) = object_svg(&doc, o, &o.render(&FontLibrary::empty())).unwrap();
        assert!(area.y1 >= 150.0 + 10.0 + 30.0, "{area:?}");
    }
}
