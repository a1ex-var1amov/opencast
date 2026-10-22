//! Inline calculator: `2 * (3 + 4) ^ 2 / 7` → `14`.
//!
//! A small recursive-descent parser so results use real (float) division and
//! there is no dependency for something this size. Supports:
//! - `+ - * / % ^`, unary minus, parentheses, `x`/`×` as multiply, `÷`
//! - `,` as a thousands separator; `0x1f` hex and `0b101` binary literals
//! - functions: `sqrt sin cos tan asin acos atan ln log log2 log10 exp abs
//!   round floor ceil`; constants `pi` and `e`
//! - percentages: `15% of 80` → 12, `80 + 15%` → 92, `80 - 15%` → 68,
//!   `80 * 15%` → 12, `15%` → 0.15
//! - unit conversion: `10 km in mi`, `72 f to c`, `3 gb in mb`, `2 h in min`

pub fn eval(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has_digit = s.chars().any(|c| c.is_ascii_digit());
    let has_func = words.iter().any(|w| FUNCS.contains(w));
    let has_operator = s.chars().any(|c| "+-*/%^x×÷(".contains(c));
    // Digits are required, except for function calls on constants (`ln(e)`, `sin(pi)`),
    // so a bare `e` or `pi` typed as a search never turns into a number.
    if !(has_digit || (has_func && has_operator)) {
        return None;
    }
    if let Some((value, from, to)) = split_conversion(s) {
        let v = eval_number(value)?;
        let (out, label) = convert(v.n, from, to)?;
        return Some(format!("{} {label}", format_number(out)));
    }
    // Only treat it as maths if it looks like maths: digits plus an operator,
    // a function/constant, or a hex/binary literal.
    let has_word = has_func || words.iter().any(|w| *w == "pi" || *w == "e" || *w == "of");
    let has_radix = lower.contains("0x") || lower.contains("0b");
    if !(has_operator || has_word || has_radix) {
        return None;
    }
    let v = eval_number(s)?;
    Some(format_number(v.n))
}

const FUNCS: &[&str] = &[
    "sqrt", "sin", "cos", "tan", "asin", "acos", "atan", "ln", "log", "log2", "log10", "exp", "abs", "round", "floor",
    "ceil",
];

/// A number, remembering whether it was written as a bare percentage.
#[derive(Debug, Clone, Copy)]
struct Val {
    n: f64,
    pct: bool,
}

fn eval_number(s: &str) -> Option<Val> {
    let tokens = tokenize(s)?;
    if tokens.is_empty() {
        return None;
    }
    let mut p = Parser { t: &tokens, i: 0 };
    let v = p.expr()?;
    if p.i != tokens.len() || !v.n.is_finite() {
        return None;
    }
    Some(v)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
    Percent,
    LParen,
    RParen,
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => i += 1,
            '0'..='9' | '.' => {
                // 0x / 0b literals
                if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'x' || chars[i + 1] == 'b') {
                    let radix = if chars[i + 1] == 'x' { 16 } else { 2 };
                    let start = i + 2;
                    let mut j = start;
                    while j < chars.len() && (chars[j].is_digit(radix) || chars[j] == '_') {
                        j += 1;
                    }
                    if j > start {
                        let digits: String = chars[start..j].iter().filter(|c| **c != '_').collect();
                        let n = i64::from_str_radix(&digits, radix).ok()?;
                        out.push(Tok::Num(n as f64));
                        i = j;
                        continue;
                    }
                    // `0x` with no digits: fall through as `0` then `x` (multiply).
                }
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == ',') {
                    i += 1;
                }
                let n: String = chars[start..i].iter().filter(|c| **c != ',').collect();
                out.push(Tok::Num(n.parse().ok()?));
            }
            '+' | '-' | '*' | '/' | '^' => {
                out.push(Tok::Op(c));
                i += 1;
            }
            '%' => {
                out.push(Tok::Percent);
                i += 1;
            }
            '×' => {
                out.push(Tok::Op('*'));
                i += 1;
            }
            '÷' => {
                out.push(Tok::Op('/'));
                i += 1;
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            // `2x3`, `3 x 4`: a lone x (not part of a word) multiplies.
            'x' | 'X' if !chars.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic()) => {
                out.push(Tok::Op('*'));
                i += 1;
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric()) {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
                // A lone `x` between values is multiplication (`3 x 4`).
                if word == "x" {
                    out.push(Tok::Op('*'));
                } else {
                    out.push(Tok::Ident(word));
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

struct Parser<'a> {
    t: &'a [Tok],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }

    fn peek_op(&self) -> Option<char> {
        match self.peek() {
            Some(Tok::Op(c)) => Some(*c),
            _ => None,
        }
    }

    // expr := term (('+'|'-'|'of') term)*
    fn expr(&mut self) -> Option<Val> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some(Tok::Op(op @ ('+' | '-'))) => {
                    let op = *op;
                    self.i += 1;
                    let r = self.term()?;
                    let rhs = if r.pct { v.n * r.n } else { r.n };
                    v = Val {
                        n: if op == '+' { v.n + rhs } else { v.n - rhs },
                        pct: false,
                    };
                }
                Some(Tok::Ident(w)) if w == "of" => {
                    // `15% of 80`
                    self.i += 1;
                    let r = self.term()?;
                    v = Val {
                        n: v.n * r.n,
                        pct: false,
                    };
                }
                _ => return Some(v),
            }
        }
    }

    // term := unary (('*'|'/'|'%'mod) unary)*
    fn term(&mut self) -> Option<Val> {
        let mut v = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(op @ ('*' | '/'))) => *op,
                // `%` is modulo only when an operand follows (`10 % 4`); otherwise it is a percent sign.
                Some(Tok::Percent) if self.operand_follows() => '%',
                _ => return Some(v),
            };
            self.i += 1;
            let r = self.unary()?;
            v = Val {
                n: match op {
                    '*' => v.n * r.n,
                    '/' => v.n / r.n,
                    _ => v.n % r.n,
                },
                pct: false,
            };
        }
    }

    fn operand_follows(&self) -> bool {
        matches!(
            self.t.get(self.i + 1),
            Some(Tok::Num(_)) | Some(Tok::LParen) | Some(Tok::Ident(_)) | Some(Tok::Op('-'))
        ) && !matches!(self.t.get(self.i + 1), Some(Tok::Ident(w)) if w == "of")
    }

    // unary := '-' unary | power
    fn unary(&mut self) -> Option<Val> {
        if self.peek_op() == Some('-') {
            self.i += 1;
            let v = self.unary()?;
            return Some(Val { n: -v.n, pct: v.pct });
        }
        if self.peek_op() == Some('+') {
            self.i += 1;
            return self.unary();
        }
        self.power()
    }

    // power := postfix ('^' unary)?   (right-associative)
    fn power(&mut self) -> Option<Val> {
        let base = self.postfix()?;
        if self.peek_op() == Some('^') {
            self.i += 1;
            let exp = self.unary()?;
            return Some(Val {
                n: base.n.powf(exp.n),
                pct: false,
            });
        }
        Some(base)
    }

    // postfix := atom ('%')?
    fn postfix(&mut self) -> Option<Val> {
        let v = self.atom()?;
        if matches!(self.peek(), Some(Tok::Percent)) && !self.operand_follows() {
            self.i += 1;
            return Some(Val {
                n: v.n / 100.0,
                pct: true,
            });
        }
        Some(v)
    }

    fn atom(&mut self) -> Option<Val> {
        match self.peek()?.clone() {
            Tok::Num(n) => {
                self.i += 1;
                Some(Val { n, pct: false })
            }
            Tok::LParen => {
                self.i += 1;
                let v = self.expr()?;
                if self.peek() != Some(&Tok::RParen) {
                    return None;
                }
                self.i += 1;
                Some(Val { n: v.n, pct: false })
            }
            Tok::Ident(w) => {
                self.i += 1;
                match w.as_str() {
                    "pi" => Some(Val {
                        n: std::f64::consts::PI,
                        pct: false,
                    }),
                    "e" => Some(Val {
                        n: std::f64::consts::E,
                        pct: false,
                    }),
                    name if FUNCS.contains(&name) => {
                        // `sqrt 2` and `sqrt(2)` both work.
                        let arg = if self.peek() == Some(&Tok::LParen) {
                            self.i += 1;
                            let v = self.expr()?;
                            if self.peek() != Some(&Tok::RParen) {
                                return None;
                            }
                            self.i += 1;
                            v
                        } else {
                            self.unary()?
                        };
                        let x = arg.n;
                        let n = match name {
                            "sqrt" => x.sqrt(),
                            "sin" => x.sin(),
                            "cos" => x.cos(),
                            "tan" => x.tan(),
                            "asin" => x.asin(),
                            "acos" => x.acos(),
                            "atan" => x.atan(),
                            "ln" => x.ln(),
                            "log" | "log10" => x.log10(),
                            "log2" => x.log2(),
                            "exp" => x.exp(),
                            "abs" => x.abs(),
                            "round" => x.round(),
                            "floor" => x.floor(),
                            "ceil" => x.ceil(),
                            _ => return None,
                        };
                        Some(Val { n, pct: false })
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

// ---- units ------------------------------------------------------------------

/// `<expr> <unit> in|to <unit>` → (expr, from, to).
fn split_conversion(s: &str) -> Option<(&str, &str, &str)> {
    let words: Vec<(usize, &str)> = s
        .split_whitespace()
        .map(|w| (w.as_ptr() as usize - s.as_ptr() as usize, w))
        .collect();
    if words.len() < 4 {
        return None;
    }
    let (_, to_unit) = words[words.len() - 1];
    let (_, kw) = words[words.len() - 2];
    if !(kw.eq_ignore_ascii_case("in") || kw.eq_ignore_ascii_case("to") || kw == "→") {
        return None;
    }
    let (from_at, from_unit) = words[words.len() - 3];
    if !is_unit_word(from_unit) || !is_unit_word(to_unit) {
        return None;
    }
    let value = s[..from_at].trim();
    if value.is_empty() {
        return None;
    }
    Some((value, from_unit, to_unit))
}

fn is_unit_word(w: &str) -> bool {
    unit(w).is_some() || temp_unit(w).is_some()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Dim {
    Length,
    Mass,
    Volume,
    Data,
    Time,
    Speed,
    Area,
}

/// Unit → (dimension, factor to the base unit, canonical label).
fn unit(w: &str) -> Option<(Dim, f64, &'static str)> {
    let w = w.trim().to_lowercase();
    let w = w.trim_end_matches('.');
    Some(match w {
        // length (m)
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => (Dim::Length, 0.001, "mm"),
        "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => (Dim::Length, 0.01, "cm"),
        "m" | "meter" | "meters" | "metre" | "metres" => (Dim::Length, 1.0, "m"),
        "km" | "kilometer" | "kilometers" | "kilometre" | "kilometres" => (Dim::Length, 1000.0, "km"),
        "in" | "inch" | "inches" | "\"" => (Dim::Length, 0.0254, "in"),
        "ft" | "foot" | "feet" | "'" => (Dim::Length, 0.3048, "ft"),
        "yd" | "yard" | "yards" => (Dim::Length, 0.9144, "yd"),
        "mi" | "mile" | "miles" => (Dim::Length, 1609.344, "mi"),
        "nmi" | "nauticalmile" | "nauticalmiles" => (Dim::Length, 1852.0, "nmi"),
        // mass (g)
        "mg" | "milligram" | "milligrams" => (Dim::Mass, 0.001, "mg"),
        "g" | "gram" | "grams" => (Dim::Mass, 1.0, "g"),
        "kg" | "kilogram" | "kilograms" | "kilo" | "kilos" => (Dim::Mass, 1000.0, "kg"),
        "t" | "tonne" | "tonnes" | "ton" | "tons" => (Dim::Mass, 1_000_000.0, "t"),
        "oz" | "ounce" | "ounces" => (Dim::Mass, 28.349_523_125, "oz"),
        "lb" | "lbs" | "pound" | "pounds" => (Dim::Mass, 453.592_37, "lb"),
        "st" | "stone" | "stones" => (Dim::Mass, 6_350.293_18, "st"),
        // volume (l)
        "ml" | "milliliter" | "milliliters" | "millilitre" | "millilitres" => (Dim::Volume, 0.001, "ml"),
        "l" | "liter" | "liters" | "litre" | "litres" => (Dim::Volume, 1.0, "l"),
        "gal" | "gallon" | "gallons" => (Dim::Volume, 3.785_411_784, "gal"),
        "qt" | "quart" | "quarts" => (Dim::Volume, 0.946_352_946, "qt"),
        "pt" | "pint" | "pints" => (Dim::Volume, 0.473_176_473, "pt"),
        "cup" | "cups" => (Dim::Volume, 0.236_588_236_5, "cup"),
        "floz" => (Dim::Volume, 0.029_573_529_6, "fl oz"),
        "tbsp" | "tablespoon" | "tablespoons" => (Dim::Volume, 0.014_786_764_8, "tbsp"),
        "tsp" | "teaspoon" | "teaspoons" => (Dim::Volume, 0.004_928_921_6, "tsp"),
        // data (byte), binary multiples like Finder
        "b" | "byte" | "bytes" => (Dim::Data, 1.0, "B"),
        "kb" | "kib" | "kilobyte" | "kilobytes" => (Dim::Data, 1024.0, "KB"),
        "mb" | "mib" | "megabyte" | "megabytes" => (Dim::Data, 1024.0 * 1024.0, "MB"),
        "gb" | "gib" | "gigabyte" | "gigabytes" => (Dim::Data, 1024.0 * 1024.0 * 1024.0, "GB"),
        "tb" | "tib" | "terabyte" | "terabytes" => (Dim::Data, 1024.0 * 1024.0 * 1024.0 * 1024.0, "TB"),
        "bit" | "bits" => (Dim::Data, 0.125, "bit"),
        "kbit" | "kilobit" | "kilobits" => (Dim::Data, 125.0, "kbit"),
        "mbit" | "megabit" | "megabits" => (Dim::Data, 125_000.0, "Mbit"),
        "gbit" | "gigabit" | "gigabits" => (Dim::Data, 125_000_000.0, "Gbit"),
        // time (s)
        "ms" | "millisecond" | "milliseconds" => (Dim::Time, 0.001, "ms"),
        "s" | "sec" | "secs" | "second" | "seconds" => (Dim::Time, 1.0, "s"),
        "min" | "mins" | "minute" | "minutes" => (Dim::Time, 60.0, "min"),
        "h" | "hr" | "hrs" | "hour" | "hours" => (Dim::Time, 3600.0, "h"),
        "d" | "day" | "days" => (Dim::Time, 86_400.0, "d"),
        "wk" | "week" | "weeks" => (Dim::Time, 604_800.0, "wk"),
        "y" | "yr" | "year" | "years" => (Dim::Time, 31_557_600.0, "yr"),
        // speed (m/s)
        "kph" | "kmh" | "km/h" => (Dim::Speed, 1000.0 / 3600.0, "km/h"),
        "mph" => (Dim::Speed, 1609.344 / 3600.0, "mph"),
        "mps" | "m/s" => (Dim::Speed, 1.0, "m/s"),
        "kn" | "kt" | "knot" | "knots" => (Dim::Speed, 1852.0 / 3600.0, "kn"),
        // area (m²)
        "sqm" | "m2" | "m²" => (Dim::Area, 1.0, "m²"),
        "sqft" | "ft2" | "ft²" => (Dim::Area, 0.092_903_04, "ft²"),
        "sqkm" | "km2" | "km²" => (Dim::Area, 1_000_000.0, "km²"),
        "sqmi" | "mi2" | "mi²" => (Dim::Area, 2_589_988.110_336, "mi²"),
        "acre" | "acres" => (Dim::Area, 4_046.856_422_4, "acre"),
        "ha" | "hectare" | "hectares" => (Dim::Area, 10_000.0, "ha"),
        _ => return None,
    })
}

fn temp_unit(w: &str) -> Option<&'static str> {
    match w.trim().to_lowercase().trim_start_matches('°') {
        "c" | "celsius" => Some("°C"),
        "f" | "fahrenheit" => Some("°F"),
        "k" | "kelvin" => Some("K"),
        _ => None,
    }
}

fn convert(v: f64, from: &str, to: &str) -> Option<(f64, &'static str)> {
    if let (Some(f), Some(t)) = (temp_unit(from), temp_unit(to)) {
        let c = match f {
            "°C" => v,
            "°F" => (v - 32.0) * 5.0 / 9.0,
            _ => v - 273.15,
        };
        let out = match t {
            "°C" => c,
            "°F" => c * 9.0 / 5.0 + 32.0,
            _ => c + 273.15,
        };
        return Some((out, t));
    }
    let (fd, ff, _) = unit(from)?;
    let (td, tf, label) = unit(to)?;
    if fd != td {
        return None;
    }
    Some((v * ff / tf, label))
}

fn format_number(v: f64) -> String {
    if v.fract().abs() < 1e-9 && v.abs() < 1e15 {
        return format!("{}", v.round() as i64);
    }
    // Enough digits to be useful, trimmed; small values keep significance.
    let s = if v.abs() < 1e-4 {
        format!("{v:.10e}")
    } else {
        format!("{v:.10}")
    };
    if s.contains('e') {
        return s;
    }
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::eval;

    #[test]
    fn arithmetic() {
        assert_eq!(eval("1/2").as_deref(), Some("0.5"));
        assert_eq!(eval("2 * (3 + 4) ^ 2 / 7").as_deref(), Some("14"));
        assert_eq!(eval("-3 + 5").as_deref(), Some("2"));
        assert_eq!(eval("2^3^2").as_deref(), Some("512"));
        assert_eq!(eval("1,200 x 3").as_deref(), Some("3600"));
        assert_eq!(eval("10 % 4").as_deref(), Some("2"));
        assert_eq!(eval("0.1 + 0.2").as_deref(), Some("0.3"));
        assert_eq!(eval("3 × 4 ÷ 2").as_deref(), Some("6"));
        assert_eq!(eval("2x3").as_deref(), Some("6"));
    }

    #[test]
    fn not_maths() {
        assert_eq!(eval("safari"), None);
        assert_eq!(eval("42"), None);
        assert_eq!(eval("1/0"), None);
        assert_eq!(eval("(1+2"), None);
        assert_eq!(eval("ex 1+2"), None);
        assert_eq!(eval("file 2024"), None);
        assert_eq!(eval("note 1"), None);
        assert_eq!(eval("e"), None);
        assert_eq!(eval("pi"), None);
        assert_eq!(eval("log in"), None);
        assert_eq!(eval("slack 2"), None);
    }

    #[test]
    fn functions_and_constants() {
        assert_eq!(eval("sqrt(16)").as_deref(), Some("4"));
        assert_eq!(eval("sqrt 16 + 1").as_deref(), Some("5"));
        assert_eq!(eval("round(pi * 100) / 100").as_deref(), Some("3.14"));
        assert_eq!(eval("log(1000)").as_deref(), Some("3"));
        assert_eq!(eval("ln(e)").as_deref(), Some("1"));
        assert_eq!(eval("abs(-7)").as_deref(), Some("7"));
        assert_eq!(eval("2 * pi").map(|s| s.starts_with("6.283")), Some(true));
        assert_eq!(eval("sin(0)").as_deref(), Some("0"));
        assert_eq!(eval("floor(2.7) + ceil(2.1)").as_deref(), Some("5"));
    }

    #[test]
    fn radix_literals() {
        assert_eq!(eval("0x1f + 1").as_deref(), Some("32"));
        assert_eq!(eval("0b101 * 2").as_deref(), Some("10"));
        assert_eq!(eval("0xff").as_deref(), Some("255"));
    }

    #[test]
    fn percentages() {
        assert_eq!(eval("15% of 80").as_deref(), Some("12"));
        assert_eq!(eval("80 + 15%").as_deref(), Some("92"));
        assert_eq!(eval("80 - 15%").as_deref(), Some("68"));
        assert_eq!(eval("80 * 15%").as_deref(), Some("12"));
        assert_eq!(eval("15%").as_deref(), Some("0.15"));
        assert_eq!(eval("200 % 30").as_deref(), Some("20"));
    }

    #[test]
    fn unit_conversions() {
        assert_eq!(eval("10 km in mi").map(|s| s.starts_with("6.21")), Some(true));
        assert_eq!(eval("1 mi to km").as_deref(), Some("1.609344 km"));
        assert_eq!(eval("72 f in c").map(|s| s.starts_with("22.2")), Some(true));
        assert_eq!(eval("100 c to f").as_deref(), Some("212 °F"));
        assert_eq!(eval("3 gb in mb").as_deref(), Some("3072 MB"));
        assert_eq!(eval("2 h in min").as_deref(), Some("120 min"));
        assert_eq!(eval("2.5 * 2 kg in lb").map(|s| s.starts_with("11.02")), Some(true));
        assert_eq!(eval("1 kg in min"), None);
        assert_eq!(eval("5 in"), None);
    }
}
