//! profiles-core.js, the key layout: the first profile keeps plain keys, others store
//! `pulse_p:<id>:<key>`; device keys are shared by every profile.
pub const LIST: &str = "pulse_profiles";
pub const ACTIVE: &str = "pulse_profile_active";
pub const PREFIX: &str = "pulse_p:";
pub const FIRST: &str = "default";

const DEVICE: &[&str] = &[
    LIST,
    ACTIVE,
    "pulse_tray_on_close",
    "pulse_global_media_keys",
    "pulse_win_global_media_keys",
    "pulse_hide_titlebar",
    "pulse_perf_mode",
    "pulse_battery_saver",
    "pulse_fps_cap",
    "pulse_cache_limit_mb",
    "pulse_stream_timeout",
    "pulse_audio_output_device",
    "pulse_update_found",
    "pulse_update_last",
    "pulse_update_snooze",
    "pulse_welcome_done",
    "pulse_bg_asked",
    "pulse_scale_rebased_v2",
    "pulse_debug_log",
    "pulse_i18n_snap",
];
const DEVICE_PREFIXES: &[&str] = &["pulse_audio_analysis_", "pulse_secret_"];

pub const COLORS: &[&str] = &[
    "#D0BCFF", "#A8C7FA", "#7FD8C0", "#C5E1A5", "#FDD663", "#FFB59A", "#F4A7C6", "#B8C4D0",
];

/// Keys that belong to the device and are shared by every profile.
pub fn is_device(k: &str) -> bool {
    DEVICE.contains(&k) || DEVICE_PREFIXES.iter().any(|p| k.starts_with(p))
}

fn ns_of(id: &str) -> String {
    if id == FIRST {
        String::new()
    } else {
        format!("{PREFIX}{id}:")
    }
}

/// Where profile `id` keeps `key`.
pub fn raw_key(id: &str, key: &str) -> String {
    if is_device(key) {
        key.to_string()
    } else {
        ns_of(id) + key
    }
}

/// The key profile `id` sees for a stored key, or `None` when it belongs to another profile.
pub fn own_key(id: &str, raw: &str) -> Option<String> {
    if raw.starts_with(PREFIX) {
        let ns = ns_of(id);
        let key = raw.strip_prefix(ns.as_str()).filter(|_| !ns.is_empty())?;
        return (!is_device(key)).then(|| key.to_string());
    }
    (id == FIRST || is_device(raw)).then(|| raw.to_string())
}

/// profiles.js EMOJI: the pictures a profile can have ("" = its first letter).
pub const EMOJI: &[&str] = &["", "🎧", "🎸", "🎹", "🎤", "🥁", "🦊", "🐱", "🐼", "🌙", "⭐", "🔥", "🌈", "🍀"];

fn js_slice_chars(s: &str, n: usize) -> String {
    // String#slice counts UTF-16 units; whole characters are kept (a cut surrogate pair is dropped)
    let mut out = String::new();
    let mut units = 0;
    for c in s.chars() {
        units += c.len_utf16();
        if units > n {
            break;
        }
        out.push(c);
    }
    out
}

/// profiles-core.js cleanName: whitespace runs to one space, trimmed, at most 24.
pub fn clean_name(s: &str) -> String {
    js_slice_chars(&s.split_whitespace().collect::<Vec<_>>().join(" "), 24)
}

/// profiles-core.js cleanEmoji: trimmed, at most 16.
pub fn clean_emoji(s: &str) -> String {
    js_slice_chars(s.trim(), 16)
}

/// profiles-core.js cleanColor: `#rrggbb` upper-cased, else "".
pub fn clean_color(c: &str) -> String {
    let ok = c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit());
    if ok { c.to_uppercase() } else { String::new() }
}

/// profiles.js initial: the name's first character, upper-cased ("?" when empty).
pub fn initial(name: &str) -> String {
    name.trim().chars().next().map_or_else(|| "?".to_string(), |c| c.to_uppercase().to_string())
}

/// profiles.js inkOn: dark letters on light colours, white on dark ones.
pub fn ink_on(hex: &str) -> &'static str {
    let n = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    let l = (0.299 * f64::from(n >> 16) + 0.587 * f64::from((n >> 8) & 255) + 0.114 * f64::from(n & 255)) / 255.0;
    if l > 0.6 { "#1D1B20" } else { "#FFFFFF" }
}

/// profiles-core.js validId.
pub fn valid_id(id: &str) -> bool {
    id == FIRST || ((4..=16).contains(&id.len()) && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// The raw keys that hold profile `id`'s own things (profiles-core.js ownRawKeys).
pub fn is_own_raw(id: &str, raw: &str) -> bool {
    own_key(id, raw).is_some_and(|k| !is_device(&k))
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn cleaning_like_profiles_core() {
        assert_eq!(clean_name("  Anna   Lee  "), "Anna Lee");
        assert_eq!(clean_name("abcdefghijklmnopqrstuvwxyz"), "abcdefghijklmnopqrstuvwx");
        assert_eq!(clean_color("#d0bcff"), "#D0BCFF");
        assert_eq!(clean_color("red"), "");
        assert_eq!(ink_on("#D0BCFF"), "#1D1B20");
        assert_eq!(ink_on("#333333"), "#FFFFFF");
        assert_eq!(initial(" mia"), "M");
        assert!(valid_id("ab12") && !valid_id("AB12") && !valid_id("a"));
    }
}
