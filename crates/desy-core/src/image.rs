//! Raster images embedded in a project (references to trace over, photos…).
//!
//! The encoded file bytes are stored once per document and shared with `Arc`,
//! so undo snapshots don't copy them. In the project file they are base64.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageAsset {
    pub name: String,
    /// Pixel size of the original image.
    pub width: u32,
    pub height: u32,
    /// Encoded file (PNG, JPEG, WebP…).
    #[serde(serialize_with = "ser_b64", deserialize_with = "de_b64")]
    pub data: Arc<Vec<u8>>,
}

impl ImageAsset {
    pub fn mime(&self) -> &'static str {
        let d = self.data.as_slice();
        if d.starts_with(b"\x89PNG") {
            "image/png"
        } else if d.starts_with(&[0xff, 0xd8]) {
            "image/jpeg"
        } else if d.len() > 12 && &d[0..4] == b"RIFF" && &d[8..12] == b"WEBP" {
            "image/webp"
        } else if d.starts_with(b"GIF8") {
            "image/gif"
        } else {
            "application/octet-stream"
        }
    }

    pub fn data_uri(&self) -> String {
        format!("data:{};base64,{}", self.mime(), STANDARD.encode(self.data.as_slice()))
    }
}

fn ser_b64<S: Serializer>(data: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&STANDARD.encode(data.as_slice()))
}

fn de_b64<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
    let s = String::deserialize(d)?;
    STANDARD.decode(s).map(Arc::new).map_err(serde::de::Error::custom)
}
