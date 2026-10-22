//! "Check for Updates": compare the running version with the latest GitHub
//! release of `general.update_repo`. Nothing runs unless the user asks or
//! turns on `check_updates`.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    /// `.dmg` or `.zip` asset, when the release has one.
    pub download: Option<String>,
    pub notes: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// `v1.2.3` / `1.2` → `[1, 2, 3]` / `[1, 2, 0]`. Pre-release suffixes are ignored.
pub fn parse_version(v: &str) -> Option<[u64; 3]> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some([major, minor, patch])
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Parse the GitHub `releases/latest` JSON; `Some` when it is newer than `current`.
pub fn newer_from_json(json: &str, current: &str) -> Result<Option<Release>> {
    let r: GhRelease = serde_json::from_str(json).context("unexpected GitHub reply")?;
    if r.draft || r.prerelease || !is_newer(&r.tag_name, current) {
        return Ok(None);
    }
    let download = r
        .assets
        .iter()
        .find(|a| a.name.ends_with(".dmg"))
        .or_else(|| r.assets.iter().find(|a| a.name.ends_with(".zip")))
        .map(|a| a.browser_download_url.clone());
    Ok(Some(Release {
        version: r.tag_name.trim_start_matches(['v', 'V']).to_string(),
        url: r.html_url,
        download,
        notes: r.body,
    }))
}

/// Ask GitHub. `repo` is `owner/name`.
pub async fn check(repo: &str, current: &str) -> Result<Option<Release>> {
    let repo = repo.trim().trim_matches('/');
    if repo.is_empty() || repo.split('/').count() != 2 {
        bail!("set update_repo = \"owner/repo\" under [general] in config.toml");
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("OpenCast/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;
    let resp = client
        .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("accept", "application/vnd.github+json")
        .send()
        .await?;
    if resp.status().as_u16() == 404 {
        return Ok(None); // no releases yet
    }
    if !resp.status().is_success() {
        bail!("GitHub answered HTTP {}", resp.status().as_u16());
    }
    newer_from_json(&resp.text().await?, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_version("0.4"), Some([0, 4, 0]));
        assert_eq!(parse_version("2.0.0-beta.1"), Some([2, 0, 0]));
        assert_eq!(parse_version("nope"), None);
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn release_json() {
        let json = r#"{"tag_name":"v0.3.0","html_url":"https://gh/r","body":"notes",
            "assets":[{"name":"OpenCast.zip","browser_download_url":"https://gh/z"},
                      {"name":"OpenCast-0.3.0.dmg","browser_download_url":"https://gh/d"}]}"#;
        let r = newer_from_json(json, "0.1.0").unwrap().unwrap();
        assert_eq!(r.version, "0.3.0");
        assert_eq!(r.download.as_deref(), Some("https://gh/d"));
        assert!(newer_from_json(json, "0.3.0").unwrap().is_none());
        let pre = json.replace("\"body\"", "\"prerelease\":true,\"body\"");
        assert!(newer_from_json(&pre, "0.1.0").unwrap().is_none());
    }
}
