//! Dispatch for the recorded cases of the larger modules.
use pulse_core::*;
use serde_json::{Value, json};

fn s(v: &Value) -> String {
    pulse_core::js::str_or_empty(Some(v))
}

fn query(v: &Value) -> lyrics_match::LyricsQuery {
    let list = |k: &str| {
        v[k].as_array()
            .map(|a| a.iter().map(s).collect())
            .unwrap_or_default()
    };
    lyrics_match::LyricsQuery {
        titles: list("titles"),
        artists: list("artists"),
    }
}

pub static SKIP: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| json!("__skip__"));
fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}
/// The ids record-backup.js pins: pl_<base36(clock)>i, the clock counting from 1000.
fn pinned_ids() -> impl FnMut() -> String {
    let mut n = 1000u64;
    move || {
        let id = format!("pl_{}i", backup::base36(n));
        n += 1;
        id
    }
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(s).collect())
        .unwrap_or_default()
}
fn arr(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}
fn cand(v: &Value) -> track_match::Candidate {
    track_match::Candidate {
        title: s(&v["title"]),
        artist: s(&v["artist"]),
        uploader: s(&v["uploader"]),
        duration: pulse_core::js::num_or_zero(v.get("duration")),
    }
}

pub fn run(m: &str, f: &str, a: &[Value]) -> Option<Value> {
    let arg = |i: usize| a.get(i).cloned().unwrap_or(Value::Null);
    Some(match (m, f) {
        ("lyrics_match", "norm") => json!(lyrics_match::norm(&s(&arg(0)))),
        ("lyrics_match", "clean_title") => json!(lyrics_match::clean_title(
            &s(&arg(0)),
            arg(1).as_bool().unwrap_or(false)
        )),
        ("lyrics_match", "clean_artist") => json!(lyrics_match::clean_artist(&s(&arg(0)))),
        ("lyrics_match", "split_artists") => json!(lyrics_match::split_artists(&s(&arg(0)))),
        ("lyrics_match", "parse_for_lyrics") => {
            json!(lyrics_match::parse_for_lyrics(&s(&arg(0)), &s(&arg(1))))
        }
        ("lyrics_match", "pick_best") => {
            let list = arg(0).as_array().cloned().unwrap_or_default();
            if !arg(0).is_array() {
                return Some(Value::Null);
            }
            lyrics_match::pick_best(&list, &query(&arg(1)), arg(2).as_f64().unwrap_or(f64::NAN))
                .cloned()
                .unwrap_or(Value::Null)
        }
        ("track_match", "match_norm") => json!(track_match::match_norm(&s(&arg(0)))),
        ("track_match", "split_artists") => json!(track_match::split_artists(&s(&arg(0)))),
        ("track_match", "clean_title") => {
            json!(track_match::clean_title(&s(&arg(0)), &strings(&arg(1))))
        }
        ("track_match", "title_similarity") => {
            json!(track_match::title_similarity(&s(&arg(0)), &s(&arg(1))))
        }
        ("track_match", "score_candidate") => {
            json!(track_match::score_candidate(&cand(&arg(0)), &cand(&arg(1))))
        }
        ("track_match", "match_queries") => json!(track_match::match_queries(&cand(&arg(0)))),
        ("track_match", "pick_best_match") => {
            let list: Vec<Value> = arg(1).as_array().cloned().unwrap_or_default();
            let cands: Vec<_> = list.iter().map(cand).collect();
            match track_match::pick_best_match(&cand(&arg(0)), &cands) {
                Some(b) => list[cands.iter().position(|c| std::ptr::eq(c, b)).unwrap()].clone(),
                None => Value::Null,
            }
        }
        ("track_sort", "sort_tracks") => json!(track_sort::sort_tracks(&arr(&arg(0)), &s(&arg(1)))),
        ("track_sort", "sort_library") => json!(track_sort::sort_library(
            &s(&arg(0)),
            &arr(&arg(1)),
            &s(&arg(2)),
            None
        )),
        ("track_sort", "sort_library_plays_field") => {
            let plays = |x: &Value| pulse_core::js::to_number(x.get("plays"));
            json!(track_sort::sort_library(
                &s(&arg(0)),
                &arr(&arg(1)),
                &s(&arg(2)),
                Some(&plays)
            ))
        }
        ("track_sort", "library_sort_mode") => {
            json!(track_sort::library_sort_mode(&s(&arg(0)), &s(&arg(1))))
        }
        ("track_sort", "favorite_track_key") => json!(track_sort::favorite_track_key(&arg(0))),
        ("track_sort", "is_same_song") => json!(track_sort::is_same_song(&arg(0), &arg(1))),
        ("track_sort", "dedupe_favorite_tracks") => {
            json!(track_sort::dedupe_favorite_tracks(&arr(&arg(0))))
        }
        ("track_sort", "backfill_added_at_fixed") => json!(track_sort::backfill_added_at(
            &arr(&arg(0)),
            arg(1).as_bool().unwrap_or(false),
            1e12
        )),
        ("library_pages", "name_key") => json!(library_pages::name_key(a.first())),
        ("library_pages", "is_explicit_text") => json!(library_pages::is_explicit_text(a.first())),
        ("library_pages", "same_album") => json!(library_pages::same_album(&arg(0), &arg(1))),
        ("library_pages", "is_album_saved") => {
            json!(library_pages::is_album_saved(&arr(&arg(0)), &arg(1)))
        }
        ("library_pages", "album_entry") if a.len() > 1 => {
            library_pages::album_entry(&arg(0), num(&arg(1)))
        }
        ("library_pages", "toggle_saved_album") if a.len() > 2 => {
            let (list, saved) =
                library_pages::toggle_saved_album(&arr(&arg(0)), &arg(1), num(&arg(2)));
            json!({"list": list, "saved": saved})
        }
        ("library_pages", "follow_matches") => {
            json!(library_pages::follow_matches(&arg(0), &arg(1)))
        }
        ("library_pages", "is_artist_followed") => {
            json!(library_pages::is_artist_followed(&arr(&arg(0)), &arg(1)))
        }
        ("library_pages", "follow_entry") if a.len() > 1 => {
            library_pages::follow_entry(&arg(0), num(&arg(1)))
        }
        ("library_pages", "toggle_follow_artist") if a.len() > 2 => {
            let (list, followed) =
                library_pages::toggle_follow_artist(&arr(&arg(0)), &arg(1), num(&arg(2)));
            json!({"list": list, "followed": followed})
        }
        ("library_pages", "refresh_followed") => {
            let (list, changed) = library_pages::refresh_followed(&arr(&arg(0)), &arg(1));
            json!({"list": list, "changed": changed})
        }
        ("library_pages", "plan_mixes") if a.len() > 2 => json!(library_pages::plan_mixes(
            &arr(&arg(0)),
            &s(&arg(1)),
            num(&arg(2)) as usize
        )),
        ("library_pages", "plan_mixes") if arg(1).get("day").is_some() => {
            let o = arg(1);
            json!(library_pages::plan_mixes(
                &arr(&arg(0)),
                &s(&o["day"]),
                o.get("max").and_then(Value::as_u64).unwrap_or(5) as usize
            ))
        }
        ("library_pages", "clean_feedback") => library_pages::clean_feedback(&arg(0)),
        ("library_pages", "is_mix_rated") => {
            json!(library_pages::is_mix_rated(&arg(0), &s(&arg(1))))
        }
        ("library_pages", "rate_mix") if a.len() <= 3 => {
            library_pages::rate_mix(&arg(0), &arg(1), arg(2).as_bool().unwrap_or(false))
        }
        ("library_pages", "feedback_avoid") => {
            let (keys, artists) = library_pages::feedback_avoid(&arg(0));
            json!({"keys": keys, "artists": artists})
        }
        ("library_pages", "is_track_explicit") => json!(library_pages::is_track_explicit(&arg(0))),
        ("library_pages", "is_track_blocked") => {
            json!(library_pages::is_track_blocked(&arg(0), &arg(1)))
        }
        ("library_pages", "toggle_blocked_artist") => {
            let (list, blocked) = library_pages::toggle_blocked_artist(&arr(&arg(0)), a.get(1));
            json!({"list": list, "blocked": blocked})
        }
        ("library_pages", "toggle_blocked_song") => {
            let (list, blocked) = library_pages::toggle_blocked_song(&arr(&arg(0)), &arg(1));
            json!({"list": list, "blocked": blocked})
        }
        ("recommend", "parse_length") => json!(recommend::parse_length(&s(&arg(0)))),
        ("recommend", "clean_video_title") => {
            json!(recommend::clean_video_title(&s(&arg(0)), &s(&arg(1))))
        }
        ("recommend", "parse_watch_playlist") => {
            json!(recommend::parse_watch_playlist(&arg(0), &s(&arg(1))))
        }
        ("recommend", "parse_similar_tracks") => json!(recommend::parse_similar_tracks(&arg(0))),
        ("recommend", "parse_similar_artists") => json!(recommend::parse_similar_artists(&arg(0))),
        ("recommend", "parse_top_tracks") => {
            json!(recommend::parse_top_tracks(&arg(0), &s(&arg(1))))
        }
        ("recommend", "drop_unwanted") => {
            let o = arg(1);
            let set = |k: &str| -> Option<std::collections::HashSet<String>> {
                match o.get(k) {
                    None | Some(Value::Null) => Some(Default::default()),
                    Some(Value::Array(v)) => {
                        Some(v.iter().map(pulse_core::js::to_string).collect())
                    }
                    Some(_) => None,
                }
            };
            let (Some(recent_keys), Some(recent_ids)) = (set("recentKeys"), set("recentIds"))
            else {
                return Some(SKIP.clone());
            };
            let blocks = o.get("blocks");
            json!(recommend::drop_unwanted(
                &arr(&arg(0)),
                &recommend::Unwanted {
                    blocks,
                    recent_keys,
                    recent_ids,
                    key_of: None
                }
            ))
        }
        (
            "library_pages",
            "album_entry"
            | "toggle_saved_album"
            | "follow_entry"
            | "toggle_follow_artist"
            | "plan_mixes"
            | "local_day"
            | "rate_mix",
        ) => SKIP.clone(), // read the clock
        ("backup", "clean_shared_track") => json!(backup::clean_shared_track(&arg(0))),
        ("backup", "export_track") => json!(backup::export_track(&arg(0))),
        ("backup", "clean_stored_track") => json!(backup::clean_stored_track(&arg(0))),
        ("backup", "parse_playlist_file") => json!(backup::parse_playlist_file(&s(&arg(0)))),
        ("backup", "playlist_file") => {
            backup::playlist_file(&arr(&arg(0)), &s(&arg(1)), "2026-11-21T00:00:00.000Z")
        }
        ("backup", "file_name") => json!(backup::file_name(&s(&arg(0)), &s(&arg(1)))),
        ("backup", "backup_key_allowed") => {
            json!(backup::backup_key_allowed(&s(&arg(0)), &|_| false))
        }
        ("backup", "backup_key_allowed_device") => {
            json!(backup::backup_key_allowed(&s(&arg(0)), &|k| k.contains("device")))
        }
        ("backup", "make_backup") => {
            let pairs: Vec<(String, Value)> = arr(&arg(0))
                .iter()
                .map(|p| (s(&p[0]), p[1].clone()))
                .collect();
            backup::make_backup(
                pairs.iter().map(|(k, v)| (k.as_str(), v)),
                &s(&arg(1)),
                &s(&arg(2)),
                "2026-11-21T00:00:00.000Z",
                &|k| k.contains("device"),
            )
        }
        ("backup", "clean_stored_playlists") => {
            json!(backup::clean_stored_playlists(&arg(0), &mut pinned_ids()))
        }
        ("backup", "read_backup") => {
            match backup::read_backup(&arg(0), &|k| k.contains("device"), &mut pinned_ids()) {
                Ok(r) => json!(r),
                Err(e) => json!({ "__err": e }),
            }
        }
        ("profiles", "raw_key") => json!(profiles::raw_key(&s(&arg(0)), &s(&arg(1)))),
        ("profiles", "own_key") => json!(profiles::own_key(&s(&arg(0)), &s(&arg(1)))),
        ("profiles", "is_device") => json!(profiles::is_device(&s(&arg(0)))),
        _ => return None,
    })
}
