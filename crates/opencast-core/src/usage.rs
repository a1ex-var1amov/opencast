//! What you run and what you pick: drives ranking.
//!
//! Two signals, both kept in memory by the engine (search never reads disk):
//! - **frecency**: run count, faded by how long ago the command last ran;
//! - **picks**: which command you chose after typing a query. Typing `sl` and
//!   choosing Slack once makes Slack the top hit for `sl` (and `s`) afterwards,
//!   the way Alfred and Raycast learn.
//!
//! On disk this is `usage.json`. The old `{ "counts": {...} }` format still loads.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Most queries we remember picks for (oldest are dropped first).
const MAX_QUERIES: usize = 600;
/// Longest query prefix we learn from; longer queries are already specific.
const MAX_QUERY_LEN: usize = 24;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub count: u32,
    /// Unix seconds of the last run.
    #[serde(default)]
    pub last: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Pick {
    /// Command id → times chosen for this query.
    pub ids: HashMap<String, u32>,
    #[serde(default)]
    pub last: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub commands: HashMap<String, Entry>,
    /// Lowercased query → what was picked.
    #[serde(default)]
    pub picks: HashMap<String, Pick>,
    /// Pre-v2 format, migrated on load.
    #[serde(default, skip_serializing)]
    counts: HashMap<String, u32>,
}

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("usage.json")
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Ids whose runs are not worth remembering (one-off values, history rows).
pub fn tracked(id: &str) -> bool {
    !(id.is_empty()
        || id.starts_with("calc:")
        || id.starts_with("clip:")
        || id.starts_with("shot:")
        || id.starts_with("note:")
        || id.starts_with("chat:")
        || id.starts_with("item:")
        || id.starts_with("menu:")
        || id.starts_with("file:"))
}

impl Usage {
    pub fn load(dir: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path(dir)) else {
            return Self::default();
        };
        let mut u: Usage = serde_json::from_str(&raw).unwrap_or_default();
        if u.version < 2 {
            for (id, count) in std::mem::take(&mut u.counts) {
                u.commands.entry(id).or_insert(Entry { count, last: 0 });
            }
            u.version = 2;
        }
        u
    }

    pub fn save(&self, dir: &Path) {
        let _ = fs::create_dir_all(dir);
        if let Ok(raw) = serde_json::to_string(self) {
            let tmp = dir.join("usage.json.tmp");
            if fs::write(&tmp, raw).is_ok() {
                let _ = fs::rename(&tmp, path(dir));
            }
        }
    }

    /// Record a run of `id`, chosen after typing `query` (may be empty).
    /// Returns false when the id is not tracked.
    pub fn record(&mut self, id: &str, query: &str, at: u64) -> bool {
        if !tracked(id) {
            return false;
        }
        let e = self.commands.entry(id.to_string()).or_default();
        e.count = e.count.saturating_add(1);
        e.last = at;
        let q = normalize(query);
        if !q.is_empty() && q.chars().count() <= MAX_QUERY_LEN {
            let p = self.picks.entry(q).or_default();
            *p.ids.entry(id.to_string()).or_insert(0) += 1;
            p.last = at;
            if self.picks.len() > MAX_QUERIES {
                self.evict_oldest_picks();
            }
        }
        true
    }

    fn evict_oldest_picks(&mut self) {
        let mut ages: Vec<(u64, String)> = self.picks.iter().map(|(q, p)| (p.last, q.clone())).collect();
        ages.sort();
        let drop = self.picks.len() - MAX_QUERIES;
        for (_, q) in ages.into_iter().take(drop) {
            self.picks.remove(&q);
        }
    }

    /// Count only (for the empty-query listing).
    pub fn count(&self, id: &str) -> u32 {
        self.commands.get(id).map_or(0, |e| e.count)
    }

    /// Frecency boost: up to 240 for a command you use a lot and used recently.
    pub fn frecency(&self, id: &str, now: u64) -> i64 {
        let Some(e) = self.commands.get(id) else { return 0 };
        let base = e.count.min(80) as i64 * 3;
        let age_days = now.saturating_sub(e.last) / 86_400;
        // Legacy entries (last = 0) count as a month old.
        let age_days = if e.last == 0 { 30 } else { age_days };
        let pct = match age_days {
            0 => 100,
            1..=3 => 85,
            4..=14 => 65,
            15..=60 => 40,
            _ => 20,
        };
        base * pct / 100
    }

    /// Learned boost for `id` given the typed query (already lowercased + trimmed).
    /// Exact query match is strong; a longer remembered query that starts with
    /// what you typed (you typed `sl`, earlier you picked Slack after `sla`)
    /// is weaker.
    pub fn learned(&self, id: &str, ql: &str) -> i64 {
        self.learned_all(ql).get(id).copied().unwrap_or(0)
    }

    /// [`Self::learned`] for every id at once: one pass over the remembered
    /// queries per keystroke instead of one per candidate.
    pub fn learned_all(&self, ql: &str) -> HashMap<String, i64> {
        // Scale by how consistently an id wins the query, with a little credit for volume.
        fn shares(p: &Pick) -> Vec<(&String, i64)> {
            let total: u32 = p.ids.values().sum();
            if total == 0 {
                return Vec::new();
            }
            p.ids
                .iter()
                .filter(|(_, &n)| n > 0)
                .map(|(id, &mine)| (id, (mine as i64 * 100 / total as i64) * (mine.min(5) as i64 + 5) / 10))
                .collect()
        }
        let mut out = HashMap::new();
        if ql.is_empty() || self.picks.is_empty() {
            return out;
        }
        let mut exact: HashMap<&String, i64> = HashMap::new();
        if let Some(p) = self.picks.get(ql) {
            for (id, s) in shares(p) {
                exact.insert(id, s * 8); // up to 800
            }
        }
        let mut prefix: HashMap<&String, i64> = HashMap::new();
        for (q, p) in &self.picks {
            if q.len() > ql.len() && q.starts_with(ql) {
                for (id, s) in shares(p) {
                    let v = prefix.entry(id).or_insert(0);
                    *v = (*v).max(s * 3); // up to 300
                }
            }
        }
        for (id, e) in &exact {
            out.insert((*id).clone(), *e);
        }
        for (id, p) in prefix {
            let e = exact.get(id).copied().unwrap_or(0);
            if e < 400 {
                out.insert(id.clone(), e.max(p));
            }
        }
        out
    }
}

fn normalize(q: &str) -> String {
    q.trim().to_lowercase()
}

// ---- compatibility helpers (counts-only callers) ---------------------------

/// Read-modify-write on disk (used when no engine is around, e.g. tests).
pub fn bump(dir: &Path, id: &str) {
    let mut u = Usage::load(dir);
    if u.record(id, "", now()) {
        u.save(dir);
    }
}

pub fn counts(dir: &Path) -> HashMap<String, u32> {
    Usage::load(dir)
        .commands
        .into_iter()
        .map(|(k, e)| (k, e.count))
        .collect()
}

/// Cap so a single favorite cannot drown category/title matches.
pub fn boost(counts: &HashMap<String, u32>, id: &str) -> i64 {
    counts.get(id).map(|n| (*n).min(80) as i64 * 3).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_and_boost() {
        let dir = tempfile::tempdir().unwrap();
        bump(dir.path(), "window:left-half");
        bump(dir.path(), "window:left-half");
        bump(dir.path(), "calc:42");
        let c = counts(dir.path());
        assert_eq!(c.get("window:left-half"), Some(&2));
        assert_eq!(c.len(), 1);
        assert_eq!(boost(&c, "window:left-half"), 6);
    }

    #[test]
    fn migrates_the_v1_format() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("usage.json"), r#"{"counts":{"app:/A.app":7}}"#).unwrap();
        let u = Usage::load(dir.path());
        assert_eq!(u.count("app:/A.app"), 7);
        assert_eq!(u.version, 2);
        u.save(dir.path());
        assert_eq!(Usage::load(dir.path()).count("app:/A.app"), 7);
    }

    #[test]
    fn frecency_fades() {
        let mut u = Usage::default();
        let t = 10_000_000;
        for _ in 0..10 {
            u.record("a", "", t);
        }
        assert_eq!(u.frecency("a", t), 30);
        assert!(u.frecency("a", t + 10 * 86_400) < 30);
        assert!(u.frecency("a", t + 400 * 86_400) <= 6);
        assert_eq!(u.frecency("nope", t), 0);
    }

    #[test]
    fn learns_picks_for_a_query() {
        let mut u = Usage::default();
        u.record("app:/Slack.app", "sl", 1);
        assert!(u.learned("app:/Slack.app", "sl") > 300);
        assert_eq!(u.learned("app:/Sublime.app", "sl"), 0);
        // Prefix of a remembered query still helps, less.
        let full = u.learned("app:/Slack.app", "sl");
        let prefix = u.learned("app:/Slack.app", "s");
        assert!(prefix > 0 && prefix < full);
        // Split picks share the boost.
        u.record("app:/Sublime.app", "sl", 2);
        assert!(u.learned("app:/Slack.app", "sl") < full);
        // Not tracked: no learning.
        assert!(!u.record("calc:4", "2+2", 3));
        assert!(!u.picks.contains_key("2+2"));
    }

    #[test]
    fn pick_memory_is_bounded() {
        let mut u = Usage::default();
        for i in 0..(MAX_QUERIES + 50) {
            u.record("a", &format!("q{i}"), i as u64);
        }
        assert_eq!(u.picks.len(), MAX_QUERIES);
        assert!(!u.picks.contains_key("q0"));
        assert!(u.picks.contains_key(&format!("q{}", MAX_QUERIES + 49)));
    }
}
