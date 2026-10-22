//! Time zones in the search field: `time in tokyo`, `3pm pst in kyiv`,
//! `15:30 london to new york`, `9am in sydney` (from your zone).
//!
//! Reads the system's tz database (`/usr/share/zoneinfo`, TZif files) so DST
//! is exact without a dependency; the POSIX rule in the file's footer covers
//! dates after its last listed transition.

use std::path::{Path, PathBuf};

pub const ZONEINFO: &str = "/usr/share/zoneinfo";

/// City, country or abbreviation → IANA zone.
const PLACES: &[(&str, &str)] = &[
    // Abbreviations
    ("utc", "UTC"),
    ("gmt", "UTC"),
    ("z", "UTC"),
    ("pst", "America/Los_Angeles"),
    ("pdt", "America/Los_Angeles"),
    ("pt", "America/Los_Angeles"),
    ("pacific", "America/Los_Angeles"),
    ("mst", "America/Denver"),
    ("mdt", "America/Denver"),
    ("mt", "America/Denver"),
    ("mountain", "America/Denver"),
    ("cst", "America/Chicago"),
    ("cdt", "America/Chicago"),
    ("ct", "America/Chicago"),
    ("central", "America/Chicago"),
    ("est", "America/New_York"),
    ("edt", "America/New_York"),
    ("et", "America/New_York"),
    ("eastern", "America/New_York"),
    ("akst", "America/Anchorage"),
    ("hst", "Pacific/Honolulu"),
    ("bst", "Europe/London"),
    ("cet", "Europe/Paris"),
    ("cest", "Europe/Paris"),
    ("eet", "Europe/Kyiv"),
    ("eest", "Europe/Kyiv"),
    ("msk", "Europe/Moscow"),
    ("ist", "Asia/Kolkata"),
    ("jst", "Asia/Tokyo"),
    ("kst", "Asia/Seoul"),
    ("hkt", "Asia/Hong_Kong"),
    ("sgt", "Asia/Singapore"),
    ("aest", "Australia/Sydney"),
    ("aedt", "Australia/Sydney"),
    ("nzst", "Pacific/Auckland"),
    ("nzdt", "Pacific/Auckland"),
    // North America
    ("new york", "America/New_York"),
    ("nyc", "America/New_York"),
    ("ny", "America/New_York"),
    ("boston", "America/New_York"),
    ("washington", "America/New_York"),
    ("dc", "America/New_York"),
    ("miami", "America/New_York"),
    ("atlanta", "America/New_York"),
    ("raleigh", "America/New_York"),
    ("cary", "America/New_York"),
    ("charlotte", "America/New_York"),
    ("philadelphia", "America/New_York"),
    ("detroit", "America/Detroit"),
    ("toronto", "America/Toronto"),
    ("montreal", "America/Toronto"),
    ("ottawa", "America/Toronto"),
    ("chicago", "America/Chicago"),
    ("dallas", "America/Chicago"),
    ("austin", "America/Chicago"),
    ("houston", "America/Chicago"),
    ("minneapolis", "America/Chicago"),
    ("denver", "America/Denver"),
    ("phoenix", "America/Phoenix"),
    ("salt lake city", "America/Denver"),
    ("los angeles", "America/Los_Angeles"),
    ("la", "America/Los_Angeles"),
    ("san francisco", "America/Los_Angeles"),
    ("sf", "America/Los_Angeles"),
    ("santa clara", "America/Los_Angeles"),
    ("san jose", "America/Los_Angeles"),
    ("seattle", "America/Los_Angeles"),
    ("portland", "America/Los_Angeles"),
    ("vancouver", "America/Vancouver"),
    ("anchorage", "America/Anchorage"),
    ("honolulu", "Pacific/Honolulu"),
    ("hawaii", "Pacific/Honolulu"),
    ("mexico city", "America/Mexico_City"),
    // South America
    ("sao paulo", "America/Sao_Paulo"),
    ("são paulo", "America/Sao_Paulo"),
    ("rio", "America/Sao_Paulo"),
    ("buenos aires", "America/Argentina/Buenos_Aires"),
    ("santiago", "America/Santiago"),
    ("bogota", "America/Bogota"),
    ("lima", "America/Lima"),
    // Europe
    ("london", "Europe/London"),
    ("uk", "Europe/London"),
    ("dublin", "Europe/Dublin"),
    ("lisbon", "Europe/Lisbon"),
    ("madrid", "Europe/Madrid"),
    ("barcelona", "Europe/Madrid"),
    ("paris", "Europe/Paris"),
    ("france", "Europe/Paris"),
    ("brussels", "Europe/Brussels"),
    ("amsterdam", "Europe/Amsterdam"),
    ("berlin", "Europe/Berlin"),
    ("germany", "Europe/Berlin"),
    ("munich", "Europe/Berlin"),
    ("zurich", "Europe/Zurich"),
    ("geneva", "Europe/Zurich"),
    ("vienna", "Europe/Vienna"),
    ("rome", "Europe/Rome"),
    ("milan", "Europe/Rome"),
    ("prague", "Europe/Prague"),
    ("warsaw", "Europe/Warsaw"),
    ("stockholm", "Europe/Stockholm"),
    ("oslo", "Europe/Oslo"),
    ("copenhagen", "Europe/Copenhagen"),
    ("helsinki", "Europe/Helsinki"),
    ("tallinn", "Europe/Tallinn"),
    ("riga", "Europe/Riga"),
    ("vilnius", "Europe/Vilnius"),
    ("kyiv", "Europe/Kyiv"),
    ("kiev", "Europe/Kyiv"),
    ("ukraine", "Europe/Kyiv"),
    ("lviv", "Europe/Kyiv"),
    ("odesa", "Europe/Kyiv"),
    ("kharkiv", "Europe/Kyiv"),
    ("bucharest", "Europe/Bucharest"),
    ("athens", "Europe/Athens"),
    ("istanbul", "Europe/Istanbul"),
    ("moscow", "Europe/Moscow"),
    ("minsk", "Europe/Minsk"),
    ("chisinau", "Europe/Chisinau"),
    ("tbilisi", "Asia/Tbilisi"),
    ("yerevan", "Asia/Yerevan"),
    // Africa / Middle East
    ("cairo", "Africa/Cairo"),
    ("lagos", "Africa/Lagos"),
    ("nairobi", "Africa/Nairobi"),
    ("johannesburg", "Africa/Johannesburg"),
    ("cape town", "Africa/Johannesburg"),
    ("tel aviv", "Asia/Jerusalem"),
    ("jerusalem", "Asia/Jerusalem"),
    ("dubai", "Asia/Dubai"),
    ("abu dhabi", "Asia/Dubai"),
    ("riyadh", "Asia/Riyadh"),
    ("doha", "Asia/Qatar"),
    ("tehran", "Asia/Tehran"),
    // Asia / Pacific
    ("karachi", "Asia/Karachi"),
    ("mumbai", "Asia/Kolkata"),
    ("delhi", "Asia/Kolkata"),
    ("new delhi", "Asia/Kolkata"),
    ("bangalore", "Asia/Kolkata"),
    ("bengaluru", "Asia/Kolkata"),
    ("pune", "Asia/Kolkata"),
    ("hyderabad", "Asia/Kolkata"),
    ("chennai", "Asia/Kolkata"),
    ("india", "Asia/Kolkata"),
    ("kathmandu", "Asia/Kathmandu"),
    ("dhaka", "Asia/Dhaka"),
    ("bangkok", "Asia/Bangkok"),
    ("hanoi", "Asia/Bangkok"),
    ("ho chi minh", "Asia/Ho_Chi_Minh"),
    ("jakarta", "Asia/Jakarta"),
    ("singapore", "Asia/Singapore"),
    ("kuala lumpur", "Asia/Kuala_Lumpur"),
    ("manila", "Asia/Manila"),
    ("hong kong", "Asia/Hong_Kong"),
    ("shanghai", "Asia/Shanghai"),
    ("beijing", "Asia/Shanghai"),
    ("shenzhen", "Asia/Shanghai"),
    ("china", "Asia/Shanghai"),
    ("taipei", "Asia/Taipei"),
    ("seoul", "Asia/Seoul"),
    ("tokyo", "Asia/Tokyo"),
    ("japan", "Asia/Tokyo"),
    ("osaka", "Asia/Tokyo"),
    ("perth", "Australia/Perth"),
    ("adelaide", "Australia/Adelaide"),
    ("brisbane", "Australia/Brisbane"),
    ("sydney", "Australia/Sydney"),
    ("melbourne", "Australia/Melbourne"),
    ("auckland", "Pacific/Auckland"),
    ("wellington", "Pacific/Auckland"),
];

/// A zone's UTC offset at an instant.
#[derive(Debug, Clone)]
pub struct Zone {
    pub name: String,
    /// (transition unix time, index into `types`)
    transitions: Vec<(i64, usize)>,
    /// (utc offset seconds, abbreviation)
    types: Vec<(i32, String)>,
    footer: Option<Rule>,
}

#[derive(Debug, Clone)]
struct Rule {
    std_abbr: String,
    std_off: i32,
    dst: Option<(String, i32, RuleDate, i32, RuleDate, i32)>,
}

#[derive(Debug, Clone, Copy)]
struct RuleDate {
    month: u32,
    week: u32,
    weekday: u32,
}

fn be32(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be64(b: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

impl Zone {
    pub fn load(name: &str) -> Option<Self> {
        Self::load_from(Path::new(ZONEINFO), name)
    }

    pub fn load_from(root: &Path, name: &str) -> Option<Self> {
        if name.contains("..") {
            return None;
        }
        let bytes = std::fs::read(root.join(name)).ok()?;
        Self::parse(name, &bytes)
    }

    pub fn parse(name: &str, b: &[u8]) -> Option<Self> {
        if b.get(0..4)? != b"TZif" {
            return None;
        }
        let version = b[4];
        let counts = |at: usize| -> Option<[usize; 6]> {
            let mut c = [0usize; 6];
            for (i, slot) in c.iter_mut().enumerate() {
                *slot = be32(b, at + 20 + i * 4)? as usize;
            }
            Some(c)
        };
        // [isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt]
        let c1 = counts(0)?;
        let v1_len = c1[3] * 4 + c1[3] + c1[4] * 6 + c1[5] + c1[2] * 8 + c1[1] + c1[0];
        let (start, c, time_size) = if version >= b'2' {
            let at = 44 + v1_len;
            (at, counts(at)?, 8)
        } else {
            (0, c1, 4)
        };
        let mut p = start + 44;
        let mut times = Vec::with_capacity(c[3]);
        for i in 0..c[3] {
            times.push(if time_size == 8 {
                be64(b, p + i * 8)?
            } else {
                be32(b, p + i * 4)? as i64
            });
        }
        p += c[3] * time_size;
        let idx: Vec<usize> = b.get(p..p + c[3])?.iter().map(|&x| x as usize).collect();
        p += c[3];
        let mut raw_types = Vec::with_capacity(c[4]);
        for i in 0..c[4] {
            let at = p + i * 6;
            raw_types.push((be32(b, at)?, *b.get(at + 5)? as usize));
        }
        p += c[4] * 6;
        let chars = b.get(p..p + c[5])?;
        let types = raw_types
            .into_iter()
            .map(|(off, ai)| {
                let end = chars[ai.min(chars.len())..].iter().position(|&x| x == 0).unwrap_or(0) + ai;
                (
                    off,
                    String::from_utf8_lossy(&chars[ai.min(chars.len())..end.min(chars.len())]).into_owned(),
                )
            })
            .collect::<Vec<_>>();
        p += c[5] + c[2] * (time_size + 4) + c[1] + c[0];
        let footer = if version >= b'2' {
            let rest = b.get(p..).unwrap_or(&[]);
            let text = String::from_utf8_lossy(rest);
            let line = text.trim_matches('\n').lines().next().unwrap_or("").to_string();
            parse_rule(&line)
        } else {
            None
        };
        Some(Self {
            name: name.to_string(),
            transitions: times.into_iter().zip(idx).collect(),
            types,
            footer,
        })
    }

    /// (offset seconds, abbreviation) at unix time `t`.
    pub fn offset_at(&self, t: i64) -> (i32, String) {
        let last = self.transitions.last().map(|(time, _)| *time);
        if let (Some(rule), Some(last)) = (&self.footer, last) {
            if t >= last {
                return rule.offset_at(t);
            }
        }
        if self.transitions.is_empty() {
            if let Some(rule) = &self.footer {
                return rule.offset_at(t);
            }
        }
        let i = self.transitions.partition_point(|(time, _)| *time <= t);
        let ty = if i == 0 {
            // Before the first transition: the first non-DST type.
            0
        } else {
            self.transitions[i - 1].1
        };
        self.types
            .get(ty)
            .cloned()
            .or_else(|| self.types.first().cloned())
            .unwrap_or((0, "UTC".into()))
    }
}

/// POSIX TZ string: `EST5EDT,M3.2.0,M11.1.0`, `<+03>-3`, `EET-2EEST,M3.5.0/3,M10.5.0/4`.
fn parse_rule(s: &str) -> Option<Rule> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut p = 0usize;
    let b = s.as_bytes();
    let abbr = |p: &mut usize| -> Option<String> {
        if b.get(*p) == Some(&b'<') {
            let end = s[*p..].find('>')? + *p;
            let a = s[*p + 1..end].to_string();
            *p = end + 1;
            Some(a)
        } else {
            let start = *p;
            while *p < b.len() && b[*p].is_ascii_alphabetic() {
                *p += 1;
            }
            (*p > start).then(|| s[start..*p].to_string())
        }
    };
    let offset = |p: &mut usize| -> Option<i32> {
        let start = *p;
        while *p < b.len() && (b[*p].is_ascii_digit() || b"+-:".contains(&b[*p])) {
            *p += 1;
        }
        hms(&s[start..*p])
    };
    let std_abbr = abbr(&mut p)?;
    // POSIX offsets are west-positive.
    let std_off = -offset(&mut p)?;
    if p >= b.len() {
        return Some(Rule {
            std_abbr,
            std_off,
            dst: None,
        });
    }
    let dst_abbr = abbr(&mut p)?;
    let dst_off = if b.get(p) == Some(&b',') {
        std_off + 3600
    } else {
        -offset(&mut p)?
    };
    let rest = s[p..].trim_start_matches(',');
    let mut parts = rest.split(',');
    let (start, start_t) = rule_date(parts.next()?)?;
    let (end, end_t) = rule_date(parts.next()?)?;
    Some(Rule {
        std_abbr,
        std_off,
        dst: Some((dst_abbr, dst_off, start, start_t, end, end_t)),
    })
}

fn hms(s: &str) -> Option<i32> {
    let (sign, s) = match s.strip_prefix('-') {
        Some(r) => (-1, r),
        None => (1, s.trim_start_matches('+')),
    };
    let mut it = s.split(':');
    let h: i32 = it.next()?.parse().ok()?;
    let m: i32 = it.next().map(|x| x.parse().unwrap_or(0)).unwrap_or(0);
    let sec: i32 = it.next().map(|x| x.parse().unwrap_or(0)).unwrap_or(0);
    Some(sign * (h * 3600 + m * 60 + sec))
}

/// `M3.2.0/2` → (date, seconds after local midnight).
fn rule_date(s: &str) -> Option<(RuleDate, i32)> {
    let (date, time) = match s.split_once('/') {
        Some((d, t)) => (d, hms(t)?),
        None => (s, 7200),
    };
    let m = date.strip_prefix('M')?;
    let mut it = m.split('.');
    Some((
        RuleDate {
            month: it.next()?.parse().ok()?,
            week: it.next()?.parse().ok()?,
            weekday: it.next()?.parse().ok()?,
        },
        time,
    ))
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 0 = Sunday.
pub fn weekday(days: i64) -> u32 {
    ((days + 4).rem_euclid(7)) as u32
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

impl RuleDate {
    /// Local day number of this rule date in year `y`.
    fn day(&self, y: i64) -> i64 {
        let first = days_from_civil(y, self.month, 1);
        let wd = weekday(first);
        let mut d = 1 + (self.weekday + 7 - wd) % 7 + (self.week.saturating_sub(1)) * 7;
        let dim = days_in_month(y, self.month);
        while d > dim {
            d -= 7;
        }
        days_from_civil(y, self.month, d)
    }
}

impl Rule {
    fn offset_at(&self, t: i64) -> (i32, String) {
        let Some((dst_abbr, dst_off, start, start_t, end, end_t)) = &self.dst else {
            return (self.std_off, self.std_abbr.clone());
        };
        let (y, _, _) = civil_from_days((t + self.std_off as i64).div_euclid(86_400));
        // Transition instants in UTC: start is in standard time, end in DST.
        let s = start.day(y) * 86_400 + *start_t as i64 - self.std_off as i64;
        let e = end.day(y) * 86_400 + *end_t as i64 - *dst_off as i64;
        let in_dst = if s < e { t >= s && t < e } else { t >= s || t < e };
        if in_dst {
            (*dst_off, dst_abbr.clone())
        } else {
            (self.std_off, self.std_abbr.clone())
        }
    }
}

/// The Mac's own zone: `$TZ`, else the `/etc/localtime` link.
pub fn local_zone_name() -> String {
    if let Ok(tz) = std::env::var("TZ") {
        let tz = tz.trim_start_matches(':');
        if !tz.is_empty() && !tz.starts_with('/') {
            return tz.to_string();
        }
    }
    std::fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p: PathBuf| {
            let s = p.to_string_lossy().to_string();
            s.split_once("zoneinfo/").map(|(_, z)| z.to_string())
        })
        .unwrap_or_else(|| "UTC".into())
}

/// Resolve what the user typed to a zone name.
pub fn place(word: &str) -> Option<(String, String)> {
    let w = word.trim().trim_end_matches(" time").trim().to_lowercase();
    if w.is_empty() {
        return None;
    }
    if let Some((_, z)) = PLACES.iter().find(|(p, _)| *p == w) {
        return Some((z.to_string(), title_case(&w)));
    }
    // A zone name or its last part: `europe/kyiv`, `tokyo`.
    for (_, z) in PLACES {
        let last = z.rsplit('/').next().unwrap_or(z).replace('_', " ").to_lowercase();
        if last == w || z.to_lowercase() == w {
            return Some((z.to_string(), title_case(&last)));
        }
    }
    None
}

/// Abbreviations shown in capitals (`PST`, `NYC`); everything else is a name.
const CAPS: &[&str] = &[
    "utc", "gmt", "z", "pst", "pdt", "pt", "mst", "mdt", "mt", "cst", "cdt", "ct", "est", "edt", "et", "akst", "hst",
    "bst", "cet", "cest", "eet", "eest", "msk", "ist", "jst", "kst", "hkt", "sgt", "aest", "aedt", "nzst", "nzdt",
    "nyc", "ny", "la", "sf", "dc", "uk",
];

fn title_case(s: &str) -> String {
    if CAPS.contains(&s) {
        return s.to_uppercase();
    }
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `3pm`, `3:30 pm`, `15:00`, `noon` → minutes after midnight.
fn parse_clock(s: &str) -> Option<u32> {
    let s = s.trim().to_lowercase().replace(' ', "");
    match s.as_str() {
        "noon" => return Some(12 * 60),
        "midnight" => return Some(0),
        _ => {}
    }
    let (body, ampm) = if let Some(b) = s.strip_suffix("am") {
        (b, Some(false))
    } else if let Some(b) = s.strip_suffix("pm") {
        (b, Some(true))
    } else {
        (s.as_str(), None)
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None if ampm.is_some() => (body.parse::<u32>().ok()?, 0),
        // A bare number is only a time with a colon or am/pm.
        None => return None,
    };
    if m >= 60 {
        return None;
    }
    let h = match ampm {
        Some(pm) => {
            if !(1..=12).contains(&h) {
                return None;
            }
            (h % 12) + if pm { 12 } else { 0 }
        }
        None if h < 24 => h,
        None => return None,
    };
    Some(h * 60 + m)
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimeRow {
    pub title: String,
    pub subtitle: String,
    /// Text to copy.
    pub copy: String,
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

fn fmt_offset(off: i32) -> String {
    let sign = if off < 0 { '-' } else { '+' };
    let a = off.abs();
    if a % 3600 == 0 {
        format!("UTC{sign}{}", a / 3600)
    } else {
        format!("UTC{sign}{}:{:02}", a / 3600, (a % 3600) / 60)
    }
}

fn clock(t: i64, off: i32) -> (String, String) {
    let local = t + off as i64;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    (
        format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60),
        DAYS[weekday(days) as usize].to_string(),
    )
}

/// Answer a time-zone question, or `None` when `query` is not one.
pub fn answer(query: &str, now: i64, root: &Path, local: &str) -> Option<TimeRow> {
    answer_with(query, now, local, &|name: &str| {
        Zone::load_from(root, name).map(std::sync::Arc::new)
    })
}

/// [`answer`] with a zone loader (the engine caches parsed zones).
pub fn answer_with(
    query: &str,
    now: i64,
    local: &str,
    load: &dyn Fn(&str) -> Option<std::sync::Arc<Zone>>,
) -> Option<TimeRow> {
    let q = query.trim().to_lowercase();
    // "time in tokyo", "tokyo time", "now in london", "what time is it in paris"
    let place_only = q
        .strip_prefix("what time is it in ")
        .or_else(|| q.strip_prefix("time in "))
        .or_else(|| q.strip_prefix("now in "))
        .or_else(|| q.strip_suffix(" time"));
    if let Some(p) = place_only {
        let (zone, label) = place(p)?;
        let z = load(&zone)?;
        let (off, abbr) = z.offset_at(now);
        let (hm, day) = clock(now, off);
        let lz = load(local);
        let (loff, _) = lz.map(|l| l.offset_at(now)).unwrap_or((0, String::new()));
        let diff = (off - loff) as f64 / 3600.0;
        let rel = if diff == 0.0 {
            "same as you".to_string()
        } else {
            format!("{}{} h from you", if diff > 0.0 { "+" } else { "" }, trim_num(diff))
        };
        return Some(TimeRow {
            title: format!("{hm} {day} · {label}"),
            subtitle: format!("{abbr} ({}) · {rel}", fmt_offset(off)),
            copy: format!("{hm} {label}"),
        });
    }
    // "<time> [zone] (in|to) <zone>", or "<time> <zone>" (to your zone).
    let (lhs, target) = match [" in ", " to "].iter().find_map(|s| q.split_once(s)) {
        Some((l, r)) => (l.trim().to_string(), Some(r.trim().to_string())),
        None => (q.clone(), None),
    };
    // Split the time from the source place: longest clock prefix wins.
    let words: Vec<&str> = lhs.split_whitespace().collect();
    let (mins, src_words) = (1..=words.len().min(2))
        .rev()
        .find_map(|n| parse_clock(&words[..n].join(" ")).map(|m| (m, &words[n..])))?;
    let src_text = src_words.join(" ");
    let (src_zone, src_label) = if src_text.is_empty() {
        (local.to_string(), "your time".to_string())
    } else {
        place(&src_text)?
    };
    let (dst_zone, dst_label) = match &target {
        Some(t) => place(t)?,
        None if !src_text.is_empty() => (local.to_string(), "your time".to_string()),
        None => return None,
    };
    let src = load(&src_zone)?;
    let dst = load(&dst_zone)?;
    // Today's date in the source zone, at the typed wall time.
    let (soff_now, _) = src.offset_at(now);
    let src_day = (now + soff_now as i64).div_euclid(86_400);
    let guess = src_day * 86_400 + mins as i64 * 60 - soff_now as i64;
    // Re-check the offset at that instant (DST change today).
    let (soff, sabbr) = src.offset_at(guess);
    let t = src_day * 86_400 + mins as i64 * 60 - soff as i64;
    let (doff, dabbr) = dst.offset_at(t);
    let (shm, sday) = clock(t, soff);
    let (dhm, dday) = clock(t, doff);
    let day_note = if dday != sday {
        format!(" {dday}")
    } else {
        String::new()
    };
    Some(TimeRow {
        title: format!("{dhm}{day_note} · {dst_label}"),
        subtitle: format!("= {shm} {sday} {src_label} ({sabbr}) · {dabbr} {}", fmt_offset(doff)),
        copy: format!("{dhm} {dst_label}"),
    })
}

fn trim_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> Option<&'static Path> {
        let p = Path::new(ZONEINFO);
        p.join("Europe/Kyiv").exists().then_some(p)
    }

    // 2026-07-01 12:00:00 UTC and 2026-01-15 12:00:00 UTC.
    const SUMMER: i64 = 1_782_907_200;
    const WINTER: i64 = 1_768_478_400;

    #[test]
    fn civil_dates_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2026, 10, 21)), (2026, 10, 21));
        assert_eq!(weekday(days_from_civil(2026, 10, 21)), 3); // Wednesday
        assert_eq!(SUMMER.div_euclid(86_400), days_from_civil(2026, 7, 1));
        assert_eq!(WINTER.div_euclid(86_400), days_from_civil(2026, 1, 15));
    }

    #[test]
    fn posix_rules() {
        let r = parse_rule("EST5EDT,M3.2.0,M11.1.0").unwrap();
        assert_eq!(r.offset_at(SUMMER), (-4 * 3600, "EDT".into()));
        assert_eq!(r.offset_at(WINTER), (-5 * 3600, "EST".into()));
        let r = parse_rule("EET-2EEST,M3.5.0/3,M10.5.0/4").unwrap();
        assert_eq!(r.offset_at(SUMMER).0, 3 * 3600);
        assert_eq!(r.offset_at(WINTER).0, 2 * 3600);
        let r = parse_rule("<+0530>-5:30").unwrap();
        assert_eq!(r.offset_at(SUMMER), (19800, "+0530".into()));
        // Southern hemisphere: DST in January.
        let r = parse_rule("AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        assert_eq!(r.offset_at(WINTER).0, 11 * 3600);
        assert_eq!(r.offset_at(SUMMER).0, 10 * 3600);
    }

    #[test]
    fn clocks() {
        assert_eq!(parse_clock("3pm"), Some(15 * 60));
        assert_eq!(parse_clock("12am"), Some(0));
        assert_eq!(parse_clock("12pm"), Some(12 * 60));
        assert_eq!(parse_clock("9:30 am"), Some(9 * 60 + 30));
        assert_eq!(parse_clock("15:45"), Some(15 * 60 + 45));
        assert_eq!(parse_clock("noon"), Some(720));
        assert_eq!(parse_clock("15"), None);
        assert_eq!(parse_clock("13pm"), None);
    }

    #[test]
    fn real_zones() {
        let Some(root) = root() else { return };
        let kyiv = Zone::load_from(root, "Europe/Kyiv").unwrap();
        assert_eq!(kyiv.offset_at(SUMMER).0, 3 * 3600);
        assert_eq!(kyiv.offset_at(WINTER).0, 2 * 3600);
        let la = Zone::load_from(root, "America/Los_Angeles").unwrap();
        assert_eq!(la.offset_at(SUMMER).0, -7 * 3600);
        assert_eq!(la.offset_at(WINTER).0, -8 * 3600);
        // Far future relies on the footer rule.
        assert_eq!(la.offset_at(4_102_444_800 + 180 * 86_400).0, -7 * 3600); // mid-2100

        let row = answer("3pm pst in kyiv", WINTER, root, "America/New_York").unwrap();
        assert!(row.title.starts_with("01:00 Fri · Kyiv"), "{row:?}");
        let row = answer("time in tokyo", SUMMER, root, "America/New_York").unwrap();
        assert!(row.title.starts_with("21:00 Wed · Tokyo"), "{row:?}");
        assert!(row.subtitle.contains("+13 h from you"), "{row:?}");
        let row = answer("9am london", SUMMER, root, "America/New_York").unwrap();
        assert!(row.title.starts_with("04:00 · your time"), "{row:?}");
        assert!(answer("hello", SUMMER, root, "UTC").is_none());
        assert!(answer("15", SUMMER, root, "UTC").is_none());
        assert!(answer("3pm in atlantis", SUMMER, root, "UTC").is_none());
    }
}
