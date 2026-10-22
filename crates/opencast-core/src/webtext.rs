//! Readable text from a web page, for "Ask About This Page" when the browser
//! won't hand over the page itself (no "Allow JavaScript from Apple Events").

use anyhow::{bail, Result};

/// Longest page text handed to a model (characters).
pub const MAX_CHARS: usize = 60_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub title: String,
    pub text: String,
}

/// Fetch `url` and reduce it to text. Only http(s).
pub async fn fetch(url: &str) -> Result<Page> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        bail!("only web pages can be read (got {url})");
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("Mozilla/5.0 (Macintosh) OpenCast/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        bail!("the page answered HTTP {}", resp.status().as_u16());
    }
    let html = resp.text().await?;
    Ok(html_to_text(&html))
}

/// Strip markup: drop script/style/nav-like blocks, keep paragraph breaks,
/// decode common entities, collapse whitespace.
pub fn html_to_text(html: &str) -> Page {
    let lower = html.to_lowercase();
    let title = between(html, &lower, "<title", "</title>")
        .map(|t| t.split_once('>').map(|(_, t)| t).unwrap_or(t))
        .map(|t| decode(t.trim()))
        .unwrap_or_default();
    // Prefer <article> or <main> when the page has one.
    let body = ["<article", "<main"]
        .iter()
        .find_map(|tag| {
            let close = format!("</{}>", &tag[1..]);
            between(html, &lower, tag, &close)
                .map(|b| b.split_once('>').map(|(_, b)| b).unwrap_or(b))
                .filter(|b| b.len() > 500)
        })
        .unwrap_or(html);

    let mut out = String::new();
    let bytes = body.as_bytes();
    let lower_body = body.to_lowercase();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let rest = &lower_body[i..];
            // Skip whole elements that are never content.
            if let Some(tag) = [
                "script", "style", "noscript", "svg", "nav", "footer", "header", "form", "template", "title", "head",
            ]
            .iter()
            .find(|t| {
                rest[1..].starts_with(**t) && rest[1 + t.len()..].starts_with(|c: char| c == '>' || c.is_whitespace())
            }) {
                let close = format!("</{tag}");
                match rest.find(&close) {
                    Some(end) => {
                        i += end + close.len();
                        if let Some(gt) = lower_body[i..].find('>') {
                            i += gt + 1;
                        }
                        continue;
                    }
                    None => break,
                }
            }
            let end = match rest.find('>') {
                Some(e) => e,
                None => break,
            };
            let tag = rest[1..end].trim_start_matches('/');
            let name: String = tag.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if matches!(
                name.as_str(),
                "p" | "div"
                    | "br"
                    | "li"
                    | "tr"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "section"
                    | "blockquote"
                    | "pre"
            ) {
                out.push('\n');
            }
            if name == "li" && !rest[1..].starts_with('/') {
                out.push_str("- ");
            }
            i += end + 1;
            continue;
        }
        let next = body[i..].find('<').map(|n| i + n).unwrap_or(body.len());
        out.push_str(&body[i..next]);
        i = next;
    }
    let text = decode(&out);
    // Collapse runs of spaces and blank lines.
    let mut lines: Vec<String> = Vec::new();
    for l in text.lines() {
        let l = l.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() {
            continue;
        }
        lines.push(l);
    }
    let mut text = lines.join("\n").trim().to_string();
    if text.chars().count() > MAX_CHARS {
        text = text.chars().take(MAX_CHARS).collect::<String>() + "\n…";
    }
    Page { title, text }
}

fn between<'a>(s: &'a str, lower: &str, open: &str, close: &str) -> Option<&'a str> {
    let a = lower.find(open)?;
    let b = lower[a..].find(close)? + a;
    Some(&s[a + open.len()..b])
}

fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        let Some(semi) = after[..after.len().min(10)].find(';') else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let ent = &after[1..semi];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "rsquo" => Some('’'),
            "lsquo" => Some('‘'),
            "ldquo" => Some('“'),
            "rdquo" => Some('”'),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_text() {
        let html = r#"<html><head><title>Tom &amp; Jerry</title><style>p{}</style></head>
<body><nav>Home | About</nav><script>var x = "<p>no</p>";</script>
<h1>Chase</h1><p>A cat&nbsp;and a <b>mouse</b>.</p><ul><li>One</li><li>Two &#8212; 2</li></ul>
<footer>© 2026</footer></body></html>"#;
        let p = html_to_text(html);
        assert_eq!(p.title, "Tom & Jerry");
        assert_eq!(p.text, "Chase\nA cat and a mouse.\n- One\n- Two — 2");
    }

    #[test]
    fn prefers_the_article() {
        let filler = "x".repeat(600);
        let html = format!("<body><div>menu junk</div><article><p>{filler}</p></article></body>");
        assert_eq!(html_to_text(&html).text, filler);
    }
}
