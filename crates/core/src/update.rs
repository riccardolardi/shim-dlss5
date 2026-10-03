//! Update check: ask GitHub for our latest release and compare versions.
//! Nothing is downloaded or run; the UI only offers to open the release page.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, Result};

pub const RELEASES_API: &str =
    "https://api.github.com/repos/riccardolardi/shim-dlss5/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// `Some` when a newer non-prerelease exists.
pub fn check(current: &str) -> Result<Option<UpdateInfo>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("shim/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(err)?;
    let body = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(err)?
        .text()
        .map_err(err)?;
    evaluate(current, &body)
}

/// Pure part, tested without network.
pub fn evaluate(current: &str, body: &str) -> Result<Option<UpdateInfo>> {
    let release: Release = serde_json::from_str(body).map_err(|e| Error::parse("release", e))?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let latest = release.tag_name.trim_start_matches('v').to_string();
    Ok((is_newer(&latest, current)).then(|| UpdateInfo {
        current: current.to_string(),
        latest,
        url: release.html_url,
    }))
}

/// `a > b` as dotted numeric versions; anything unparseable is not newer.
pub fn is_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Option<Vec<u64>> {
        s.split('-')
            .next()?
            .split('.')
            .map(|p| p.parse().ok())
            .collect()
    };
    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Download {
        url: RELEASES_API.to_string(),
        detail: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn evaluate_reports_newer_releases_only() {
        let body =
            r#"{"tag_name":"v0.2.0","html_url":"https://x/rel","draft":false,"prerelease":false}"#;
        let info = evaluate("0.1.0", body).unwrap().unwrap();
        assert_eq!(info.latest, "0.2.0");
        assert_eq!(info.url, "https://x/rel");
        assert_eq!(evaluate("0.2.0", body).unwrap(), None);
        let pre = r#"{"tag_name":"v0.3.0","html_url":"u","prerelease":true}"#;
        assert_eq!(evaluate("0.1.0", pre).unwrap(), None);
        assert_eq!(evaluate("0.1.0", "nope").unwrap_err().code(), "parse");
    }
}
