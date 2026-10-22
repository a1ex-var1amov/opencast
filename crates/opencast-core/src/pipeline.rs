//! Pipelines: run text through several commands in a row, Alfred-style.
//!
//! ```toml
//! [[pipelines]]
//! id = "clean-summary"
//! title = "Summarize Selection as Bullets"
//! input = "selection"          # selection | clipboard | query | none
//! steps = ["transform:trim", "ai:summarize", "prompt:Turn this into 3 bullets:\n{input}"]
//! output = "show"              # show | copy | paste | replace
//! ```
//!
//! A step is `ai:<ai command id>` (its prompt gets the text as `{selection}`
//! and `{input}`), `prompt:<text with {input}>` (an ad hoc AI prompt),
//! `script:<path or title>` (the text is `$1`, stdout is the result),
//! `shell:<command>` (text on stdin), or `transform:<name>`. The engine
//! resolves steps into [`Step`]s; the shell runs them.

use serde::{Deserialize, Serialize};

use crate::scripts::ScriptMeta;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub id: String,
    pub title: String,
    pub steps: Vec<String>,
    /// `selection` (default), `clipboard`, `query` (typed argument) or `none`.
    #[serde(default)]
    pub input: Option<String>,
    /// `show` (default), `copy`, `paste`, `replace`.
    #[serde(default)]
    pub output: Option<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub hotkey: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Selection,
    Clipboard,
    Query,
    None,
}

impl Input {
    pub fn parse(s: Option<&str>) -> Self {
        match s.map(|s| s.trim().to_lowercase()).as_deref() {
            Some("clipboard") => Self::Clipboard,
            Some("query" | "input" | "argument" | "text") => Self::Query,
            Some("none" | "") => Self::None,
            _ => Self::Selection,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Show,
    Copy,
    Paste,
    Replace,
}

impl Output {
    pub fn parse(s: Option<&str>) -> Self {
        match s.map(|s| s.trim().to_lowercase()).as_deref() {
            Some("copy") => Self::Copy,
            Some("paste") => Self::Paste,
            Some("replace") => Self::Replace,
            _ => Self::Show,
        }
    }
}

/// A step ready to run.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Ai {
        label: String,
        /// Prompt with `{input}` / `{selection}` still in it.
        prompt: String,
        system: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    },
    Script(Box<ScriptMeta>),
    Shell(String),
    Transform(Transform),
}

impl Step {
    pub fn label(&self) -> String {
        match self {
            Step::Ai { label, .. } => label.clone(),
            Step::Script(m) => m.title.clone(),
            Step::Shell(c) => format!("shell: {c}"),
            Step::Transform(t) => t.name().to_string(),
        }
    }
}

/// Fill an AI step's prompt with the text flowing through the pipeline. A
/// prompt without a placeholder gets the text appended.
pub fn fill_prompt(prompt: &str, text: &str) -> String {
    if prompt.contains("{input}") || prompt.contains("{selection}") {
        prompt.replace("{input}", text).replace("{selection}", text)
    } else {
        format!("{prompt}\n\n{text}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    Trim,
    Upper,
    Lower,
    Title,
    SortLines,
    UniqueLines,
    ReverseLines,
    RemoveEmptyLines,
    JoinLines,
    UrlEncode,
    UrlDecode,
    Base64Encode,
    Base64Decode,
    JsonFormat,
    JsonMinify,
    Slug,
    CountWords,
    StripMarkdown,
}

pub const TRANSFORMS: &[(Transform, &str)] = &[
    (Transform::Trim, "trim"),
    (Transform::Upper, "upper"),
    (Transform::Lower, "lower"),
    (Transform::Title, "title"),
    (Transform::SortLines, "sort-lines"),
    (Transform::UniqueLines, "unique-lines"),
    (Transform::ReverseLines, "reverse-lines"),
    (Transform::RemoveEmptyLines, "remove-empty-lines"),
    (Transform::JoinLines, "join-lines"),
    (Transform::UrlEncode, "url-encode"),
    (Transform::UrlDecode, "url-decode"),
    (Transform::Base64Encode, "base64-encode"),
    (Transform::Base64Decode, "base64-decode"),
    (Transform::JsonFormat, "json-format"),
    (Transform::JsonMinify, "json-minify"),
    (Transform::Slug, "slug"),
    (Transform::CountWords, "count"),
    (Transform::StripMarkdown, "strip-markdown"),
];

impl Transform {
    pub fn parse(name: &str) -> Option<Self> {
        let n = name.trim().to_lowercase().replace('_', "-");
        TRANSFORMS.iter().find(|(_, k)| *k == n).map(|(t, _)| *t)
    }

    pub fn name(self) -> &'static str {
        TRANSFORMS
            .iter()
            .find(|(t, _)| *t == self)
            .map(|(_, k)| *k)
            .unwrap_or("transform")
    }

    pub fn apply(self, s: &str) -> Result<String, String> {
        let lines = || s.lines().map(str::to_string).collect::<Vec<_>>();
        Ok(match self {
            Transform::Trim => s.trim().to_string(),
            Transform::Upper => s.to_uppercase(),
            Transform::Lower => s.to_lowercase(),
            Transform::Title => title_case(s),
            Transform::SortLines => {
                let mut l = lines();
                l.sort_by_key(|a| a.to_lowercase());
                l.join("\n")
            }
            Transform::UniqueLines => {
                let mut seen = std::collections::HashSet::new();
                lines()
                    .into_iter()
                    .filter(|l| seen.insert(l.clone()))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Transform::ReverseLines => lines().into_iter().rev().collect::<Vec<_>>().join("\n"),
            Transform::RemoveEmptyLines => lines()
                .into_iter()
                .filter(|l| !l.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            Transform::JoinLines => s.split_whitespace().collect::<Vec<_>>().join(" "),
            Transform::UrlEncode => url_encode(s),
            Transform::UrlDecode => url_decode(s),
            Transform::Base64Encode => base64_encode(s.as_bytes()),
            Transform::Base64Decode => {
                let bytes = base64_decode(s.trim()).ok_or("not valid base64")?;
                String::from_utf8(bytes).map_err(|_| "decoded data is not text".to_string())?
            }
            Transform::JsonFormat | Transform::JsonMinify => {
                let v: serde_json::Value = serde_json::from_str(s.trim()).map_err(|e| format!("not JSON: {e}"))?;
                if self == Transform::JsonFormat {
                    serde_json::to_string_pretty(&v).unwrap_or_default()
                } else {
                    v.to_string()
                }
            }
            Transform::Slug => crate::alfred::slug(s),
            Transform::CountWords => format!(
                "{} words · {} characters · {} lines",
                s.split_whitespace().count(),
                s.chars().count(),
                s.lines().count()
            ),
            Transform::StripMarkdown => strip_markdown(s),
        })
    }
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            start = false;
        } else {
            out.extend(c.to_lowercase());
            if c.is_whitespace() || c == '-' {
                start = true;
            }
        }
    }
    out
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let clean: Vec<u8> = s
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .map(|b| match b {
            b'-' => b'+',
            b'_' => b'/',
            b => b,
        })
        .collect();
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for b in clean {
        if b == b'=' {
            break;
        }
        let v = B64.iter().position(|c| *c == b)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn strip_markdown(s: &str) -> String {
    s.lines()
        .map(|l| {
            let t = l.trim_start();
            let t = t.trim_start_matches('#').trim_start();
            let t = t
                .strip_prefix("- ")
                .or_else(|| t.strip_prefix("* "))
                .or_else(|| t.strip_prefix("> "))
                .unwrap_or(t);
            let mut out = String::new();
            let mut chars = t.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '*' | '_' | '`' => {}
                    '[' => {
                        // [text](url) → text
                        let mut text = String::new();
                        for c in chars.by_ref() {
                            if c == ']' {
                                break;
                            }
                            text.push(c);
                        }
                        if chars.peek() == Some(&'(') {
                            for c in chars.by_ref() {
                                if c == ')' {
                                    break;
                                }
                            }
                        }
                        out.push_str(&text);
                    }
                    c => out.push(c),
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// "a → b → c" for subtitles.
pub fn describe(steps: &[String]) -> String {
    steps
        .iter()
        .map(|s| {
            let s = s.trim();
            match s.split_once(':') {
                Some(("prompt", p)) => format!("AI “{}”", p.chars().take(24).collect::<String>().trim()),
                Some(("ai" | "transform", rest)) => rest.to_string(),
                Some(("shell", c)) => c.split_whitespace().next().unwrap_or("shell").to_string(),
                Some(("script", rest)) => std::path::Path::new(rest)
                    .file_stem()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| rest.to_string()),
                _ => s.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms() {
        let t = |n: &str, s: &str| Transform::parse(n).unwrap().apply(s).unwrap();
        assert_eq!(t("upper", "abc"), "ABC");
        assert_eq!(t("title", "hello big-world"), "Hello Big-World");
        assert_eq!(t("sort-lines", "b\na\nC"), "a\nb\nC");
        assert_eq!(t("unique-lines", "a\nb\na"), "a\nb");
        assert_eq!(t("url-encode", "a b&c"), "a%20b%26c");
        assert_eq!(t("url-decode", "a%20b%26c+d"), "a b&c d");
        for s in ["", "f", "fo", "foo", "foob", "hello, wörld"] {
            assert_eq!(t("base64-decode", &t("base64-encode", s)), s);
        }
        assert_eq!(t("base64-encode", "foob"), "Zm9vYg==");
        assert_eq!(t("json-minify", "{ \"a\": [1, 2] }"), "{\"a\":[1,2]}");
        assert!(Transform::JsonFormat.apply("nope").is_err());
        assert_eq!(
            t("strip-markdown", "# Title\n- **bold** [link](http://x)"),
            "Title\nbold link"
        );
        assert_eq!(t("count", "two words"), "2 words · 9 characters · 1 lines");
        assert!(Transform::parse("nonsense").is_none());
    }

    #[test]
    fn prompts_and_descriptions() {
        assert_eq!(fill_prompt("Fix: {input}", "teh"), "Fix: teh");
        assert_eq!(fill_prompt("Summarize", "text"), "Summarize\n\ntext");
        assert_eq!(
            describe(&["transform:trim".into(), "ai:summarize".into(), "shell:wc -w".into()]),
            "trim → summarize → wc"
        );
        assert_eq!(Input::parse(None), Input::Selection);
        assert_eq!(Output::parse(Some("replace")), Output::Replace);
    }
}
