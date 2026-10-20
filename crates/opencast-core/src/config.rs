//! `config.toml` model, loading, validation and first-run setup.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Written to `<config dir>/config.toml` on first run.
pub const DEFAULT_CONFIG: &str = include_str!("../../../examples/config.toml");

/// Example scripts written to `<config dir>/scripts/` on first run.
pub const EXAMPLE_SCRIPTS: &[(&str, &str)] = &[
    (
        "toggle-dark-mode.sh",
        include_str!("../../../examples/scripts/toggle-dark-mode.sh"),
    ),
    ("ip-address.sh", include_str!("../../../examples/scripts/ip-address.sh")),
    ("uuid.sh", include_str!("../../../examples/scripts/uuid.sh")),
    (
        "capslock-launcher-key.sh",
        include_str!("../../../examples/scripts/capslock-launcher-key.sh"),
    ),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub ai_commands: Vec<AiCommandConfig>,
    #[serde(default)]
    pub quicklinks: Vec<QuicklinkConfig>,
    #[serde(default)]
    pub hotkeys: Vec<HotkeyBinding>,
}

/// A global hotkey bound to any command: an app, a window action, an AI
/// command, a script... `command` is a command id or a (case-insensitive)
/// command title, e.g. "Slack", "Left Half", "ai:fix-grammar".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyBinding {
    pub key: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    pub default_provider: Option<String>,
    pub scripts_dir: Option<PathBuf>,
    #[serde(default = "default_timeout")]
    pub script_timeout_secs: u64,
    /// Caps Lock becomes ⌃⌥⌘⇧ while held and opens the launcher when tapped.
    #[serde(default)]
    pub hyperkey: bool,
    /// `system`, `light`, or `dark`.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// `island` (top bar) or `spotlight` (centered).
    #[serde(default = "default_presentation")]
    pub presentation: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            default_provider: None,
            scripts_dir: None,
            script_timeout_secs: default_timeout(),
            hyperkey: false,
            theme: default_theme(),
            presentation: default_presentation(),
        }
    }
}

fn default_hotkey() -> String {
    "Alt+Space".into()
}

fn default_timeout() -> u64 {
    30
}

fn default_theme() -> String {
    "system".into()
}

fn default_presentation() -> String {
    "island".into()
}

/// Replace or insert `key = "value"` under `[general]` in `config.toml`.
pub fn set_general_key(dir: &Path, key: &str, value: &str) -> Result<()> {
    anyhow::ensure!(
        key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "bad config key"
    );
    anyhow::ensure!(!value.contains('"') && !value.contains('\n'), "bad config value");
    let path = dir.join("config.toml");
    let src = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let updated = set_general_key_in(&src, key, value);
    std::fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn set_general_key_in(src: &str, key: &str, value: &str) -> String {
    let line = format!("{key} = \"{value}\"");
    let key_eq = format!("{key} =");
    let general_at = if src.starts_with("[general]") {
        Some(0)
    } else {
        src.find("\n[general]").map(|i| i + 1)
    };
    let Some(start) = general_at else {
        return format!("[general]\n{line}\n\n{src}");
    };
    let after_header = start + "[general]".len();
    let section_end = src[after_header..]
        .find("\n[")
        .map(|j| after_header + j)
        .unwrap_or(src.len());
    let section = &src[start..section_end];
    let new_section = if let Some(pos) = section.find(&key_eq) {
        let abs = start + pos;
        let line_end = src[abs..].find('\n').map(|j| abs + j).unwrap_or(src.len());
        format!("{}{}{}", &src[..abs], line, &src[line_end..section_end])
    } else {
        format!("{section}\n{line}")
    };
    format!("{}{}{}", &src[..start], new_section, &src[section_end..])
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Openai,
    Anthropic,
    Gemini,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub api_key_cmd: Option<String>,
    pub model: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

impl ProviderConfig {
    /// Resolve the API key: env var first, then the key command.
    /// `Ok(None)` means no key configured (fine for local servers).
    pub fn resolve_api_key(&self) -> Result<Option<String>> {
        if let Some(var) = &self.api_key_env {
            if let Ok(v) = std::env::var(var) {
                let v = v.trim().to_string();
                if !v.is_empty() {
                    return Ok(Some(v));
                }
            }
        }
        if let Some(cmd) = &self.api_key_cmd {
            let out = Process::new("/bin/sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .with_context(|| format!("provider `{}`: running api_key_cmd", self.id))?;
            if !out.status.success() {
                bail!(
                    "provider `{}`: api_key_cmd exited with {}: {}",
                    self.id,
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            let key = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !key.is_empty() {
                return Ok(Some(key));
            }
        }
        Ok(None)
    }

    /// Default model first, then any extras, without duplicates.
    pub fn all_models(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        std::iter::once(&self.model)
            .chain(self.models.iter())
            .filter(|m| seen.insert(m.as_str()))
            .cloned()
            .collect()
    }

    /// True when the endpoint is on this machine, so a missing key is expected.
    pub fn is_local(&self) -> bool {
        self.base_url
            .as_deref()
            .map(|u| u.contains("://localhost") || u.contains("://127.0.0.1") || u.contains("://[::1]"))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCommandConfig {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub system: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicklinkConfig {
    pub id: Option<String>,
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

impl QuicklinkConfig {
    pub fn key(&self) -> String {
        self.id.clone().unwrap_or_else(|| slug(&self.title))
    }
}

/// Lowercase, alphanumeric-and-dashes identifier derived from a title.
pub fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Expand a leading `~/` to the home directory.
pub fn expand_tilde(p: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => dirs::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| p.to_path_buf()),
        Err(_) => p.to_path_buf(),
    }
}

impl Config {
    /// `$OPENCAST_CONFIG_DIR`, else `~/.config/opencast` on every platform
    /// (dotfile-friendly, unlike `~/Library/Application Support`).
    pub fn config_dir() -> PathBuf {
        if let Ok(d) = std::env::var("OPENCAST_CONFIG_DIR") {
            if !d.is_empty() {
                return expand_tilde(Path::new(&d));
            }
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("opencast")
    }

    pub fn parse(src: &str) -> Result<Self> {
        let cfg: Config = toml::from_str(src).context("parsing config.toml")?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let src = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&src).with_context(|| format!("in {}", path.display()))
    }

    /// Load `<dir>/config.toml`, creating it and the example scripts first if
    /// the directory has never been set up.
    pub fn load_or_init(dir: &Path) -> Result<Self> {
        let path = dir.join("config.toml");
        if !path.exists() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            std::fs::write(&path, DEFAULT_CONFIG)?;
            let scripts = dir.join("scripts");
            if !scripts.exists() {
                std::fs::create_dir_all(&scripts)?;
                for (name, body) in EXAMPLE_SCRIPTS {
                    let p = scripts.join(name);
                    std::fs::write(&p, body)?;
                    make_executable(&p)?;
                }
            }
        }
        Self::load_from(&path)
    }

    pub fn validate(&self) -> Result<()> {
        let mut ids = HashSet::new();
        for p in &self.providers {
            if p.id.trim().is_empty() {
                bail!("a provider has an empty id");
            }
            if !ids.insert(p.id.as_str()) {
                bail!("provider id `{}` is used twice", p.id);
            }
            if p.model.trim().is_empty() {
                bail!("provider `{}` needs a `model`", p.id);
            }
        }
        if let Some(d) = &self.general.default_provider {
            if !ids.contains(d.as_str()) {
                bail!("default_provider `{d}` is not one of the [[providers]]");
            }
        }
        let mut cmd_ids = HashSet::new();
        for c in &self.ai_commands {
            if !cmd_ids.insert(c.id.as_str()) {
                bail!("ai_command id `{}` is used twice", c.id);
            }
            if let Some(p) = &c.provider {
                if !ids.contains(p.as_str()) {
                    bail!("ai_command `{}` uses unknown provider `{p}`", c.id);
                }
            }
        }
        for h in &self.hotkeys {
            if h.key.trim().is_empty() || h.command.trim().is_empty() {
                bail!("every [[hotkeys]] entry needs both `key` and `command`");
            }
        }
        Ok(())
    }

    pub fn scripts_dir(&self, config_dir: &Path) -> PathBuf {
        match &self.general.scripts_dir {
            Some(p) => expand_tilde(p),
            None => config_dir.join("scripts"),
        }
    }

    pub fn default_provider_id(&self) -> Option<&str> {
        self.general
            .default_provider
            .as_deref()
            .or_else(|| self.providers.first().map(|p| p.id.as_str()))
    }
}

#[cfg(unix)]
fn make_executable(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(p)?.permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(p, perm)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_parses() {
        let cfg = Config::parse(DEFAULT_CONFIG).unwrap();
        assert_eq!(cfg.general.hotkey, "Alt+Space");
        assert_eq!(cfg.default_provider_id(), Some("ollama"));
        assert!(cfg.providers[0].is_local());
        assert!(cfg.ai_commands.iter().any(|c| c.id == "fix-grammar"));
        assert_eq!(cfg.quicklinks[0].key(), "search-github");
    }

    #[test]
    fn empty_config_is_valid() {
        let cfg = Config::parse("").unwrap();
        assert!(cfg.providers.is_empty());
        assert_eq!(cfg.default_provider_id(), None);
    }

    #[test]
    fn rejects_unknown_default_provider() {
        let err = Config::parse("[general]\ndefault_provider = \"nope\"").unwrap_err();
        assert!(format!("{err:#}").contains("nope"));
    }

    #[test]
    fn rejects_duplicate_provider_ids() {
        let src = r#"
            [[providers]]
            id = "a"
            kind = "openai"
            model = "m"
            [[providers]]
            id = "a"
            kind = "gemini"
            model = "m"
        "#;
        assert!(Config::parse(src).is_err());
    }

    #[test]
    fn api_key_from_cmd_and_models_dedupe() {
        let p = ProviderConfig {
            id: "x".into(),
            kind: ProviderKind::Openai,
            base_url: None,
            api_key_env: Some("OPENCAST_TEST_SURELY_UNSET_VAR".into()),
            api_key_cmd: Some("printf ' sk-test \\n'".into()),
            model: "a".into(),
            models: vec!["b".into(), "a".into()],
            headers: Default::default(),
            max_tokens: None,
            temperature: None,
        };
        assert_eq!(p.resolve_api_key().unwrap().as_deref(), Some("sk-test"));
        assert_eq!(p.all_models(), vec!["a", "b"]);
    }

    #[test]
    fn init_writes_config_and_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load_or_init(dir.path()).unwrap();
        assert!(dir.path().join("config.toml").exists());
        assert!(dir.path().join("scripts/uuid.sh").exists());
        assert_eq!(cfg.scripts_dir(dir.path()), dir.path().join("scripts"));
    }

    #[test]
    fn set_general_key_inserts_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            "[general]\nhotkey = \"Alt+Space\"\n\n[[providers]]\nid = \"x\"\n",
        )
        .unwrap();
        set_general_key(dir.path(), "theme", "dark").unwrap();
        let src = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
        assert!(src.contains("theme = \"dark\""));
        set_general_key(dir.path(), "theme", "light").unwrap();
        let src = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
        assert!(src.contains("theme = \"light\""));
        assert!(!src.contains("theme = \"dark\""));
        assert!(src.contains("hotkey = \"Alt+Space\""));
    }
}
