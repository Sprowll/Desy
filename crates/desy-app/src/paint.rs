//! Bridge from kurbo paths to GPUI painting.

use gpui_kit::*;
use kurbo::{Affine, BezPath, PathEl};

fn pt(p: kurbo::Point) -> Point<Pixels> {
    point(px(p.x as f32), px(p.y as f32))
}

fn build(mut builder: PathBuilder, path: &BezPath, xf: Affine) -> Option<Path> {
    let mut has = false;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => builder.move_to(pt(xf * p)),
            PathEl::LineTo(p) => {
                has = true;
                builder.line_to(pt(xf * p))
            }
            PathEl::QuadTo(c, p) => {
                has = true;
                builder.curve_to(pt(xf * p), pt(xf * c))
            }
            PathEl::CurveTo(c1, c2, p) => {
                has = true;
                builder.cubic_bezier_to(pt(xf * p), pt(xf * c1), pt(xf * c2))
            }
            PathEl::ClosePath => builder.close(),
        }
    }
    if !has {
        return None;
    }
    builder.build().ok()
}

/// Fill a path. `even_odd` selects the fill rule.
pub fn fill_bez(window: &mut Window, path: &BezPath, xf: Affine, color: impl Into<Background>, even_odd: bool) {
    let rule = if even_odd { FillRule::EvenOdd } else { FillRule::NonZero };
    let builder = PathBuilder::fill().with_style(PathStyle::Fill(FillOptions { fill_rule: rule }));
    if let Some(p) = build(builder, path, xf) {
        window.paint_path(p, color);
    }
}

/// Stroke a path with a hairline-style screen-space width.
pub fn stroke_bez(window: &mut Window, path: &BezPath, xf: Affine, width: f32, color: impl Into<Background>) {
    if let Some(p) = build(
        PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(StrokeOptions {
            line_width: width,
            line_join: LineJoin::Round,
            line_cap: LineCap::Round,
            ..Default::default()
        })),
        path,
        xf,
    ) {
        window.paint_path(p, color);
    }
}

/// Dashed screen-space line (mirror axes).
pub fn stroke_bez_dashed(window: &mut Window, path: &BezPath, width: f32, color: impl Into<Background>) {
    let builder = PathBuilder::stroke(px(width)).dash_array(&[px(6.0), px(4.0)]);
    if let Some(p) = build(builder, path, Affine::IDENTITY) {
        window.paint_path(p, color);
    }
}
