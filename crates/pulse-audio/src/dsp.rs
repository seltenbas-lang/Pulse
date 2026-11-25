//! Biquad filters with WebAudio's formulas (Audio EQ Cookbook, as BiquadFilterNode computes them)
//! and the EQ chain of audio-engine.js.

/// The 10-band EQ's centre frequencies (audio-engine.js EQ_BAND_FREQS).
pub const EQ_BAND_FREQS: [f64; 10] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    LowShelf,
    HighShelf,
    Peaking,
}

#[derive(Debug, Clone, Copy, Default)]
struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

/// One biquad, per-channel state for up to 2 channels (direct form I).
#[derive(Debug, Clone)]
pub struct Biquad {
    pub kind: Kind,
    pub freq: f64,
    pub q: f64,
    pub gain_db: f64,
    c: Coeffs,
    state: [[f64; 4]; 2],
}

impl Biquad {
    pub fn new(kind: Kind, freq: f64, q: f64, sample_rate: f64) -> Self {
        let mut b = Self {
            kind,
            freq,
            q,
            gain_db: 0.0,
            c: Coeffs::default(),
            state: [[0.0; 4]; 2],
        };
        b.set_gain(0.0, sample_rate);
        b
    }

    pub fn set_gain(&mut self, gain_db: f64, sample_rate: f64) {
        self.gain_db = gain_db;
        let a = 10f64.powf(gain_db / 40.0);
        let nyquist = sample_rate / 2.0;
        let f = (self.freq / nyquist).clamp(0.0, 1.0);
        if f >= 1.0 {
            // at or above Nyquist: WebAudio gives a flat gain (shelves: A^2 for high shelf)
            let g = match self.kind {
                Kind::HighShelf => a * a,
                _ => 1.0,
            };
            self.c = Coeffs {
                b0: g,
                ..Default::default()
            };
            return;
        }
        let w0 = std::f64::consts::PI * f;
        let (sn, cs) = w0.sin_cos();
        let (b0, b1, b2, a0, a1, a2) = match self.kind {
            Kind::Peaking => {
                let alpha = sn / (2.0 * self.q.max(1e-4));
                (
                    1.0 + alpha * a,
                    -2.0 * cs,
                    1.0 - alpha * a,
                    1.0 + alpha / a,
                    -2.0 * cs,
                    1.0 - alpha / a,
                )
            }
            // WebAudio shelves use slope S = 1
            Kind::LowShelf => {
                let alpha = sn / 2.0 * 2f64.sqrt();
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cs + k),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cs),
                    a * ((a + 1.0) - (a - 1.0) * cs - k),
                    (a + 1.0) + (a - 1.0) * cs + k,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cs),
                    (a + 1.0) + (a - 1.0) * cs - k,
                )
            }
            Kind::HighShelf => {
                let alpha = sn / 2.0 * 2f64.sqrt();
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cs + k),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cs),
                    a * ((a + 1.0) + (a - 1.0) * cs - k),
                    (a + 1.0) - (a - 1.0) * cs + k,
                    2.0 * ((a - 1.0) - (a + 1.0) * cs),
                    (a + 1.0) - (a - 1.0) * cs - k,
                )
            }
        };
        self.c = Coeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        };
    }

    #[inline]
    pub fn process(&mut self, ch: usize, x: f64) -> f64 {
        let s = &mut self.state[ch & 1];
        let y = self.c.b0 * x + self.c.b1 * s[0] + self.c.b2 * s[1]
            - self.c.a1 * s[2]
            - self.c.a2 * s[3];
        s[1] = s[0];
        s[0] = x;
        s[3] = s[2];
        s[2] = y;
        y
    }

    /// Gain of this filter at `freq` (dB): for tests.
    pub fn response_db(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let c = self.c;
        let num = ((c.b0 + c.b1 * c1 + c.b2 * c2).powi(2) + (c.b1 * s1 + c.b2 * s2).powi(2)).sqrt();
        let den = ((1.0 + c.a1 * c1 + c.a2 * c2).powi(2) + (c.a1 * s1 + c.a2 * s2).powi(2)).sqrt();
        20.0 * (num / den).log10()
    }
}

/// The original's settings: `eq` (low/mid/high) and `eqBands`, each clamped to ±12 dB.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EqSettings {
    pub low: f64,
    pub mid: f64,
    pub high: f64,
    pub bands: [f64; 10],
}

impl EqSettings {
    pub fn clamped(self) -> Self {
        let c = |v: f64| {
            if v.is_finite() {
                v.clamp(-12.0, 12.0)
            } else {
                0.0
            }
        };
        Self {
            low: c(self.low),
            mid: c(self.mid),
            high: c(self.high),
            bands: self.bands.map(c),
        }
    }

    pub fn is_flat(&self) -> bool {
        self.low == 0.0
            && self.mid == 0.0
            && self.high == 0.0
            && self.bands.iter().all(|b| *b == 0.0)
    }
}

/// lowshelf 200 Hz -> peaking 1 kHz (Q 0.9) -> highshelf 4 kHz -> ten peaking bands (Q 1.1).
pub struct EqChain {
    sample_rate: f64,
    filters: Vec<Biquad>,
    flat: bool,
}

impl EqChain {
    pub fn new(sample_rate: f64) -> Self {
        let mut filters = vec![
            Biquad::new(Kind::LowShelf, 200.0, 1.0, sample_rate),
            Biquad::new(Kind::Peaking, 1000.0, 0.9, sample_rate),
            Biquad::new(Kind::HighShelf, 4000.0, 1.0, sample_rate),
        ];
        filters.extend(
            EQ_BAND_FREQS
                .iter()
                .map(|&f| Biquad::new(Kind::Peaking, f, 1.1, sample_rate)),
        );
        Self {
            sample_rate,
            filters,
            flat: true,
        }
    }

    pub fn set(&mut self, s: EqSettings) {
        let s = s.clamped();
        let gains = [s.low, s.mid, s.high].into_iter().chain(s.bands);
        for (f, g) in self.filters.iter_mut().zip(gains) {
            if f.gain_db != g {
                f.set_gain(g, self.sample_rate);
            }
        }
        self.flat = s.is_flat();
    }

    /// Processes interleaved frames in place.
    pub fn process(&mut self, buf: &mut [f32], channels: usize) {
        if self.flat {
            return;
        }
        for (i, x) in buf.iter_mut().enumerate() {
            let ch = i % channels;
            let mut v = f64::from(*x);
            for f in &mut self.filters {
                v = f.process(ch, v);
            }
            *x = v as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaking_hits_its_gain_at_centre() {
        let mut b = Biquad::new(Kind::Peaking, 1000.0, 1.1, 48000.0);
        b.set_gain(6.0, 48000.0);
        assert!((b.response_db(1000.0, 48000.0) - 6.0).abs() < 0.01);
        assert!(b.response_db(50.0, 48000.0).abs() < 0.2);
    }

    #[test]
    fn shelves_reach_full_gain_far_away_and_half_at_corner() {
        let mut lo = Biquad::new(Kind::LowShelf, 200.0, 1.0, 48000.0);
        lo.set_gain(-12.0, 48000.0);
        assert!((lo.response_db(20.0, 48000.0) + 12.0).abs() < 0.3);
        assert!((lo.response_db(200.0, 48000.0) + 6.0).abs() < 0.1);
        let mut hi = Biquad::new(Kind::HighShelf, 4000.0, 1.0, 48000.0);
        hi.set_gain(9.0, 48000.0);
        assert!((hi.response_db(20000.0, 48000.0) - 9.0).abs() < 0.5);
    }

    #[test]
    fn flat_chain_is_bit_exact_and_clamps() {
        let mut eq = EqChain::new(44100.0);
        let mut buf = vec![0.25f32, -0.5, 0.75, -1.0];
        eq.process(&mut buf, 2);
        assert_eq!(buf, [0.25, -0.5, 0.75, -1.0]);
        eq.set(EqSettings {
            low: 40.0,
            ..Default::default()
        });
        assert_eq!(eq.filters[0].gain_db, 12.0);
    }

    #[test]
    fn band_above_nyquist_is_flat() {
        let mut b = Biquad::new(Kind::Peaking, 16000.0, 1.1, 22050.0);
        b.set_gain(12.0, 22050.0);
        assert_eq!(b.process(0, 0.5), 0.5);
    }
}
