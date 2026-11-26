//! Decoding embedded images for display. Decoded pixels are cached per
//! asset; GPU textures per (asset, opacity), because GPUI has no public
//! per-image opacity so it is baked into the alpha channel.

use anyhow::{Context as _, Result};
use desy_core::ImageAsset;
use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Largest side used on screen; bigger images are downscaled for display only.
const MAX_DISPLAY: u32 = 2048;

type Key = (usize, usize);

thread_local! {
    static DECODED: RefCell<HashMap<Key, Option<Arc<RgbaImage>>>> = RefCell::new(HashMap::new());
    static TEXTURES: RefCell<HashMap<(Key, u8), Arc<RenderImage>>> = RefCell::new(HashMap::new());
}

fn key(asset: &ImageAsset) -> Key {
    (Arc::as_ptr(&asset.data) as usize, asset.data.len())
}

/// Read an image file into an asset (checks that it decodes).
pub fn load_asset(name: String, data: Vec<u8>) -> Result<ImageAsset> {
    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&data))
        .with_guessed_format()?
        .into_dimensions()
        .context("format d'image non reconnu (PNG, JPEG, WebP, GIF, BMP)")?;
    Ok(ImageAsset { name, width, height, data: Arc::new(data) })
}

fn decoded(asset: &ImageAsset) -> Option<Arc<RgbaImage>> {
    let k = key(asset);
    DECODED.with(|cache| {
        cache
            .borrow_mut()
            .entry(k)
            .or_insert_with(|| {
                let img = image::load_from_memory(&asset.data).ok()?;
                let img = if img.width().max(img.height()) > MAX_DISPLAY {
                    img.resize(MAX_DISPLAY, MAX_DISPLAY, image::imageops::FilterType::Triangle)
                } else {
                    img
                };
                Some(Arc::new(img.into_rgba8()))
            })
            .clone()
    })
}

/// Texture for an asset at the given opacity (0..=1).
pub fn texture(asset: &ImageAsset, opacity: f32) -> Option<Arc<RenderImage>> {
    let pct = (opacity.clamp(0.0, 1.0) * 100.0).round() as u8;
    let k = (key(asset), pct);
    if let Some(t) = TEXTURES.with(|c| c.borrow().get(&k).cloned()) {
        return Some(t);
    }
    let rgba = decoded(asset)?;
    let mut buf = (*rgba).clone();
    let a = pct as f32 / 100.0;
    for px in buf.as_chunks_mut::<4>().0 {
        px.swap(0, 2); // RGBA → BGRA, as GPUI expects
        px[3] = (px[3] as f32 * a).round() as u8;
    }
    let tex = Arc::new(RenderImage::new([Frame::new(buf)]));
    TEXTURES.with(|c| c.borrow_mut().insert(k, tex.clone()));
    Some(tex)
}

/// Pixel color at normalized coordinates (0..1) of the displayed image.
pub fn pixel(asset: &ImageAsset, u: f64, v: f64) -> Option<desy_core::Color> {
    let img = decoded(asset)?;
    let x = ((u * img.width() as f64) as u32).min(img.width().saturating_sub(1));
    let y = ((v * img.height() as f64) as u32).min(img.height().saturating_sub(1));
    let [r, g, b, a] = img.get_pixel(x, y).0;
    Some(desy_core::Color::rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0))
}

pub fn is_image_path(p: &std::path::Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp")
    )
}
