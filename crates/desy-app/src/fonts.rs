//! Google Fonts download. Uses the public CSS endpoint, which serves
//! TrueType files to non-browser clients, and stores them in the app's
//! font folder so they are available offline afterwards.

use anyhow::{Context as _, Result, bail};
use desy_core::text::user_fonts_dir;
use std::path::PathBuf;

const WEIGHTS: &str = "100,200,300,400,500,600,700,800,900,100italic,200italic,300italic,400italic,500italic,600italic,700italic,800italic,900italic";

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(30))).build().into()
}

/// Download every style of a Google Fonts family. Returns the font files' bytes.
pub fn download_google_family(family: &str) -> Result<Vec<Vec<u8>>> {
    let url = format!("https://fonts.googleapis.com/css?family={}:{}", family.trim().replace(' ', "+"), WEIGHTS);
    let agent = agent();
    let css = agent
        .get(&url)
        .header("User-Agent", "Desy")
        .call()
        .with_context(|| format!("« {family} » introuvable sur Google Fonts"))?
        .body_mut()
        .read_to_string()?;
    let urls: Vec<&str> = css
        .split("url(")
        .skip(1)
        .filter_map(|s| s.split(')').next())
        .filter(|u| u.starts_with("https://fonts.gstatic.com/"))
        .collect();
    if urls.is_empty() {
        bail!("aucun fichier de police pour « {family} »");
    }
    let dir = family_dir(family);
    std::fs::create_dir_all(&dir)?;
    let mut out = Vec::new();
    for (i, u) in urls.iter().enumerate() {
        let bytes = agent.get(*u).call()?.body_mut().with_config().limit(20 * 1024 * 1024).read_to_vec()?;
        let ext = if u.ends_with(".otf") { "otf" } else { "ttf" };
        std::fs::write(dir.join(format!("{i:02}.{ext}")), &bytes)?;
        out.push(bytes);
    }
    Ok(out)
}

pub fn family_dir(family: &str) -> PathBuf {
    let safe: String = family.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
    user_fonts_dir().join(safe)
}

/// All downloaded font files (to register them with the UI text system).
pub fn downloaded_font_files() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let Ok(families) = std::fs::read_dir(user_fonts_dir()) else { return out };
    for fam in families.flatten() {
        let Ok(files) = std::fs::read_dir(fam.path()) else { continue };
        for f in files.flatten() {
            let p = f.path();
            if matches!(p.extension().and_then(|e| e.to_str()), Some("ttf" | "otf"))
                && let Ok(b) = std::fs::read(&p)
            {
                out.push(b);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    /// Hits the network: `cargo test -p desy -- --ignored`.
    #[test]
    #[ignore]
    fn downloads_a_google_family() {
        let files = super::download_google_family("Baloo 2").unwrap();
        assert!(!files.is_empty());
        let mut lib = desy_core::FontLibrary::empty();
        for f in files {
            lib.load_data(f);
        }
        assert!(lib.has_family("Baloo 2"));
    }
}
