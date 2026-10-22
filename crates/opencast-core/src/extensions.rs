//! Extensions: git repositories of script commands (OpenCast or Raycast
//! format) or Alfred workflows, cloned into `<config>/extensions/<name>/`.
//!
//! Sources:
//! - `owner/repo` (GitHub), or `owner/repo/sub/folder` for one folder of a
//!   big repo (sparse checkout), e.g. `raycast/script-commands/commands/apps/spotify`
//! - `https://github.com/owner/repo/tree/<branch>/sub/folder`
//! - any git URL (`https://…`, `git@…`)
//!
//! Every extension is an ordinary clone: `git pull` updates it, deleting the
//! folder removes it. Discovery walks it like the scripts folder.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::scripts::{self, ScriptMeta};

/// Written into each extension folder.
const MANIFEST: &str = ".opencast-extension.json";
/// Folders never scanned for scripts.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "images",
    "assets",
    ".github",
    "__pycache__",
    "venv",
    ".venv",
];
const MAX_DEPTH: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub url: String,
    pub branch: Option<String>,
    /// Folder inside the repo (sparse checkout).
    pub subpath: Option<String>,
    /// Folder name under `extensions/`.
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub source: String,
    #[serde(default)]
    pub subpath: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub installed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionInfo {
    pub name: String,
    pub path: PathBuf,
    pub source: Option<String>,
    pub commands: usize,
    pub description: Option<String>,
    /// Contains an Alfred workflow (`info.plist`).
    pub alfred: bool,
}

pub fn root(config_dir: &Path) -> PathBuf {
    config_dir.join("extensions")
}

fn slug(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches(['-', '.']).to_lowercase()
}

/// Parse what the user typed into a [`Source`].
pub fn parse_source(input: &str) -> Result<Source> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        bail!("type a GitHub repo (owner/repo) or a git URL");
    }
    if s.contains(char::is_whitespace) {
        bail!("`{s}` is not a repo: use owner/repo or a git URL");
    }
    // github.com URLs, including /tree/<branch>/<path>.
    let gh = s
        .strip_prefix("https://github.com/")
        .or_else(|| s.strip_prefix("http://github.com/"))
        .or_else(|| s.strip_prefix("github.com/"));
    if let Some(rest) = gh {
        let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() < 2 {
            bail!("`{s}` needs owner and repo");
        }
        let repo = parts[1].trim_end_matches(".git");
        let url = format!("https://github.com/{}/{repo}.git", parts[0]);
        if parts.len() >= 4 && (parts[2] == "tree" || parts[2] == "blob") {
            let subpath = parts[4..].join("/");
            let name = parts.last().map(|p| slug(p)).unwrap_or_else(|| slug(repo));
            return Ok(Source {
                url,
                branch: Some(parts[3].to_string()),
                subpath: (!subpath.is_empty()).then_some(subpath),
                name: if parts.len() > 4 { name } else { slug(repo) },
            });
        }
        return Ok(Source {
            url,
            branch: None,
            subpath: None,
            name: slug(repo),
        });
    }
    if s.contains("://") || s.starts_with("git@") {
        let last = s.rsplit(['/', ':']).next().unwrap_or(s).trim_end_matches(".git");
        return Ok(Source {
            url: s.to_string(),
            branch: None,
            subpath: None,
            name: slug(last),
        });
    }
    // owner/repo[/sub/path]
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    let valid = |p: &str| p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if parts.len() < 2 || !parts.iter().all(|p| valid(p)) {
        bail!("`{s}` is not a repo: use owner/repo, owner/repo/folder or a git URL");
    }
    let repo = parts[1].trim_end_matches(".git");
    let subpath = (parts.len() > 2).then(|| parts[2..].join("/"));
    Ok(Source {
        url: format!("https://github.com/{}/{repo}.git", parts[0]),
        branch: None,
        name: slug(parts.last().unwrap_or(&repo)),
        subpath,
    })
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    if let Some(d) = dir {
        cmd.arg("-C").arg(d);
    }
    cmd.args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("PATH", scripts::path_with_extras());
    let out = cmd
        .output()
        .context("running git (install the Xcode command-line tools)")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("git failed")
            .trim()
            .trim_start_matches("fatal: ")
            .to_string();
        bail!("{line}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Scripts in one extension folder (recursive, skipping assets and VCS).
pub fn scan(dir: &Path, package: &str) -> (Vec<ScriptMeta>, Vec<String>) {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    walk(dir, 0, &mut |path| {
        let (mut metas, mut warns) = scripts::discover(path);
        for m in &mut metas {
            if m.package.is_none() {
                m.package = Some(package.to_string());
            }
        }
        found.append(&mut metas);
        warnings.append(&mut warns);
    });
    found.sort_by_key(|a| a.title.to_lowercase());
    (found, warnings)
}

fn walk(dir: &Path, depth: usize, f: &mut dyn FnMut(&Path)) {
    f(dir);
    if depth >= MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut subdirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            !n.starts_with('.') && !SKIP_DIRS.contains(&n.as_str())
        })
        .map(|e| e.path())
        .collect();
    subdirs.sort();
    for d in subdirs {
        walk(&d, depth + 1, f);
    }
}

/// Every installed extension's scripts (for the engine).
pub fn discover_all(config_dir: &Path) -> (Vec<ScriptMeta>, Vec<String>) {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    for ext in installed_dirs(config_dir) {
        let name = ext
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let (mut m, w) = scan(&ext, &name);
        found.append(&mut m);
        warnings.extend(w.into_iter().map(|w| format!("extension {name}: {w}")));
    }
    (found, warnings)
}

fn installed_dirs(config_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root(config_dir)) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs
}

fn manifest(dir: &Path) -> Option<Manifest> {
    std::fs::read_to_string(dir.join(MANIFEST))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// First paragraph line of the README, as a description.
fn readme_line(dir: &Path) -> Option<String> {
    for name in ["README.md", "readme.md", "README"] {
        if let Ok(text) = std::fs::read_to_string(dir.join(name)) {
            if let Some(l) = text.lines().map(str::trim).find(|l| {
                !l.is_empty()
                    && !l.starts_with('#')
                    && !l.starts_with('[')
                    && !l.starts_with('<')
                    && !l.starts_with('!')
            }) {
                return Some(l.chars().take(120).collect());
            }
        }
    }
    None
}

pub fn list(config_dir: &Path) -> Vec<ExtensionInfo> {
    installed_dirs(config_dir)
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let m = manifest(&path);
            let content = m
                .as_ref()
                .and_then(|m| m.subpath.as_ref())
                .map(|s| path.join(s))
                .unwrap_or_else(|| path.clone());
            ExtensionInfo {
                commands: scan(&path, &name).0.len(),
                description: readme_line(&content).or_else(|| readme_line(&path)),
                alfred: has_alfred_workflow(&path),
                source: m.map(|m| match m.subpath {
                    Some(s) => format!("{} › {s}", m.source),
                    None => m.source,
                }),
                name,
                path,
            }
        })
        .collect()
}

fn has_alfred_workflow(dir: &Path) -> bool {
    let mut found = false;
    walk(dir, 0, &mut |d| {
        found |= d.join("info.plist").is_file() && d.join("info.plist").metadata().is_ok()
    });
    found
}

/// Clone `input` into `extensions/`. Returns the new extension.
pub fn install(config_dir: &Path, input: &str) -> Result<ExtensionInfo> {
    let src = parse_source(input)?;
    let root = root(config_dir);
    std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
    let dest = root.join(&src.name);
    if dest.exists() {
        bail!("`{}` is already installed — use Update Extensions", src.name);
    }
    let tmp = root.join(format!(".installing-{}", src.name));
    let _ = std::fs::remove_dir_all(&tmp);
    let tmp_s = tmp.to_string_lossy().to_string();
    let mut args = vec!["clone", "--depth", "1", "--quiet"];
    if let Some(b) = &src.branch {
        args.extend(["--branch", b.as_str()]);
    }
    if src.subpath.is_some() {
        args.extend(["--filter=blob:none", "--sparse"]);
    }
    args.extend([src.url.as_str(), tmp_s.as_str()]);
    let result = (|| -> Result<()> {
        git(None, &args)?;
        if let Some(sub) = &src.subpath {
            git(Some(&tmp), &["sparse-checkout", "set", sub])?;
            if !tmp.join(sub).is_dir() {
                bail!("`{sub}` is not a folder in {}", src.url);
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let (scripts, _) = scan(&tmp, &src.name);
    if scripts.is_empty() && !has_alfred_workflow(&tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!("no script commands or Alfred workflows in {}", input.trim());
    }
    let m = Manifest {
        source: src.url.clone(),
        subpath: src.subpath.clone(),
        branch: src.branch.clone(),
        installed: crate::usage::now(),
    };
    std::fs::write(tmp.join(MANIFEST), serde_json::to_string_pretty(&m)?)?;
    std::fs::rename(&tmp, &dest).with_context(|| format!("moving into {}", dest.display()))?;
    list(config_dir)
        .into_iter()
        .find(|e| e.name == src.name)
        .context("installed, but could not read it back")
}

/// `git pull` every extension. Returns (name, Ok(changed) | Err).
pub fn update_all(config_dir: &Path) -> Vec<(String, Result<bool>)> {
    installed_dirs(config_dir)
        .into_iter()
        .filter(|d| d.join(".git").exists())
        .map(|d| {
            let name = d
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let r = (|| {
                let before = git(Some(&d), &["rev-parse", "HEAD"])?;
                git(Some(&d), &["pull", "--ff-only", "--quiet"])?;
                let after = git(Some(&d), &["rev-parse", "HEAD"])?;
                Ok(before != after)
            })();
            (name, r)
        })
        .collect()
}

/// Clone or pull every `git:<url>` entry of `include` into `<config>/includes/`.
pub fn sync_git_includes(config_dir: &Path, includes: &[String]) -> Vec<(String, Result<bool>)> {
    includes
        .iter()
        .filter_map(|e| e.strip_prefix("git:"))
        .map(|url| {
            let dir = crate::config::Config::include_clone_dir(config_dir, url);
            let r = (|| {
                if dir.join(".git").exists() {
                    let before = git(Some(&dir), &["rev-parse", "HEAD"])?;
                    git(Some(&dir), &["pull", "--ff-only", "--quiet"])?;
                    Ok(before != git(Some(&dir), &["rev-parse", "HEAD"])?)
                } else {
                    if let Some(parent) = dir.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let d = dir.to_string_lossy().to_string();
                    git(None, &["clone", "--depth", "1", "--quiet", url, &d])?;
                    Ok(true)
                }
            })();
            (url.to_string(), r)
        })
        .collect()
}

pub fn remove(config_dir: &Path, name: &str) -> Result<()> {
    if name.is_empty() || name.contains('/') || name.contains("..") || name.starts_with('.') {
        bail!("bad extension name");
    }
    let dir = root(config_dir).join(name);
    if !dir.is_dir() {
        bail!("`{name}` is not installed");
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))
}

/// A repo found by [`browse`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Listing {
    /// What to pass to [`install`].
    pub source: String,
    pub title: String,
    pub description: String,
    pub stars: Option<u64>,
}

/// Folders of the open-source script-commands collection that install well
/// as a whole. Titles describe the content; the source is shown separately
/// (no third-party names in titles, see the compat doc's wording rules).
pub const CURATED: &[(&str, &str, &str)] = &[
    (
        "raycast/script-commands/commands/apps",
        "App Controls (script commands)",
        "Control Spotify, Music, Safari, Slack, Zoom and more",
    ),
    (
        "raycast/script-commands/commands/developer-utils",
        "Developer Utilities (script commands)",
        "Encoders, UUIDs, timestamps, Git, Xcode, Docker helpers",
    ),
    (
        "raycast/script-commands/commands/system",
        "System Controls (script commands)",
        "Wi-Fi, Bluetooth, audio devices, caffeinate, dock, network",
    ),
    (
        "raycast/script-commands/commands/navigation",
        "Navigation (script commands)",
        "Open folders and places quickly",
    ),
    (
        "raycast/script-commands/commands/web-searches",
        "Web Searches (script commands)",
        "Search sites straight from the launcher",
    ),
    (
        "raycast/script-commands/commands/media",
        "Media (script commands)",
        "Images, video and audio helpers",
    ),
    (
        "raycast/script-commands/commands/productivity",
        "Productivity (script commands)",
        "Todos, timers, focus and notes helpers",
    ),
    (
        "raycast/script-commands/commands/conversions",
        "Conversions (script commands)",
        "Units, colors, cases and formats",
    ),
];

/// "from github.com/owner/repo" for a listing's source.
pub fn source_label(source: &str) -> String {
    let mut parts = source.split('/');
    match (parts.next(), parts.next()) {
        (Some(o), Some(r)) if !source.contains("://") => format!("from github.com/{o}/{r}"),
        _ => format!("from {source}"),
    }
}

/// Curated folders plus GitHub repos tagged `opencast-extension` matching `query`.
pub async fn browse(query: &str) -> Result<Vec<Listing>> {
    let q = query.trim().to_lowercase();
    let mut out: Vec<Listing> = CURATED
        .iter()
        .filter(|(s, t, d)| q.is_empty() || format!("{s} {t} {d}").to_lowercase().contains(&q))
        .map(|(s, t, d)| Listing {
            source: s.to_string(),
            title: t.to_string(),
            description: d.to_string(),
            stars: None,
        })
        .collect();
    let search = if q.is_empty() {
        "topic:opencast-extension".to_string()
    } else {
        format!("{q} topic:opencast-extension")
    };
    let url = format!(
        "https://api.github.com/search/repositories?q={}&sort=stars&per_page=30",
        crate::engine::url_encode(&search)
    );
    let resp = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "OpenCast")
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => {
            let v: serde_json::Value = r.json().await.unwrap_or_default();
            for item in v["items"].as_array().cloned().unwrap_or_default() {
                let Some(full) = item["full_name"].as_str() else {
                    continue;
                };
                out.push(Listing {
                    source: full.to_string(),
                    title: item["name"].as_str().unwrap_or(full).to_string(),
                    description: item["description"].as_str().unwrap_or("").to_string(),
                    stars: item["stargazers_count"].as_u64(),
                });
            }
        }
        // Offline or rate limited: the curated list still works.
        Ok(_) | Err(_) if !out.is_empty() => {}
        Ok(r) => bail!("GitHub search failed ({})", r.status()),
        Err(e) => bail!("GitHub search failed: {e}"),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sources() {
        let s = parse_source("owner/repo").unwrap();
        assert_eq!(s.url, "https://github.com/owner/repo.git");
        assert_eq!(s.name, "repo");
        assert!(s.subpath.is_none());

        let s = parse_source("raycast/script-commands/commands/apps/spotify").unwrap();
        assert_eq!(s.url, "https://github.com/raycast/script-commands.git");
        assert_eq!(s.subpath.as_deref(), Some("commands/apps/spotify"));
        assert_eq!(s.name, "spotify");

        let s = parse_source("https://github.com/raycast/script-commands/tree/master/commands/system").unwrap();
        assert_eq!(s.branch.as_deref(), Some("master"));
        assert_eq!(s.subpath.as_deref(), Some("commands/system"));
        assert_eq!(s.name, "system");

        let s = parse_source("https://github.com/me/My-Tools.git").unwrap();
        assert_eq!(s.name, "my-tools");
        assert!(s.subpath.is_none());

        let s = parse_source("git@example.com:team/tools.git").unwrap();
        assert_eq!(s.name, "tools");

        assert!(parse_source("").is_err());
        assert!(parse_source("just-a-word").is_err());
        assert!(parse_source("owner/repo; rm -rf ~").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn install_update_list_remove_from_local_repo() {
        use std::os::unix::fs::PermissionsExt;
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let work = tempfile::tempdir().unwrap();
        let repo = work.path().join("tools");
        std::fs::create_dir_all(repo.join("cmds/images")).unwrap();
        let script = repo.join("cmds/hello.sh");
        std::fs::write(&script, "#!/bin/sh\n# @raycast.title Hello Ext\necho hi\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(repo.join("README.md"), "# Tools\n\nHandy things.\n").unwrap();
        let g = |args: &[&str]| git(Some(&repo), args).unwrap();
        g(&["init", "--quiet"]);
        g(&["-c", "user.email=a@b", "-c", "user.name=a", "add", "."]);
        g(&[
            "-c",
            "user.email=a@b",
            "-c",
            "user.name=a",
            "commit",
            "--quiet",
            "-m",
            "init",
        ]);

        let cfg = tempfile::tempdir().unwrap();
        let url = format!("file://{}", repo.display());
        let ext = install(cfg.path(), &url).unwrap();
        assert_eq!(ext.name, "tools");
        assert_eq!(ext.commands, 1);
        assert_eq!(ext.description.as_deref(), Some("Handy things."));
        assert!(install(cfg.path(), &url).is_err(), "second install must fail");

        let (scripts, _) = discover_all(cfg.path());
        assert_eq!(scripts[0].title, "Hello Ext");
        assert_eq!(scripts[0].package.as_deref(), Some("tools"));

        // Upstream change → update pulls it.
        std::fs::write(
            repo.join("cmds/hello.sh"),
            "#!/bin/sh\n# @raycast.title Hello Two\necho hi\n",
        )
        .unwrap();
        g(&[
            "-c",
            "user.email=a@b",
            "-c",
            "user.name=a",
            "commit",
            "--quiet",
            "-am",
            "two",
        ]);
        let updates = update_all(cfg.path());
        assert!(matches!(updates[0].1, Ok(true)), "{updates:?}");
        assert_eq!(discover_all(cfg.path()).0[0].title, "Hello Two");

        assert!(remove(cfg.path(), "../x").is_err());
        remove(cfg.path(), "tools").unwrap();
        assert!(list(cfg.path()).is_empty());
    }

    #[test]
    fn empty_repo_is_rejected() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let work = tempfile::tempdir().unwrap();
        let repo = work.path().join("empty");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("README.md"), "nothing").unwrap();
        let g = |args: &[&str]| git(Some(&repo), args).unwrap();
        g(&["init", "--quiet"]);
        g(&["-c", "user.email=a@b", "-c", "user.name=a", "add", "."]);
        g(&[
            "-c",
            "user.email=a@b",
            "-c",
            "user.name=a",
            "commit",
            "--quiet",
            "-m",
            "init",
        ]);
        let cfg = tempfile::tempdir().unwrap();
        let err = install(cfg.path(), &format!("file://{}", repo.display())).unwrap_err();
        assert!(err.to_string().contains("no script commands"), "{err}");
        assert!(!root(cfg.path()).join(".installing-empty").exists());
    }
}
