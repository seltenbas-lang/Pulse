//! search-text.js
use crate::js;

fn is_mark(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036f}' | '\u{0591}'..='\u{05c7}' | '\u{0610}'..='\u{061a}'
        | '\u{064b}'..='\u{065f}' | '\u{0670}' | '\u{06d6}'..='\u{06ed}' | '\u{0640}')
}

/// Folds text for search: NFKD, drops combining marks and tatweel, unifies Arabic/Persian letters,
/// lowercases and collapses whitespace.
pub fn normalize_search(value: &str) -> String {
    let folded: String = js::nfkd(value)
        .chars()
        .filter(|&c| !is_mark(c))
        .map(|c| match c {
            '\u{06cc}' | '\u{0649}' => '\u{064a}',
            '\u{06a9}' => '\u{0643}',
            '\u{0629}' | '\u{06c1}' => '\u{0647}',
            c => c,
        })
        .collect();
    js::trim(&js::collapse_ws(&folded.to_lowercase())).to_string()
}
