//! Small vector icons drawn with the same path machinery as the canvas,
//! defined as SVG path data on a 24×24 grid.

use crate::paint::{fill_bez, stroke_bez};
use gpui_kit::*;
use kurbo::{Affine, BezPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
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
    Hand,
    Mirror,
    Union,
    Subtract,
    Intersect,
    Exclude,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    Plus,
    Trash,
    Up,
    Down,
    AlignLeft,
    AlignHCenter,
    AlignRight,
    AlignTop,
    AlignVCenter,
    AlignBottom,
    Back,
    Undo,
    Redo,
    Fit,
    Minus,
    Check,
    Image,
    Export,
    Corner,
    FlipH,
    FlipV,
    Eyedropper,
    Info,
    ChevronUp,
    ChevronDown,
    Cut,
    Copy,
    Paste,
    Duplicate,
    ToFront,
    ToBack,
    Layers,
    Corners,
    CornerTL,
    CornerTR,
    CornerBR,
    CornerBL,
    /// Stroke weight: three bars of growing thickness.
    StrokeWeight,
    /// Transparency: a checkerboard.
    Opacity,
    /// Corner radius (quick bar): a rounded corner and its handle.
    Radius,
    /// Rotation angle field.
    Angle,
    /// Rotate by 90°.
    Rotate90,
    /// Effects: Pucker & Bloat.
    FxPucker,
    /// Effects: Twist.
    FxTwist,
    /// Effects: Roughen.
    FxRoughen,
    /// Effects: Zigzag.
    FxZigZag,
    /// Effects: Transform (copies).
    FxTransform,
    /// Effects: Offset path.
    FxOffset,
    FxShadow,
    FxInnerShadow,
    FxGlow,
    FxInnerGlow,
    FxBlur,
    FxNoise,
    /// Swap two values (gradient colors).
    Swap,
    /// Snapping (magnetic guides).
    Magnet,
    /// Stroke ends: butt, round, square (outline of a thick line, path dimmed).
    CapButt,
    CapRound,
    CapSquare,
    /// Stroke corners: miter, round, bevel.
    JoinMiter,
    JoinRound,
    JoinBevel,
}

struct Def {
    main: &'static str,
    fill_main: bool,
    dim: Option<&'static str>,
}

fn def(icon: Icon) -> Def {
    let s = |main| Def { main, fill_main: false, dim: None };
    let f = |main| Def { main, fill_main: true, dim: None };
    match icon {
        Icon::Select => f("M6 3 L6 19 L10.2 14.8 L13.3 21 L15.8 19.8 L12.7 13.7 L18.5 13.7 Z"),
        Icon::Direct => s("M6 3 L6 19 L10.2 14.8 L13.3 21 L15.8 19.8 L12.7 13.7 L18.5 13.7 Z"),
        Icon::Pen => s(
            "M12 21 L5.5 13.5 L12 3 L18.5 13.5 Z M12 3 L12 11.2 M13.4 12.6 A1.6 1.6 0 1 1 10.6 12.6 A1.6 1.6 0 1 1 13.4 12.6 Z",
        ),
        Icon::Pencil => s("M4 20 L5 15.5 L15.5 5 L19 8.5 L8.5 19 Z M13.5 7 L17 10.5"),
        Icon::Line => s("M5 19 L19 5"),
        Icon::Rect => s("M4 5 H20 V19 H4 Z"),
        Icon::Ellipse => s("M4 12 A8 8 0 1 0 20 12 A8 8 0 1 0 4 12 Z"),
        Icon::Polygon => s("M12 3 L20 7.5 L20 16.5 L12 21 L4 16.5 L4 7.5 Z"),
        Icon::Star => s("M12 3 L14.6 9 L21 9.3 L16 13.5 L17.6 20 L12 16.5 L6.4 20 L8 13.5 L3 9.3 L9.4 9 Z"),
        Icon::Wave => s("M3 12 C5 7 7 7 9 12 C11 17 13 17 15 12 C17 7 19 7 21 12"),
        Icon::Blob => s(
            "M11 3.5 C14.5 2.8 15.5 6 18 7 C21 8.2 21.5 12 19.5 14.5 C17.8 16.6 18.5 19.8 14.5 20.5 C11 21.1 9.5 18.5 6.5 18 C3.2 17.4 2.6 13.5 4.2 11 C5.6 8.8 6.8 4.3 11 3.5 Z",
        ),
        Icon::Text => s("M5 7 V4 H19 V7 M12 4 V20 M9 20 H15"),
        Icon::Width => f("M3 12 C8 7 16 7 21 12 C16 17 8 17 3 12 Z"),
        Icon::Hand => s(
            "M8 13 V6 A1.5 1.5 0 0 1 11 6 V11 M11 11 V4.5 A1.5 1.5 0 0 1 14 4.5 V11 M14 11 V6 A1.5 1.5 0 0 1 17 6 V12 M17 12 V9 A1.5 1.5 0 0 1 20 9 V15 A6 6 0 0 1 14 21 H12.5 A6 6 0 0 1 7.6 18.4 L4.5 14 A1.5 1.5 0 0 1 7 12.3 L8 13.5",
        ),
        Icon::Mirror => Def {
            main: "M10 5 L3 19 H10 Z M14 5 L21 19 H14 Z",
            fill_main: false,
            dim: Some("M12 2 V4 M12 6 V8 M12 10 V12 M12 14 V16 M12 18 V20 M12 22 V22.5"),
        },
        Icon::Union => f("M4 4 H14 V10 H20 V20 H10 V14 H4 Z"),
        Icon::Subtract => Def { main: "M4 4 H14 V10 H10 V14 H4 Z", fill_main: true, dim: Some("M10 10 H20 V20 H10 Z") },
        Icon::Intersect => {
            Def { main: "M10 10 H14 V14 H10 Z", fill_main: true, dim: Some("M4 4 H14 V14 H4 Z M10 10 H20 V20 H10 Z") }
        }
        Icon::Exclude => Def {
            main: "M4 4 H14 V10 H10 V14 H4 Z M14 10 H20 V20 H10 V14 H14 Z",
            fill_main: true,
            dim: Some("M10 10 H14 V14 H10 Z"),
        },
        Icon::Eye => s(
            "M2.5 12 C5.5 6.5 18.5 6.5 21.5 12 C18.5 17.5 5.5 17.5 2.5 12 Z M14.5 12 A2.5 2.5 0 1 1 9.5 12 A2.5 2.5 0 1 1 14.5 12 Z",
        ),
        Icon::EyeOff => Def {
            main: "M4 4 L20 20",
            fill_main: false,
            dim: Some("M2.5 12 C5.5 6.5 18.5 6.5 21.5 12 C18.5 17.5 5.5 17.5 2.5 12 Z"),
        },
        Icon::Lock => s("M6 11 H18 V20 H6 Z M8.5 11 V8 A3.5 3.5 0 0 1 15.5 8 V11"),
        Icon::Unlock => {
            Def { main: "M6 11 H18 V20 H6 Z", fill_main: false, dim: Some("M8.5 11 V8 A3.5 3.5 0 0 1 15.3 6.8") }
        }
        Icon::Plus => s("M12 5 V19 M5 12 H19"),
        Icon::Trash => s("M5 7 H19 M9.5 7 V4.5 H14.5 V7 M7 7 L8 20 H16 L17 7"),
        Icon::Up => s("M12 19 V5 M6 11 L12 5 L18 11"),
        Icon::Down => s("M12 5 V19 M6 13 L12 19 L18 13"),
        Icon::AlignLeft => s("M4 3 V21 M8 7 H19 V11 H8 Z M8 14 H14 V18 H8 Z"),
        Icon::AlignHCenter => s("M12 3 V21 M6 7 H18 V11 H6 Z M8.5 14 H15.5 V18 H8.5 Z"),
        Icon::AlignRight => s("M20 3 V21 M5 7 H16 V11 H5 Z M10 14 H16 V18 H10 Z"),
        Icon::AlignTop => s("M3 4 H21 M7 8 V19 H11 V8 Z M14 8 V14 H18 V8 Z"),
        Icon::AlignVCenter => s("M3 12 H21 M7 6 V18 H11 V6 Z M14 8.5 V15.5 H18 V8.5 Z"),
        Icon::AlignBottom => s("M3 20 H21 M7 5 V16 H11 V5 Z M14 10 V16 H18 V10 Z"),
        Icon::Back => s("M20 12 H4.5 M10.5 6 L4.5 12 L10.5 18"),
        Icon::Undo => s("M8.5 14.5 L4 10 L8.5 5.5 M4 10 H14.5 A5.5 5.5 0 0 1 14.5 21 H10"),
        Icon::Redo => s("M15.5 14.5 L20 10 L15.5 5.5 M20 10 H9.5 A5.5 5.5 0 0 0 9.5 21 H14"),
        Icon::Fit => s("M4 9 V4 H9 M15 4 H20 V9 M20 15 V20 H15 M9 20 H4 V15 M9 9 H15 V15 H9 Z"),
        Icon::Minus => s("M5 12 H19"),
        Icon::Check => s("M5 12.5 L10 17.5 L19.5 7"),
        Icon::Image => s(
            "M4 5 H20 V19 H4 Z M4 16 L9 11 L13.5 15.5 L16 13 L20 17 M17 8.5 A1.5 1.5 0 1 1 14 8.5 A1.5 1.5 0 1 1 17 8.5 Z",
        ),
        Icon::Export => s("M12 4 V15 M7 10 L12 15 L17 10 M5 19 H19"),
        Icon::Corner => f("M22 9 L22 22 L9 22 Z"),
        Icon::FlipH => Def { main: "M9.5 6 V18 H3 Z M14.5 6 V18 H21 Z", fill_main: false, dim: Some("M12 3 V21") },
        Icon::Eyedropper => s(
            "M2 22 L3 21 H6 L15 12 M3 21 V18 L12 9 M15 6 L18.4 2.6 A2.1 2.1 0 1 1 21.4 5.6 L18 9 L18.4 9.4 A2.1 2.1 0 1 1 15.4 12.4 L11.6 8.6 A2.1 2.1 0 1 1 14.6 5.6 L15 6 Z",
        ),
        Icon::Cut => s(
            "M5 7 A2.5 2.5 0 1 1 10 7 A2.5 2.5 0 1 1 5 7 Z M14 7 A2.5 2.5 0 1 1 19 7 A2.5 2.5 0 1 1 14 7 Z M9 9 L16 20 M15 9 L8 20",
        ),
        Icon::Copy => s("M9 9 H20 V20 H9 Z M15 9 V4 H4 V15 H9"),
        Icon::Paste => s("M9 5 H6 V21 H18 V5 H15 M9 3 H15 V7 H9 Z"),
        Icon::Duplicate => s("M4 4 H14 V14 H4 Z M10 17 V20 H20 V10 H17"),
        Icon::ToFront => s("M5 4 H19 M12 20 V8 M7 13 L12 8 L17 13"),
        Icon::ToBack => s("M5 20 H19 M12 4 V16 M7 11 L12 16 L17 11"),
        Icon::Layers => s("M3 8 L12 4 L21 8 L12 12 Z M3 12 L12 16 L21 12 M3 16 L12 20 L21 16"),
        Icon::Corners => s(
            "M4 10 V7 A3 3 0 0 1 7 4 H10 M14 4 H17 A3 3 0 0 1 20 7 V10 M20 14 V17 A3 3 0 0 1 17 20 H14 M10 20 H7 A3 3 0 0 1 4 17 V14",
        ),
        Icon::CornerTL => s("M6 19 V11 A5 5 0 0 1 11 6 H19"),
        Icon::CornerTR => s("M5 6 H13 A5 5 0 0 1 18 11 V19"),
        Icon::CornerBR => s("M18 5 V13 A5 5 0 0 1 13 18 H5"),
        Icon::CornerBL => s("M6 5 V13 A5 5 0 0 0 11 18 H19"),
        Icon::ChevronUp => s("M6 15 L12 9 L18 15"),
        Icon::ChevronDown => s("M6 9 L12 15 L18 9"),
        Icon::Info => s("M21 12 A9 9 0 1 1 3 12 A9 9 0 1 1 21 12 Z M12 11 V16.5 M12 7.2 V7.8"),
        Icon::StrokeWeight => f("M4 5 H20 V6.2 H4 Z M4 10 H20 V12.6 H4 Z M4 16 H20 V20 H4 Z"),
        Icon::Opacity => Def {
            main: "M4 4 H8 V8 H4 Z M12 4 H16 V8 H12 Z M8 8 H12 V12 H8 Z M16 8 H20 V12 H16 Z M4 12 H8 V16 H4 Z M12 12 H16 V16 H12 Z M8 16 H12 V20 H8 Z M16 16 H20 V20 H16 Z",
            fill_main: true,
            dim: Some("M4 4 H20 V20 H4 Z"),
        },
        Icon::Radius => s("M5 20 V13 A7 7 0 0 1 12 6 H15.5 M20.5 6 A2 2 0 1 1 16.5 6 A2 2 0 1 1 20.5 6 Z"),
        Icon::Angle => s("M4 19 H20 M4 19 L15 6 M10 19 A6 6 0 0 0 8.6 14.3"),
        Icon::Rotate90 => s("M19 13 A7 7 0 1 1 12 6 H16 M13.5 3 L16.5 6 L13.5 9"),
        Icon::FxPucker => s("M4 4 Q12 10 20 4 Q14 12 20 20 Q12 14 4 20 Q10 12 4 4 Z"),
        Icon::FxTwist => s(
            "M12 12 A1.5 1.5 0 0 1 13.5 13.5 A3 3 0 0 1 10.5 16.5 A4.5 4.5 0 0 1 6 12 A6 6 0 0 1 12 6 A7.5 7.5 0 0 1 19.5 13.5",
        ),
        Icon::FxRoughen => s(
            "M12 3 L14 6 L17.5 5 L17.8 8.6 L21 10.5 L18.8 13.3 L20 16.8 L16.4 17.2 L14.6 20.5 L12 18 L9.4 20.5 L7.6 17.2 L4 16.8 L5.2 13.3 L3 10.5 L6.2 8.6 L6.5 5 L10 6 Z",
        ),
        Icon::FxZigZag => s("M3 16 L6 8 L9 16 L12 8 L15 16 L18 8 L21 16"),
        Icon::FxTransform => {
            Def { main: "M12 3.5 L20.5 12 L12 20.5 L3.5 12 Z", fill_main: false, dim: Some("M6 6 H18 V18 H6 Z") }
        }
        Icon::FxOffset => Def {
            main: "M9 9 H15 V15 H9 Z",
            fill_main: false,
            dim: Some(
                "M7.5 4 H16.5 A3.5 3.5 0 0 1 20 7.5 V16.5 A3.5 3.5 0 0 1 16.5 20 H7.5 A3.5 3.5 0 0 1 4 16.5 V7.5 A3.5 3.5 0 0 1 7.5 4 Z",
            ),
        },
        Icon::FxShadow => Def { main: "M4 4 H15 V15 H4 Z", fill_main: false, dim: Some("M8 15 V19 H19 V8 H15") },
        Icon::FxInnerShadow => Def { main: "M4 4 H20 V20 H4 Z", fill_main: false, dim: Some("M7.5 16.5 V7.5 H16.5") },
        Icon::FxGlow => Def {
            main: "M16.5 12 A4.5 4.5 0 1 1 7.5 12 A4.5 4.5 0 1 1 16.5 12 Z",
            fill_main: false,
            dim: Some(
                "M12 2.5 V5 M12 19 V21.5 M2.5 12 H5 M19 12 H21.5 M5.3 5.3 L7 7 M17 17 L18.7 18.7 M5.3 18.7 L7 17 M17 7 L18.7 5.3",
            ),
        },
        Icon::FxInnerGlow => Def {
            main: "M20 12 A8 8 0 1 1 4 12 A8 8 0 1 1 20 12 Z",
            fill_main: false,
            dim: Some("M16 12 A4 4 0 1 1 8 12 A4 4 0 1 1 16 12 Z"),
        },
        Icon::FxBlur => Def {
            main: "M12 3.5 C9.5 8 6 11 6 14.5 A6 6 0 0 0 18 14.5 C18 11 14.5 8 12 3.5 Z",
            fill_main: false,
            dim: Some("M9 15 A3 3 0 0 0 12 18"),
        },
        Icon::FxNoise => f(
            "M4 4 H6 V6 H4 Z M11 5 H13 V7 H11 Z M17 4 H19 V6 H17 Z M7 10 H9 V12 H7 Z M14 9 H16 V11 H14 Z M4 16 H6 V18 H4 Z M10 14 H12 V16 H10 Z M17 15 H19 V17 H17 Z M13 19 H15 V21 H13 Z M19 10 H21 V12 H19 Z M7 19 H9 V21 H7 Z",
        ),
        Icon::Swap => s("M7 4 L4 7 L7 10 M4 7 H17 M17 14 L20 17 L17 20 M20 17 H7"),
        Icon::Magnet => s("M5 4 H10 V12 A2 2 0 0 0 14 12 V4 H19 V12 A7 7 0 0 1 5 12 Z M5 8 H10 M14 8 H19"),
        Icon::CapButt => Def {
            main: "M3 7 H14 V17 H3",
            fill_main: false,
            dim: Some("M3 12 H14 M15.2 12 A1.2 1.2 0 1 1 12.8 12 A1.2 1.2 0 1 1 15.2 12 Z"),
        },
        Icon::CapRound => Def {
            main: "M3 7 H14 A5 5 0 0 1 14 17 H3",
            fill_main: false,
            dim: Some("M3 12 H14 M15.2 12 A1.2 1.2 0 1 1 12.8 12 A1.2 1.2 0 1 1 15.2 12 Z"),
        },
        Icon::CapSquare => Def {
            main: "M3 7 H19 V17 H3",
            fill_main: false,
            dim: Some("M3 12 H14 M15.2 12 A1.2 1.2 0 1 1 12.8 12 A1.2 1.2 0 1 1 15.2 12 Z"),
        },
        Icon::JoinMiter => Def { main: "M4 21 V4 H21 V11 H11 V21 Z", fill_main: false, dim: Some("M7.5 21 V7.5 H21") },
        Icon::JoinRound => {
            Def { main: "M4 21 V11 A7 7 0 0 1 11 4 H21 V11 H11 V21 Z", fill_main: false, dim: Some("M7.5 21 V7.5 H21") }
        }
        Icon::JoinBevel => {
            Def { main: "M4 21 V10 L10 4 H21 V11 H11 V21 Z", fill_main: false, dim: Some("M7.5 21 V7.5 H21") }
        }
        Icon::FlipV => Def { main: "M6 9.5 H18 V3 Z M6 14.5 H18 V21 Z", fill_main: false, dim: Some("M3 12 H21") },
    }
}

fn parse(d: &str) -> BezPath {
    BezPath::from_svg(d).unwrap_or_default()
}

/// An icon element of the given pixel size.
pub fn icon(icon: Icon, size: f32, color: Hsla) -> impl IntoElement {
    let d = def(icon);
    let main = parse(d.main);
    let dim = d.dim.map(parse);
    let fill_main = d.fill_main;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let s = f32::from(bounds.size.width) as f64 / 24.0;
            let xf = Affine::translate((f32::from(bounds.origin.x) as f64, f32::from(bounds.origin.y) as f64))
                * Affine::scale(s);
            let line = (1.5 * s).max(1.0) as f32;
            if let Some(dim) = &dim {
                let mut c = color;
                c.a *= 0.45;
                stroke_bez(window, dim, xf, line, c);
            }
            if fill_main {
                fill_bez(window, &main, xf, color, false);
            } else {
                stroke_bez(window, &main, xf, line, color);
            }
        },
    )
    .size(px(size))
    .flex_none()
}
