//! app.js theme: generateM3Palette (the M3 colour tokens from one accent), analyzeCover (accent,
//! second accent, luminance and saturation of a cover) and the Auto (album) mode that derives the
//! theme from the playing song's cover.

/// app.js hexToRgb (accepts `#rgb` and `#rrggbb`; anything else is black).
pub fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim().trim_start_matches('#');
    let full: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    let v = |i: usize| full.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).unwrap_or(0);
    (v(0), v(2), v(4))
}

/// app.js rgbToHsl: degrees and percents, each rounded like `Math.round`.
pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let (h, s) = if max == min {
        (0.0, 0.0)
    } else {
        let d = max - min;
        let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
        let h = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } / 6.0;
        (h, s)
    };
    (js_round(h * 360.0), js_round(s * 100.0), js_round(l * 100.0))
}

fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// app.js hslToHex.
pub fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let (s, l) = (s / 100.0, l / 100.0);
    let a = s * l.min(1.0 - l);
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        l - a * (k - 3.0).min(9.0 - k).min(1.0).max(-1.0)
    };
    format!("#{:02x}{:02x}{:02x}", js_round(f(0.0) * 255.0) as u8, js_round(f(8.0) * 255.0) as u8, js_round(f(4.0) * 255.0) as u8)
}

/// app.js PALETTE_STYLES.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaletteStyle {
    pub sec_hue_shift: f64,
    pub tert_hue_shift: f64,
    pub pri_sat: f64,
    pub surf_sat: f64,
    pub cont_sat: f64,
}

pub const PALETTE_STYLES: [(&str, &str); 5] = [("classic", "Classic"), ("tonal-spot", "Tonal spot"), ("expressive", "Expressive"), ("vibrant", "Vibrant"), ("neutral", "Neutral")];

pub fn palette_style(key: &str) -> PaletteStyle {
    let p = |a, b, c, d, e| PaletteStyle { sec_hue_shift: a, tert_hue_shift: b, pri_sat: c, surf_sat: d, cont_sat: e };
    match key {
        "tonal-spot" => p(0.0, 60.0, 1.0, 0.8, 0.9),
        "expressive" => p(60.0, 120.0, 1.25, 1.5, 1.3),
        "vibrant" => p(0.0, 60.0, 1.4, 1.2, 1.4),
        "neutral" => p(0.0, 0.0, 0.35, 0.2, 0.25),
        _ => p(0.0, 0.0, 1.0, 1.0, 1.0),
    }
}

/// The colour tokens (`--md-sys-color-*`). `primary_container` can carry alpha (dark, coloured).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct Palette {
    pub primary: String,
    pub on_primary: String,
    pub primary_container: String,
    pub on_primary_container: String,
    pub secondary: String,
    pub secondary_container: String,
    pub on_secondary_container: String,
    pub tertiary: String,
    pub tertiary_container: String,
    pub background: String,
    pub surface: String,
    pub surface_dim: String,
    pub surface_bright: String,
    pub surface_container_lowest: String,
    pub surface_container_low: String,
    pub surface_container: String,
    pub surface_container_high: String,
    pub surface_container_highest: String,
    pub on_surface: String,
    pub on_surface_variant: String,
    pub outline: String,
    pub outline_variant: String,
}

/// app.js generateM3Palette. `intensity` and `brightness` are 0-100; `tint_surfaces` is
/// Settings > "Apply intensity to background" (pulse_adv_intensity_bg).
pub fn generate_palette(accent: &str, mode: &str, intensity: f64, brightness: f64, style_key: &str, tint_surfaces: bool) -> Palette {
    let accent = if accent.is_empty() { "#6750A4" } else { accent };
    let (r, g, b) = hex_to_rgb(accent);
    let (h, hs, _) = rgb_to_hsl(r, g, b);
    let ifac = intensity / 100.0;
    let style = palette_style(style_key);
    let surf_sat = |n: f64, factor: f64| {
        if tint_surfaces {
            js_round((if hs == 0.0 { 60.0 } else { hs }) * factor * 2.4).min(85.0)
        } else {
            js_round(n * factor)
        }
    };
    let h_sec = (h + style.sec_hue_shift) % 360.0;
    let h_tert = (h + style.tert_hue_shift) % 360.0;
    let mono = ifac < 0.08 || (style_key == "neutral" && ifac < 0.05);
    let br = brightness / 100.0;
    let x = hsl_to_hex;
    let s = |v: &str| v.to_string();
    let cap = |v: f64| js_round(v).min(100.0);
    let mut p = Palette::default();
    if mode == "light" {
        let lb = |v: f64| js_round(v + (br - 0.5) * 24.0).clamp(0.0, 100.0);
        if mono {
            p.primary = s("#111111");
            p.on_primary = s("#FFFFFF");
            p.primary_container = x(0.0, 0.0, lb(90.0));
            p.on_primary_container = s("#111111");
            p.secondary = s("#444444");
            p.secondary_container = x(0.0, 0.0, lb(90.0));
            p.on_secondary_container = s("#111111");
            p.tertiary = s("#444444");
            p.tertiary_container = x(0.0, 0.0, lb(90.0));
            p.background = x(0.0, 0.0, lb(99.0));
            p.surface = x(0.0, 0.0, lb(98.0));
            p.surface_dim = x(0.0, 0.0, lb(87.0));
            p.surface_bright = x(0.0, 0.0, lb(98.0));
            p.surface_container_lowest = x(0.0, 0.0, lb(100.0));
            p.surface_container_low = x(0.0, 0.0, lb(96.0));
            p.surface_container = x(0.0, 0.0, lb(94.0));
            p.surface_container_high = x(0.0, 0.0, lb(92.0));
            p.surface_container_highest = x(0.0, 0.0, lb(90.0));
            p.on_surface = s("#111111");
            p.on_surface_variant = s("#444444");
            p.outline = s("#777777");
            p.outline_variant = s("#CFCFCF");
        } else {
            let sat = cap(48.0 * ifac * style.pri_sat);
            let sat_hi = cap(80.0 * ifac * style.cont_sat);
            let sat_med = cap(55.0 * ifac * style.pri_sat);
            let sat_low = cap(30.0 * ifac * style.cont_sat);
            let sat_low2 = cap(20.0 * ifac * style.cont_sat);
            let sf = ifac * style.surf_sat;
            p.primary = x(h, sat, 40.0);
            p.on_primary = s("#FEFBFF");
            p.primary_container = x(h, sat_hi, lb(90.0));
            p.on_primary_container = x(h, sat_med, 18.0);
            p.secondary = x(h_sec, sat_low, 45.0);
            p.secondary_container = x(h_sec, sat_low, lb(90.0));
            p.on_secondary_container = x(h_sec, sat_low2, 15.0);
            p.tertiary = x(h_tert, sat_low, 45.0);
            p.tertiary_container = x(h_tert, sat_low, lb(90.0));
            p.background = x(h, surf_sat(5.0, sf), lb(99.0));
            p.surface = x(h, surf_sat(6.0, sf), lb(98.0));
            p.surface_dim = x(h, surf_sat(5.0, sf), lb(87.0));
            p.surface_bright = x(h, surf_sat(6.0, sf), lb(98.0));
            p.surface_container_lowest = x(h, surf_sat(4.0, sf), lb(100.0));
            p.surface_container_low = x(h, surf_sat(5.0, sf), lb(96.0));
            p.surface_container = x(h, surf_sat(6.0, sf), lb(94.0));
            p.surface_container_high = x(h, surf_sat(5.0, sf), lb(92.0));
            p.surface_container_highest = x(h, surf_sat(5.0, sf), lb(90.0));
            p.on_surface = s("#1C1B1F");
            p.on_surface_variant = s("#49454F");
            p.outline = s("#79747E");
            p.outline_variant = s("#CAC4D0");
        }
    } else {
        let bb = |v: f64| js_round(v * (br * 2.0)).clamp(0.0, 100.0);
        if mono {
            p.primary = s("#FFFFFF");
            p.on_primary = s("#121212");
            p.primary_container = x(0.0, 0.0, bb(24.0));
            p.on_primary_container = s("#FFFFFF");
            p.secondary = s("#D0D0D0");
            p.secondary_container = x(0.0, 0.0, bb(24.0));
            p.on_secondary_container = s("#FFFFFF");
            p.tertiary = s("#D0D0D0");
            p.tertiary_container = x(0.0, 0.0, bb(24.0));
            p.background = x(0.0, 0.0, bb(7.0));
            p.surface = x(0.0, 0.0, bb(7.0));
            p.surface_dim = x(0.0, 0.0, bb(7.0));
            p.surface_bright = x(0.0, 0.0, bb(19.0));
            p.surface_container_lowest = x(0.0, 0.0, bb(5.0));
            p.surface_container_low = x(0.0, 0.0, bb(10.0));
            p.surface_container = x(0.0, 0.0, bb(13.0));
            p.surface_container_high = x(0.0, 0.0, bb(16.0));
            p.surface_container_highest = x(0.0, 0.0, bb(20.0));
            p.on_surface = s("#FFFFFF");
            p.on_surface_variant = s("#CFCFCF");
            p.outline = s("#8A8A8A");
            p.outline_variant = s("#3A3A3A");
        } else {
            let sat = cap(60.0 * ifac * style.pri_sat);
            let sat_med = cap(50.0 * ifac * style.pri_sat);
            let sat_low = cap(15.0 * ifac * style.cont_sat);
            let sat_low2 = cap(30.0 * ifac * style.cont_sat);
            let sf = ifac * style.surf_sat;
            let pri = x(h, sat, 75.0);
            let (pr, pg, pb) = hex_to_rgb(&pri);
            p.primary = pri.clone();
            p.on_primary = x(h, sat_med, 20.0);
            p.primary_container = format!("rgba({pr}, {pg}, {pb}, 0.18)");
            p.on_primary_container = pri;
            p.secondary = x(h_sec, sat_low2, 75.0);
            p.secondary_container = x(h_sec, sat_low, 22.0);
            p.on_secondary_container = x(h_sec, sat_low2, 85.0);
            p.tertiary = x(h_tert, sat_low2, 75.0);
            p.tertiary_container = x(h_tert, sat_low, 22.0);
            p.background = x(h, surf_sat(5.0, sf), bb(7.0));
            p.surface = x(h, surf_sat(6.0, sf), bb(8.0));
            p.surface_dim = x(h, surf_sat(5.0, sf), bb(7.0));
            p.surface_bright = x(h, surf_sat(4.0, sf), bb(24.0));
            p.surface_container_lowest = x(h, surf_sat(5.0, sf), bb(5.0));
            p.surface_container_low = x(h, surf_sat(5.0, sf), bb(10.0));
            p.surface_container = x(h, surf_sat(6.0, sf), bb(13.0));
            p.surface_container_high = x(h, surf_sat(5.0, sf), bb(16.0));
            p.surface_container_highest = x(h, surf_sat(5.0, sf), bb(20.0));
            p.on_surface = s("#e2e2e9");
            p.on_surface_variant = s("#c4c6d0");
            p.outline = s("#8e9099");
            p.outline_variant = s("#282a33");
        }
    }
    p
}

/// app.js analyzeCover result.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CoverAnalysis {
    pub accent: String,
    pub accent2: String,
    pub luminance: f64,
    pub saturation: f64,
}

impl Default for CoverAnalysis {
    fn default() -> Self {
        Self { accent: "#6750A4".into(), accent2: "#9A82DB".into(), luminance: 0.2, saturation: 0.5 }
    }
}

/// app.js analyzeCover over RGBA pixels (the original draws the cover at 48x48 first).
pub fn analyze_cover(rgba: &[u8]) -> CoverAnalysis {
    let total = rgba.len() / 4;
    if total == 0 {
        return CoverAnalysis::default();
    }
    #[derive(Clone, Copy, Default)]
    struct Bin {
        weight: f64,
        r: f64,
        g: f64,
        b: f64,
        count: u32,
    }
    let mut bins = [Bin::default(); 24];
    let (mut lum_sum, mut sat_sum, mut sat_n) = (0.0, 0.0, 0u32);
    let (mut fr, mut fg, mut fb) = (0.0, 0.0, 0.0);
    for px in rgba.chunks_exact(4) {
        let (r, g, b) = (f64::from(px[0]), f64::from(px[1]), f64::from(px[2]));
        fr += r;
        fg += g;
        fb += b;
        let (rn, gn, bn) = (r / 255.0, g / 255.0, b / 255.0);
        lum_sum += 0.2126 * rn + 0.7152 * gn + 0.0722 * bn;
        let (max, min) = (rn.max(gn).max(bn), rn.min(gn).min(bn));
        let d = max - min;
        let l = (max + min) / 2.0;
        let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
        if s >= 0.15 {
            sat_sum += s;
            sat_n += 1;
            if (0.10..=0.90).contains(&l) {
                let h = if d > 0.0 {
                    if max == rn {
                        ((gn - bn) / d + if gn < bn { 6.0 } else { 0.0 }) * 60.0
                    } else if max == gn {
                        ((bn - rn) / d + 2.0) * 60.0
                    } else {
                        ((rn - gn) / d + 4.0) * 60.0
                    }
                } else {
                    0.0
                };
                let bin = &mut bins[((h / 15.0).floor() as i64).clamp(0, 23) as usize];
                bin.weight += s;
                bin.r += r;
                bin.g += g;
                bin.b += b;
                bin.count += 1;
            }
        }
    }
    let hex = |r: f64, g: f64, b: f64| format!("#{:02x}{:02x}{:02x}", r as u8, g as u8, b as u8);
    let avg = |bin: &Bin| (js_round(bin.r / f64::from(bin.count)), js_round(bin.g / f64::from(bin.count)), js_round(bin.b / f64::from(bin.count)));
    let mut best: Option<usize> = None;
    for i in 0..24 {
        if bins[i].count > 0 && best.is_none_or(|b| bins[i].weight > bins[b].weight) {
            best = Some(i);
        }
    }
    let n = total as f64;
    let (ar, ag, ab) = match best {
        Some(i) => avg(&bins[i]),
        None => (js_round(fr / n), js_round(fg / n), js_round(fb / n)),
    };
    let mut best2: Option<usize> = None;
    if let Some(bi) = best {
        let centre = |i: usize| (i as f64 + 0.5) * 15.0;
        for i in 0..24 {
            if bins[i].count > 0 && i != bi {
                let diff = (centre(i) - centre(bi)).abs();
                if diff.min(360.0 - diff) >= 60.0 && best2.is_none_or(|b| bins[i].weight > bins[b].weight) {
                    best2 = Some(i);
                }
            }
        }
    }
    let accent2 = match best2 {
        Some(i) => {
            let (r, g, b) = avg(&bins[i]);
            hex(r, g, b)
        }
        None => {
            let (h, s, l) = rgb_to_hsl(ar as u8, ag as u8, ab as u8);
            hsl_to_hex((h + 40.0) % 360.0, s, l)
        }
    };
    CoverAnalysis {
        accent: hex(ar, ag, ab),
        accent2,
        luminance: lum_sum / n,
        saturation: if sat_n > 0 { sat_sum / f64::from(sat_n) } else { 0.0 },
    }
}

/// Auto (album) mode: the mode, intensity and brightness app.js derives from a cover.
pub fn auto_theme(a: &CoverAnalysis) -> (&'static str, f64, f64) {
    let mode = if a.luminance > 0.58 { "light" } else { "dark" };
    let intensity = js_round(a.saturation * 100.0 * 0.9).clamp(20.0, 85.0);
    let brightness = if mode == "dark" {
        js_round(40.0 + (a.luminance.min(0.58) / 0.58) * 20.0).clamp(40.0, 60.0)
    } else {
        let norm = ((a.luminance - 0.58) / (1.0 - 0.58)).clamp(0.0, 1.0);
        js_round(55.0 + norm * 20.0).clamp(55.0, 75.0)
    };
    (mode, intensity, brightness)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_css() {
        // the dark, intensity-zero palette is styles.css's hard-coded one
        let p = generate_palette("#6750A4", "dark", 0.0, 50.0, "classic", false);
        assert_eq!(p.background, "#121212");
        assert_eq!(p.surface_container, "#212121");
        assert_eq!(p.surface_container_high, "#292929");
        assert_eq!(p.primary, "#FFFFFF");
        assert_eq!(hsl_to_hex(255.0, 55.0, 50.0), "#5c39c6");
        assert_eq!(rgb_to_hsl(0x67, 0x50, 0xA4), (256.0, 34.0, 48.0));
    }
}
