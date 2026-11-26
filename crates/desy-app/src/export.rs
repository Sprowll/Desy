//! Export formats. PNG and JPG are rasterized from the SVG export, so every
//! format renders the same drawing (references excluded, text as outlines).

use anyhow::{Context as _, Result, anyhow};
use resvg::{tiny_skia, usvg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Svg,
    Png,
    Jpg,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Svg, Format::Png, Format::Jpg];

    pub fn label(self) -> &'static str {
        match self {
            Format::Svg => "SVG",
            Format::Png => "PNG",
            Format::Jpg => "JPG",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Svg => "svg",
            Format::Png => "png",
            Format::Jpg => "jpg",
        }
    }
}

/// Encode an exported SVG document in `format`. Raster formats are drawn at
/// the document size; JPG has no transparency, so it gets a white background.
pub fn encode(svg: &str, format: Format) -> Result<Vec<u8>> {
    match format {
        Format::Svg => Ok(svg.as_bytes().to_vec()),
        Format::Png => rasterize(svg)?.encode_png().context("encodage PNG"),
        Format::Jpg => {
            let pixmap = rasterize(svg)?;
            let mut rgb = Vec::with_capacity((pixmap.width() * pixmap.height() * 3) as usize);
            for p in pixmap.pixels() {
                // Premultiplied over white: c + (1 - a) * 255.
                let inv = 255 - p.alpha() as u16;
                rgb.push((p.red() as u16 + inv).min(255) as u8);
                rgb.push((p.green() as u16 + inv).min(255) as u8);
                rgb.push((p.blue() as u16 + inv).min(255) as u8);
            }
            let mut out = Vec::new();
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
            enc.encode(&rgb, pixmap.width(), pixmap.height(), image::ExtendedColorType::Rgb8)
                .context("encodage JPG")?;
            Ok(out)
        }
    }
}

fn rasterize(svg: &str) -> Result<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).context("lecture du SVG exporté")?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| anyhow!("taille d'image invalide ({} × {})", size.width(), size.height()))?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    Ok(pixmap)
}

#[cfg(test)]
mod tests {
    use super::{Format, encode};

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10">
  <rect x="0" y="0" width="10" height="10" fill="#ff0000"/>
</svg>"##;

    #[test]
    fn png_keeps_size_and_transparency() {
        let png = encode(SVG, Format::Png).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (20, 10));
        assert_eq!(img.get_pixel(5, 5).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(15, 5).0[3], 0);
    }

    #[test]
    fn jpg_is_opaque_on_white() {
        let jpg = encode(SVG, Format::Jpg).unwrap();
        assert!(jpg.starts_with(&[0xff, 0xd8]));
        let img = image::load_from_memory(&jpg).unwrap().to_rgb8();
        assert_eq!(img.dimensions(), (20, 10));
        let [r, g, b] = img.get_pixel(15, 5).0;
        assert!(r > 245 && g > 245 && b > 245, "transparent area should be white");
        let [r, g, b] = img.get_pixel(3, 5).0;
        assert!(r > 200 && g < 60 && b < 60, "red area stays red");
    }

    #[test]
    fn svg_is_passed_through() {
        assert_eq!(encode(SVG, Format::Svg).unwrap(), SVG.as_bytes());
    }
}
