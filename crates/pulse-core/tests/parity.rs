//! Replays the original JS functions' recorded results (app-cases/cases.jsonl) against the ports.
use pulse_core::*;
use serde_json::{Value, json};

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}
fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// Runs one recorded call; `None` for a function this test does not know yet.
fn run(m: &str, f: &str, a: &[Value]) -> Option<Value> {
    Some(match (m, f) {
        ("search_text", "normalize_search") => json!(search_text::normalize_search(s(&a[0]))),
        ("lyrics_rtl", "is_rtl_text") => json!(lyrics_rtl::is_rtl_text(s(&a[0]))),
        ("now_playing", "position_state") => {
            match now_playing::position_state(num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3])) {
                Some(p) => {
                    json!({"duration": p.duration, "position": p.position, "playbackRate": p.playback_rate})
                }
                None => Value::Null,
            }
        }
        ("now_playing", "was_playing_at_interruption") => {
            json!(now_playing::was_playing_at_interruption(
                a[0].as_bool()?,
                a[1].as_bool()?,
                a[2].as_bool()?,
                num(&a[3]),
                num(&a[4])
            ))
        }
        _ => return pulse_core_parity_ext::run(m, f, a),
    })
}

#[path = "parity_ext.rs"]
mod pulse_core_parity_ext;

/// JSON numbers compare by value (`3` == `3.0`).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

#[test]
fn recorded_cases_match() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../app-cases/");
    let text = std::fs::read_to_string(format!("{dir}cases.jsonl"))
        .expect("record first: node app-cases/record.js > app-cases/cases.jsonl")
        + &std::fs::read_to_string(format!("{dir}from-tests.jsonl")).unwrap_or_default();
    let (mut pass, mut fails, mut skipped) = (0, Vec::new(), 0);
    for line in text.lines().filter(|l| !l.is_empty()) {
        let case: Value = serde_json::from_str(line).unwrap();
        let (m, f) = (s(&case["mod"]), s(&case["fn"]));
        if f == "backfill_added_at" {
            continue;
        } // reads the clock; tested in track_sort
        let args = case["args"].as_array().unwrap();
        let Some(got) = run(m, f, args) else {
            skipped += 1;
            eprintln!("not ported: {m}::{f}");
            continue;
        };
        if got == *pulse_core_parity_ext::SKIP {
            continue;
        }
        let want = match case.get("err") {
            Some(e) => json!({ "__err": e }),
            None => case.get("ok").cloned().unwrap_or(Value::Null),
        };
        if same(&got, &want) {
            pass += 1
        } else {
            fails.push(format!("{m}::{f}({args:?})\n   want {want}\n    got {got}"))
        }
    }
    eprintln!(
        "parity: {pass} pass, {} fail, {skipped} not ported",
        fails.len()
    );
    for f in fails.iter().take(40) {
        eprintln!("{f}");
    }
    assert!(fails.is_empty(), "{} recorded cases differ", fails.len());
    assert_eq!(skipped, 0, "recorded cases without a port");
}
