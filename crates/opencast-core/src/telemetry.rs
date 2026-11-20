//! Local timings and a ring of log lines. Nothing here leaves the process.
//!
//! The FFI crate installs a `tracing` subscriber. With no subscriber, spans
//! and `warn!` are no-ops; the ring and the histograms still record.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Last N log lines the debug panel reads. ponytail: one global mutex; shard if a profile says this lock shows up.
pub const RING_CAP: usize = 5_000;
const SAMPLE_CAP: usize = 256;
const SLOW_CAP: usize = 200;

#[derive(Clone)]
struct Sample {
    op: String,
    ms: u32,
    budget_ms: u32,
    at: u64,
}

struct Store {
    lines: VecDeque<String>,
    samples: HashMap<String, VecDeque<Sample>>,
    slow: VecDeque<Sample>,
}

impl Store {
    fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            samples: HashMap::new(),
            slow: VecDeque::new(),
        }
    }
}

fn store() -> &'static Mutex<Store> {
    static S: OnceLock<Mutex<Store>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Store::new()))
}

fn lock() -> std::sync::MutexGuard<'static, Store> {
    store().lock().unwrap_or_else(|e| e.into_inner())
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Record one finished operation. Warns when `budget_ms` is non-zero and exceeded.
pub fn record(op: &str, ms: u32, budget_ms: u32) {
    let sample = Sample {
        op: op.to_string(),
        ms,
        budget_ms,
        at: now_ms(),
    };
    {
        let mut g = lock();
        let bucket = g.samples.entry(sample.op.clone()).or_default();
        if bucket.len() >= SAMPLE_CAP {
            bucket.pop_front();
        }
        bucket.push_back(sample.clone());
        if budget_ms > 0 && ms > budget_ms {
            if g.slow.len() >= SLOW_CAP {
                g.slow.pop_front();
            }
            g.slow.push_back(sample);
        } else {
            return;
        }
    }
    tracing::warn!(op, elapsed_ms = ms, budget_ms, "over budget");
}

pub fn push_line(line: &str) {
    let line = redact_line(line);
    if line.is_empty() {
        return;
    }
    let mut g = lock();
    if g.lines.len() >= RING_CAP {
        g.lines.pop_front();
    }
    g.lines.push_back(line);
}

/// Times `op` and records on drop, including on panic unwind.
pub struct Guard {
    op: &'static str,
    budget_ms: u32,
    start: Instant,
}

impl Guard {
    pub fn new(op: &'static str, budget_ms: u32) -> Self {
        Self {
            op,
            budget_ms,
            start: Instant::now(),
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let ms = u32::try_from(self.start.elapsed().as_millis()).unwrap_or(u32::MAX);
        record(self.op, ms, self.budget_ms);
    }
}

pub fn percentile(sorted: &[u32], p: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let i = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

pub fn snapshot(limit: usize) -> Value {
    let g = lock();
    let start = g.lines.len().saturating_sub(limit);
    let lines: Vec<&String> = g.lines.iter().skip(start).collect();
    let mut ops: Vec<Value> = g
        .samples
        .iter()
        .map(|(name, samples)| {
            let mut ms: Vec<u32> = samples.iter().map(|s| s.ms).collect();
            ms.sort_unstable();
            json!({
                "name": name,
                "n": ms.len(),
                "p50": percentile(&ms, 0.50),
                "p95": percentile(&ms, 0.95),
                "p99": percentile(&ms, 0.99),
            })
        })
        .collect();
    ops.sort_by(|a, b| a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or("")));
    let cutoff = now_ms().saturating_sub(10 * 60 * 1000);
    let slow: Vec<Value> = g
        .slow
        .iter()
        .rev()
        .filter(|s| s.at >= cutoff)
        .take(40)
        .map(|s| json!({ "name": s.op, "ms": s.ms, "budget": s.budget_ms, "at": s.at }))
        .collect();
    json!({ "lines": lines, "ops": ops, "slow": slow })
}

/// Debug may log a query. Cap it so a pasted document cannot fill the disk.
/// ponytail: 200 chars; raise if a real debug session needs the rest.
pub fn clip(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push_str("...");
    }
    out
}

pub fn redact_line(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lower = trimmed.to_ascii_lowercase();
    let scrub = [
        "api_key",
        "apikey",
        "token",
        "secret",
        "password",
        "authorization",
        "bearer ",
    ];
    let line = if scrub.iter().any(|k| lower.contains(k)) {
        scrub_sensitive(trimmed)
    } else {
        trimmed.to_string()
    };
    clip(&line, 480)
}

pub fn redact_config(src: &str) -> String {
    src.lines().map(redact_config_line).collect::<Vec<_>>().join("\n")
}

fn redact_config_line(line: &str) -> String {
    let t = line.trim_start().to_ascii_lowercase();
    const KEYS: &[&str] = &["api_key", "apikey", "token", "secret", "password", "authorization"];
    if KEYS.iter().any(|k| t.contains(k)) {
        if let Some((k, _)) = line.split_once('=') {
            return format!("{k}= \"[redacted]\"");
        }
    }
    line.to_string()
}

fn scrub_sensitive(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            let quoted = &line[start..i.min(line.len())];
            let before = line[..start].to_ascii_lowercase();
            let sensitive = [
                "api_key",
                "apikey",
                "token",
                "secret",
                "password",
                "authorization",
                "bearer",
            ]
            .iter()
            .any(|k| before.contains(k));
            if sensitive && quoted.len() > 2 {
                out.push_str("\"[redacted]\"");
            } else {
                out.push_str(quoted);
            }
        } else {
            let ch = line[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_ring_and_redaction() {
        let mut ms = vec![10, 20, 30, 40, 100];
        ms.sort_unstable();
        assert_eq!(percentile(&ms, 0.50), 30);
        assert_eq!(percentile(&ms, 0.95), 100);
        assert_eq!(percentile(&[], 0.95), 0);

        record("telemetry-test-op", 5, 10);
        record("telemetry-test-op", 40, 10);
        let snap = snapshot(10);
        let op = snap["ops"]
            .as_array()
            .and_then(|a| a.iter().find(|v| v["name"] == "telemetry-test-op"))
            .cloned()
            .expect("sample");
        assert_eq!(op["n"], 2);
        assert!(op["p99"].as_u64().unwrap() >= 40);
        assert!(snap["slow"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "telemetry-test-op"));

        for _ in 0..RING_CAP + 10 {
            push_line("plain line");
        }
        assert!(lock().lines.len() <= RING_CAP);

        let cfg = "hotkey = \"Alt+Space\"\napi_key = \"sk-live-secret\"\nname = \"ok\"\n";
        let red = redact_config(cfg);
        assert!(!red.contains("sk-live"));
        assert!(red.contains("[redacted]"));
        assert!(red.contains("Alt+Space"));
        let line = redact_line(r#"{"token":"sk-live-secret","op":"search"}"#);
        assert!(!line.contains("sk-live"));
        assert_eq!(clip("abcdef", 3), "abc...");
    }
}
