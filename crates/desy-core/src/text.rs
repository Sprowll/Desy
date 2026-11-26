//! Text → vector outlines. Text objects are converted to Bézier paths with
//! the font's own glyph outlines, so they render, export and combine
//! (booleans) exactly like shapes.
//!
//! Font sources: system fonts (which include Adobe Fonts activated through
//! Creative Cloud), the Adobe Fonts sync folder, and the app's own font folder
//! where downloaded Google Fonts are stored.

use kurbo::{Affine, BezPath, Point};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextSpec {
    pub content: String,
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    pub size: f64,
    /// Extra space between letters, in px.
    pub letter_spacing: f64,
    /// Line height as a multiple of `size`.
    pub line_height: f64,
    pub align: TextAlign,
}

impl Default for TextSpec {
    fn default() -> Self {
        Self {
            content: "Logo".into(),
            family: "Helvetica".into(),
            weight: 700,
            italic: false,
            size: 96.0,
            letter_spacing: 0.0,
            line_height: 1.2,
            align: TextAlign::Left,
        }
    }
}

/// Where the app stores its own data (downloaded fonts, recent projects…).
pub fn app_data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Desy")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(home).join("Desy")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share")).join("desy")
    }
}

pub fn user_fonts_dir() -> PathBuf {
    app_data_dir().join("fonts")
}

/// Folder where Creative Cloud stores fonts activated from Adobe Fonts.
pub fn adobe_fonts_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/Application Support/Adobe/CoreSync/plugins/livetype/.r"))
    } else if cfg!(target_os = "windows") {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("Adobe/CoreSync/plugins/livetype/r"))
    } else {
        None
    }
}

#[derive(Clone)]
pub struct FontLibrary {
    db: fontdb::Database,
    families: Vec<String>,
}

/// The default library is empty; use [`FontLibrary::new`] to load fonts.
impl Default for FontLibrary {
    fn default() -> Self {
        Self::empty()
    }
}

impl FontLibrary {
    /// An empty library (tests, headless use).
    pub fn empty() -> Self {
        Self { db: fontdb::Database::new(), families: Vec::new() }
    }

    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        if let Some(dir) = adobe_fonts_dir()
            && dir.is_dir()
        {
            db.load_fonts_dir(dir);
        }
        let user = user_fonts_dir();
        if user.is_dir() {
            db.load_fonts_dir(user);
        }
        let mut lib = Self { db, families: Vec::new() };
        lib.refresh_families();
        lib
    }

    fn refresh_families(&mut self) {
        let mut fams: Vec<String> = self
            .db
            .faces()
            .filter_map(|f| f.families.first().map(|(n, _)| n.clone()))
            .filter(|n| !n.starts_with('.'))
            .collect();
        fams.sort_by_key(|a| a.to_lowercase());
        fams.dedup();
        self.families = fams;
    }

    pub fn families(&self) -> &[String] {
        &self.families
    }

    pub fn has_family(&self, family: &str) -> bool {
        self.families.iter().any(|f| f.eq_ignore_ascii_case(family))
    }

    pub fn load_file(&mut self, path: impl Into<PathBuf>) -> anyhow::Result<()> {
        self.db.load_font_file(path.into())?;
        self.refresh_families();
        Ok(())
    }

    pub fn load_data(&mut self, data: Vec<u8>) {
        self.db.load_font_data(data);
        self.refresh_families();
    }

    fn query(&self, spec: &TextSpec) -> Option<fontdb::ID> {
        let families = [fontdb::Family::Name(&spec.family), fontdb::Family::SansSerif];
        self.db.query(&fontdb::Query {
            families: &families,
            weight: fontdb::Weight(spec.weight),
            stretch: fontdb::Stretch::Normal,
            style: if spec.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        })
    }

    /// Outline of the text, origin at the top-left of the text box.
    pub fn outline(&self, spec: &TextSpec) -> BezPath {
        let Some(id) = self.query(spec) else { return BezPath::new() };
        self.db
            .with_face_data(id, |data, index| {
                let Ok(face) = ttf_parser::Face::parse(data, index) else { return BezPath::new() };
                layout(&face, spec)
            })
            .unwrap_or_default()
    }
}

struct Outliner {
    path: BezPath,
    xf: Affine,
}

impl ttf_parser::OutlineBuilder for Outliner {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.xf * Point::new(x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.xf * Point::new(x as f64, y as f64));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(self.xf * Point::new(x1 as f64, y1 as f64), self.xf * Point::new(x as f64, y as f64));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.curve_to(
            self.xf * Point::new(x1 as f64, y1 as f64),
            self.xf * Point::new(x2 as f64, y2 as f64),
            self.xf * Point::new(x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

fn kerning(face: &ttf_parser::Face, left: ttf_parser::GlyphId, right: ttf_parser::GlyphId) -> i16 {
    let Some(kern) = face.tables().kern else { return 0 };
    kern.subtables
        .into_iter()
        .filter(|s| s.horizontal && !s.variable)
        .find_map(|s| s.glyphs_kerning(left, right))
        .unwrap_or(0)
}

fn layout(face: &ttf_parser::Face, spec: &TextSpec) -> BezPath {
    let upem = face.units_per_em() as f64;
    let scale = spec.size / upem;
    let ascender = face.ascender() as f64 * scale;
    let line_advance = spec.size * spec.line_height;

    struct Line {
        glyphs: Vec<(ttf_parser::GlyphId, f64)>,
        width: f64,
    }
    let mut lines = Vec::new();
    for text_line in spec.content.split('\n') {
        let mut glyphs = Vec::new();
        let mut x = 0.0;
        let mut prev: Option<ttf_parser::GlyphId> = None;
        for ch in text_line.chars() {
            let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
            if let Some(p) = prev {
                x += kerning(face, p, gid) as f64 * scale;
            }
            glyphs.push((gid, x));
            x += face.glyph_hor_advance(gid).unwrap_or(0) as f64 * scale + spec.letter_spacing;
            prev = Some(gid);
        }
        let width = if glyphs.is_empty() { 0.0 } else { x - spec.letter_spacing };
        lines.push(Line { glyphs, width });
    }
    let max_width = lines.iter().map(|l| l.width).fold(0.0, f64::max);

    let mut out = Outliner { path: BezPath::new(), xf: Affine::IDENTITY };
    for (i, line) in lines.iter().enumerate() {
        let dx = match spec.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (max_width - line.width) * 0.5,
            TextAlign::Right => max_width - line.width,
        };
        let baseline = ascender + i as f64 * line_advance;
        for &(gid, x) in &line.glyphs {
            // Font units are y-up; the canvas is y-down.
            out.xf = Affine::new([scale, 0.0, 0.0, -scale, dx + x, baseline]);
            face.outline_glyph(gid, &mut out);
        }
    }
    out.path
}

/// A curated list of popular Google Fonts families offered out of the box.
pub const GOOGLE_FONTS: &[&str] = &[
    "Inter",
    "Roboto",
    "Montserrat",
    "Poppins",
    "Open Sans",
    "Lato",
    "Raleway",
    "Oswald",
    "Playfair Display",
    "Merriweather",
    "Nunito",
    "Rubik",
    "Work Sans",
    "DM Sans",
    "Manrope",
    "Space Grotesk",
    "Sora",
    "Outfit",
    "Lexend",
    "Archivo",
    "Archivo Black",
    "Bebas Neue",
    "Anton",
    "Righteous",
    "Fredoka",
    "Baloo 2",
    "Comfortaa",
    "Pacifico",
    "Lobster",
    "Bungee",
    "Black Ops One",
    "Russo One",
    "Orbitron",
    "Audiowide",
    "Syne",
    "Unbounded",
    "Bricolage Grotesque",
    "Plus Jakarta Sans",
    "Josefin Sans",
    "Quicksand",
    "Caveat",
    "Permanent Marker",
    "Abril Fatface",
    "Cinzel",
    "DM Serif Display",
    "Libre Baskerville",
    "Bitter",
    "Kanit",
    "Titan One",
    "Luckiest Guy",
    "Bangers",
    "Chewy",
    "Lilita One",
];

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    #[test]
    fn outlines_system_text() {
        let lib = FontLibrary::new();
        if lib.families().is_empty() {
            return; // no fonts on this machine (CI container)
        }
        let spec = TextSpec { content: "Hi\nThere".into(), family: lib.families()[0].clone(), ..Default::default() };
        let path = lib.outline(&spec);
        let bb = path.bounding_box();
        assert!(bb.width() > 10.0 && bb.height() > spec.size, "{bb:?}");
        assert!(bb.min_y() >= -1.0, "text starts below the top edge: {bb:?}");
    }
}
