//! Development helper (feature `dev-screenshot`): open a demo scene, render
//! the window to a PNG and quit. Used to check the UI without a screen.
//!
//! `DESY_SHOT=/tmp/shot.png DESY_SCENE=editor|home cargo run --features dev-screenshot`

use crate::editor::{NodeRef, Tool};
use crate::view::EditorView;
use desy_core::document::new_object;
use desy_core::{BoolOp, Color, Document, PathNode, Shape, Stroke, SubPath, TextSpec, VectorPath};
use gpui_kit::*;
use kurbo::{Affine, Point};

pub fn demo_document() -> Document {
    let hex = |h| Color::from_hex(h).unwrap();
    let mut doc = Document::new("Démo mascotte", 1000.0, 1000.0);
    let head = doc.insert(
        0,
        new_object(
            Shape::Ellipse { w: 520.0, h: 470.0 },
            Affine::translate((240.0, 300.0)),
            Some(hex("#f4a340")),
            None,
        ),
    );
    let ear_l = doc.insert(
        0,
        new_object(
            Shape::Polygon { radius: 150.0, sides: 3 },
            Affine::translate((330.0, 300.0)) * Affine::rotate(-0.35),
            Some(hex("#f4a340")),
            None,
        ),
    );
    let ear_r = doc.insert(
        0,
        new_object(
            Shape::Polygon { radius: 150.0, sides: 3 },
            Affine::translate((670.0, 300.0)) * Affine::rotate(0.35),
            Some(hex("#f4a340")),
            None,
        ),
    );
    let fonts = desy_core::FontLibrary::empty();
    let body = doc.boolean(&fonts, &[head, ear_l, ear_r], BoolOp::Union).unwrap();
    doc.get_mut(body).unwrap().name = "Tête".into();

    let face = doc.add_layer(Some("Visage".into()));
    let li = doc.layer_index(face).unwrap();
    for x in [400.0, 600.0] {
        doc.insert(
            li,
            new_object(
                Shape::Ellipse { w: 56.0, h: 70.0 },
                Affine::translate((x - 28.0, 480.0)),
                Some(hex("#1d1d1f")),
                None,
            ),
        );
    }
    let mut a = PathNode::corner(Point::new(400.0, 620.0));
    a.width = 0.0;
    a.h_out = Point::new(440.0, 680.0);
    let mut m = PathNode::corner(Point::new(500.0, 680.0));
    m.h_in = Point::new(460.0, 680.0);
    m.h_out = Point::new(540.0, 680.0);
    m.kind = desy_core::NodeKind::Smooth;
    m.width = 1.6;
    let mut b = PathNode::corner(Point::new(600.0, 620.0));
    b.h_in = Point::new(560.0, 680.0);
    b.width = 0.0;
    let smile = VectorPath { subpaths: vec![SubPath { nodes: vec![a, m, b], closed: false }] };
    let mut o = new_object(
        Shape::Path(smile),
        Affine::IDENTITY,
        None,
        Some(Stroke { color: hex("#1d1d1f"), width: 14.0, ..Default::default() }),
    );
    o.name = "Sourire".into();
    doc.insert(li, o);
    doc.insert(
        li,
        new_object(
            Shape::Star { outer: 70.0, inner: 32.0, points: 5 },
            Affine::translate((820.0, 180.0)),
            Some(hex("#ffd23f")),
            Some(Stroke { color: hex("#1d1d1f"), width: 6.0, ..Default::default() }),
        ),
    );
    let mut text = new_object(
        Shape::Text(TextSpec { content: "Desy".into(), family: "Helvetica".into(), size: 110.0, ..Default::default() }),
        Affine::translate((340.0, 820.0)),
        Some(hex("#0d9488")),
        None,
    );
    text.name = "Titre".into();
    doc.insert(li, text);
    doc
}

/// A generated "sketch" image (PNG bytes).
fn sketch_png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(400, 300, |x, y| {
        let (dx, dy) = (x as f32 - 200.0, y as f32 - 150.0);
        let ring = ((dx * dx + dy * dy).sqrt() - 110.0).abs() < 6.0;
        if ring || (x / 40 + y / 40) % 7 == 0 {
            image::Rgba([40, 40, 60, 255])
        } else {
            image::Rgba([(x / 2) as u8, 160, (y / 2) as u8 + 60, 255])
        }
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// Half a cat head in a layer mirrored across x = 500.
pub fn mirror_document() -> Document {
    let hex = |h| Color::from_hex(h).unwrap();
    let mut doc = Document::new("Symétrie", 1000.0, 1000.0);
    // The whole layer is mirrored: each shape is drawn only once.
    doc.layers[0].mirror = Some(desy_core::Mirror::vertical(500.0));
    let n = |x: f64, y: f64| PathNode::corner(Point::new(x, y));
    let mut cheek = n(260.0, 560.0);
    cheek.h_in = Point::new(250.0, 470.0);
    cheek.h_out = Point::new(270.0, 680.0);
    cheek.kind = desy_core::NodeKind::Smooth;
    let mut chin = n(500.0, 800.0);
    chin.h_in = Point::new(360.0, 800.0);
    let half = VectorPath {
        subpaths: vec![SubPath {
            nodes: vec![n(500.0, 330.0), n(380.0, 340.0), n(290.0, 200.0), n(270.0, 420.0), cheek, chin],
            closed: false,
        }],
    };
    let mut head = new_object(
        Shape::Path(half),
        Affine::IDENTITY,
        Some(hex("#6c5ce7")),
        Some(Stroke { color: hex("#1d1d1f"), width: 10.0, ..Default::default() }),
    );
    head.name = "Tête (demi)".into();
    doc.insert(0, head);
    let mut eye = new_object(
        Shape::Ellipse { w: 70.0, h: 90.0 },
        Affine::translate((360.0, 500.0)),
        Some(hex("#ffffff")),
        Some(Stroke { color: hex("#1d1d1f"), width: 8.0, ..Default::default() }),
    );
    eye.name = "Œil".into();
    doc.insert(0, eye);
    let mut w0 = n(440.0, 680.0);
    w0.width = 0.0;
    let mut w1 = n(330.0, 650.0);
    w1.width = 0.0;
    let mut whisker = new_object(
        Shape::Path(VectorPath { subpaths: vec![SubPath { nodes: vec![w0, w1], closed: false }] }),
        Affine::IDENTITY,
        None,
        Some(Stroke { color: hex("#1d1d1f"), width: 10.0, ..Default::default() }),
    );
    if let Shape::Path(vp) = &mut whisker.shape {
        vp.insert_node(0, 0, 0.5);
        vp.subpaths[0].nodes[1].width = 1.0;
    }
    whisker.name = "Moustache".into();
    doc.insert(0, whisker);
    doc
}

pub fn prepare_editor(v: &mut EditorView) {
    // `DESY_MENU=<group>`: open the tool rail flyout of that group.
    v.tool_menu = std::env::var("DESY_MENU").ok().and_then(|g| g.parse().ok());
    // `DESY_CONTEXT=x,y`: open the right-click menu at that window position.
    if let Some((x, y)) = std::env::var("DESY_CONTEXT").ok().and_then(|s| {
        let (x, y) = s.split_once(',')?;
        Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))
    }) {
        v.context_menu = Some(gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y)));
    }
    // `DESY_QUICK=stroke|radius|opacity|effects|position`: open that menu
    // of the quick bar above the selection.
    v.quick.menu = std::env::var("DESY_QUICK").ok().and_then(|m| {
        use crate::quick_bar::QuickMenu::*;
        Some(match m.as_str() {
            "stroke" => StrokeWeight,
            "radius" => Radius,
            "opacity" => Opacity,
            "effects" => Effects,
            "position" => Position,
            _ => return None,
        })
    });
    // `DESY_EXPORT=1`: open the export menu.
    v.export_menu = std::env::var("DESY_EXPORT").is_ok();
    // `DESY_COLLAPSE=1`: fold the tool option frames.
    if std::env::var("DESY_COLLAPSE").is_ok() {
        v.collapsed_frames = vec![crate::tool_frames::Frame::Mirror, crate::tool_frames::Frame::Width];
    }
    let ed = &mut v.ed;
    if std::env::var("DESY_SELECT").as_deref() == Ok("mirror") {
        let id = ed.doc.objects().find(|o| o.name == "Tête (demi)").map(|o| o.id).unwrap();
        ed.select_only(id);
        ed.set_tool(Tool::Direct);
        ed.nodes = vec![NodeRef { id, sp: 0, node: 0 }];
        return;
    }
    if std::env::var("DESY_SELECT").as_deref() == Ok("image") {
        ed.canvas_size = kurbo::Vec2::new(1200.0, 1000.0);
        ed.fit();
        let asset = crate::images::load_asset("croquis.png".into(), sketch_png()).unwrap();
        let reference = ed.add_image(asset.clone());
        ed.doc.translate_objects(&[reference], kurbo::Vec2::new(-150.0, -150.0));
        ed.make_reference();
        let photo = ed.add_image(asset);
        ed.doc.translate_objects(&[photo], kurbo::Vec2::new(250.0, 300.0));
        ed.update_selected(|o, _| o.apply_transform(Affine::scale_about(0.5, o.transform * Point::ORIGIN)));
        ed.select_only(photo);
        ed.needs_fit = true;
        return;
    }
    if let Ok(sel @ ("rect" | "corners")) = std::env::var("DESY_SELECT").as_deref() {
        let corners = (sel == "corners").then_some([60.0, 20.0, 60.0, 0.0]);
        let o = desy_core::document::new_object(
            Shape::Rect { w: 260.0, h: 180.0, radius: 40.0, corners },
            Affine::translate((80.0, 760.0)),
            desy_core::Color::from_hex("#0d9488"),
            None,
        );
        let li = ed.doc.layers.len() - 1;
        let id = ed.doc.insert(li, o);
        ed.select_only(id);
        ed.set_tool(Tool::Select);
        return;
    }
    // `DESY_SELECT=wave|blob`: water, a wavy line and blobs, one selected.
    if let Ok(sel @ ("wave" | "blob")) = std::env::var("DESY_SELECT").as_deref() {
        let hex = |h| desy_core::Color::from_hex(h);
        let dark = Some(Stroke { color: Color::from_hex("#1d1d1f").unwrap(), width: 8.0, ..Default::default() });
        let li = ed.doc.layers.len() - 1;
        let water = ed.doc.insert(
            li,
            new_object(
                Shape::Wave { w: 1000.0, h: 200.0, waves: 4, filled: true, depth: 0.3 },
                Affine::translate((0.0, 800.0)),
                hex("#38bdf8"),
                None,
            ),
        );
        ed.doc.insert(
            li,
            new_object(
                Shape::Wave { w: 300.0, h: 40.0, waves: 3, filled: false, depth: 0.35 },
                Affine::translate((60.0, 60.0)),
                None,
                dark,
            ),
        );
        let mut blob = 0;
        for (i, (x, color)) in [(140.0, "#f472b6"), (360.0, "#a3e635")].into_iter().enumerate() {
            blob = ed.doc.insert(
                li,
                new_object(
                    Shape::Blob { radius: 90.0, points: 6 + i as u32, variance: 0.4, seed: 7 + i as u32 },
                    Affine::translate((x, 300.0)),
                    hex(color),
                    dark,
                ),
            );
        }
        ed.select_only(if sel == "wave" { water } else { blob });
        ed.set_tool(Tool::Select);
        return;
    }
    // `DESY_SELECT=render`: gradients and one shape per render effect
    // (shadows, glows, blur, noise), the drop shadow selected.
    if std::env::var("DESY_SELECT").as_deref() == Ok("render") {
        use desy_core::{Effect, EffectKind, Gradient, GradientKind};
        let hex = |h: &str| desy_core::Color::from_hex(h).unwrap();
        let li = ed.doc.add_layer(Some("Rendu".into()));
        let li = ed.doc.layer_index(li).unwrap();
        for l in 0..li {
            ed.doc.layers[l].visible = false;
        }
        ed.doc.background = Some(hex("#f1f5f9"));
        let mut add = |shape: Shape, at: (f64, f64), fill: &str, g: Option<Gradient>, fx: Vec<EffectKind>| {
            let mut o = new_object(shape, Affine::translate(at), Some(hex(fill)), None);
            o.gradient = g;
            o.effects = fx.into_iter().map(Effect::new).collect();
            ed.doc.insert(li, o)
        };
        let card = |w, h| Shape::Rect { w, h, radius: 36.0, corners: None };
        let black = |a: f32| desy_core::Color { a, ..hex("#000000") };
        let shadow = add(
            card(240.0, 200.0),
            (70.0, 70.0),
            "#6366f1",
            Some(Gradient::two(GradientKind::Linear, hex("#6366f1"), hex("#ec4899"))),
            vec![EffectKind::DropShadow { x: 0.0, y: 16.0, blur: 28.0, spread: 0.0, color: black(0.35) }],
        );
        add(
            card(240.0, 200.0),
            (380.0, 70.0),
            "#e2e8f0",
            None,
            vec![EffectKind::InnerShadow { x: 0.0, y: 10.0, blur: 18.0, color: black(0.45) }],
        );
        add(
            Shape::Ellipse { w: 200.0, h: 200.0 },
            (710.0, 70.0),
            "#fde047",
            Some(Gradient::two(GradientKind::Radial, hex("#fef9c3"), hex("#f59e0b"))),
            vec![EffectKind::OuterGlow { blur: 40.0, spread: 6.0, color: hex("#f59e0b") }],
        );
        add(
            Shape::Star { outer: 120.0, inner: 55.0, points: 5 },
            (190.0, 520.0),
            "#0ea5e9",
            None,
            vec![EffectKind::InnerGlow { blur: 30.0, color: hex("#e0f2fe") }],
        );
        add(card(220.0, 200.0), (390.0, 420.0), "#10b981", None, vec![EffectKind::Blur { radius: 14.0 }]);
        add(
            card(220.0, 200.0),
            (690.0, 420.0),
            "#f97316",
            Some(Gradient { angle: 45.0, ..Gradient::from_color(hex("#f97316")) }),
            vec![EffectKind::Noise { amount: 0.5, size: 1.2, seed: 3 }],
        );
        ed.doc.palette_gradients = vec![
            Gradient::two(GradientKind::Linear, hex("#6366f1"), hex("#ec4899")),
            Gradient::two(GradientKind::Radial, hex("#fef9c3"), hex("#f59e0b")),
            Gradient { angle: 0.0, ..Gradient::two(GradientKind::Linear, hex("#0ea5e9"), hex("#10b981")) },
        ];
        ed.select_only(shadow);
        ed.set_tool(Tool::Select);
        return;
    }
    // `DESY_SELECT=effects`: one shape per effect, the rosette selected
    // (`DESY_FXMENU=1` opens the add menu).
    if std::env::var("DESY_SELECT").as_deref() == Ok("effects") {
        use desy_core::{Effect, EffectKind};
        let hex = |h: &str| desy_core::Color::from_hex(h);
        let dark = Some(Stroke { color: Color::from_hex("#1d1d1f").unwrap(), width: 6.0, ..Default::default() });
        let li = ed.doc.add_layer(Some("Effets".into()));
        let li = ed.doc.layer_index(li).unwrap();
        for l in 0..li {
            ed.doc.layers[l].visible = false;
        }
        let mut add = |shape: Shape, at: (f64, f64), fill: &str, stroke, fx: Vec<EffectKind>| {
            let mut o = new_object(shape, Affine::translate(at), hex(fill), stroke);
            o.effects = fx.into_iter().map(Effect::new).collect();
            ed.doc.insert(li, o)
        };
        let square = |s: f64| Shape::Rect { w: s, h: s, radius: 0.0, corners: None };
        add(square(200.0), (60.0, 60.0), "#f472b6", dark, vec![EffectKind::PuckerBloat { amount: -0.35 }]);
        add(square(200.0), (390.0, 60.0), "#a3e635", dark, vec![EffectKind::PuckerBloat { amount: 0.5 }]);
        add(
            Shape::Star { outer: 110.0, inner: 50.0, points: 5 },
            (820.0, 170.0),
            "#ffd23f",
            dark,
            vec![EffectKind::Twist { angle: 120.0 }],
        );
        add(
            Shape::Ellipse { w: 220.0, h: 220.0 },
            (60.0, 400.0),
            "#38bdf8",
            dark,
            vec![EffectKind::ZigZag { size: 12.0, ridges: 6, smooth: true }],
        );
        add(
            Shape::Rect { w: 220.0, h: 200.0, radius: 30.0, corners: None },
            (390.0, 400.0),
            "#fb923c",
            dark,
            vec![EffectKind::Roughen { size: 8.0, detail: 12.0, smooth: false, seed: 3 }],
        );
        let rosette = add(
            Shape::Ellipse { w: 50.0, h: 110.0 },
            (815.0, 380.0),
            "#c084fc",
            None,
            vec![EffectKind::Transform {
                scale_x: 1.0,
                scale_y: 1.0,
                rotate: 30.0,
                move_x: 0.0,
                move_y: 0.0,
                copies: 11,
                origin: [0.5, 1.0],
            }],
        );
        let mut text = new_object(
            Shape::Text(TextSpec {
                content: "Desy".into(),
                family: "Helvetica".into(),
                size: 150.0,
                weight: 800,
                ..Default::default()
            }),
            Affine::translate((240.0, 760.0)),
            hex("#0d9488"),
            None,
        );
        text.effects =
            vec![Effect::new(EffectKind::Offset { distance: 14.0, join: desy_core::stroke::LineJoin::Round })];
        let sticker = ed.doc.insert(li, text.clone());
        text.effects.clear();
        text.fill = hex("#1d1d1f");
        ed.doc.insert(li, text);
        ed.doc.get_mut(sticker).unwrap().fill = hex("#ffffff");
        ed.doc.get_mut(sticker).unwrap().stroke = dark;
        ed.select_only(rosette);
        ed.set_tool(Tool::Select);
        v.props.effects.menu_open = std::env::var("DESY_FXMENU").is_ok();
        // Show the effects block at the bottom of the properties.
        let y = if v.props.effects.menu_open { -330.0 } else { -10_000.0 };
        v.props.scroll.set_offset(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(y)));
        return;
    }
    if std::env::var("DESY_SELECT").as_deref() == Ok("multi") {
        let ids: Vec<_> = ed
            .doc
            .objects()
            .filter(|o| o.name == "Tête" || o.name == "Star" || o.name == "Étoile")
            .map(|o| o.id)
            .collect();
        ed.selected = ids;
        ed.set_tool(Tool::Select);
        return;
    }
    let smile = ed.doc.objects().find(|o| o.name == "Sourire").map(|o| o.id);
    if let Some(id) = smile {
        ed.select_only(id);
        ed.set_tool(Tool::Direct);
        ed.nodes = vec![NodeRef { id, sp: 0, node: 1 }];
    }
}

/// `DESY_FILL=quick|panel`: open the fill color popover of the quick bar
/// or of the inspector (`DESY_STOP=1` edits the gradient's last stop).
pub fn open_fill_picker(v: &mut EditorView, cx: &mut gpui_kit::Context<EditorView>) {
    let state = match std::env::var("DESY_FILL").as_deref() {
        Ok("quick") => v.quick.fill.clone(),
        Ok("panel") => v.props.fill.clone(),
        _ => return,
    };
    v.props.gradient_stop = std::env::var("DESY_STOP").is_ok();
    state.update(cx, |s, cx| s.set_open(true, cx));
}

pub fn schedule(path: String, window: &mut Window, cx: &mut App) {
    window
        .spawn(cx, async move |cx| {
            // `DESY_SHOT_MS`: capture earlier, e.g. mid entrance motion.
            let ms = std::env::var("DESY_SHOT_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(1500);
            cx.background_executor().timer(std::time::Duration::from_millis(ms)).await;
            let _ = cx.update(|window, cx| {
                window.refresh();
                match window.render_to_image() {
                    Ok(img) => {
                        let _ = img.save(&path);
                        eprintln!("screenshot saved to {path}");
                    }
                    Err(e) => eprintln!("screenshot failed: {e}"),
                }
                cx.quit();
            });
        })
        .detach();
}
