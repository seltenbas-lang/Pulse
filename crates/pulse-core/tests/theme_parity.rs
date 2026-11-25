//! generateM3Palette / hslToHex parity against outputs recorded from app.js (app-cases/record-theme.js).
use pulse_core::theme::{generate_palette, hsl_to_hex};
use serde_json::Value;

#[test]
fn theme_matches_app_js() {
    let cases = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../app-cases/theme.jsonl")).unwrap();
    let (mut n, mut bad) = (0, Vec::new());
    for line in cases.lines().filter(|l| !l.is_empty()) {
        let c: Value = serde_json::from_str(line).unwrap();
        n += 1;
        match c["f"].as_str().unwrap() {
            "hsl" => {
                let got = hsl_to_hex(c["h"].as_f64().unwrap(), c["s"].as_f64().unwrap(), c["l"].as_f64().unwrap());
                if got != c["out"].as_str().unwrap() {
                    bad.push(format!("{line} -> {got}"));
                }
            }
            _ => {
                let p = generate_palette(
                    c["a"].as_str().unwrap(),
                    c["mode"].as_str().unwrap(),
                    c["intensity"].as_f64().unwrap(),
                    c["brightness"].as_f64().unwrap(),
                    c["style"].as_str().unwrap(),
                    c["bg"].as_bool().unwrap(),
                );
                let got = serde_json::to_value(&p).unwrap();
                for (k, want) in c["out"].as_object().unwrap() {
                    if matches!(k.as_str(), "error" | "on-error") {
                        continue; // only set in mono mode, and never read by the UI
                    }
                    let have = &got[k.replace('-', "_")];
                    if have != want {
                        bad.push(format!("{} {k}: js {want} rust {have}", c["a"]));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}/{} mismatches, first: {:#?}", bad.len(), n, &bad[..bad.len().min(8)]);
}
