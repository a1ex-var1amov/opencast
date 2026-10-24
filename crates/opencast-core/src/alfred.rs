//! Alfred workflow import.
//!
//! An `.alfredworkflow` is a zip of a folder with an `info.plist` that wires
//! *objects* (inputs, actions, outputs) with *connections*. We turn each
//! Keyword and Script Filter input into an OpenCast script command
//! (`opencast-commands/*.sh` inside the workflow folder). The generated
//! script runs Alfred's chain of connected objects in `bash`, so the
//! workflow's own scripts run unchanged, with Alfred's environment
//! (`alfred_workflow_data`, `alfred_workflow_cache`, workflow variables…).
//!
//! Supported objects: Script Filter, Keyword, Hotkey (becomes the command's
//! global hotkey; argument: none, selection, clipboard or text), Run Script,
//! Open URL, Open File, Reveal in Finder, Launch Apps, Copy to Clipboard (with
//! auto-paste), Post Notification, Large Type, Arg and Vars, Junction,
//! Conditional, Transform (trim, lower/upper case). Anything else stops the
//! chain with a message. A workflow with a user configuration also gets a
//! "Configure <name>" command; values live next to Alfred's workflow data.
//! `.alfredsnippets` collections are read by [`snippets_in`].
//! Plist parsing happens in the shell (`plutil`); this module takes JSON.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::Value;

/// Folder (inside the workflow) holding the generated commands.
pub const OUT_DIR: &str = "opencast-commands";

/// Alfred's modifier bitmask → OpenCast's `OPENCAST_MOD` value.
const MODS: &[(u64, &str)] = &[
    (1_048_576, "cmd"),
    (524_288, "alt"),
    (262_144, "ctrl"),
    (131_072, "shift"),
    (8_388_608, "fn"),
];

fn mod_name(mask: u64) -> String {
    MODS.iter()
        .filter(|(bit, _)| mask & bit != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join("+")
}

/// Shell-quote for bash single quotes.
fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r#"'\''"#))
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn int_of(v: &Value, key: &str) -> Option<i64> {
    match v.get(key)? {
        Value::Number(n) => n.as_i64(),
        Value::Bool(b) => Some(*b as i64),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// Interpreter command line for Alfred's script `type`, and a file extension.
fn interpreter(kind: i64) -> (&'static str, &'static str) {
    match kind {
        1 => ("/usr/bin/php", "php"),
        2 => ("/usr/bin/ruby", "rb"),
        3 | 9 => ("/usr/bin/env python3", "py"),
        4 => ("/usr/bin/perl", "pl"),
        5 | 10 | 11 => ("/bin/zsh --no-rcs", "zsh"),
        6 => ("/usr/bin/osascript", "applescript"),
        7 => ("/usr/bin/osascript -l JavaScript", "js"),
        // 0 and anything new: bash.
        _ => ("/bin/bash", "sh"),
    }
}

fn is_shell(kind: i64) -> bool {
    matches!(kind, 0 | 5 | 10 | 11)
}

#[derive(Debug)]
struct Workflow<'a> {
    dir: &'a Path,
    name: String,
    bundle: String,
    objects: BTreeMap<String, &'a Value>,
    /// uid → [(destination uid, modifier mask)]
    connections: BTreeMap<String, Vec<(String, u64)>>,
    /// Conditional uid → [(destination uid, condition uid; None = else)].
    branches: BTreeMap<String, Vec<(String, Option<String>)>>,
    variables: BTreeMap<String, String>,
}

impl<'a> Workflow<'a> {
    fn parse(dir: &'a Path, plist: &'a Value) -> Result<Self> {
        let objects: BTreeMap<String, &Value> = plist
            .get("objects")
            .and_then(Value::as_array)
            .context("info.plist has no objects")?
            .iter()
            .filter_map(|o| Some((o.get("uid")?.as_str()?.to_string(), o)))
            .collect();
        let mut connections = BTreeMap::new();
        let mut branches = BTreeMap::new();
        if let Some(c) = plist.get("connections").and_then(Value::as_object) {
            for (from, list) in c {
                let b: Vec<(String, Option<String>)> = list
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter(|d| !d.get("disabled").and_then(Value::as_bool).unwrap_or(false))
                            .filter_map(|d| {
                                Some((
                                    d.get("destinationuid")?.as_str()?.to_string(),
                                    d.get("sourceoutputuid").and_then(Value::as_str).map(str::to_string),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                branches.insert(from.clone(), b);
                let dests = list
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter(|d| !d.get("disabled").and_then(Value::as_bool).unwrap_or(false))
                            .filter_map(|d| {
                                Some((
                                    d.get("destinationuid")?.as_str()?.to_string(),
                                    d.get("modifiers").and_then(Value::as_u64).unwrap_or(0),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                connections.insert(from.clone(), dests);
            }
        }
        // Workflow variables, then user-configuration defaults.
        let mut variables = BTreeMap::new();
        if let Some(v) = plist.get("variables").and_then(Value::as_object) {
            for (k, v) in v {
                variables.insert(
                    k.clone(),
                    v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()),
                );
            }
        }
        if let Some(cfg) = plist.get("userconfigurationconfig").and_then(Value::as_array) {
            for c in cfg {
                let Some(var) = c.get("variable").and_then(Value::as_str) else {
                    continue;
                };
                let default = c.get("config").and_then(|c| c.get("default"));
                let value = match default {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Bool(b)) => (if *b { "1" } else { "0" }).into(),
                    Some(Value::Number(n)) => n.to_string(),
                    _ => continue,
                };
                variables.entry(var.to_string()).or_insert(value);
            }
        }
        let name = str_of(plist, "name").trim().to_string();
        let bundle = str_of(plist, "bundleid").trim().to_string();
        Ok(Self {
            dir,
            name: if name.is_empty() {
                "Alfred Workflow".into()
            } else {
                name
            },
            bundle: if bundle.is_empty() {
                format!(
                    "opencast.imported.{}",
                    slug(dir.file_name().and_then(|n| n.to_str()).unwrap_or("workflow"))
                )
            } else {
                bundle
            },
            objects,
            connections,
            branches,
            variables,
        })
    }

    fn kind(&self, uid: &str) -> &str {
        self.objects.get(uid).map(|o| str_of(o, "type")).unwrap_or("")
    }

    fn config(&self, uid: &str) -> &Value {
        static EMPTY: Value = Value::Null;
        self.objects.get(uid).and_then(|o| o.get("config")).unwrap_or(&EMPTY)
    }
}

pub fn slug(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "workflow".into()
    } else {
        out
    }
}

/// Shared prelude: Alfred environment, helpers.
fn prelude(wf: &Workflow) -> String {
    let home_data = format!("$HOME/Library/Application Support/Alfred/Workflow Data/{}", wf.bundle);
    let home_cache = format!(
        "$HOME/Library/Caches/com.runningwithcrayons.Alfred/Workflow Data/{}",
        wf.bundle
    );
    let mut s = String::new();
    let _ = writeln!(s, "WF={}", sq(&wf.dir.to_string_lossy()));
    s.push_str("cd \"$WF\" || exit 1\n");
    let _ = writeln!(s, "export alfred_workflow_bundleid={}", sq(&wf.bundle));
    let _ = writeln!(s, "export alfred_workflow_name={}", sq(&wf.name));
    let _ = writeln!(s, "export alfred_workflow_data=\"{home_data}\"");
    let _ = writeln!(s, "export alfred_workflow_cache=\"{home_cache}\"");
    s.push_str("export alfred_preferences=\"$HOME/Library/Application Support/Alfred/Alfred.alfredpreferences\"\n");
    s.push_str("export alfred_version=\"5.5\" alfred_version_build=\"2257\" alfred_debug=\"0\"\n");
    s.push_str("export alfred_theme_background=\"rgba(255,255,255,0.98)\" alfred_theme_subtext=\"0\"\n");
    s.push_str("mkdir -p \"$alfred_workflow_data\" \"$alfred_workflow_cache\" 2>/dev/null\n");
    // Values set with "Configure <workflow>" (kept across re-imports).
    s.push_str("_OC_USERCONF=\"$alfred_workflow_data/opencast-userconfig.sh\"\n");
    s.push_str("[ -f \"$_OC_USERCONF\" ] && . \"$_OC_USERCONF\"\n");
    for (k, v) in &wf.variables {
        if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.is_empty() {
            // Keep values the user (or a list row) already set.
            let _ = writeln!(
                s,
                ": \"${{{k}:={}}}\"; export {k}",
                v.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('$', "\\$")
                    .replace('`', "\\`")
            );
        }
    }
    s.push_str(HELPERS);
    s
}

const HELPERS: &str = r#"
# {query} substitution without letting the query run as code.
_oc_sub() { # file escaped-query
  _OC_REPL="$2" awk '{ s=$0; out=""; while ((i=index(s,"{query}"))>0) { out=out substr(s,1,i-1) ENVIRON["_OC_REPL"]; s=substr(s,i+7) } print out s }' "$1"
}
_oc_fill() { # template query  ({query} and {var:name})
  _OC_TEXT="$1" _OC_REPL="$2" awk 'BEGIN { s=ENVIRON["_OC_TEXT"]; out=""; while ((i=index(s,"{query}"))>0) { out=out substr(s,1,i-1) ENVIRON["_OC_REPL"]; s=substr(s,i+7) } s=out s; out=""; while ((i=index(s,"{var:"))>0) { rest=substr(s,i+5); j=index(rest,"}"); if (j==0) break; out=out substr(s,1,i-1) ENVIRON[substr(rest,1,j-1)]; s=substr(rest,j+1) } printf "%s", out s }'
}
# Alfred Conditional: input mode match case-sensitive(1/0)
_oc_cond() {
  local a="$1" b="$3"
  if [ "$2" = 4 ]; then
    if [ "$4" = 1 ]; then printf '%s' "$a" | grep -Eq -- "$b"; else printf '%s' "$a" | grep -Eiq -- "$b"; fi
    return
  fi
  if [ "$4" != 1 ]; then
    a=$(printf '%s' "$a" | tr '[:upper:]' '[:lower:]'); b=$(printf '%s' "$b" | tr '[:upper:]' '[:lower:]')
  fi
  case "$2" in
    0) [ "$a" = "$b" ] ;;
    1) [ "$a" != "$b" ] ;;
    2) awk -v a="$a" -v b="$b" 'BEGIN { exit !((a ~ /^-?[0-9.]+$/ && b ~ /^-?[0-9.]+$/) ? a+0 > b+0 : a > b) }' ;;
    3) awk -v a="$a" -v b="$b" 'BEGIN { exit !((a ~ /^-?[0-9.]+$/ && b ~ /^-?[0-9.]+$/) ? a+0 < b+0 : a < b) }' ;;
    5) [[ "$a" == *"$b"* ]] ;;
    6) [[ "$a" == "$b"* ]] ;;
    7) [[ "$a" == *"$b" ]] ;;
    *) false ;;
  esac
}
# Selected text in the front app (Alfred hotkey "Selection in macOS").
_oc_selection() {
  local before; before=$(/usr/bin/pbpaste 2>/dev/null)
  /usr/bin/osascript -e 'tell application "System Events" to keystroke "c" using command down' >/dev/null 2>&1
  sleep 0.25
  local now; now=$(/usr/bin/pbpaste 2>/dev/null)
  [ "$now" != "$before" ] && printf '%s' "$now"
  printf '%s' "$before" | /usr/bin/pbcopy
}
_oc_esc_shell() { printf '%s' "$1" | sed -e 's/[\\`$"]/\\&/g'; }
_oc_esc_quotes() { printf '%s' "$1" | sed -e 's/[\\"]/\\&/g'; }
_oc_urlencode() {
  local LC_ALL=C s="$1" out="" c i
  for (( i=0; i<${#s}; i++ )); do
    c="${s:i:1}"
    case "$c" in [a-zA-Z0-9.~_-]) out+="$c" ;; *) out+=$(printf '%%%02X' "'$c") ;; esac
  done
  printf '%s' "$out"
}
# A script may answer with {"alfredworkflow": {"arg": ...}}: pass the arg on.
_oc_arg() {
  case "$1" in
    *'"alfredworkflow"'*) printf '%s' "$1" | /usr/bin/plutil -extract alfredworkflow.arg raw -o - - 2>/dev/null || printf '%s' "$1" ;;
    *) printf '%s' "$1" ;;
  esac
}
"#;

/// Bash expression: `template` with `{query}` replaced by `query_expr`'s value.
fn subst_expr(template: &str, query_expr: &str) -> String {
    format!("\"$(_oc_fill {} {query_expr})\"", sq(template))
}

/// Write the object's script to `steps/<uid>.<ext>` and return the bash that runs it with `$q`.
fn script_runner(wf: &Workflow, uid: &str, cfg: &Value, steps: &Path) -> Result<String> {
    let kind = int_of(cfg, "type").unwrap_or(0);
    // External script file (type 8): run it from the workflow folder with argv.
    if kind == 8 {
        let file = str_of(cfg, "scriptfile");
        if file.is_empty() {
            bail!("Run Script `{uid}` has no file");
        }
        return Ok(format!("\"./\"{} \"$q\"", sq(file)));
    }
    let script = str_of(cfg, "script");
    let (interp, ext) = interpreter(kind);
    let path = steps.join(format!("{uid}.{ext}"));
    std::fs::write(&path, script).with_context(|| format!("writing {}", path.display()))?;
    let path_q = sq(&path.to_string_lossy());
    // scriptargtype 1 = argv ($1); 0 (and old workflows) = inline {query}.
    let argv = int_of(cfg, "scriptargtype").unwrap_or(if script.contains("{query}") { 0 } else { 1 }) == 1;
    let _ = wf;
    Ok(if argv {
        format!("{interp} {path_q} \"$q\"")
    } else {
        let esc = if is_shell(kind) {
            "_oc_esc_shell"
        } else {
            "_oc_esc_quotes"
        };
        format!(
            "_oc_tmp=$(mktemp \"${{TMPDIR:-/tmp}}/ocwf.XXXXXX\"); _oc_sub {path_q} \"$({esc} \"$q\")\" > \"$_oc_tmp\"; {interp} \"$_oc_tmp\"; _oc_rc=$?; rm -f \"$_oc_tmp\"; (exit $_oc_rc)"
        )
    })
}

/// Bash function `n_<uid>` for one object: reads `$1`, acts, passes output on.
fn node_fn(wf: &Workflow, uid: &str, steps: &Path, seen: &mut Vec<String>, out: &mut String) -> Result<()> {
    if seen.iter().any(|s| s == uid) {
        return Ok(());
    }
    seen.push(uid.to_string());
    let cfg = wf.config(uid);
    let kind = wf.kind(uid);
    let dests = wf.connections.get(uid).cloned().unwrap_or_default();
    for (d, _) in &dests {
        node_fn(wf, d, steps, seen, out)?;
    }
    if kind == "alfred.workflow.utility.conditional" {
        out.push_str(&conditional_fn(wf, uid, cfg));
        return Ok(());
    }
    let body = match kind {
        "alfred.workflow.action.script" => {
            format!("res=$(_oc_arg \"$({})\")", script_runner(wf, uid, cfg, steps)?)
        }
        "alfred.workflow.action.openurl" => {
            let url = str_of(cfg, "url").trim();
            if url.is_empty() || url == "{query}" {
                // The query is the URL itself: do not encode it.
                "/usr/bin/open \"$q\"; res=\"\"".to_string()
            } else {
                format!(
                    "u={}; /usr/bin/open \"$u\"; res=\"\"",
                    subst_expr(url, "\"$(_oc_urlencode \"$q\")\"")
                )
            }
        }
        "alfred.workflow.action.openfile" => {
            let file = str_of(cfg, "sourcefile");
            if file.is_empty() {
                "/usr/bin/open \"$q\"; res=\"\"".into()
            } else {
                format!("/usr/bin/open {} ; res=\"\"", sq(file))
            }
        }
        "alfred.workflow.action.revealfile" => "/usr/bin/open -R \"$q\"; res=\"\"".into(),
        "alfred.workflow.action.launchapps" => {
            let apps: Vec<String> = cfg
                .get("applications")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(sq)).collect())
                .unwrap_or_default();
            if apps.is_empty() {
                "res=\"\"".into()
            } else {
                format!(
                    "for a in {}; do /usr/bin/open -a \"$a\"; done; res=\"\"",
                    apps.join(" ")
                )
            }
        }
        "alfred.workflow.output.clipboard" => {
            let text = str_of(cfg, "clipboardtext");
            let text = if text.is_empty() { "{query}" } else { text };
            let paste = cfg.get("autopaste").and_then(Value::as_bool).unwrap_or(false);
            let mut b = format!(
                "c={}; printf '%s' \"$c\" | /usr/bin/pbcopy; res=\"Copied\"",
                subst_expr(text, "\"$q\"")
            );
            if paste {
                b.push_str("; sleep 0.25; /usr/bin/osascript -e 'tell application \"System Events\" to keystroke \"v\" using command down'; res=\"\"");
            }
            b
        }
        "alfred.workflow.output.notification" | "alfred.workflow.output.largetype" => {
            let text = if kind.ends_with("largetype") {
                str_of(cfg, "largetypetext")
            } else {
                str_of(cfg, "text")
            };
            let text = if text.is_empty() { "{query}" } else { text };
            format!("res={}", subst_expr(text, "\"$q\""))
        }
        "alfred.workflow.utility.argument" => {
            let mut b = String::new();
            if let Some(vars) = cfg.get("variables").and_then(Value::as_object) {
                for (k, v) in vars {
                    if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        let _ = write!(b, "export {k}={}; ", subst_expr(v.as_str().unwrap_or(""), "\"$q\""));
                    }
                }
            }
            let arg = str_of(cfg, "argument");
            if arg.is_empty() {
                b.push_str("res=\"$q\"");
            } else {
                let _ = write!(b, "res={}", subst_expr(arg, "\"$q\""));
            }
            b
        }
        "alfred.workflow.utility.junction" | "alfred.workflow.utility.debug" => "res=\"$q\"".into(),
        "alfred.workflow.utility.transform" => match int_of(cfg, "type").unwrap_or(0) {
            1 => "res=$(printf '%s' \"$q\" | tr '[:upper:]' '[:lower:]')".into(),
            2 => "res=$(printf '%s' \"$q\" | tr '[:lower:]' '[:upper:]')".into(),
            _ => "res=$(printf '%s' \"$q\" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')".into(),
        },
        other => format!(
            "echo {} >&2; exit 1",
            sq(&format!(
                "This workflow uses an Alfred step OpenCast cannot run yet ({})",
                other.trim_start_matches("alfred.workflow.")
            ))
        ),
    };
    let _ = writeln!(out, "n_{uid}() {{\n  local q=\"$1\" res\n  {body}");
    if dests.is_empty() {
        out.push_str("  [ -n \"$res\" ] && printf '%s\\n' \"$res\"\n");
    } else {
        out.push_str(&dispatch(&dests, "\"$res\""));
    }
    out.push_str("}\n");
    Ok(())
}

/// Bash function for a Conditional: the first matching condition's branch
/// gets the query unchanged; otherwise the else branch.
fn conditional_fn(wf: &Workflow, uid: &str, cfg: &Value) -> String {
    let branches = wf.branches.get(uid).cloned().unwrap_or_default();
    let to = |cond: Option<&str>| -> String {
        let calls: Vec<String> = branches
            .iter()
            .filter(|(_, c)| c.as_deref() == cond)
            .map(|(d, _)| format!("n_{d} \"$q\";"))
            .collect();
        if calls.is_empty() {
            ":".into()
        } else {
            calls.join(" ")
        }
    };
    let mut s = format!("n_{uid}() {{\n  local q=\"$1\"\n");
    let conds = cfg
        .get("conditions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut first = true;
    for c in &conds {
        let input = str_of(c, "inputstring");
        let input = if input.is_empty() { "{query}" } else { input };
        let mode = int_of(c, "matchmode").unwrap_or(0);
        let cs = c.get("matchcasesensitive").and_then(Value::as_bool).unwrap_or(false) as u8;
        let cond_uid = str_of(c, "uid");
        let _ = writeln!(
            s,
            "  {} _oc_cond {} {mode} {} {cs}; then {}",
            if first { "if" } else { "elif" },
            subst_expr(input, "\"$q\""),
            subst_expr(str_of(c, "matchstring"), "\"$q\""),
            to(Some(cond_uid))
        );
        first = false;
    }
    if first {
        let _ = writeln!(s, "  {}", to(None));
    } else {
        let _ = writeln!(s, "  else {}\n  fi", to(None));
    }
    s.push_str("}\n");
    s
}

/// Alfred's virtual key code → OpenCast key name (ANSI layout).
fn key_name(code: i64) -> Option<&'static str> {
    const KEYS: &[(i64, &str)] = &[
        (0, "A"),
        (1, "S"),
        (2, "D"),
        (3, "F"),
        (4, "H"),
        (5, "G"),
        (6, "Z"),
        (7, "X"),
        (8, "C"),
        (9, "V"),
        (11, "B"),
        (12, "Q"),
        (13, "W"),
        (14, "E"),
        (15, "R"),
        (16, "Y"),
        (17, "T"),
        (18, "1"),
        (19, "2"),
        (20, "3"),
        (21, "4"),
        (22, "6"),
        (23, "5"),
        (24, "="),
        (25, "9"),
        (26, "7"),
        (27, "-"),
        (28, "8"),
        (29, "0"),
        (30, "]"),
        (31, "O"),
        (32, "U"),
        (33, "["),
        (34, "I"),
        (35, "P"),
        (36, "Return"),
        (37, "L"),
        (38, "J"),
        (39, "'"),
        (40, "K"),
        (41, ";"),
        (42, "\\"),
        (43, ","),
        (44, "/"),
        (45, "N"),
        (46, "M"),
        (47, "."),
        (48, "Tab"),
        (49, "Space"),
        (50, "`"),
        (51, "Backspace"),
        (53, "Escape"),
        (64, "F17"),
        (79, "F18"),
        (80, "F19"),
        (90, "F20"),
        (96, "F5"),
        (97, "F6"),
        (98, "F7"),
        (99, "F3"),
        (100, "F8"),
        (101, "F9"),
        (103, "F11"),
        (105, "F13"),
        (106, "F16"),
        (107, "F14"),
        (109, "F10"),
        (111, "F12"),
        (113, "F15"),
        (117, "Delete"),
        (118, "F4"),
        (120, "F2"),
        (122, "F1"),
        (123, "Left"),
        (124, "Right"),
        (125, "Down"),
        (126, "Up"),
    ];
    KEYS.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

/// An Alfred hotkey (`hotkey` key code, `hotmod` mask) as `Ctrl+Alt+K`, or
/// None when it is unset (Alfred workflows usually ship hotkeys blank).
pub fn hotkey_string(cfg: &Value) -> Option<String> {
    let mods = int_of(cfg, "hotmod").unwrap_or(0) as u64;
    if mods == 0 {
        return None;
    }
    let key = key_name(int_of(cfg, "hotkey")?)?;
    let mut parts: Vec<&str> = Vec::new();
    for (bit, name) in [
        (262_144u64, "Ctrl"),
        (524_288, "Alt"),
        (131_072, "Shift"),
        (1_048_576, "Cmd"),
        (8_388_608, "Fn"), // Alfred relatedApps / fn-key bit
    ] {
        if mods & bit != 0 {
            parts.push(name);
        }
    }
    parts.push(key);
    Some(parts.join("+"))
}

/// Call the connected objects: modifier connections only when that modifier was used.
fn dispatch(dests: &[(String, u64)], arg: &str) -> String {
    let mut s = String::new();
    let plain: Vec<&String> = dests.iter().filter(|(_, m)| *m == 0).map(|(d, _)| d).collect();
    let modded: Vec<(&String, String)> = dests
        .iter()
        .filter(|(_, m)| *m != 0)
        .map(|(d, m)| (d, mod_name(*m)))
        .collect();
    if modded.is_empty() {
        for d in plain {
            let _ = writeln!(s, "  n_{d} {arg}");
        }
        return s;
    }
    s.push_str("  case \"$OPENCAST_MOD\" in\n");
    for (d, m) in &modded {
        let _ = writeln!(s, "    {m}) n_{d} {arg} ;;");
    }
    s.push_str("    *)");
    for d in &plain {
        let _ = write!(s, " n_{d} {arg};");
    }
    s.push_str(" : ;;\n  esac\n");
    s
}

fn header(title: &str, mode: &str, icon: Option<&Path>, wf: &Workflow, keyword: &str, description: &str) -> String {
    header_with(title, mode, icon, wf, keyword, description, None)
}

fn header_with(
    title: &str,
    mode: &str,
    icon: Option<&Path>,
    wf: &Workflow,
    keyword: &str,
    description: &str,
    hotkey: Option<&str>,
) -> String {
    let mut h = String::from("#!/bin/bash\n# Generated by OpenCast from an Alfred workflow — re-import to refresh.\n");
    let _ = writeln!(h, "# @opencast.title {title}");
    let _ = writeln!(h, "# @opencast.mode {mode}");
    let _ = writeln!(h, "# @opencast.packageName {}", wf.name);
    if let Some(i) = icon {
        let _ = writeln!(h, "# @opencast.icon {}", i.display());
    }
    if !keyword.is_empty() {
        let _ = writeln!(h, "# @opencast.keywords {keyword}");
    }
    if !description.is_empty() {
        let _ = writeln!(h, "# @opencast.description {}", description.replace('\n', " "));
    }
    if let Some(k) = hotkey {
        let _ = writeln!(h, "# @opencast.hotkey {k}");
    }
    h
}

fn icon_for(wf: &Workflow, uid: &str) -> Option<PathBuf> {
    [format!("{uid}.png"), "icon.png".into()]
        .into_iter()
        .map(|f| wf.dir.join(f))
        .find(|p| p.is_file())
}

/// Generate commands for the workflow in `dir` (its `info.plist` as JSON).
/// Returns the generated script paths.
pub fn generate(dir: &Path, plist: &Value) -> Result<Vec<PathBuf>> {
    let wf = Workflow::parse(dir, plist)?;
    let out_dir = dir.join(OUT_DIR);
    let _ = std::fs::remove_dir_all(&out_dir);
    let steps = out_dir.join("steps");
    std::fs::create_dir_all(&steps).with_context(|| format!("creating {}", steps.display()))?;
    let mut written = Vec::new();
    for (uid, obj) in &wf.objects {
        if obj.get("disabled").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let kind = str_of(obj, "type");
        let cfg = wf.config(uid);
        let keyword = str_of(cfg, "keyword").trim();
        let is_filter = kind == "alfred.workflow.input.scriptfilter";
        let is_keyword = kind == "alfred.workflow.input.keyword";
        if kind == "alfred.workflow.trigger.hotkey" {
            written.push(hotkey_command(&wf, uid, cfg, &steps, &out_dir)?);
            continue;
        }
        if !is_filter && !is_keyword {
            continue;
        }
        let title = [str_of(cfg, "title"), str_of(cfg, "text"), keyword]
            .into_iter()
            .map(str::trim)
            .find(|t| !t.is_empty())
            .map(|t| t.replace("{query}", "").trim().to_string())
            .unwrap_or_else(|| wf.name.clone());
        let subtext = str_of(cfg, "subtext").replace("{query}", "…");
        // argumenttype: 0 required, 1 optional, 2 none.
        let argtype = int_of(cfg, "argumenttype").unwrap_or(1);
        let dests = wf.connections.get(uid).cloned().unwrap_or_default();
        let mut funcs = String::new();
        let mut seen = Vec::new();
        for (d, _) in &dests {
            node_fn(&wf, d, &steps, &mut seen, &mut funcs)?;
        }
        let icon = icon_for(&wf, uid);
        let mode = if is_filter { "list" } else { "compact" };
        let mut s = header(&title, mode, icon.as_deref(), &wf, keyword, &subtext);
        if is_filter {
            // Alfred filters results itself only when asked; otherwise re-run per keystroke.
            let alfred_filters = cfg
                .get("alfredfiltersresults")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let _ = writeln!(
                s,
                "# @opencast.filter {}",
                if alfred_filters { "true" } else { "false" }
            );
        } else if argtype != 2 {
            let _ = writeln!(
                s,
                "# @opencast.argument1 {{ \"type\": \"text\", \"placeholder\": \"{}\", \"optional\": {} }}",
                if keyword.is_empty() {
                    "query".to_string()
                } else {
                    keyword.replace('"', "")
                },
                argtype != 0
            );
        }
        s.push('\n');
        s.push_str(&prelude(&wf));
        s.push_str(&funcs);
        s.push_str("q=\"$1\"\n");
        if is_filter {
            let runner = script_runner(&wf, uid, cfg, &steps)?;
            s.push_str("if [ \"$OPENCAST_ACTION\" = run ]; then\n");
            s.push_str(&dispatch(&dests, "\"$q\""));
            s.push_str("  exit 0\nfi\n");
            s.push_str(&runner);
            s.push('\n');
        } else {
            s.push_str(&dispatch(&dests, "\"$q\""));
        }
        let path = out_dir.join(format!("{}-{}.sh", slug(&title), &uid[..uid.len().min(8)]));
        std::fs::write(&path, s).with_context(|| format!("writing {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
        }
        written.push(path);
    }
    if written.is_empty() {
        bail!(
            "`{}` has no Keyword, Script Filter or Hotkey inputs OpenCast can run",
            wf.name
        );
    }
    if let Some(p) = configure_command(&wf, plist, &out_dir)? {
        written.push(p);
    }
    Ok(written)
}

fn write_exec(path: &Path, s: &str) -> Result<()> {
    std::fs::write(path, s).with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// A Hotkey trigger: a command with that global hotkey (or none, to bind
/// yourself), feeding the selection, the clipboard or fixed text into the chain.
fn hotkey_command(wf: &Workflow, uid: &str, cfg: &Value, steps: &Path, out_dir: &Path) -> Result<PathBuf> {
    let dests = wf.connections.get(uid).cloned().unwrap_or_default();
    let mut funcs = String::new();
    let mut seen = Vec::new();
    for (d, _) in &dests {
        node_fn(wf, d, steps, &mut seen, &mut funcs)?;
    }
    // Title: the first connected step's own title, else the workflow name.
    let step_title = dests
        .first()
        .map(|(d, _)| wf.config(d))
        .and_then(|c| {
            [str_of(c, "title"), str_of(c, "text")]
                .into_iter()
                .find(|t| !t.trim().is_empty())
        })
        .map(|t| t.replace("{query}", "").trim().to_string())
        .filter(|t| !t.is_empty());
    let title = match step_title {
        Some(t) => t,
        None => wf.name.clone(),
    };
    let hotkey = hotkey_string(cfg);
    let desc = match &hotkey {
        Some(k) => format!("Alfred hotkey {k}"),
        None => "Alfred hotkey trigger (no key set — bind one with ⌘K › Record Hotkey)".to_string(),
    };
    let mut s = header_with(
        &title,
        "compact",
        icon_for(wf, uid).as_deref(),
        wf,
        "",
        &desc,
        hotkey.as_deref(),
    );
    s.push('\n');
    s.push_str(&prelude(wf));
    s.push_str(&funcs);
    // argument: 0 none, 1 selection, 2 clipboard, 3 text. A passed-in value (Act on Selection) wins.
    let input = match int_of(cfg, "argument").unwrap_or(0) {
        1 => "q=\"${1:-$(_oc_selection)}\"".to_string(),
        2 => "q=\"${1:-$(/usr/bin/pbpaste)}\"".to_string(),
        3 => format!("q={}", subst_expr(str_of(cfg, "argumenttext"), "\"$1\"")),
        _ => "q=\"$1\"".to_string(),
    };
    s.push_str(&input);
    s.push('\n');
    s.push_str(&dispatch(&dests, "\"$q\""));
    let path = out_dir.join(format!("{}-hotkey-{}.sh", slug(&title), &uid[..uid.len().min(8)]));
    write_exec(&path, &s)?;
    Ok(path)
}

/// "Configure <workflow>": a list of the workflow's settings. Type a value and
/// press Return on a row to set it; Return on a checkbox toggles it.
fn configure_command(wf: &Workflow, plist: &Value, out_dir: &Path) -> Result<Option<PathBuf>> {
    let Some(cfg) = plist.get("userconfigurationconfig").and_then(Value::as_array) else {
        return Ok(None);
    };
    let mut rows = String::new();
    for c in cfg {
        let var = str_of(c, "variable");
        if var.is_empty() || !var.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            continue;
        }
        let kind = str_of(c, "type");
        let label = [
            str_of(c, "label"),
            str_of(c.get("config").unwrap_or(&Value::Null), "text"),
            var,
        ]
        .into_iter()
        .find(|t| !t.trim().is_empty())
        .unwrap_or(var);
        let options: Vec<String> = c
            .get("config")
            .and_then(|x| x.get("pairs"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        p.as_array()
                            .and_then(|p| p.get(1))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let _ = writeln!(
            rows,
            "_oc_row {} {} {} {}",
            sq(var),
            sq(label),
            sq(kind),
            sq(&options.join(", "))
        );
    }
    if rows.is_empty() {
        return Ok(None);
    }
    let title = format!("Configure {}", wf.name);
    let mut s = header(
        &title,
        "list",
        icon_for(wf, "").as_deref(),
        wf,
        "",
        "Alfred workflow settings",
    );
    s.push_str("# @opencast.filter false\n# @opencast.action_mode compact\n\n");
    s.push_str(&prelude(wf));
    s.push_str(CONFIGURE);
    s.push_str("q=\"$1\"\nif [ \"$OPENCAST_ACTION\" = run ]; then\n  _oc_set \"$q\"\n  exit $?\nfi\nprintf '{\"items\":['\n_oc_first=1\n");
    s.push_str(&rows);
    s.push_str("printf ']}\\n'\n");
    let path = out_dir.join(format!("{}-configure.sh", slug(&title)));
    write_exec(&path, &s)?;
    Ok(Some(path))
}

const CONFIGURE: &str = r#"
_oc_json() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | awk 'BEGIN{ORS=""} NR>1{print "\\n"} {print}'; }
_oc_row() { # var label type options
  local var="$1" label="$2" kind="$3" opts="$4" cur sub arg
  cur="${!var}"
  if [ "$kind" = checkbox ]; then
    if [ "$cur" = 1 ] || [ "$cur" = true ]; then sub="On — Return turns it off"; arg="0"; else sub="Off — Return turns it on"; arg="1"; fi
  elif [ -n "$q" ]; then
    sub="Return sets it to “$q” (now: ${cur:-empty})"; arg="$q"
  else
    sub="Now: ${cur:-empty} — type a new value, then Return"; arg=""
    [ -n "$opts" ] && sub="$sub · options: $opts"
  fi
  [ "$_oc_first" = 1 ] || printf ','
  _oc_first=0
  printf '{"title":"%s","subtitle":"%s","arg":"%s"}' "$(_oc_json "$label")" "$(_oc_json "$sub")" "$(_oc_json "$var=$arg")"
}
_oc_set() { # var=value
  local var="${1%%=*}" val="${1#*=}"
  case "$var" in ''|*[!A-Za-z0-9_]*) echo "bad setting" >&2; return 1 ;; esac
  [ "$var=" = "$1" ] && [ -z "$val" ] && { echo "Type a value first"; return 0; }
  mkdir -p "$(dirname "$_OC_USERCONF")"
  touch "$_OC_USERCONF"
  grep -v "^export $var=" "$_OC_USERCONF" > "$_OC_USERCONF.tmp"
  printf 'export %s=%q\n' "$var" "$val" >> "$_OC_USERCONF.tmp"
  mv "$_OC_USERCONF.tmp" "$_OC_USERCONF"
  echo "Set $var"
}
"#;

/// One snippet of an `.alfredsnippets` collection.
#[derive(Debug, Clone, PartialEq)]
pub struct Snippet {
    pub keyword: String,
    pub name: String,
    pub text: String,
}

/// The snippets in an unpacked `.alfredsnippets` folder (one JSON file per
/// snippet). `info` is the collection's `info.plist` as JSON, if any: its
/// keyword prefix/suffix are applied. Snippets without a keyword get one from
/// their name so they can still be searched and pasted.
pub fn snippets_in(dir: &Path, info: Option<&Value>) -> Vec<Snippet> {
    let prefix = info.map(|i| str_of(i, "snippetkeywordprefix")).unwrap_or("");
    let suffix = info.map(|i| str_of(i, "snippetkeywordsuffix")).unwrap_or("");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    let mut out: Vec<Snippet> = Vec::new();
    for f in files {
        let Ok(src) = std::fs::read_to_string(&f) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&src) else {
            continue;
        };
        let Some(s) = v.get("alfredsnippet") else { continue };
        let text = str_of(s, "snippet").to_string();
        if text.is_empty() {
            continue;
        }
        let name = str_of(s, "name").trim().to_string();
        let kw = str_of(s, "keyword").trim();
        let keyword = if kw.is_empty() {
            slug(if name.is_empty() { &text } else { &name })
        } else {
            format!("{prefix}{kw}{suffix}")
        };
        if keyword.is_empty() || out.iter().any(|o| o.keyword == keyword) {
            continue;
        }
        out.push(Snippet { keyword, name, text });
    }
    out
}

/// Every folder under `root` with an `info.plist` that has `objects`.
pub fn find_workflows(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    find(root, 0, &mut out);
    out
}

fn find(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if dir.join("info.plist").is_file() {
        out.push(dir.to_path_buf());
        return;
    }
    if depth >= 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut subs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    subs.sort();
    for s in subs {
        find(&s, depth + 1, out);
    }
}

/// Workflow name from its plist JSON.
pub fn name_of(plist: &Value) -> String {
    let n = str_of(plist, "name").trim();
    if n.is_empty() {
        "alfred-workflow".into()
    } else {
        n.to_string()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(script: &Path, args: &[&str], env: &[(&str, &str)]) -> (String, String, bool) {
        let mut c = std::process::Command::new(script);
        c.args(args).env("HOME", script.parent().unwrap());
        for (k, v) in env {
            c.env(k, v);
        }
        let o = c.output().unwrap();
        (
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
            o.status.success(),
        )
    }

    fn plist() -> Value {
        json!({
            "name": "Test WF",
            "bundleid": "com.example.test",
            "variables": {"greeting": "hello"},
            "objects": [
                {"uid": "KW1", "type": "alfred.workflow.input.keyword",
                 "config": {"keyword": "shout", "text": "Shout it", "argumenttype": 0}},
                {"uid": "UP", "type": "alfred.workflow.action.script",
                 "config": {"type": 0, "scriptargtype": 1, "script": "printf '%s %s' \"$greeting\" \"$1\" | tr a-z A-Z"}},
                {"uid": "CP", "type": "alfred.workflow.output.notification",
                 "config": {"text": "said: {query}"}},
                {"uid": "SF", "type": "alfred.workflow.input.scriptfilter",
                 "config": {"keyword": "pick", "title": "Pick One", "type": 0, "scriptargtype": 0,
                            "script": "echo \"{\\\"items\\\":[{\\\"title\\\":\\\"you typed {query}\\\",\\\"arg\\\":\\\"{query}\\\"}]}\""}},
                {"uid": "INLINE", "type": "alfred.workflow.action.script",
                 "config": {"type": 0, "scriptargtype": 0, "script": "echo \"inline:{query}\""}},
                {"uid": "MODACT", "type": "alfred.workflow.utility.argument",
                 "config": {"argument": "mod:{query}"}},
                {"uid": "X", "type": "alfred.workflow.action.unknownthing", "config": {}}
            ],
            "connections": {
                "KW1": [{"destinationuid": "UP", "modifiers": 0}],
                "UP": [{"destinationuid": "CP", "modifiers": 0}],
                "SF": [{"destinationuid": "INLINE", "modifiers": 0},
                       {"destinationuid": "MODACT", "modifiers": 1048576}]
            }
        })
    }

    #[test]
    fn generates_runnable_commands() {
        let dir = tempfile::tempdir().unwrap();
        let p = plist();
        let paths = generate(dir.path(), &p).unwrap();
        assert_eq!(paths.len(), 2);

        // Discovered like any script, with the keyword as a search word.
        let (found, warnings) = crate::scripts::discover(&dir.path().join(OUT_DIR));
        assert!(warnings.is_empty(), "{warnings:?}");
        let kw = found.iter().find(|m| m.title == "Shout it").unwrap();
        assert_eq!(kw.keywords, vec!["shout"]);
        assert_eq!(kw.arguments.len(), 1);
        assert!(!kw.arguments[0].optional);
        let sf = found.iter().find(|m| m.title == "Pick One").unwrap();
        assert_eq!(sf.mode, crate::scripts::OutputMode::List);
        assert!(!sf.filter);

        // Keyword → script (argv, workflow variable) → notification text.
        let (out, err, ok) = run(&kw.path, &["world"], &[]);
        assert!(ok, "{err}");
        assert_eq!(out, "said: HELLO WORLD");

        // Script Filter list step, with a query that tries to break out of quotes.
        let (out, err, ok) = run(&sf.path, &["a $(echo pwned) `id`"], &[]);
        assert!(ok, "{err}");
        let items = crate::scripts::parse_items(&out).unwrap();
        assert_eq!(items[0].title, "you typed a $(echo pwned) `id`");

        // Run step: plain → inline script; ⌘ → the modifier branch.
        let (out, _, ok) = run(&sf.path, &["x"], &[("OPENCAST_ACTION", "run")]);
        assert!(ok);
        assert_eq!(out, "inline:x");
        let (out, _, _) = run(&sf.path, &["x"], &[("OPENCAST_ACTION", "run"), ("OPENCAST_MOD", "cmd")]);
        assert_eq!(out, "mod:x");
        // Alfred's data folders exist.
        // (The test runs with HOME = the generated folder.)
        assert!(sf
            .path
            .parent()
            .unwrap()
            .join("Library/Application Support/Alfred/Workflow Data/com.example.test")
            .is_dir());
    }

    #[test]
    fn unsupported_steps_explain_themselves() {
        let dir = tempfile::tempdir().unwrap();
        let p = json!({
            "name": "Odd",
            "objects": [
                {"uid": "K", "type": "alfred.workflow.input.keyword", "config": {"keyword": "odd", "argumenttype": 2}},
                {"uid": "X", "type": "alfred.workflow.action.somethingnew", "config": {}}
            ],
            "connections": {"K": [{"destinationuid": "X", "modifiers": 0}]}
        });
        let paths = generate(dir.path(), &p).unwrap();
        let (_, err, ok) = run(&paths[0], &[], &[]);
        assert!(!ok);
        assert!(err.contains("action.somethingnew"), "{err}");
    }

    #[test]
    fn no_inputs_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let p = json!({"name": "Outputs only", "objects": [{"uid": "N", "type": "alfred.workflow.output.notification", "config": {}}]});
        assert!(generate(dir.path(), &p).is_err());
    }

    #[test]
    fn conditionals_hotkeys_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        let p = json!({
            "name": "Branchy",
            "bundleid": "com.example.branchy",
            "variables": {"greeting": "hi"},
            "userconfigurationconfig": [
                {"variable": "greeting", "type": "textfield", "label": "Greeting", "config": {"default": "hello"}},
                {"variable": "loud", "type": "checkbox", "label": "Shout", "config": {"default": false}}
            ],
            "objects": [
                {"uid": "K", "type": "alfred.workflow.input.keyword", "config": {"keyword": "br", "title": "Branch", "argumenttype": 1}},
                {"uid": "C", "type": "alfred.workflow.utility.conditional", "config": {
                    "conditions": [
                        {"inputstring": "{query}", "matchmode": 6, "matchstring": "go", "matchcasesensitive": false, "uid": "GO"},
                        {"inputstring": "{query}", "matchmode": 4, "matchstring": "^[0-9]+$", "uid": "NUM"}
                    ], "elselabel": "else"}},
                {"uid": "A", "type": "alfred.workflow.action.script", "config": {"type": 0, "scriptargtype": 1, "script": "echo \"go:$1 $greeting\""}},
                {"uid": "B", "type": "alfred.workflow.action.script", "config": {"type": 0, "scriptargtype": 1, "script": "echo \"num:$1\""}},
                {"uid": "E", "type": "alfred.workflow.action.script", "config": {"type": 0, "scriptargtype": 1, "script": "echo \"else:$1\""}},
                {"uid": "H", "type": "alfred.workflow.trigger.hotkey", "config": {"hotkey": 40, "hotmod": 1572864, "argument": 3, "argumenttext": "Go {var:greeting}"}}
            ],
            "connections": {
                "K": [{"destinationuid": "C", "modifiers": 0}],
                "H": [{"destinationuid": "C", "modifiers": 0}],
                "C": [
                    {"destinationuid": "A", "modifiers": 0, "sourceoutputuid": "GO"},
                    {"destinationuid": "B", "modifiers": 0, "sourceoutputuid": "NUM"},
                    {"destinationuid": "E", "modifiers": 0}
                ]
            }
        });
        let home = dir.path().join("home");
        let h = home.to_str().unwrap();
        let paths = generate(dir.path(), &p).unwrap();
        assert_eq!(paths.len(), 3, "{paths:?}");
        let (found, warnings) = crate::scripts::discover(&dir.path().join(OUT_DIR));
        assert!(warnings.is_empty(), "{warnings:?}");
        let kw = found.iter().find(|m| m.title == "Branch").unwrap();
        let hk = found.iter().find(|m| m.hotkey.is_some()).unwrap();
        assert_eq!(hk.hotkey.as_deref(), Some("Alt+Cmd+K"));
        let conf = found.iter().find(|m| m.title == "Configure Branchy").unwrap();

        assert_eq!(run(&kw.path, &["GOing"], &[("HOME", h)]).0, "go:GOing hi");
        assert_eq!(run(&kw.path, &["123"], &[("HOME", h)]).0, "num:123");
        assert_eq!(run(&kw.path, &["x go"], &[("HOME", h)]).0, "else:x go");
        // The hotkey feeds its text ({var:…} expanded) into the same chain.
        assert_eq!(run(&hk.path, &[], &[("HOME", h)]).0, "go:Go hi hi");

        // Settings: list shows the current value; Return on a row stores it.
        let (out, err, ok) = run(&conf.path, &["howdy"], &[("HOME", h)]);
        assert!(ok, "{err}");
        let items = crate::scripts::parse_items(&out).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].arg.as_deref(), Some("greeting=howdy"));
        assert_eq!(items[1].arg.as_deref(), Some("loud=1"));
        let (out, err, ok) = run(
            &conf.path,
            &["greeting=howdy"],
            &[("OPENCAST_ACTION", "run"), ("HOME", h)],
        );
        assert!(ok, "{err}");
        assert_eq!(out, "Set greeting");
        assert_eq!(run(&kw.path, &["go"], &[("HOME", h)]).0, "go:go howdy");
        // Survives a re-import (stored with Alfred's data, not in the workflow folder).
        let paths = generate(dir.path(), &p).unwrap();
        let kw = paths.iter().find(|p| p.to_string_lossy().contains("branch-K")).unwrap();
        assert_eq!(run(kw, &["go"], &[("HOME", h)]).0, "go:go howdy");
    }

    #[test]
    fn hotkey_names() {
        assert_eq!(
            hotkey_string(&json!({"hotkey": 49, "hotmod": 262144})).as_deref(),
            Some("Ctrl+Space")
        );
        assert_eq!(hotkey_string(&json!({"hotkey": 0, "hotmod": 0})), None);
        assert_eq!(
            hotkey_string(&json!({"hotkey": 123, "hotmod": 1179648})).as_deref(),
            Some("Shift+Cmd+Left")
        );
        assert_eq!(
            hotkey_string(&json!({"hotkey": 40, "hotmod": 8388608 + 1048576})).as_deref(),
            Some("Cmd+Fn+K")
        );
    }

    #[test]
    fn reads_snippet_collections() {
        let dir = tempfile::tempdir().unwrap();
        let w = |n: &str, v: Value| std::fs::write(dir.path().join(n), v.to_string()).unwrap();
        w(
            "a.json",
            json!({"alfredsnippet": {"snippet": "Best,\nAlex", "keyword": "sig", "name": "Signature"}}),
        );
        w(
            "b.json",
            json!({"alfredsnippet": {"snippet": "{date}", "keyword": "", "name": "Today"}}),
        );
        w(
            "c.json",
            json!({"alfredsnippet": {"snippet": "dup", "keyword": "sig", "name": "Dup"}}),
        );
        let info = json!({"snippetkeywordprefix": ";", "snippetkeywordsuffix": ""});
        let s = snippets_in(dir.path(), Some(&info));
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].keyword, ";sig");
        assert_eq!(s[0].text, "Best,\nAlex");
        assert_eq!(s[1].keyword, "today");
    }
}
