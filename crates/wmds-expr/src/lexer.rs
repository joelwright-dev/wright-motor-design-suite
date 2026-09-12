use wmds_units::Quantity;

use crate::parser::ParseError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Num(Quantity),
    Str(String),
    Ident(String),
    /// Punctuation and operators: ( ) [ ] , . + - * / ^ < <= > >= == != ->
    Op(&'static str),
}

pub(crate) fn lex(src: &str) -> Result<Vec<Tok>, ParseError> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // exponent
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let save = i;
                i += 1;
                if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                if i < chars.len() && chars[i].is_ascii_digit() {
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                } else {
                    i = save;
                }
            }
            let text: String = chars[start..i].iter().collect();
            let value: f64 = text
                .parse()
                .map_err(|_| ParseError::BadNumber(text.clone()))?;
            // Optional unit directly after the number: `380 mm`, `1550 kg/m^3`, `0.18 kg*m^2`.
            let unit = lex_unit_suffix(&chars, &mut i);
            let q = match unit {
                Some(u) => {
                    Quantity::from_unit(value, &u).map_err(|e| ParseError::Unit(e.to_string()))?
                }
                None => Quantity::dimensionless(value),
            };
            out.push(Tok::Num(q));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect()));
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let mut s = String::new();
            loop {
                if i >= chars.len() {
                    return Err(ParseError::UnterminatedString);
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                }
                s.push(chars[i]);
                i += 1;
            }
            out.push(Tok::Str(s));
            continue;
        }
        let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
        let op2: Option<&'static str> = match two.as_str() {
            "<=" => Some("<="),
            ">=" => Some(">="),
            "==" => Some("=="),
            "!=" => Some("!="),
            "->" => Some("->"),
            "&&" => Some("and"),
            "||" => Some("or"),
            _ => None,
        };
        if let Some(op) = op2 {
            out.push(Tok::Op(op));
            i += 2;
            continue;
        }
        let op1: Option<&'static str> = match c {
            '(' => Some("("),
            ')' => Some(")"),
            '[' => Some("["),
            ']' => Some("]"),
            ',' => Some(","),
            '.' => Some("."),
            '+' => Some("+"),
            '-' => Some("-"),
            '*' => Some("*"),
            '/' => Some("/"),
            '^' => Some("^"),
            '<' => Some("<"),
            '>' => Some(">"),
            '=' => Some("=="),
            '!' => Some("not"),
            '|' => Some("abs"),
            _ => None,
        };
        match op1 {
            Some(op) => {
                out.push(Tok::Op(op));
                i += 1;
            }
            None => return Err(ParseError::UnexpectedChar(c)),
        }
    }
    Ok(out)
}

/// After a number, greedily consume a unit expression made of known unit symbols joined by
/// `*`, `/` and `^int`. Leaves `i` untouched if the next word is not a unit.
fn lex_unit_suffix(chars: &[char], i: &mut usize) -> Option<String> {
    let mut j = *i;
    while j < chars.len() && chars[j] == ' ' {
        j += 1;
    }
    let word = read_unit_word(chars, j)?;
    let mut unit = word.0;
    j = word.1;
    loop {
        // optional ^int
        if j < chars.len() && chars[j] == '^' {
            let mut k = j + 1;
            if k < chars.len() && chars[k] == '-' {
                k += 1;
            }
            let ds = k;
            while k < chars.len() && chars[k].is_ascii_digit() {
                k += 1;
            }
            if k > ds {
                unit.push_str(&chars[j..k].iter().collect::<String>());
                j = k;
            }
        }
        // optional * or / followed by a unit word (no spaces allowed inside a unit expression)
        if j < chars.len()
            && (chars[j] == '*' || chars[j] == '/')
            && let Some((w, next)) = read_unit_word(chars, j + 1)
        {
            unit.push(chars[j]);
            unit.push_str(&w);
            j = next;
            continue;
        }
        break;
    }
    *i = j;
    Some(unit)
}

fn read_unit_word(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut j = start;
    if j < chars.len() && chars[j] == '%' {
        return Some(("%".to_string(), j + 1));
    }
    while j < chars.len() && chars[j].is_alphabetic() {
        j += 1;
    }
    if j == start {
        return None;
    }
    let w: String = chars[start..j].iter().collect();
    if wmds_units::is_unit_symbol(&w) && !is_keyword(&w) {
        Some((w, j))
    } else {
        None
    }
}

fn is_keyword(w: &str) -> bool {
    matches!(
        w,
        "if" | "then" | "else" | "and" | "or" | "not" | "in" | "true" | "false"
    )
}
