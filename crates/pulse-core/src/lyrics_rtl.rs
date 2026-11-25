//! lyrics-rtl.js
use std::sync::LazyLock;

use regex::Regex;

static TAG: LazyLock<Regex> = LazyLock::new(|| crate::js::re(r"<[^>]*>"));
static LETTER: LazyLock<Regex> = LazyLock::new(|| crate::js::re(r"\p{L}"));

fn is_rtl(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
}

/// True when the first letter of `text` (HTML tags ignored) is a right-to-left letter.
pub fn is_rtl_text(text: &str) -> bool {
    let clean = TAG.replace_all(text, "");
    LETTER
        .find(&clean)
        .and_then(|m| m.as_str().chars().next())
        .is_some_and(is_rtl)
}
