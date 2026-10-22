//! In-panel key bindings (`[keys]` in the config), parsed into a shape the
//! panel can match against a key event without re-parsing strings.

use serde::Serialize;

use crate::config::Keys;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBinding {
    /// `ask_ai`, `actions`, `pin`, ...
    pub name: String,
    /// Lowercase: `cmd`, `shift`, `alt`, `ctrl` (sorted, deduplicated).
    pub mods: Vec<String>,
    /// Lowercase single character (`k`, `` ` ``), or a named key:
    /// `tab`, `return`, `escape`, `space`, `up`, `down`, `left`, `right`,
    /// `backspace`, `delete`, `f1`..`f20`.
    pub key: String,
}

const NAMED: &[&str] = &[
    "tab",
    "return",
    "enter",
    "escape",
    "esc",
    "space",
    "up",
    "down",
    "left",
    "right",
    "backspace",
    "delete",
];

/// Canonical form of a global hotkey spec for comparing bindings:
/// `Hyper+V` and `shift+cmd+ctrl+alt+v` both become `alt+cmd+ctrl+shift+v`.
pub fn normalize_hotkey(spec: &str) -> Option<String> {
    let mut mods: Vec<&str> = Vec::new();
    let mut key: Option<String> = None;
    for raw in spec.split('+').map(str::trim) {
        let l = raw.to_lowercase();
        match l.as_str() {
            "hyper" => mods.extend(["alt", "cmd", "ctrl", "shift"]),
            "cmd" | "command" | "super" | "meta" => mods.push("cmd"),
            "shift" => mods.push("shift"),
            "alt" | "option" | "opt" => mods.push("alt"),
            "ctrl" | "control" => mods.push("ctrl"),
            "" => return None,
            _ if key.is_none() => {
                key = Some(match l.as_str() {
                    "enter" => "return".into(),
                    "esc" => "escape".into(),
                    other => other.to_string(),
                })
            }
            _ => return None,
        }
    }
    mods.sort();
    mods.dedup();
    let key = key?;
    Some(if mods.is_empty() {
        key
    } else {
        format!("{}+{key}", mods.join("+"))
    })
}

/// Parse `Cmd+Shift+P` style specs. Returns None (with a reason) when invalid.
pub fn parse(name: &str, spec: &str) -> Result<KeyBinding, String> {
    let parts: Vec<&str> = spec.split('+').map(str::trim).collect();
    if parts.is_empty() || spec.trim().is_empty() {
        return Err(format!("key `{name}` is empty"));
    }
    let mut mods: Vec<String> = Vec::new();
    let mut key: Option<String> = None;
    // `Cmd++` means Cmd and the plus key; handle an empty trailing part.
    let last_is_plus = spec.trim().ends_with("++") && parts.len() >= 3;
    let count = if last_is_plus { parts.len() - 2 } else { parts.len() };
    for (i, raw) in parts.iter().take(count).enumerate() {
        let lower = raw.to_lowercase();
        let is_last = i + 1 == count;
        let m = match lower.as_str() {
            "cmd" | "command" | "super" | "meta" => Some("cmd"),
            "shift" => Some("shift"),
            "alt" | "option" | "opt" => Some("alt"),
            "ctrl" | "control" => Some("ctrl"),
            _ => None,
        };
        match m {
            Some(m) if !is_last || last_is_plus => {
                if !mods.iter().any(|x| x == m) {
                    mods.push(m.to_string());
                }
            }
            Some(_) => return Err(format!("key `{name}`: `{spec}` has modifiers but no key")),
            None => {
                if !is_last {
                    return Err(format!("key `{name}`: `{raw}` is not a modifier"));
                }
                let k = match lower.as_str() {
                    "enter" => "return".to_string(),
                    "esc" => "escape".to_string(),
                    k if NAMED.contains(&k) => k.to_string(),
                    k if k.chars().count() == 1 => k.to_string(),
                    k if k
                        .strip_prefix('f')
                        .and_then(|n| n.parse::<u8>().ok())
                        .is_some_and(|n| (1..=20).contains(&n)) =>
                    {
                        k.to_string()
                    }
                    _ => return Err(format!("key `{name}`: `{raw}` is not a key")),
                };
                key = Some(k);
            }
        }
    }
    if last_is_plus {
        key = Some("+".into());
    }
    let key = key.ok_or_else(|| format!("key `{name}`: `{spec}` names no key"))?;
    mods.sort();
    Ok(KeyBinding {
        name: name.to_string(),
        mods,
        key,
    })
}

/// Every binding in `keys`; invalid specs fall back to the default and are reported.
pub fn bindings(keys: &Keys) -> (Vec<KeyBinding>, Vec<String>) {
    let defaults = Keys::default();
    let default_for: std::collections::HashMap<&str, &str> = defaults.entries().into_iter().collect();
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for (name, spec) in keys.entries() {
        match parse(name, spec) {
            Ok(b) => out.push(b),
            Err(e) => {
                warnings.push(format!("{e}; using the default"));
                if let Some(b) = default_for.get(name).and_then(|d| parse(name, d).ok()) {
                    out.push(b);
                }
            }
        }
    }
    (out, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_specs() {
        let b = parse("pin", "Cmd+Shift+P").unwrap();
        assert_eq!(b.mods, vec!["cmd", "shift"]);
        assert_eq!(b.key, "p");
        assert_eq!(parse("x", "Tab").unwrap().key, "tab");
        assert_eq!(parse("x", "`").unwrap().key, "`");
        assert_eq!(
            parse("x", "Option+Enter").unwrap(),
            KeyBinding {
                name: "x".into(),
                mods: vec!["alt".into()],
                key: "return".into()
            }
        );
        assert_eq!(parse("x", "Cmd++").unwrap().key, "+");
        assert_eq!(parse("x", "ctrl+f5").unwrap().key, "f5");
        assert!(parse("x", "Cmd+").is_err());
        assert!(parse("x", "Cmd+Shift").is_err());
        assert!(parse("x", "Bogus+K").is_err());
        assert!(parse("x", "").is_err());
    }

    #[test]
    fn invalid_falls_back_to_default() {
        let keys = Keys {
            actions: "Nope+".into(),
            ..Keys::default()
        };
        let (b, w) = bindings(&keys);
        assert_eq!(w.len(), 1);
        let actions = b.iter().find(|x| x.name == "actions").unwrap();
        assert_eq!(actions.key, "k");
        assert_eq!(actions.mods, vec!["cmd"]);
        assert_eq!(b.len(), Keys::default().entries().len());
    }
}

#[cfg(test)]
mod hotkey_tests {
    #[test]
    fn normalizes() {
        use super::normalize_hotkey as n;
        assert_eq!(n("Hyper+V"), n("shift+cmd+ctrl+alt+v"));
        assert_eq!(n("Cmd+Shift+K").as_deref(), Some("cmd+shift+k"));
        assert_eq!(n("Ctrl+Alt+Enter").as_deref(), Some("alt+ctrl+return"));
        assert_eq!(n("F18").as_deref(), Some("f18"));
        assert!(n("Cmd+").is_none());
        assert!(n("Cmd+A+B").is_none());
    }
}
