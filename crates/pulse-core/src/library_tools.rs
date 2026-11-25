//! library-tools.js
use std::collections::HashSet;
use std::hash::Hash;

/// Keeps the first of each track, by identity key (one pass).
pub fn dedupe_by_key<T: Clone, K: Eq + Hash>(tracks: &[T], key_of: impl Fn(&T) -> K) -> Vec<T> {
    let mut seen = HashSet::new();
    tracks
        .iter()
        .filter(|t| seen.insert(key_of(t)))
        .cloned()
        .collect()
}

/// Keeps the first of each track, comparing every pair with `same(kept, candidate)`.
pub fn dedupe_by<T: Clone>(tracks: &[T], same: impl Fn(&T, &T) -> bool) -> Vec<T> {
    let mut kept: Vec<T> = Vec::new();
    for t in tracks {
        if !kept.iter().any(|o| same(o, t)) {
            kept.push(t.clone());
        }
    }
    kept
}
