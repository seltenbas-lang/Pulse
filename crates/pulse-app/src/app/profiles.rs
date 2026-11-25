//! Settings > Profiles and the top-bar switcher (profiles.js), over profiles-core.js's list rules:
//! each profile has a name (24 characters), a colour and a picture (an emoji, or its first letter).

use super::*;
use pulse_core::profiles::{self as pc, COLORS, EMOJI, FIRST};

/// The form being filled in: `id` None = a new profile.
#[derive(Clone, Default)]
pub(super) struct Draft {
    id: Option<String>,
    name: String,
    color: String,
    emoji: String,
}

fn color_of(hex: &str) -> Color {
    let n = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    Color::from_rgb_u8((n >> 16) as u8, (n >> 8) as u8, n as u8)
}

fn row_of(p: &Value, summary: String, open: bool) -> ProfileRow {
    let name = js::str_or_empty(p.get("name"));
    let emoji = js::str_or_empty(p.get("emoji"));
    let color = js::str_or_empty(p.get("color"));
    ProfileRow {
        pic: if emoji.is_empty() { pc::initial(&name) } else { emoji }.into(),
        name: name.into(),
        ink: color_of(pc::ink_on(&color)),
        color: color_of(&color),
        summary: summary.into(),
        open,
    }
}

impl App {
    /// profiles-core.js list(): the saved profiles, cleaned; the first one when none is saved; and a
    /// profile whose things are stored but that isn't listed comes back.
    pub(super) fn profile_list(&self) -> Vec<Value> {
        let mut out: Vec<Value> = Vec::new();
        for p in self.store.list(pc::LIST) {
            let id = js::str_or_empty(p.get("id"));
            if !p.is_object() || !pc::valid_id(&id) || out.iter().any(|q| js::str_or_empty(q.get("id")) == id) {
                continue;
            }
            let mut q = p.clone();
            let name = pc::clean_name(&js::str_or_empty(p.get("name")));
            q["name"] = json!(if name.is_empty() { "Profile".to_string() } else { name });
            let color = pc::clean_color(&js::str_or_empty(p.get("color")));
            q["color"] = json!(if color.is_empty() { COLORS[out.len() % COLORS.len()].to_string() } else { color });
            q["emoji"] = json!(pc::clean_emoji(&js::str_or_empty(p.get("emoji"))));
            q["created"] = json!(js::num_or_zero(p.get("created")));
            out.push(q);
        }
        if out.is_empty() {
            out.push(json!({ "id": FIRST, "name": "Me", "color": COLORS[0], "emoji": "", "created": 0 }));
        }
        for r in self.store.raw_keys() {
            let Some(rest) = r.strip_prefix(pc::PREFIX) else { continue };
            let Some(id) = rest.split(':').next() else { continue };
            if pc::valid_id(id) && id != FIRST && !out.iter().any(|p| js::str_or_empty(p.get("id")) == id) {
                let n = out.len();
                out.push(json!({ "id": id, "name": format!("Profile {}", n + 1), "color": COLORS[n % COLORS.len()], "emoji": "", "created": 0 }));
            }
        }
        out
    }

    fn save_profiles(&mut self, list: Vec<Value>) {
        self.store.set_json(pc::LIST, &Value::Array(list));
        self.persist();
    }

    /// profiles.js summary: "2 playlists · 5 favorites", or "Nothing saved yet".
    fn profile_summary(&self, id: &str) -> String {
        let count = |k: &str| {
            self.store
                .peek(id, k)
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .and_then(|v| v.as_array().map(Vec::len))
                .unwrap_or(0)
        };
        let mut parts = Vec::new();
        let (pl, fav) = (count(PLAYLISTS), count(FAVORITES));
        if pl > 0 {
            parts.push(plural(pl, "playlist"));
        }
        if fav > 0 {
            parts.push(plural(fav, "favorite"));
        }
        if parts.is_empty() { "Nothing saved yet".into() } else { parts.join(" · ") }
    }

    /// The rows, the open profile in the top bar, and the form's state.
    pub(super) fn refresh_profiles(&mut self) {
        let list = self.profile_list();
        let rows: Vec<ProfileRow> = list
            .iter()
            .map(|p| {
                let id = js::str_or_empty(p.get("id"));
                row_of(p, self.profile_summary(&id), id == self.store.profile)
            })
            .collect();
        if let Some(me) = rows.iter().find(|r| r.open) {
            self.ui.set_me(me.clone());
        }
        self.ui.set_profile_rows(ModelRc::new(VecModel::from(rows)));
        self.ui.set_prof_colors(ModelRc::new(VecModel::from(COLORS.iter().map(|c| color_of(c)).collect::<Vec<_>>())));
        let em = |r: std::ops::Range<usize>| ModelRc::new(VecModel::from(EMOJI[r].iter().map(|e| SharedString::from(*e)).collect::<Vec<_>>()));
        self.ui.set_prof_emojis_1(em(0..7));
        self.ui.set_prof_emojis_2(em(7..EMOJI.len()));
        self.show_draft();
    }

    /// The form's preview follows the name, colour and picture as they change.
    fn show_draft(&self) {
        let Some(d) = &self.prof_draft else { return };
        let shown = if d.name.trim().is_empty() {
            d.id.as_ref()
                .and_then(|id| self.profile_list().into_iter().find(|p| js::str_or_empty(p.get("id")) == *id))
                .map_or_else(|| "New profile".to_string(), |p| js::str_or_empty(p.get("name")))
        } else {
            d.name.clone()
        };
        let p = json!({ "name": shown, "color": d.color, "emoji": d.emoji });
        self.ui.set_prof_preview(row_of(&p, String::new(), false));
        self.ui.set_prof_letter(pc::initial(&shown).into());
        self.ui.set_prof_color(COLORS.iter().position(|c| c.eq_ignore_ascii_case(&d.color)).map_or(-1, |i| i as i32));
        self.ui.set_prof_emoji(EMOJI.iter().position(|e| *e == d.emoji).map_or(-1, |i| i as i32));
    }

    /// Edit row `i`, or (`None`) a new profile with the first unused colour (profiles.js nextColor).
    pub(super) fn start_profile_form(&mut self, i: Option<usize>) {
        let list = self.profile_list();
        let draft = match i.and_then(|i| list.get(i)) {
            Some(p) => Draft {
                id: Some(js::str_or_empty(p.get("id"))),
                name: js::str_or_empty(p.get("name")),
                color: js::str_or_empty(p.get("color")),
                emoji: js::str_or_empty(p.get("emoji")),
            },
            None => {
                let used: Vec<String> = list.iter().map(|p| js::str_or_empty(p.get("color"))).collect();
                Draft {
                    color: COLORS.iter().find(|c| !used.iter().any(|u| u.eq_ignore_ascii_case(c))).unwrap_or(&COLORS[0]).to_string(),
                    ..Default::default()
                }
            }
        };
        self.ui.set_prof_name(draft.name.as_str().into());
        self.prof_draft = Some(draft);
        self.ui.set_prof_edit(i.map_or(-2, |i| i as i32));
        self.show_draft();
    }

    pub(super) fn profile_draft_name(&mut self, name: &str) {
        if let Some(d) = &mut self.prof_draft {
            d.name = name.to_string();
        }
        self.show_draft();
    }

    pub(super) fn profile_draft_color(&mut self, i: usize) {
        if let (Some(d), Some(c)) = (&mut self.prof_draft, COLORS.get(i)) {
            d.color = c.to_string();
        }
        self.show_draft();
    }

    pub(super) fn profile_draft_emoji(&mut self, i: usize) {
        if let (Some(d), Some(e)) = (&mut self.prof_draft, EMOJI.get(i)) {
            d.emoji = e.to_string();
        }
        self.show_draft();
    }

    /// profiles.js onSubmit: update (profiles-core update) or create (profiles-core create).
    pub(super) fn save_profile_form(&mut self) {
        let Some(d) = self.prof_draft.take() else { return };
        let name = pc::clean_name(&self.ui.get_prof_name());
        let mut list = self.profile_list();
        match &d.id {
            Some(id) => {
                if let Some(p) = list.iter_mut().find(|p| js::str_or_empty(p.get("id")) == *id) {
                    if !name.is_empty() {
                        p["name"] = json!(name);
                    }
                    let c = pc::clean_color(&d.color);
                    if !c.is_empty() {
                        p["color"] = json!(c);
                    }
                    p["emoji"] = json!(pc::clean_emoji(&d.emoji));
                }
                self.save_profiles(list);
            }
            None => {
                // profiles-core randomId: 8 base-36 characters, never one in use
                let mut seed = self.seed ^ (now_ms() as u64);
                let id = loop {
                    let id: String = (0..8)
                        .map(|_| {
                            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                            std::char::from_digit(((seed >> 33) % 36) as u32, 36).unwrap_or('0')
                        })
                        .collect();
                    let ns = format!("{}{id}:", pc::PREFIX);
                    if id != FIRST
                        && !list.iter().any(|p| js::str_or_empty(p.get("id")) == id)
                        && !self.store.raw_keys().iter().any(|r| r.starts_with(&ns))
                    {
                        break id;
                    }
                };
                let n = list.len();
                let shown = if name.is_empty() { format!("Profile {}", n + 1) } else { name };
                let c = pc::clean_color(&d.color);
                let color = if c.is_empty() { COLORS[n % COLORS.len()].to_string() } else { c };
                list.push(json!({
                    "id": id,
                    "name": shown,
                    "color": color,
                    "emoji": pc::clean_emoji(&d.emoji),
                    "created": now_ms(),
                }));
                self.save_profiles(list);
                self.toast(&format!("Added {shown}"));
            }
        }
        self.ui.set_prof_edit(-1);
        self.ui.invoke_drop_focus();
        self.refresh_profiles();
        self.refresh_settings();
    }

    /// profiles-core remove: never the open profile, never the last one; its own keys go too.
    pub(super) fn delete_profile_row(&mut self, i: usize) {
        let list = self.profile_list();
        let Some(p) = list.get(i) else { return };
        let id = js::str_or_empty(p.get("id"));
        if id == self.store.profile || list.len() < 2 {
            return;
        }
        let name = js::str_or_empty(p.get("name"));
        for r in self.store.raw_keys() {
            if pc::is_own_raw(&id, &r) {
                self.store.remove_raw(&r);
            }
        }
        let rest: Vec<Value> = list.into_iter().filter(|q| js::str_or_empty(q.get("id")) != id).collect();
        if self.store.get(pc::ACTIVE) == Some(id.as_str()) {
            let open = self.store.profile.clone();
            self.store.set(pc::ACTIVE, open);
        }
        self.save_profiles(rest);
        if self.ui.get_prof_edit() == i as i32 {
            self.ui.set_prof_edit(-1);
            self.prof_draft = None;
        }
        self.refresh_profiles();
        self.toast(&format!("Deleted {name}"));
    }
}
