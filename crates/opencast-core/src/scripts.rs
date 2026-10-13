//! Script commands: executable files with `@opencast.*` (or `@raycast.*`)
//! header comments, compatible with Raycast's script-command format.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::commands::Argument;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputMode {
    /// Run, close the launcher, show nothing.
    Silent,
    /// Show the last line of output as a short toast.
    Compact,
    /// Show all output in a scrollable view.
    FullOutput,
}

impl OutputMode {
    fn parse(s: &str) -> Self {
        match s {
            "silent" => Self::Silent,
            "fullOutput" | "fulloutput" | "full" => Self::FullOutput,
            // Raycast's "inline" refreshes in the list; compact is the closest.
            _ => Self::Compact,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScriptMeta {
    pub path: PathBuf,
    pub title: String,
    pub mode: OutputMode,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub package: Option<String>,
    pub arguments: Vec<Argument>,
}

#[derive(Debug, Deserialize)]
struct RawArgument {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    placeholder: Option<String>,
    #[serde(default)]
    optional: bool,
}

/// Parse the header of a script. Returns `None` if it has no title (i.e. it
/// is not a script command).
pub fn parse_header(path: &Path, src: &str) -> Option<ScriptMeta> {
    let mut title = None;
    let mut mode = OutputMode::Compact;
    let mut icon = None;
    let mut description = None;
    let mut package = None;
    let mut args: Vec<(u8, Argument)> = Vec::new();

    for line in src.lines().take(80) {
        let Some(rest) = line
            .find("@opencast.")
            .map(|i| &line[i + "@opencast.".len()..])
            .or_else(|| line.find("@raycast.").map(|i| &line[i + "@raycast.".len()..]))
        else {
            continue;
        };
        let (key, value) = match rest.find(char::is_whitespace) {
            Some(i) => (&rest[..i], rest[i..].trim()),
            None => (rest.trim(), ""),
        };
        match key {
            "title" if !value.is_empty() => title = Some(value.to_string()),
            "mode" => mode = OutputMode::parse(value.split_whitespace().next().unwrap_or("")),
            "icon" if !value.is_empty() => icon = Some(value.to_string()),
            "description" if !value.is_empty() => description = Some(value.to_string()),
            "packageName" if !value.is_empty() => package = Some(value.to_string()),
            k if k.starts_with("argument") => {
                let Ok(n) = k["argument".len()..].parse::<u8>() else {
                    continue;
                };
                if let Ok(raw) = serde_json::from_str::<RawArgument>(value) {
                    args.push((
                        n,
                        Argument {
                            placeholder: raw.placeholder.unwrap_or_else(|| format!("Argument {n}")),
                            optional: raw.optional,
                            secure: raw.kind.as_deref() == Some("password"),
                        },
                    ));
                }
            }
            _ => {}
        }
    }

    args.sort_by_key(|(n, _)| *n);
    Some(ScriptMeta {
        path: path.to_path_buf(),
        title: title?,
        mode,
        icon,
        description,
        package,
        arguments: args.into_iter().map(|(_, a)| a).take(3).collect(),
    })
}

/// Find script commands in `dir` (one level deep). Returns the scripts and
/// human-readable warnings for files that were skipped.
pub fn discover(dir: &Path) -> (Vec<ScriptMeta>, Vec<String>) {
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (found, warnings);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !path.is_file() {
            continue;
        }
        let Ok(bytes) = read_prefix(&path, 64 * 1024) else {
            continue;
        };
        let src = String::from_utf8_lossy(&bytes);
        let Some(meta) = parse_header(&path, &src) else {
            continue;
        };
        if !is_executable(&path) {
            warnings.push(format!(
                "script `{name}` is not executable — run: chmod +x \"{}\"",
                path.display()
            ));
            continue;
        }
        found.push(meta);
    }
    found.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    (found, warnings)
}

fn read_prefix(path: &Path, max: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::with_capacity(4096);
    std::fs::File::open(path)?.take(max as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_: &Path) -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct ScriptOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

impl ScriptOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    /// Last non-empty stdout line, used by compact mode.
    pub fn last_line(&self) -> Option<&str> {
        self.stdout.lines().rev().map(str::trim).find(|l| !l.is_empty())
    }
}

/// Run a script with arguments, out of process, killed after `timeout`.
pub async fn run(meta: &ScriptMeta, args: &[String], timeout: Duration) -> Result<ScriptOutput> {
    let mut cmd = tokio::process::Command::new(&meta.path);
    cmd.args(args).kill_on_drop(true).stdin(std::process::Stdio::null());
    if let Some(parent) = meta.path.parent() {
        cmd.current_dir(parent);
    }
    let out = tokio::time::timeout(timeout, cmd.output())
        .await
        .map_err(|_| anyhow!("`{}` timed out after {}s", meta.title, timeout.as_secs()))?
        .with_context(|| format!("running {}", meta.path.display()))?;
    Ok(ScriptOutput {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_opencast_header() {
        let src = include_str!("../../../examples/scripts/uuid.sh");
        let m = parse_header(Path::new("/x/uuid.sh"), src).unwrap();
        assert_eq!(m.title, "Generate UUID");
        assert_eq!(m.mode, OutputMode::Compact);
        assert_eq!(m.icon.as_deref(), Some("🆔"));
        assert_eq!(m.arguments.len(), 1);
        assert!(m.arguments[0].optional);
        assert_eq!(m.arguments[0].placeholder, "lowercase? (y/n)");
    }

    #[test]
    fn parses_raycast_header_with_trailing_comment_and_args_in_order() {
        let src = "#!/usr/bin/env python3\n\
            # Required parameters:\n\
            # @raycast.schemaVersion 1\n\
            # @raycast.title Open Ticket\n\
            # @raycast.mode fullOutput   # show everything\n\
            # @raycast.argument2 { \"type\": \"password\", \"placeholder\": \"token\" }\n\
            # @raycast.argument1 { \"type\": \"text\", \"placeholder\": \"id\" }\n";
        let m = parse_header(Path::new("t.py"), src).unwrap();
        assert_eq!(m.title, "Open Ticket");
        assert_eq!(m.mode, OutputMode::FullOutput);
        assert_eq!(m.arguments[0].placeholder, "id");
        assert!(m.arguments[1].secure);
    }

    #[test]
    fn no_title_means_not_a_command() {
        assert!(parse_header(Path::new("x"), "#!/bin/sh\necho hi\n").is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn discover_and_run() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("hello.sh");
        std::fs::write(
            &ok,
            "#!/bin/sh\n# @opencast.title Hello\necho \"hi $1\"\necho oops >&2\n",
        )
        .unwrap();
        std::fs::set_permissions(&ok, std::fs::Permissions::from_mode(0o755)).unwrap();
        let noexec = dir.path().join("noexec.sh");
        std::fs::write(&noexec, "#!/bin/sh\n# @opencast.title Nope\n").unwrap();

        let (found, warnings) = discover(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(warnings.len(), 1);

        let out = run(&found[0], &["there".into()], Duration::from_secs(5)).await.unwrap();
        assert!(out.success());
        assert_eq!(out.last_line(), Some("hi there"));
        assert_eq!(out.stderr.trim(), "oops");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_times_out() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("slow.sh");
        std::fs::write(&p, "#!/bin/sh\n# @opencast.title Slow\nsleep 5\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        let meta = parse_header(&p, &std::fs::read_to_string(&p).unwrap()).unwrap();
        let err = run(&meta, &[], Duration::from_millis(200)).await.unwrap_err();
        assert!(err.to_string().contains("timed out"));
    }
}
