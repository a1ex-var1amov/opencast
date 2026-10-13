//! Inline calculator: `2 * (3 + 4) ^ 2 / 7` → `14`.
//!
//! A small recursive-descent parser so results use real (float) division and
//! there is no dependency for something this size.
//! Supports + - * / % ^, unary minus, parentheses, `x` as multiply, and `,`
//! as a thousands separator.

pub fn eval(input: &str) -> Option<String> {
    let s = input.trim();
    // Only treat it as maths if it looks like maths: digits plus an operator.
    if !s.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    if !s.chars().any(|c| "+-*/%^x×÷(".contains(c)) {
        return None;
    }
    if !s.chars().all(|c| c.is_ascii_digit() || " .,+-*/%^()x×÷".contains(c)) {
        return None;
    }
    let tokens = tokenize(s)?;
    let mut p = Parser { t: &tokens, i: 0 };
    let v = p.expr()?;
    if p.i != tokens.len() || !v.is_finite() {
        return None;
    }
    Some(format_number(v))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Num(f64),
    Op(char),
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
            ' ' => i += 1,
            '0'..='9' | '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == ',') {
                    i += 1;
                }
                let n: String = chars[start..i].iter().filter(|c| **c != ',').collect();
                out.push(Tok::Num(n.parse().ok()?));
            }
            '+' | '-' | '*' | '/' | '%' | '^' => {
                out.push(Tok::Op(c));
                i += 1;
            }
            'x' | '×' => {
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
    fn peek(&self) -> Option<Tok> {
        self.t.get(self.i).copied()
    }

    // expr := term (('+'|'-') term)*
    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek() {
            self.i += 1;
            let r = self.term()?;
            v = if op == '+' { v + r } else { v - r };
        }
        Some(v)
    }

    // term := unary (('*'|'/'|'%') unary)*
    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        while let Some(Tok::Op(op @ ('*' | '/' | '%'))) = self.peek() {
            self.i += 1;
            let r = self.unary()?;
            v = match op {
                '*' => v * r,
                '/' => v / r,
                _ => v % r,
            };
        }
        Some(v)
    }

    // unary := '-' unary | power
    fn unary(&mut self) -> Option<f64> {
        if let Some(Tok::Op('-')) = self.peek() {
            self.i += 1;
            return Some(-self.unary()?);
        }
        if let Some(Tok::Op('+')) = self.peek() {
            self.i += 1;
            return self.unary();
        }
        self.power()
    }

    // power := atom ('^' unary)?   (right-associative)
    fn power(&mut self) -> Option<f64> {
        let base = self.atom()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.i += 1;
            let exp = self.unary()?;
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn atom(&mut self) -> Option<f64> {
        match self.peek()? {
            Tok::Num(n) => {
                self.i += 1;
                Some(n)
            }
            Tok::LParen => {
                self.i += 1;
                let v = self.expr()?;
                if self.peek()? != Tok::RParen {
                    return None;
                }
                self.i += 1;
                Some(v)
            }
            _ => None,
        }
    }
}

fn format_number(v: f64) -> String {
    if v.fract().abs() < 1e-9 && v.abs() < 1e15 {
        return format!("{}", v.round() as i64);
    }
    let s = format!("{v:.10}");
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
    }

    #[test]
    fn not_maths() {
        assert_eq!(eval("safari"), None);
        assert_eq!(eval("42"), None);
        assert_eq!(eval("1/0"), None);
        assert_eq!(eval("(1+2"), None);
        assert_eq!(eval("ex 1+2"), None);
    }
}
