//! Desy core: document model and geometry, independent from the UI.

pub mod boolean;
pub mod color;
pub mod document;
pub mod effects;
pub mod freehand;
pub mod history;
pub mod image;
pub mod mirror;
pub mod palette;
pub mod path;
pub mod snap;
pub mod stroke;
pub mod svg;
pub mod text;

pub use boolean::BoolOp;
pub use color::Color;
pub use document::{Document, FillRule, Gradient, GradientKind, GradientStop, Id, Layer, Object, Shape, Stroke};
pub use effects::{Effect, EffectKind};
pub use image::ImageAsset;
pub use kurbo;
pub use mirror::Mirror;
pub use path::{NodeKind, PathNode, SubPath, VectorPath};
pub use text::{FontLibrary, TextAlign, TextSpec};
