//! Currency conversion in the search field: `100 usd to eur`, `€50 in gbp`,
//! `20$ = uah`. Rates come from a daily cache (`rates.json`) that the shell
//! refreshes in the background (open.er-api.com, no key); search never waits
//! on the network.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Rates {
    /// Rates are units of each currency per 1 `base`.
    pub base: String,
    pub rates: HashMap<String, f64>,
    /// Unix seconds when fetched.
    pub fetched: u64,
    /// Provider's own date line, for the subtitle.
    #[serde(default)]
    pub date: String,
}

const FILE: &str = "rates.json";
/// Refresh after this long.
pub const MAX_AGE_SECS: u64 = 12 * 3600;

pub fn load(dir: &Path) -> Option<Rates> {
    std::fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

pub fn save(dir: &Path, rates: &Rates) {
    if let Ok(s) = serde_json::to_string(rates) {
        let _ = std::fs::write(dir.join(FILE), s);
    }
}

pub fn is_stale(r: Option<&Rates>, now: u64) -> bool {
    r.is_none_or(|r| now.saturating_sub(r.fetched) > MAX_AGE_SECS || r.rates.is_empty())
}

/// Fetch fresh rates (USD base).
pub async fn fetch() -> anyhow::Result<Rates> {
    let v: serde_json::Value = reqwest::Client::new()
        .get("https://open.er-api.com/v6/latest/USD")
        .header("User-Agent", "OpenCast")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let rates: HashMap<String, f64> = v["rates"]
        .as_object()
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| Some((k.to_uppercase(), v.as_f64()?)))
                .collect()
        })
        .unwrap_or_default();
    anyhow::ensure!(!rates.is_empty(), "no rates in the response");
    Ok(Rates {
        base: "USD".into(),
        rates,
        fetched: crate::usage::now(),
        date: v["time_last_update_utc"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(16)
            .collect(),
    })
}

const SYMBOLS: &[(&str, &str)] = &[
    ("$", "USD"),
    ("€", "EUR"),
    ("£", "GBP"),
    ("¥", "JPY"),
    ("₴", "UAH"),
    ("₹", "INR"),
    ("₽", "RUB"),
    ("₩", "KRW"),
    ("₺", "TRY"),
    ("₪", "ILS"),
    ("zł", "PLN"),
    ("฿", "THB"),
    ("₫", "VND"),
    ("₱", "PHP"),
    ("r$", "BRL"),
    ("c$", "CAD"),
    ("a$", "AUD"),
    ("chf", "CHF"),
];

const NAMES: &[(&str, &str)] = &[
    ("dollar", "USD"),
    ("dollars", "USD"),
    ("bucks", "USD"),
    ("euro", "EUR"),
    ("euros", "EUR"),
    ("pound", "GBP"),
    ("pounds", "GBP"),
    ("yen", "JPY"),
    ("hryvnia", "UAH"),
    ("hryvnias", "UAH"),
    ("grn", "UAH"),
    ("rupee", "INR"),
    ("rupees", "INR"),
    ("franc", "CHF"),
    ("francs", "CHF"),
    ("yuan", "CNY"),
    ("rmb", "CNY"),
    ("zloty", "PLN"),
    ("bitcoin", "BTC"),
];

fn code_of(word: &str, rates: &Rates) -> Option<String> {
    let w = word.trim().to_lowercase();
    if w.is_empty() {
        return None;
    }
    if let Some((_, c)) = SYMBOLS.iter().find(|(s, _)| *s == w) {
        return Some(c.to_string());
    }
    if let Some((_, c)) = NAMES.iter().find(|(n, _)| *n == w) {
        return Some(c.to_string());
    }
    let up = w.to_uppercase();
    (up.len() == 3
        && up.chars().all(|c| c.is_ascii_alphabetic())
        && (rates.rates.contains_key(&up) || up == rates.base))
        .then_some(up)
}

fn rate(rates: &Rates, code: &str) -> Option<f64> {
    if code == rates.base {
        Some(1.0)
    } else {
        rates.rates.get(code).copied().filter(|r| *r > 0.0)
    }
}

/// Split `"€50"`, `"50€"`, `"50 eur"` into (amount, currency word).
fn amount_and_unit(s: &str) -> Option<(f64, String)> {
    let s = s.trim();
    let num_start = s.find(|c: char| c.is_ascii_digit() || c == '.')?;
    let num_end = s[num_start..]
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .map(|i| num_start + i)
        .unwrap_or(s.len());
    let number: String = s[num_start..num_end].chars().filter(|c| *c != ',').collect();
    let amount: f64 = number.parse().ok()?;
    let before = s[..num_start].trim();
    let after = s[num_end..].trim();
    let unit = match (before.is_empty(), after.is_empty()) {
        (false, true) => before,
        (true, false) => after,
        _ => return None,
    };
    // Shorthand multipliers: `5k usd`.
    let (mult, unit) = match unit.split_once(char::is_whitespace) {
        Some(("k", rest)) => (1e3, rest.trim()),
        Some(("m", rest)) => (1e6, rest.trim()),
        _ => (1.0, unit),
    };
    Some((amount * mult, unit.to_string()))
}

/// A conversion result for the search row.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversion {
    pub value: f64,
    pub to: String,
    pub amount: f64,
    pub from: String,
}

impl Conversion {
    pub fn title(&self) -> String {
        format!("{} {}", format_money(self.value), self.to)
    }
    pub fn subtitle(&self, rates: &Rates) -> String {
        let when = if rates.date.is_empty() {
            String::new()
        } else {
            format!(" · rates {}", rates.date)
        };
        format!("= {} {}{when} · Enter to copy", format_money(self.amount), self.from)
    }
}

pub fn format_money(v: f64) -> String {
    let decimals = if v.abs() >= 1.0 || v == 0.0 { 2 } else { 6 };
    let s = format!("{v:.decimals$}");
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let (sign, digits) = int.strip_prefix('-').map(|d| ("-", d)).unwrap_or(("", int));
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let frac = if decimals > 2 { frac.trim_end_matches('0') } else { frac };
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{frac}")
    }
}

/// `100 usd to eur` → 92.13 EUR.
pub fn convert(query: &str, rates: &Rates) -> Option<Conversion> {
    let q = query.trim().to_lowercase();
    let (lhs, rhs) = [" to ", " in ", " = ", " into ", " as "]
        .iter()
        .find_map(|sep| q.split_once(sep))?;
    let (amount, unit) = amount_and_unit(lhs)?;
    let from = code_of(&unit, rates)?;
    let to = code_of(rhs, rates)?;
    if from == to {
        return None;
    }
    let value = amount / rate(rates, &from)? * rate(rates, &to)?;
    Some(Conversion {
        value,
        to,
        amount,
        from,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rates() -> Rates {
        Rates {
            base: "USD".into(),
            rates: [("EUR", 0.9), ("GBP", 0.8), ("UAH", 41.0), ("JPY", 150.0)]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            fetched: 1000,
            date: "Wed, 21 Oct 2026".into(),
        }
    }

    #[test]
    fn converts_codes_symbols_and_names() {
        let r = rates();
        let c = convert("100 usd to eur", &r).unwrap();
        assert!((c.value - 90.0).abs() < 1e-9);
        assert_eq!(c.title(), "90.00 EUR");
        assert!((convert("€90 in usd", &r).unwrap().value - 100.0).abs() < 1e-9);
        assert!((convert("10$ = uah", &r).unwrap().value - 410.0).abs() < 1e-9);
        assert!((convert("2 pounds to euros", &r).unwrap().value - 2.25).abs() < 1e-9);
        assert!((convert("5k usd to jpy", &r).unwrap().value - 750_000.0).abs() < 1e-6);
        assert_eq!(convert("5k usd to jpy", &r).unwrap().title(), "750,000.00 JPY");
        assert!((convert("1,000 eur to gbp", &r).unwrap().value - 888.888_888_888_9).abs() < 1e-6);
    }

    #[test]
    fn ignores_non_currency() {
        let r = rates();
        assert!(convert("10 km to mi", &r).is_none());
        assert!(convert("usd to eur", &r).is_none());
        assert!(convert("100 usd to usd", &r).is_none());
        assert!(convert("100 xyz to eur", &r).is_none());
        assert!(convert("hello world", &r).is_none());
    }

    #[test]
    fn staleness_and_format() {
        assert!(is_stale(None, 0));
        assert!(!is_stale(Some(&rates()), 1000 + 60));
        assert!(is_stale(Some(&rates()), 1000 + MAX_AGE_SECS + 1));
        assert_eq!(format_money(-1234567.891), "-1,234,567.89");
        assert_eq!(format_money(0.000123), "0.000123");
    }
}
