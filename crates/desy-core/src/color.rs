use serde::{Deserialize, Serialize};

/// Straight (non-premultiplied) sRGB color, components in 0..=1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let s = hex.trim().trim_start_matches('#');
        let parse = |i: usize, len: usize| -> Option<f32> {
            let v = u8::from_str_radix(&s[i..i + len], 16).ok()?;
            Some(if len == 1 { (v * 17) as f32 / 255.0 } else { v as f32 / 255.0 })
        };
        match s.len() {
            3 => Some(Self::rgb(parse(0, 1)?, parse(1, 1)?, parse(2, 1)?)),
            6 => Some(Self::rgb(parse(0, 2)?, parse(2, 2)?, parse(4, 2)?)),
            8 => Some(Self::rgba(parse(0, 2)?, parse(2, 2)?, parse(4, 2)?, parse(6, 2)?)),
            _ => None,
        }
    }

    /// `#rrggbb`, alpha ignored.
    pub fn to_hex(self) -> String {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Color::from_hex("#0d9488").unwrap();
        assert_eq!(c.to_hex(), "#0d9488");
        assert_eq!(Color::from_hex("fff").unwrap(), Color::WHITE);
        assert!(Color::from_hex("#12").is_none());
        assert!(Color::from_hex("zzzzzz").is_none());
    }
}
