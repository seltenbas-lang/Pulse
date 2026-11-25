//! Pulse's storage: the same keys and string values the app kept in localStorage (`pulse_*`,
//! `devsize_*`, profiles under `pulse_p:<id>:`), in one JSON file written atomically (ADR-009).
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use pulse_core::{backup, profiles};
use serde_json::{Value, json};

pub const PLAYLISTS: &str = "devsize_playlists";
pub const FAVORITES: &str = "devsize_favorites";
pub const HISTORY: &str = "devsize_history";

pub struct Store {
    path: PathBuf,
    raw: IndexMap<String, String>,
    /// The open profile.
    pub profile: String,
}

impl Store {
    /// Opens (or starts) the store file; the open profile is the stored active one.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let raw = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => IndexMap::new(),
            Err(e) => return Err(e),
        };
        let mut s = Self {
            path,
            raw,
            profile: profiles::FIRST.into(),
        };
        if let Some(active) = s.raw.get(profiles::ACTIVE).cloned() {
            s.profile = active;
        }
        Ok(s)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the file (to a temporary name, then renamed over the old one).
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(&self.raw)?)?;
        std::fs::rename(tmp, &self.path)
    }

    /// `localStorage.getItem(key)` as the open profile sees it.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.raw
            .get(&profiles::raw_key(&self.profile, key))
            .map(String::as_str)
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.raw
            .insert(profiles::raw_key(&self.profile, key), value.into());
    }

    pub fn remove(&mut self, key: &str) {
        self.raw
            .shift_remove(&profiles::raw_key(&self.profile, key));
    }

    /// Every stored key, as stored (all profiles).
    pub fn raw_keys(&self) -> Vec<String> {
        self.raw.keys().cloned().collect()
    }

    pub fn remove_raw(&mut self, raw: &str) {
        self.raw.shift_remove(raw);
    }

    /// `key` as profile `id` sees it, without opening that profile (profiles-core.js peek).
    pub fn peek(&self, id: &str, key: &str) -> Option<&str> {
        self.raw.get(&profiles::raw_key(id, key)).map(String::as_str)
    }

    /// The keys the open profile sees.
    pub fn keys(&self) -> Vec<String> {
        self.raw
            .keys()
            .filter_map(|r| profiles::own_key(&self.profile, r))
            .collect()
    }

    /// A key holding JSON (the app's lists and objects); `fallback` when missing or broken.
    pub fn json(&self, key: &str, fallback: Value) -> Value {
        self.get(key)
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(fallback)
    }

    pub fn set_json(&mut self, key: &str, v: &Value) {
        let s = pulse_core::js::stringify(v);
        self.set(key, s);
    }

    pub fn list(&self, key: &str) -> Vec<Value> {
        match self.json(key, json!([])) {
            Value::Array(a) => a,
            _ => Vec::new(),
        }
    }

    /// Settings the app stores as plain strings ("1"/"0", numbers).
    pub fn number(&self, key: &str, fallback: f64) -> f64 {
        self.get(key)
            .and_then(|s| s.trim().parse().ok())
            .filter(|x: &f64| x.is_finite())
            .unwrap_or(fallback)
    }

    /// Restores a backup file's bytes into the open profile: its playlists, favourites, history and
    /// settings replace this profile's (what a backup cannot carry stays). Returns what was restored.
    pub fn restore_backup(&mut self, bytes: &[u8], now_ms: u64) -> Result<backup::Counts, String> {
        let parsed = backup::parse_backup_file(bytes)?;
        let mut n = 0u64;
        let mut new_id = || {
            n += 1;
            backup::new_playlist_id(now_ms + n, now_ms.wrapping_mul(2654435761).wrapping_add(n))
        };
        let r = backup::read_backup(&parsed, &profiles::is_device, &mut new_id)?;
        if r.counts.keys == 0 {
            return Err("That backup has nothing Pulse can restore".into());
        }
        for k in self.keys() {
            if !backup::keep_on_restore(&k, &profiles::is_device) && !r.entries.contains_key(&k) {
                self.remove(&k);
            }
        }
        for (k, v) in &r.entries {
            if let Some(s) = v.as_str() {
                self.set(k, s);
            }
        }
        Ok(r.counts)
    }

    /// The open profile's keys as a backup file.
    pub fn make_backup(&self, profile_name: &str, app_version: &str, exported_at: &str) -> Value {
        let pairs: Vec<(String, Value)> = self
            .keys()
            .into_iter()
            .filter_map(|k| self.get(&k).map(|v| (k.clone(), json!(v))))
            .collect();
        backup::make_backup(
            pairs.iter().map(|(k, v)| (k.as_str(), v)),
            profile_name,
            app_version,
            exported_at,
            &profiles::is_device,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("pulse-store-{name}-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn profiles_see_their_own_keys() {
        let mut s = Store::open(tmp("profiles")).unwrap();
        s.set("pulse_theme", "dark");
        s.set("pulse_fps_cap", "30");
        s.profile = "p2".into();
        assert_eq!(s.get("pulse_theme"), None);
        assert_eq!(s.get("pulse_fps_cap"), Some("30")); // a device key
        s.set("pulse_theme", "light");
        assert!(s.raw.contains_key("pulse_p:p2:pulse_theme"));
        s.save().unwrap();
        let mut again = Store::open(s.path()).unwrap();
        assert_eq!(again.get("pulse_theme"), Some("dark"));
        again.profile = "p2".into();
        assert_eq!(again.get("pulse_theme"), Some("light"));
    }

    #[test]
    fn restore_replaces_data_and_keeps_device_keys() {
        let mut s = Store::open(tmp("restore")).unwrap();
        s.set("pulse_theme", "dark");
        s.set("pulse_old_setting", "x");
        s.set("pulse_custom_css", "body{}");
        s.set("pulse_fps_cap", "30");
        let backup = json!({ "kind": "pulse-backup", "profile": { "name": "Me" }, "data": {
            "pulse_theme": "light",
            "devsize_playlists": "[{\"id\":\"p1\",\"name\":\"Mix\",\"tracks\":[{\"id\":\"dQw4w9WgXcQ\",\"title\":\"x\"}]}]",
            "devsize_favorites": "[{\"id\":\"a\",\"title\":\"A\"},{\"nope\":1}]",
            "pulse_fps_cap": "60" } });
        let c = s.restore_backup(backup.to_string().as_bytes(), 1).unwrap();
        assert_eq!((c.playlists, c.favorites, c.keys), (1, 1, 3));
        assert_eq!(s.get("pulse_theme"), Some("light"));
        assert_eq!(s.get("pulse_old_setting"), None);
        assert_eq!(s.get("pulse_custom_css"), Some("body{}"));
        assert_eq!(s.get("pulse_fps_cap"), Some("30"));
        assert_eq!(s.list(PLAYLISTS)[0]["name"], "Mix");
        assert!(s.restore_backup(b"{}", 1).is_err());
    }
}
