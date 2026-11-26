//! Update check: asks GitHub for the latest published release and tells the
//! home screen when it is newer than the running version. Nothing is
//! downloaded or installed automatically; the user opens the release page.
//!
//! Disabled in debug builds and when `DESY_NO_UPDATE_CHECK` is set.

use anyhow::{Context as _, Result};

/// GitHub repository the releases are published to (`owner/name`).
pub const REPO: &str = "Sprowll/Desy";

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
}

/// The latest release if it is newer than this build, `None` otherwise
/// (also when the check is disabled or fails: it must never bother the user).
pub fn check() -> Option<Release> {
    if cfg!(debug_assertions) || std::env::var_os("DESY_NO_UPDATE_CHECK").is_some() {
        return None;
    }
    let latest = fetch_latest().ok()?;
    is_newer(&latest.version, env!("CARGO_PKG_VERSION")).then_some(latest)
}

fn fetch_latest() -> Result<Release> {
    let agent: ureq::Agent =
        ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(10))).build().into();
    let body = agent
        .get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", "Desy")
        .header("Accept", "application/vnd.github+json")
        .call()?
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()?;
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let tag = json["tag_name"].as_str().context("tag_name manquant")?;
    let url = json["html_url"].as_str().context("html_url manquant")?;
    // Only ever open GitHub pages from this check.
    anyhow::ensure!(url.starts_with("https://github.com/"), "URL inattendue");
    Ok(Release { version: tag.trim_start_matches('v').to_string(), url: url.to_string() })
}

/// `major.minor.patch` of a version string; a pre-release suffix (`-beta.1`)
/// or build metadata is ignored.
fn parse(v: &str) -> Option<[u64; 3]> {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut it = core.split('.').map(|n| n.parse::<u64>().ok());
    let out = [it.next()??, it.next().unwrap_or(Some(0))?, it.next().unwrap_or(Some(0))?];
    it.next().is_none().then_some(out)
}

fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse(candidate), parse(current)), (Some(a), Some(b)) if a > b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert!(!is_newer("1.2.3.4", "0.1.0"));
    }

    #[test]
    fn parses_versions() {
        assert_eq!(parse("v1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse("1.2"), Some([1, 2, 0]));
        assert_eq!(parse("2.0.0-beta.1"), Some([2, 0, 0]));
        assert_eq!(parse(""), None);
    }
}
