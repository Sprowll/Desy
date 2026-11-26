//! Color palettes: built-in presets and palette files (GIMP `.gpl`,
//! `.hex` / `.txt` lists as shared on Lospec, Paint.NET).

use crate::color::Color;

/// Built-in palettes: (name, colors).
pub const PRESETS: &[(&str, &[&str])] = &[
    (
        "Desy",
        &[
            "#1d1d1f", "#5b5b63", "#a3a3ad", "#d9d9d9", "#ffffff", "#e5484d", "#f4a340", "#ffd23f", "#46a758",
            "#0d9488", "#0d99ff", "#6e56cf", "#d6409f", "#8d5b3f",
        ],
    ),
    (
        "Pastel",
        &[
            "#fbe7e5", "#f8c8c4", "#fde2c4", "#fff1b8", "#d8f0c8", "#c3ebe1", "#c9e4fb", "#d7d2f7", "#f1d0ec",
            "#e9ddd0", "#4a4458", "#fffdf8",
        ],
    ),
    (
        "Vif",
        &[
            "#ff3b30", "#ff8a00", "#ffd60a", "#34c759", "#00c7be", "#0a84ff", "#5e5ce6", "#bf5af2", "#ff2d92",
            "#111111", "#ffffff",
        ],
    ),
    (
        "Terre",
        &[
            "#2b2118", "#5a3e2b", "#8c5f3d", "#b9875a", "#dcb98c", "#f2e3c6", "#6b7a3a", "#9aa65b", "#c45c3b",
            "#e0a458", "#3e5c64",
        ],
    ),
    (
        "Nuit",
        &[
            "#0b0f1a", "#16213a", "#263b6a", "#3f5fa8", "#6f8fd8", "#b7c8f5", "#f3e9c6", "#ffcf6b", "#ff7a59",
            "#c0396b", "#5b2a6e",
        ],
    ),
    (
        "Gris",
        &["#000000", "#1f1f1f", "#3d3d3d", "#5c5c5c", "#7a7a7a", "#999999", "#b8b8b8", "#d6d6d6", "#f5f5f5", "#ffffff"],
    ),
];

pub fn preset(name: &str) -> Option<Vec<Color>> {
    PRESETS.iter().find(|(n, _)| *n == name).map(|(_, hexes)| hexes.iter().filter_map(|h| Color::from_hex(h)).collect())
}

pub fn default_palette() -> Vec<Color> {
    preset(PRESETS[0].0).unwrap_or_default()
}

/// Same color as displayed (8-bit channels + alpha).
pub fn same(a: Color, b: Color) -> bool {
    a.to_hex() == b.to_hex() && (a.a - b.a).abs() < 0.01
}

/// Parse a palette file. Accepts GIMP palettes (`R G B name` lines) and
/// plain lists of hex colors (`#rrggbb`, `rrggbb`, or Paint.NET `aarrggbb`).
/// Duplicates are dropped; order is kept.
pub fn parse(text: &str) -> Vec<Color> {
    let gimp = text.trim_start().starts_with("GIMP Palette");
    let mut out: Vec<Color> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
            continue;
        }
        let color = if gimp { parse_gpl_line(line) } else { parse_hex_line(line) };
        if let Some(c) = color
            && !out.iter().any(|o| same(*o, c))
        {
            out.push(c);
        }
    }
    out
}

fn parse_gpl_line(line: &str) -> Option<Color> {
    if line.starts_with('#') {
        return None; // comment
    }
    let mut it = line.split_whitespace();
    let mut ch = || it.next()?.parse::<u8>().ok().map(|v| v as f32 / 255.0);
    Some(Color::rgb(ch()?, ch()?, ch()?))
}

fn parse_hex_line(line: &str) -> Option<Color> {
    let token = line.split(|c: char| c.is_whitespace() || c == ',' || c == ';').next()?.trim_start_matches('#');
    if !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match token.len() {
        3 | 6 => Color::from_hex(token),
        // Paint.NET / Lospec .txt: AARRGGBB.
        8 => Color::from_hex(&format!("{}{}", &token[2..], &token[..2])),
        _ => None,
    }
}

/// Serialize as a GIMP palette (readable by Aseprite, Krita, Inkscape, GIMP).
pub fn to_gpl(name: &str, colors: &[Color]) -> String {
    let mut s = format!("GIMP Palette\nName: {}\nColumns: 8\n#\n", name.replace('\n', " "));
    for c in colors {
        let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        s.push_str(&format!("{:3} {:3} {:3}\t{}\n", ch(c.r), ch(c.g), ch(c.b), c.to_hex()));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid() {
        for (name, hexes) in PRESETS {
            assert_eq!(preset(name).unwrap().len(), hexes.len(), "{name}");
        }
    }

    #[test]
    fn gpl_roundtrip() {
        let colors = preset("Vif").unwrap();
        let back = parse(&to_gpl("Vif", &colors));
        assert_eq!(back.len(), colors.len());
        assert!(back.iter().zip(&colors).all(|(a, b)| same(*a, *b)));
    }

    #[test]
    fn parses_hex_lists() {
        let txt = "; paint.net palette\nFFFF0000\nff00ff00 green\n#0000ff\n00f\nnot a color\n#0000FF\n";
        let hexes: Vec<String> = parse(txt).into_iter().map(|c| c.to_hex()).collect();
        assert_eq!(hexes, ["#ff0000", "#00ff00", "#0000ff"]);
    }

    #[test]
    fn parses_gimp_with_comments() {
        let gpl = "GIMP Palette\nName: Test\nColumns: 4\n# comment\n255 128   0 Orange\n  0   0   0\tBlack\n";
        let hexes: Vec<String> = parse(gpl).into_iter().map(|c| c.to_hex()).collect();
        assert_eq!(hexes, ["#ff8000", "#000000"]);
    }
}
