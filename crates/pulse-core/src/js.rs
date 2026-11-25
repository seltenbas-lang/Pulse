//! JS semantics the ports share: regex classes, whitespace, truthiness, String() and Number().
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

const WS_INNER: &str = r"\t\n\x0B\x0C\r \u{A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}";

pub fn is_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\x0B' | '\x0C' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_ws)
}

/// `s.normalize('NFKD')`.
pub fn nfkd(s: &str) -> String {
    s.nfkd().collect()
}

/// `.replace(/[\u0300-\u036f]/g, '')`.
pub fn strip_marks(s: &str) -> String {
    s.chars()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect()
}

/// `s.replace(/\s+/g, ' ')`.
pub fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if is_ws(c) {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// Length in UTF-16 code units, as JS `.length`.
pub fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Translates a JS regex source (non-`u` escapes: ASCII `\b \d \w`, JS `\s`, `.` without line
/// terminators) to Rust `regex` syntax. `flags` may contain `i`.
pub fn translate(pattern: &str, flags: &str) -> String {
    let mut out = String::with_capacity(pattern.len() * 2);
    if flags.contains('i') {
        out.push_str("(?i)");
    }
    let mut chars = pattern.chars().peekable();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let Some(n) = chars.next() else {
                    out.push_str(r"\\");
                    break;
                };
                match (n, in_class) {
                    ('s', false) => out.push_str(&format!("[{WS_INNER}]")),
                    ('s', true) => out.push_str(WS_INNER),
                    ('S', _) => out.push_str(&format!("[^{WS_INNER}]")),
                    ('d', false) => out.push_str("[0-9]"),
                    ('d', true) => out.push_str("0-9"),
                    ('D', _) => out.push_str("[^0-9]"),
                    ('w', false) => out.push_str("[A-Za-z0-9_]"),
                    ('w', true) => out.push_str("A-Za-z0-9_"),
                    ('W', _) => out.push_str("[^A-Za-z0-9_]"),
                    ('b', false) => out.push_str(r"(?-u:\b)"),
                    ('B', false) => out.push_str(r"(?-u:\B)"),
                    ('/', _) => out.push('/'),
                    ('-', _) => out.push_str(r"\-"),
                    (n, _) => {
                        out.push('\\');
                        out.push(n);
                    }
                }
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
                if chars.peek() == Some(&'^') {
                    out.push(chars.next().unwrap_or('^'));
                }
                // a leading `]` is literal in Rust but an empty class in JS; JS sources here never use it
            }
            '[' => out.push_str(r"\["),
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            '.' if !in_class => out.push_str(r"[^\n\r\u{2028}\u{2029}]"),
            '&' | '~' if in_class => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out
}

/// A Rust regex for the JS regex `/pattern/flags`.
pub fn re(pattern: &str) -> regex::Regex {
    re_flags(pattern, "")
}

pub fn re_flags(pattern: &str, flags: &str) -> regex::Regex {
    let p = translate(pattern, flags);
    // INFALLIBLE: every pattern is a literal from the original source, compiled by the tests.
    regex::Regex::new(&p).unwrap_or_else(|e| panic!("bad pattern /{pattern}/{flags}: {e}"))
}

/// JS truthiness of a JSON value (`undefined` is `Value::Null`).
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// The value at `key` when it is truthy, as `a.key || fallback` reads it.
pub fn or<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|x| truthy(x))
}

/// JS `Number#toString` (ECMAScript Number::toString, radix 10).
pub fn num_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if x == 0.0 {
        return "0".into();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // shortest round-trip digits and exponent: "d.ddde±x"
    let e = format!("{:e}", x.abs());
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().unwrap_or(0) + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let es = if e >= 0 {
            format!("+{e}")
        } else {
            e.to_string()
        };
        if k == 1 {
            format!("{digits}e{es}")
        } else {
            format!("{}.{}e{es}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

fn is_array_index(k: &str) -> bool {
    !k.is_empty()
        && k.bytes().all(|b| b.is_ascii_digit())
        && (k == "0" || !k.starts_with('0'))
        && k.parse::<u64>().is_ok_and(|n| n < 4_294_967_295)
}

/// Object keys in JS property order: array indexes ascending, then the rest as inserted.
pub fn key_order(o: &serde_json::Map<String, Value>) -> Vec<&String> {
    let (mut idx, rest): (Vec<&String>, Vec<&String>) = o.keys().partition(|k| is_array_index(k));
    idx.sort_by_key(|k| k.parse::<u64>().unwrap_or(0));
    idx.extend(rest);
    idx
}

fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn stringify_into(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let x = n.as_f64().unwrap_or(f64::NAN);
            out.push_str(&if x.is_finite() {
                num_to_string(x)
            } else {
                "null".into()
            });
        }
        Value::String(s) => quote(s, out),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                stringify_into(x, out);
            }
            out.push(']');
        }
        Value::Object(o) => {
            out.push('{');
            for (i, k) in key_order(o).into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                quote(k, out);
                out.push(':');
                stringify_into(&o[k], out);
            }
            out.push('}');
        }
    }
}

/// `JSON.stringify(v)`.
pub fn stringify(v: &Value) -> String {
    let mut out = String::new();
    stringify_into(v, &mut out);
    out
}

/// `String(v)` (objects and arrays are not expected here).
pub fn to_string(v: &Value) -> String {
    match v {
        Value::Null => "undefined".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => num_to_string(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .map(|x| {
                if x.is_null() {
                    String::new()
                } else {
                    to_string(x)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// `String(v || '')`.
pub fn str_or_empty(v: Option<&Value>) -> String {
    v.filter(|x| truthy(x)).map(to_string).unwrap_or_default()
}

/// `Number(v)`; `None` (undefined) is NaN.
pub fn to_number(v: Option<&Value>) -> f64 {
    match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => {
            let t = trim(s);
            if t.is_empty() {
                0.0
            } else if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                i64::from_str_radix(h, 16)
                    .map(|x| x as f64)
                    .unwrap_or(f64::NAN)
            } else if t == "Infinity" || t == "+Infinity" {
                f64::INFINITY
            } else if t == "-Infinity" {
                f64::NEG_INFINITY
            } else if t.contains(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E') {
                f64::NAN
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        Some(Value::Array(a)) if a.is_empty() => 0.0,
        Some(Value::Array(a)) if a.len() == 1 => to_number(a.first()),
        Some(_) => f64::NAN,
    }
}

/// `Number(v) || 0`.
pub fn num_or_zero(v: Option<&Value>) -> f64 {
    let x = to_number(v);
    if x.is_nan() { 0.0 } else { x }
}

/// `Number.isFinite(v)`: only real numbers.
pub fn finite(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|x| x.is_finite())
}

/// `Math.round`.
pub fn round(x: f64) -> f64 {
    (x + 0.5).floor() + 0.0
}

/// `parseInt(s, 10)`.
pub fn parse_int(s: &str) -> f64 {
    let t = s.trim_start_matches(is_ws);
    let (neg, rest) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    let x: f64 = digits.parse().unwrap_or(f64::NAN);
    if neg { -x } else { x }
}

/// `str.split(regex)` for a regex without capture groups.
pub fn split<'a>(re: &regex::Regex, s: &'a str) -> Vec<&'a str> {
    re.split(s).collect()
}

/// `[...new Set(items)]`.
pub fn uniq_ordered<T: PartialEq + Clone>(items: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for x in items {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    out
}

/// `s.slice(0, n)` in UTF-16 units; never splits a surrogate pair (JS would leave a lone half).
pub fn slice16(s: &str, n: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        units += c.len_utf16();
        if units > n {
            return &s[..i];
        }
    }
    s
}

/// `Array.isArray(v) ? v : []`.
pub fn list(v: Option<&Value>) -> &[Value] {
    v.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// `String(v == null ? '' : v)`.
pub fn str_nullish(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(x) => to_string(x),
    }
}

/// `a.key || fallback`, keeping the value's own type.
pub fn or_default(v: &Value, key: &str, fallback: Value) -> Value {
    or(v, key).cloned().unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_js_escapes() {
        let r = re(r"\bfoo\b");
        assert!(r.is_match("a foo b"));
        assert!(re(r"^\d+$").is_match("123") && !re(r"^\d+$").is_match("٣"));
        assert!(re("a.b").is_match("axb") && !re("a.b").is_match("a\rb"));
        assert!(re(r"[\s\-–—:]+").is_match("\u{FEFF}"));
    }

    #[test]
    fn numbers_like_js() {
        assert_eq!(num_to_string(3.0), "3");
        assert_eq!(num_to_string(1e21), "1e+21");
        assert_eq!(num_to_string(1.5e-7), "1.5e-7");
        assert_eq!(num_to_string(0.000001), "0.000001");
        assert_eq!(num_to_string(123.456), "123.456");
        assert_eq!(num_to_string(-0.1 - 0.2), "-0.30000000000000004");
        assert_eq!(
            stringify(&serde_json::json!({"b": 1.0, "2": "x\u{1}\"", "1": [true, null]})),
            r#"{"1":[true,null],"2":"x\u0001\"","b":1}"#
        );
        assert_eq!(round(-0.4).to_string(), "0");
        assert_eq!(round(2.5), 3.0);
        assert_eq!(parse_int(" 1999abc"), 1999.0);
        assert!(parse_int("abc").is_nan());
        assert_eq!(to_number(Some(&Value::from("180"))), 180.0);
    }
}
